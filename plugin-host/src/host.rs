//! Wasmtime loader with fuel + epoch (wall-clock) + memory isolation.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, Result};
use thiserror::Error;
use wasmtime::{
    Caller, Config, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder, Trap,
};

use crate::abi::{Capability, HostApi, PoiWrite};
use crate::manifest::PluginManifest;

/// Default fuel units granted per `call` when the manifest omits `fuel_limit`.
pub const DEFAULT_FUEL: u64 = 5_000_000;
/// Default wall-clock budget per `call` when the manifest omits `timeout_ms`.
pub const DEFAULT_TIMEOUT_MS: u64 = 250;
/// Default linear-memory ceiling per guest call (16 MiB).
pub const DEFAULT_MEMORY_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct PluginLimits {
    pub fuel: u64,
    pub timeout_ms: u64,
    /// Max bytes any single linear memory may grow to during a call.
    pub memory_bytes: usize,
}

impl Default for PluginLimits {
    fn default() -> Self {
        Self {
            fuel: DEFAULT_FUEL,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            memory_bytes: DEFAULT_MEMORY_BYTES,
        }
    }
}

#[derive(Debug, Error)]
pub enum PluginError {
    #[error("manifest capability not granted by host policy: {0}")]
    CapabilityDenied(String),
    #[error("plugin exceeded fuel budget")]
    FuelExhausted,
    #[error("plugin exceeded wall-clock timeout")]
    Timeout,
    #[error("plugin exceeded memory limit")]
    MemoryExceeded,
    #[error("plugin trap: {0}")]
    Trap(String),
    #[error("plugin unavailable on this ABI ({0})")]
    UnsupportedAbi(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl From<wasmtime::Error> for PluginError {
    fn from(err: wasmtime::Error) -> Self {
        PluginError::Other(anyhow!("{err:#}"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallOutcome {
    Ok,
    FuelExhausted,
    Timeout,
    MemoryExceeded,
}

/// Guest linear-memory size after a call (bytes). Used as peak RSS proxy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GuestCallStats {
    pub memory_bytes: u64,
}

struct StoreData {
    api: Box<dyn HostApi>,
    /// Capability set retained for future per-call enforcement audits.
    #[allow(dead_code)]
    allowed: HashSet<Capability>,
    limits: StoreLimits,
}

/// Cranelift native compilation is supported for the ABIs Navi ships
/// (`arm64-v8a` / `aarch64`, `x86_64`). Anything else fails closed.
pub fn cranelift_abi_supported() -> bool {
    matches!(std::env::consts::ARCH, "aarch64" | "x86_64")
}

/// Loaded, capability-checked plugin ready for sandboxed calls.
pub struct PluginHost {
    engine: Engine,
    module: Module,
    wasm_path: PathBuf,
    manifest: PluginManifest,
    allowed: HashSet<Capability>,
    limits: PluginLimits,
}

impl PluginHost {
    /// Load a plugin from a directory containing `plugin.json` (+ `.wasm`).
    ///
    /// Capabilities declared in the manifest must be a subset of `host_policy`.
    /// Linking only installs imports for the intersection that the manifest
    /// requested — undeclared HostApi calls are not wired.
    pub fn load_dir(
        dir: &Path,
        host_policy: &HashSet<Capability>,
        default_limits: PluginLimits,
    ) -> Result<Self, PluginError> {
        let manifest_path = dir.join("plugin.json");
        let manifest = PluginManifest::from_path(&manifest_path).map_err(PluginError::Other)?;
        Self::load_manifest(dir, manifest, host_policy, default_limits)
    }

    pub fn load_manifest(
        base_dir: &Path,
        manifest: PluginManifest,
        host_policy: &HashSet<Capability>,
        default_limits: PluginLimits,
    ) -> Result<Self, PluginError> {
        if !cranelift_abi_supported() {
            return Err(PluginError::UnsupportedAbi(format!(
                "{} — Cranelift unavailable; plugin disabled (fail closed)",
                std::env::consts::ARCH
            )));
        }

        let requested = manifest.capability_set();
        for cap in &requested {
            if !host_policy.contains(cap) {
                return Err(PluginError::CapabilityDenied(cap.as_str().to_string()));
            }
        }

        let wasm_path = {
            let p = PathBuf::from(&manifest.wasm);
            if p.is_absolute() {
                p
            } else {
                base_dir.join(p)
            }
        };

        let mut config = Config::new();
        config.consume_fuel(true);
        config.epoch_interruption(true);
        let engine = Engine::new(&config)?;
        let module = Module::from_file(&engine, &wasm_path)
            .map_err(|e| anyhow!("load wasm {}: {e:#}", wasm_path.display()))?;

        let limits = PluginLimits {
            fuel: manifest.fuel_limit.unwrap_or(default_limits.fuel),
            timeout_ms: manifest.timeout_ms.unwrap_or(default_limits.timeout_ms),
            memory_bytes: manifest
                .memory_limit_bytes
                .unwrap_or(default_limits.memory_bytes),
        };

        Ok(Self {
            engine,
            module,
            wasm_path,
            manifest,
            allowed: requested,
            limits,
        })
    }

    pub fn name(&self) -> &str {
        &self.manifest.name
    }

    pub fn limits(&self) -> &PluginLimits {
        &self.limits
    }

    pub fn capabilities(&self) -> &HashSet<Capability> {
        &self.allowed
    }

    pub fn wasm_path(&self) -> &Path {
        &self.wasm_path
    }

    /// Invoke the exported entry function under fuel + wall-clock + memory limits.
    pub fn call(&self, api: Box<dyn HostApi>) -> Result<CallOutcome, PluginError> {
        self.call_with_stats(api).map(|(outcome, _)| outcome)
    }

    /// Like [`call`], plus guest linear-memory size after the export returns.
    pub fn call_with_stats(
        &self,
        api: Box<dyn HostApi>,
    ) -> Result<(CallOutcome, GuestCallStats), PluginError> {
        let mut linker = Linker::new(&self.engine);
        install_imports(&mut linker, &self.allowed)?;

        let store_limits = StoreLimitsBuilder::new()
            .memory_size(self.limits.memory_bytes)
            .instances(1)
            .memories(1)
            .tables(4)
            .trap_on_grow_failure(true)
            .build();

        let mut store = Store::new(
            &self.engine,
            StoreData {
                api,
                allowed: self.allowed.clone(),
                limits: store_limits,
            },
        );
        store.limiter(|data| &mut data.limits);
        store.set_fuel(self.limits.fuel)?;
        store.set_epoch_deadline(1);

        let instance = linker.instantiate(&mut store, &self.module)?;

        let entry_name = self.manifest.entry.as_str();
        let func = instance
            .get_typed_func::<(), ()>(&mut store, entry_name)
            .map_err(|e| anyhow!("export `{entry_name}`: {e:#}"))?;

        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::clone(&stop);
        let engine = self.engine.clone();
        let timeout = Duration::from_millis(self.limits.timeout_ms.max(1));
        let ticker = thread::spawn(move || {
            let slice = Duration::from_millis(5).min(timeout / 4 + Duration::from_millis(1));
            let start = std::time::Instant::now();
            while !stop_flag.load(Ordering::SeqCst) {
                if start.elapsed() >= timeout {
                    engine.increment_epoch();
                    break;
                }
                thread::sleep(slice);
            }
        });

        let result = func.call(&mut store, ());
        stop.store(true, Ordering::SeqCst);
        let _ = ticker.join();

        let memory_bytes = instance
            .get_memory(&mut store, "memory")
            .map(|m| m.data_size(&store) as u64)
            .unwrap_or(0);
        let stats = GuestCallStats { memory_bytes };

        match result {
            Ok(()) => Ok((CallOutcome::Ok, stats)),
            Err(err) => classify_trap(err).map(|o| (o, stats)),
        }
    }
}

fn classify_trap(err: wasmtime::Error) -> Result<CallOutcome, PluginError> {
    let msg = format!("{err:#}");
    if let Some(trap) = err.downcast_ref::<Trap>() {
        match trap {
            Trap::OutOfFuel => return Ok(CallOutcome::FuelExhausted),
            Trap::Interrupt => return Ok(CallOutcome::Timeout),
            Trap::AllocationTooLarge | Trap::MemoryOutOfBounds => {
                return Ok(CallOutcome::MemoryExceeded);
            }
            _ => {}
        }
    }
    let lower = msg.to_lowercase();
    if lower.contains("all fuel consumed") || lower.contains("out of fuel") {
        return Ok(CallOutcome::FuelExhausted);
    }
    if lower.contains("epoch") || lower.contains("interrupt") || lower.contains("deadline") {
        return Ok(CallOutcome::Timeout);
    }
    if lower.contains("growing memory")
        || lower.contains("memory growth")
        || lower.contains("out of memory")
        || lower.contains("allocation too large")
        || lower.contains("exceeded memory")
    {
        return Ok(CallOutcome::MemoryExceeded);
    }
    Err(PluginError::Trap(msg))
}

fn install_imports(
    linker: &mut Linker<StoreData>,
    allowed: &HashSet<Capability>,
) -> wasmtime::Result<()> {
    if allowed.contains(&Capability::Log) {
        linker.func_wrap(
            "navi",
            "log",
            |mut caller: Caller<'_, StoreData>, ptr: u32, len: u32| -> wasmtime::Result<()> {
                let msg = read_guest_string(&mut caller, ptr, len)?;
                caller.data_mut().api.log(&msg);
                Ok(())
            },
        )?;
    }

    if allowed.contains(&Capability::PositionRead) {
        linker.func_wrap(
            "navi",
            "get_position",
            |mut caller: Caller<'_, StoreData>, out_ptr: u32| -> wasmtime::Result<i32> {
                let Some(pos) = caller.data().api.position() else {
                    return Ok(0);
                };
                write_f64_pair(&mut caller, out_ptr, pos.lat, pos.lon)?;
                Ok(1)
            },
        )?;
    }

    if allowed.contains(&Capability::PoiQuery) {
        linker.func_wrap(
            "navi",
            "poi_query",
            |mut caller: Caller<'_, StoreData>,
             lat_bits: u64,
             lon_bits: u64,
             radius_m_bits: u64,
             out_ptr: u32,
             out_cap: u32|
             -> wasmtime::Result<i32> {
                let lat = f64::from_bits(lat_bits);
                let lon = f64::from_bits(lon_bits);
                let radius_m = f64::from_bits(radius_m_bits);
                let hits = caller.data().api.poi_query(lat, lon, radius_m);
                let json = serde_json::to_string(&hits_as_json(&hits))
                    .map_err(|e| wasmtime::Error::msg(format!("serialize poi_query: {e}")))?;
                let written = write_guest_bytes(&mut caller, out_ptr, out_cap, json.as_bytes())?;
                Ok(written as i32)
            },
        )?;
    }

    if allowed.contains(&Capability::PoiWrite) {
        linker.func_wrap(
            "navi",
            "poi_write",
            |mut caller: Caller<'_, StoreData>, ptr: u32, len: u32| -> wasmtime::Result<i32> {
                let raw = read_guest_string(&mut caller, ptr, len)?;
                let v: serde_json::Value = serde_json::from_str(&raw)
                    .map_err(|e| wasmtime::Error::msg(format!("poi_write json: {e}")))?;
                let poi = PoiWrite {
                    name: v
                        .get("name")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string(),
                    lat: v.get("lat").and_then(|x| x.as_f64()).unwrap_or(0.0),
                    lon: v.get("lon").and_then(|x| x.as_f64()).unwrap_or(0.0),
                    kind: v
                        .get("kind")
                        .and_then(|x| x.as_str())
                        .unwrap_or("plugin")
                        .to_string(),
                    open_now: v
                        .get("open_now")
                        .and_then(|x| x.as_str())
                        .map(str::to_string),
                };
                match caller.data_mut().api.poi_write(poi) {
                    Ok(()) => Ok(0),
                    Err(_) => Ok(1),
                }
            },
        )?;
    }

    if allowed.contains(&Capability::WeatherRead) {
        linker.func_wrap(
            "navi",
            "weather_read",
            |mut caller: Caller<'_, StoreData>,
             lat_bits: u64,
             lon_bits: u64,
             radius_m_bits: u64,
             out_ptr: u32,
             out_cap: u32|
             -> wasmtime::Result<i32> {
                let lat = f64::from_bits(lat_bits);
                let lon = f64::from_bits(lon_bits);
                let radius_m = f64::from_bits(radius_m_bits);
                let hits = caller.data().api.weather_read(lat, lon, radius_m);
                let json = serde_json::to_string(&hits)
                    .map_err(|e| wasmtime::Error::msg(format!("serialize weather_read: {e}")))?;
                let written = write_guest_bytes(&mut caller, out_ptr, out_cap, json.as_bytes())?;
                Ok(written as i32)
            },
        )?;
    }

    if allowed.contains(&Capability::RouteRead) {
        linker.func_wrap(
            "navi",
            "route_read",
            |mut caller: Caller<'_, StoreData>,
             out_ptr: u32,
             out_cap: u32|
             -> wasmtime::Result<i32> {
                let view = caller.data().api.route_read();
                write_json_view(&mut caller, out_ptr, out_cap, &view, "route_read")
            },
        )?;
    }

    if allowed.contains(&Capability::RouteDestinationRead) {
        linker.func_wrap(
            "navi",
            "route_destination_read",
            |mut caller: Caller<'_, StoreData>,
             out_ptr: u32,
             out_cap: u32|
             -> wasmtime::Result<i32> {
                let view = caller.data().api.route_destination_read();
                write_json_view(
                    &mut caller,
                    out_ptr,
                    out_cap,
                    &view,
                    "route_destination_read",
                )
            },
        )?;
    }

    if allowed.contains(&Capability::SafetyConfigRead) {
        linker.func_wrap(
            "navi",
            "safety_config_read",
            |mut caller: Caller<'_, StoreData>,
             out_ptr: u32,
             out_cap: u32|
             -> wasmtime::Result<i32> {
                let view = caller.data().api.safety_config_read();
                // `null` JSON when unavailable — guests must not treat as 0 m.
                write_json_view(&mut caller, out_ptr, out_cap, &view, "safety_config_read")
            },
        )?;
    }

    if allowed.contains(&Capability::AdminRegionRead) {
        linker.func_wrap(
            "navi",
            "admin_region_read",
            |mut caller: Caller<'_, StoreData>,
             lat_bits: u64,
             lon_bits: u64,
             out_ptr: u32,
             out_cap: u32|
             -> wasmtime::Result<i32> {
                let lat = f64::from_bits(lat_bits);
                let lon = f64::from_bits(lon_bits);
                let view = caller.data().api.admin_region_read(lat, lon);
                write_json_view(&mut caller, out_ptr, out_cap, &view, "admin_region_read")
            },
        )?;
    }

    if allowed.contains(&Capability::ClockRead) {
        linker.func_wrap(
            "navi",
            "clock_read",
            |mut caller: Caller<'_, StoreData>,
             out_ptr: u32,
             out_cap: u32|
             -> wasmtime::Result<i32> {
                let view = caller.data().api.clock_read();
                write_json_view(&mut caller, out_ptr, out_cap, &view, "clock_read")
            },
        )?;
    }

    if allowed.contains(&Capability::PluginKv) {
        linker.func_wrap(
            "navi",
            "plugin_kv_status",
            |caller: Caller<'_, StoreData>| -> wasmtime::Result<i32> {
                Ok(match caller.data().api.plugin_kv_status() {
                    crate::abi::PluginKvStatus::Available => 1,
                    crate::abi::PluginKvStatus::Unavailable => 0,
                })
            },
        )?;
        linker.func_wrap(
            "navi",
            "plugin_kv_get",
            |mut caller: Caller<'_, StoreData>,
             key_ptr: u32,
             key_len: u32,
             out_ptr: u32,
             out_cap: u32|
             -> wasmtime::Result<i32> {
                if caller.data().api.plugin_kv_status() == crate::abi::PluginKvStatus::Unavailable {
                    return Ok(-2); // unavailable (distinct from missing key -1)
                }
                let key = read_guest_string(&mut caller, key_ptr, key_len)?;
                match caller.data().api.plugin_kv_get(&key) {
                    Some(v) => {
                        let written =
                            write_guest_bytes(&mut caller, out_ptr, out_cap, v.as_bytes())?;
                        Ok(written as i32)
                    }
                    None => Ok(-1),
                }
            },
        )?;
        linker.func_wrap(
            "navi",
            "plugin_kv_set",
            |mut caller: Caller<'_, StoreData>,
             key_ptr: u32,
             key_len: u32,
             val_ptr: u32,
             val_len: u32|
             -> wasmtime::Result<i32> {
                if caller.data().api.plugin_kv_status() == crate::abi::PluginKvStatus::Unavailable {
                    return Ok(2); // unavailable
                }
                let key = read_guest_string(&mut caller, key_ptr, key_len)?;
                let val = read_guest_string(&mut caller, val_ptr, val_len)?;
                match caller.data_mut().api.plugin_kv_set(&key, &val) {
                    Ok(()) => Ok(0),
                    Err(_) => Ok(1),
                }
            },
        )?;
    }

    if allowed.contains(&Capability::ProtectedAreaQuery) {
        linker.func_wrap(
            "navi",
            "protected_area_query",
            |mut caller: Caller<'_, StoreData>,
             lat_bits: u64,
             lon_bits: u64,
             out_ptr: u32,
             out_cap: u32|
             -> wasmtime::Result<i32> {
                let lat = f64::from_bits(lat_bits);
                let lon = f64::from_bits(lon_bits);
                let view = caller.data().api.protected_area_query(lat, lon);
                write_json_view(&mut caller, out_ptr, out_cap, &view, "protected_area_query")
            },
        )?;
    }

    if allowed.contains(&Capability::LandTenureQuery) {
        linker.func_wrap(
            "navi",
            "land_tenure_query",
            |mut caller: Caller<'_, StoreData>,
             lat_bits: u64,
             lon_bits: u64,
             out_ptr: u32,
             out_cap: u32|
             -> wasmtime::Result<i32> {
                let lat = f64::from_bits(lat_bits);
                let lon = f64::from_bits(lon_bits);
                let view = caller.data().api.land_tenure_query(lat, lon);
                write_json_view(&mut caller, out_ptr, out_cap, &view, "land_tenure_query")
            },
        )?;
    }

    if allowed.contains(&Capability::LandcoverQuery) {
        linker.func_wrap(
            "navi",
            "landcover_query",
            |mut caller: Caller<'_, StoreData>,
             lat_bits: u64,
             lon_bits: u64,
             out_ptr: u32,
             out_cap: u32|
             -> wasmtime::Result<i32> {
                let lat = f64::from_bits(lat_bits);
                let lon = f64::from_bits(lon_bits);
                let view = caller.data().api.landcover_query(lat, lon);
                write_json_view(&mut caller, out_ptr, out_cap, &view, "landcover_query")
            },
        )?;
    }

    if allowed.contains(&Capability::TravelModeRead) {
        linker.func_wrap(
            "navi",
            "travel_mode_read",
            |mut caller: Caller<'_, StoreData>,
             out_ptr: u32,
             out_cap: u32|
             -> wasmtime::Result<i32> {
                let view = caller.data().api.travel_mode_read();
                write_json_view(&mut caller, out_ptr, out_cap, &view, "travel_mode_read")
            },
        )?;
    }

    if allowed.contains(&Capability::VehicleProfileRead) {
        linker.func_wrap(
            "navi",
            "vehicle_profile_read",
            |mut caller: Caller<'_, StoreData>,
             out_ptr: u32,
             out_cap: u32|
             -> wasmtime::Result<i32> {
                let view = caller.data().api.vehicle_profile_read();
                write_json_view(&mut caller, out_ptr, out_cap, &view, "vehicle_profile_read")
            },
        )?;
    }

    if allowed.contains(&Capability::TravellerProfileRead) {
        linker.func_wrap(
            "navi",
            "traveller_profile_read",
            |mut caller: Caller<'_, StoreData>,
             out_ptr: u32,
             out_cap: u32|
             -> wasmtime::Result<i32> {
                let view = caller.data().api.traveller_profile_read();
                write_json_view(
                    &mut caller,
                    out_ptr,
                    out_cap,
                    &view,
                    "traveller_profile_read",
                )
            },
        )?;
    }

    // Always provide a no-op alloc helper so guests can request scratch space
    // without WASI. Guests that ship their own allocator ignore this.
    linker.func_wrap(
        "navi",
        "host_nop",
        |_caller: Caller<'_, StoreData>| -> wasmtime::Result<()> { Ok(()) },
    )?;

    Ok(())
}

fn write_json_view<T: serde::Serialize>(
    caller: &mut Caller<'_, StoreData>,
    out_ptr: u32,
    out_cap: u32,
    value: &T,
    label: &str,
) -> wasmtime::Result<i32> {
    let json = serde_json::to_string(value)
        .map_err(|e| wasmtime::Error::msg(format!("serialize {label}: {e}")))?;
    let written = write_guest_bytes(caller, out_ptr, out_cap, json.as_bytes())?;
    Ok(written as i32)
}

fn hits_as_json(hits: &[PoiWrite]) -> Vec<serde_json::Value> {
    hits.iter()
        .filter(|h| h.open_now.as_deref() != Some("false"))
        .map(|h| {
            let mut v = serde_json::json!({
                "name": h.name,
                "lat": h.lat,
                "lon": h.lon,
                "kind": h.kind,
            });
            if let Some(on) = &h.open_now {
                v.as_object_mut()
                    .expect("object")
                    .insert("open_now".into(), serde_json::Value::String(on.clone()));
            }
            v
        })
        .collect()
}

fn read_guest_string(
    caller: &mut Caller<'_, StoreData>,
    ptr: u32,
    len: u32,
) -> wasmtime::Result<String> {
    let mem = caller
        .get_export("memory")
        .and_then(|e| e.into_memory())
        .ok_or_else(|| wasmtime::Error::msg("guest memory export missing"))?;
    let data = mem.data(caller);
    let start = ptr as usize;
    let end = start
        .checked_add(len as usize)
        .ok_or_else(|| wasmtime::Error::msg("guest string overflow"))?;
    if end > data.len() {
        wasmtime::bail!("guest string out of bounds");
    }
    let bytes = &data[start..end];
    Ok(String::from_utf8_lossy(bytes).into_owned())
}

fn write_guest_bytes(
    caller: &mut Caller<'_, StoreData>,
    ptr: u32,
    cap: u32,
    bytes: &[u8],
) -> wasmtime::Result<usize> {
    let mem = caller
        .get_export("memory")
        .and_then(|e| e.into_memory())
        .ok_or_else(|| wasmtime::Error::msg("guest memory export missing"))?;
    let n = bytes.len().min(cap as usize);
    let start = ptr as usize;
    let end = start + n;
    let data = mem.data_mut(caller);
    if end > data.len() {
        wasmtime::bail!("guest write out of bounds");
    }
    data[start..end].copy_from_slice(&bytes[..n]);
    Ok(n)
}

fn write_f64_pair(
    caller: &mut Caller<'_, StoreData>,
    ptr: u32,
    a: f64,
    b: f64,
) -> wasmtime::Result<()> {
    let mut buf = [0u8; 16];
    buf[..8].copy_from_slice(&a.to_le_bytes());
    buf[8..].copy_from_slice(&b.to_le_bytes());
    write_guest_bytes(caller, ptr, 16, &buf)?;
    Ok(())
}
