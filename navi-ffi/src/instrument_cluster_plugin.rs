//! Instrument-cluster export: UniFFI session + HostApi embedder.
//!
//! Default OFF. When enabled, the host pushes a `navi.guidance.v1` snapshot
//! (assembled from the same HUD / warning helpers as the map), ticks the
//! `instrument-cluster` WASM guest, and routes `vehicle_signal_publish` through
//! a selectable [`ClusterSink`] (log / USB NDJSON buffer / Kuksa path log /
//! JSON-UDP record). Guests never open sockets.

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use navi_plugin_host::{
    assemble_nav_guidance_v1, cranelift_abi_supported, idle_nav_guidance_v1, merge_primary_warning,
    plugin_set_enabled, CallOutcome, Capability, ClusterSink, ClusterSinkKind,
    GuidanceAssemblyInput, HostApi, PluginEnableStore, PluginError, PluginHost, PluginLimits,
    PoiWrite, Position,
};
use serde_json::Value;

const PLUGIN_NAME: &str = "instrument-cluster";
const ENABLE_FILE: &str = "plugin_enable.json";
const PLUGINS_REL: &str = "plugins";
const SINK_PREF_FILE: &str = "cluster_sink_kind.txt";

#[derive(uniffi::Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClusterCallKind {
    Ok,
    Disabled,
    Unavailable,
    Error,
    FuelExhausted,
    Timeout,
    MemoryExceeded,
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct ClusterTickResult {
    pub kind: ClusterCallKind,
    pub message: String,
    pub last_publish_json: Option<String>,
    pub elapsed_ms: u64,
}

#[derive(uniffi::Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClusterSinkKindFfi {
    Log,
    UsbNdjson,
    Kuksa,
    JsonUdp,
}

impl From<ClusterSinkKindFfi> for ClusterSinkKind {
    fn from(v: ClusterSinkKindFfi) -> Self {
        match v {
            ClusterSinkKindFfi::Log => ClusterSinkKind::Log,
            ClusterSinkKindFfi::UsbNdjson => ClusterSinkKind::UsbNdjson,
            ClusterSinkKindFfi::Kuksa => ClusterSinkKind::Kuksa,
            ClusterSinkKindFfi::JsonUdp => ClusterSinkKind::JsonUdp,
        }
    }
}

impl From<ClusterSinkKind> for ClusterSinkKindFfi {
    fn from(v: ClusterSinkKind) -> Self {
        match v {
            ClusterSinkKind::Log => ClusterSinkKindFfi::Log,
            ClusterSinkKind::UsbNdjson => ClusterSinkKindFfi::UsbNdjson,
            ClusterSinkKind::Kuksa => ClusterSinkKindFfi::Kuksa,
            ClusterSinkKind::JsonUdp => ClusterSinkKindFfi::JsonUdp,
        }
    }
}

struct SharedSink {
    kind: ClusterSinkKind,
    inner: Box<dyn ClusterSink>,
    last_publish: Option<String>,
    /// USB NDJSON lines drained by the Android host when a port is open.
    usb_outbox: Vec<String>,
}

impl SharedSink {
    fn new(kind: ClusterSinkKind) -> Self {
        Self {
            kind,
            inner: kind.boxed_default(),
            last_publish: None,
            usb_outbox: Vec::new(),
        }
    }
}

struct Session {
    files_dir: PathBuf,
    host: Option<Arc<PluginHost>>,
    guidance_json: Arc<Mutex<String>>,
    sink: Arc<Mutex<SharedSink>>,
    session_disable_reason: Option<String>,
}

impl Session {
    fn plugin_dir(&self) -> PathBuf {
        self.files_dir.join(PLUGINS_REL).join(PLUGIN_NAME)
    }

    fn enable_path(&self) -> PathBuf {
        self.files_dir.join(ENABLE_FILE)
    }

    fn sink_pref_path(&self) -> PathBuf {
        self.files_dir.join(SINK_PREF_FILE)
    }

    fn is_enabled(&self) -> bool {
        PluginEnableStore::open(self.enable_path())
            .ok()
            .is_some_and(|s| s.is_enabled(PLUGIN_NAME))
    }
}

fn session_lock() -> &'static Mutex<Option<Session>> {
    static LOCK: OnceLock<Mutex<Option<Session>>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(None))
}

fn cluster_policy() -> HashSet<Capability> {
    [
        Capability::Log,
        Capability::PositionRead,
        Capability::NavGuidanceRead,
        Capability::VehicleSignalPublish,
    ]
    .into_iter()
    .collect()
}

fn map_plugin_error(err: PluginError) -> (ClusterCallKind, String) {
    match err {
        PluginError::FuelExhausted => (ClusterCallKind::FuelExhausted, err.to_string()),
        PluginError::Timeout => (ClusterCallKind::Timeout, err.to_string()),
        PluginError::MemoryExceeded => (ClusterCallKind::MemoryExceeded, err.to_string()),
        other => (ClusterCallKind::Error, other.to_string()),
    }
}

fn map_outcome(outcome: CallOutcome) -> (ClusterCallKind, String) {
    match outcome {
        CallOutcome::Ok => (ClusterCallKind::Ok, "ok".into()),
        CallOutcome::FuelExhausted => map_plugin_error(PluginError::FuelExhausted),
        CallOutcome::Timeout => map_plugin_error(PluginError::Timeout),
        CallOutcome::MemoryExceeded => map_plugin_error(PluginError::MemoryExceeded),
    }
}

struct ClusterHostApi {
    guidance_json: Arc<Mutex<String>>,
    sink: Arc<Mutex<SharedSink>>,
}

impl HostApi for ClusterHostApi {
    fn position(&self) -> Option<Position> {
        let guidance = self.guidance_json.lock().ok()?;
        let v: Value = serde_json::from_str(&guidance).ok()?;
        let pos = v.get("position")?;
        Some(Position {
            lat: pos.get("lat")?.as_f64()?,
            lon: pos.get("lon")?.as_f64()?,
        })
    }

    fn poi_query(&self, _: f64, _: f64, _: f64) -> Vec<PoiWrite> {
        Vec::new()
    }

    fn poi_write(&mut self, _: PoiWrite) -> Result<(), String> {
        Ok(())
    }

    fn log(&mut self, _message: &str) {}

    fn nav_guidance_read(&self) -> String {
        self.guidance_json
            .lock()
            .map(|g| g.clone())
            .unwrap_or_else(|_| idle_nav_guidance_v1())
    }

    fn vehicle_signal_publish(&mut self, payload_json: &str) -> Result<(), String> {
        let mut sink = self.sink.lock().map_err(|e| e.to_string())?;
        sink.inner.publish_cluster_json(payload_json)?;
        sink.last_publish = Some(payload_json.to_string());
        if sink.kind == ClusterSinkKind::UsbNdjson {
            let line = payload_json.trim().replace(['\n', '\r'], " ");
            if !line.is_empty() {
                sink.usb_outbox.push(line);
            }
        }
        Ok(())
    }
}

fn ensure_loaded(session: &mut Session) -> Result<Arc<PluginHost>, (ClusterCallKind, String)> {
    if let Some(h) = &session.host {
        return Ok(Arc::clone(h));
    }
    if !cranelift_abi_supported() {
        return Err((
            ClusterCallKind::Unavailable,
            format!(
                "instrument-cluster unavailable on ABI {} (fail closed)",
                std::env::consts::ARCH
            ),
        ));
    }
    let dir = session.plugin_dir();
    if !dir.join("plugin.json").is_file() || !dir.join("plugin.wasm").is_file() {
        return Err((
            ClusterCallKind::Unavailable,
            format!("instrument-cluster not installed under {}", dir.display()),
        ));
    }
    let host = PluginHost::load_dir(
        &dir,
        &cluster_policy(),
        PluginLimits {
            fuel: 8_000_000,
            timeout_ms: 800,
            memory_bytes: 8 * 1024 * 1024,
        },
    )
    .map_err(map_plugin_error)?;
    let host = Arc::new(host);
    session.host = Some(Arc::clone(&host));
    Ok(host)
}

fn load_sink_kind(path: &std::path::Path) -> ClusterSinkKind {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| ClusterSinkKind::parse(s.trim()))
        .unwrap_or(ClusterSinkKind::Log)
}

/// Bind the cluster session to the app files dir. Safe to call more than once.
#[uniffi::export]
pub fn cluster_plugin_configure(files_dir: String) -> String {
    crate::init_native_logging();
    let files_dir = PathBuf::from(files_dir);
    let sink_kind = load_sink_kind(&files_dir.join(SINK_PREF_FILE));
    let mut guard = session_lock().lock().expect("cluster session lock");
    *guard = Some(Session {
        files_dir,
        host: None,
        guidance_json: Arc::new(Mutex::new(idle_nav_guidance_v1())),
        sink: Arc::new(Mutex::new(SharedSink::new(sink_kind))),
        session_disable_reason: None,
    });
    "OK".into()
}

/// Install or replace the guest under `filesDir/plugins/instrument-cluster/`.
#[uniffi::export]
pub fn cluster_plugin_install_guest(manifest_json: String, wasm_bytes: Vec<u8>) -> String {
    let mut guard = session_lock().lock().expect("cluster session lock");
    let Some(session) = guard.as_mut() else {
        return "FAIL: not configured".into();
    };
    let dir = session.plugin_dir();
    if let Err(e) = fs::create_dir_all(&dir) {
        return format!("FAIL: mkdir: {e}");
    }
    if let Err(e) = fs::write(dir.join("plugin.json"), manifest_json.as_bytes()) {
        return format!("FAIL: write manifest: {e}");
    }
    if let Err(e) = fs::write(dir.join("plugin.wasm"), &wasm_bytes) {
        return format!("FAIL: write wasm: {e}");
    }
    session.host = None;
    "OK".into()
}

/// Enable / disable via [`PluginEnableStore`] (default OFF).
#[uniffi::export]
pub fn cluster_plugin_set_enabled(enabled: bool) -> String {
    let mut guard = session_lock().lock().expect("cluster session lock");
    let Some(session) = guard.as_mut() else {
        return "FAIL: not configured".into();
    };
    if let Err(e) = plugin_set_enabled(&session.enable_path(), PLUGIN_NAME, enabled) {
        return format!("FAIL: enable store: {e:#}");
    }
    session.session_disable_reason = None;
    if !enabled {
        session.host = None;
        return "OK: disabled".into();
    }
    "OK: enabled".into()
}

#[uniffi::export]
pub fn cluster_plugin_is_enabled() -> bool {
    let guard = session_lock().lock().expect("cluster session lock");
    guard.as_ref().is_some_and(Session::is_enabled)
}

/// Select the host sink (`log` default). Persisted under filesDir.
#[uniffi::export]
pub fn cluster_plugin_set_sink_kind(kind: ClusterSinkKindFfi) -> String {
    let mut guard = session_lock().lock().expect("cluster session lock");
    let Some(session) = guard.as_mut() else {
        return "FAIL: not configured".into();
    };
    let kind: ClusterSinkKind = kind.into();
    if let Err(e) = fs::write(session.sink_pref_path(), kind.as_str()) {
        return format!("FAIL: persist sink: {e}");
    }
    *session.sink.lock().expect("sink") = SharedSink::new(kind);
    format!("OK: {}", kind.as_str())
}

#[uniffi::export]
pub fn cluster_plugin_get_sink_kind() -> ClusterSinkKindFfi {
    let guard = session_lock().lock().expect("cluster session lock");
    guard
        .as_ref()
        .and_then(|s| s.sink.lock().ok().map(|g| ClusterSinkKindFfi::from(g.kind)))
        .unwrap_or(ClusterSinkKindFfi::Log)
}

/// Push the latest HUD guidance snapshot (full `navi.guidance.v1` JSON).
#[uniffi::export]
pub fn cluster_plugin_set_guidance_json(guidance_json: String) -> String {
    let mut guard = session_lock().lock().expect("cluster session lock");
    let Some(session) = guard.as_mut() else {
        return "FAIL: not configured".into();
    };
    let mut g = session.guidance_json.lock().expect("guidance");
    *g = if guidance_json.trim().is_empty() {
        idle_nav_guidance_v1()
    } else {
        guidance_json
    };
    "OK".into()
}

/// Assemble guidance from HUD pieces + UniFFI warning JSON blobs.
#[allow(clippy::too_many_arguments)] // UniFFI flat export for Kotlin HUD bridge
#[uniffi::export]
pub fn cluster_assemble_guidance_json(
    route_active: bool,
    lat: Option<f64>,
    lon: Option<f64>,
    heading_deg: Option<f64>,
    speed_kmh: Option<f64>,
    h_acc_m: Option<f64>,
    speed_limit_kmh: Option<f64>,
    overspeed: bool,
    maneuver_type: Option<String>,
    maneuver_icon_key: Option<String>,
    maneuver_distance_m: Option<f64>,
    maneuver_street_name: Option<String>,
    eco_enabled: bool,
    eco_profile: Option<String>,
    trip_eta: Option<String>,
    trip_remaining_duration_s: Option<f64>,
    trip_remaining_distance_m: Option<f64>,
    rest_active: bool,
    rest_minutes_to_break: Option<f64>,
    rest_distance_to_break_m: Option<f64>,
    attitude_yaw_deg: Option<f64>,
    attitude_roll_deg: Option<f64>,
    attitude_pitch_deg: Option<f64>,
    attitude_heading_deg: Option<f64>,
    imu_calibrated: Option<bool>,
    route_polyline: String,
    road_sign_warning_json: String,
    children_warning_json: String,
    camera_warning_json: String,
    cameras_opted_in: bool,
    ts: Option<String>,
) -> String {
    let warning = merge_primary_warning(
        &road_sign_warning_json,
        &children_warning_json,
        &camera_warning_json,
        cameras_opted_in,
    );
    assemble_nav_guidance_v1(&GuidanceAssemblyInput {
        ts,
        route_active,
        lat,
        lon,
        heading_deg,
        speed_kmh,
        h_acc_m,
        speed_limit_kmh,
        overspeed,
        maneuver_type,
        maneuver_icon_key,
        maneuver_distance_m,
        maneuver_street_name,
        eco_enabled,
        eco_profile,
        trip_eta,
        trip_remaining_duration_s,
        trip_remaining_distance_m,
        rest_active,
        rest_minutes_to_break,
        rest_distance_to_break_m,
        attitude_yaw_deg,
        attitude_roll_deg,
        attitude_pitch_deg,
        attitude_heading_deg,
        imu_calibrated,
        route_polyline,
        warning,
    })
}

/// Run one guest tick (publish through the selected sink).
#[uniffi::export]
pub fn cluster_plugin_tick() -> ClusterTickResult {
    let started = Instant::now();
    let done = |kind: ClusterCallKind, message: String, last: Option<String>| ClusterTickResult {
        kind,
        message,
        last_publish_json: last,
        elapsed_ms: started.elapsed().as_millis() as u64,
    };

    let (host, guidance, sink) = {
        let mut guard = session_lock().lock().expect("cluster session lock");
        let Some(session) = guard.as_mut() else {
            return done(ClusterCallKind::Unavailable, "not configured".into(), None);
        };
        if !session.is_enabled() {
            return done(
                ClusterCallKind::Disabled,
                "instrument-cluster is off".into(),
                None,
            );
        }
        if let Some(reason) = &session.session_disable_reason {
            return done(ClusterCallKind::Disabled, reason.clone(), None);
        }
        match ensure_loaded(session) {
            Ok(host) => (
                host,
                Arc::clone(&session.guidance_json),
                Arc::clone(&session.sink),
            ),
            Err((kind, message)) => return done(kind, message, None),
        }
    };

    let api = ClusterHostApi {
        guidance_json: guidance,
        sink: Arc::clone(&sink),
    };
    let result = host.call(Box::new(api));
    let last = sink.lock().ok().and_then(|g| g.last_publish.clone());

    let (kind, message) = match result {
        Ok(outcome) => map_outcome(outcome),
        Err(err) => map_plugin_error(err),
    };
    match kind {
        ClusterCallKind::Ok => done(kind, message, last),
        ClusterCallKind::Error | ClusterCallKind::Unavailable | ClusterCallKind::Disabled => {
            done(kind, message, None)
        }
        _ => {
            if let Some(session) = session_lock()
                .lock()
                .expect("cluster session lock")
                .as_mut()
            {
                session.session_disable_reason = Some(message.clone());
                session.host = None;
            }
            done(kind, message, None)
        }
    }
}

/// Drain NDJSON lines queued for USB write (empty when sink is not usb_ndjson).
#[uniffi::export]
pub fn cluster_plugin_drain_usb_ndjson() -> Vec<String> {
    let mut guard = session_lock().lock().expect("cluster session lock");
    let Some(session) = guard.as_mut() else {
        return Vec::new();
    };
    session
        .sink
        .lock()
        .map(|mut s| std::mem::take(&mut s.usb_outbox))
        .unwrap_or_default()
}

/// Last published cluster JSON (any sink), if any.
#[uniffi::export]
pub fn cluster_plugin_last_publish_json() -> Option<String> {
    let guard = session_lock().lock().expect("cluster session lock");
    guard
        .as_ref()
        .and_then(|s| s.sink.lock().ok().and_then(|g| g.last_publish.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::sync::Mutex as StdMutex;

    // Serialise wasm guest tests that mutate the process-global session.
    static TEST_LOCK: StdMutex<()> = StdMutex::new(());

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_path_buf()
    }

    fn build_and_install(files: &std::path::Path) {
        let status = Command::new("cargo")
            .args([
                "build",
                "--release",
                "--target",
                "wasm32-unknown-unknown",
                "--manifest-path",
            ])
            .arg(workspace_root().join("plugins/instrument-cluster/Cargo.toml"))
            .status()
            .unwrap();
        assert!(status.success());
        let release = workspace_root().join("target/wasm32-unknown-unknown/release");
        let wasm = [
            "libnavi_plugin_instrument_cluster.wasm",
            "navi_plugin_instrument_cluster.wasm",
        ]
        .iter()
        .map(|n| release.join(n))
        .find(|p| p.is_file())
        .expect("wasm");
        let manifest =
            fs::read_to_string(workspace_root().join("plugins/instrument-cluster/plugin.json"))
                .unwrap();
        assert_eq!(cluster_plugin_configure(files.display().to_string()), "OK");
        assert!(cluster_plugin_install_guest(manifest, fs::read(wasm).unwrap()).starts_with("OK"));
        assert!(cluster_plugin_set_enabled(true).starts_with("OK"));
    }

    #[test]
    fn vallset_109_publishes_then_clears_and_opted_out_camera_ignored() {
        let _guard = TEST_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        build_and_install(dir.path());

        let sign = r#"{"phase":"appear","distance_m":410,"code":"109","icon_key":"no_sign_109","name_en":"School","label":"School"}"#;
        let camera = r#"{"kind":"point","phase":"appear","distance_m":80,"label":"Camera"}"#;
        let guidance = cluster_assemble_guidance_json(
            true,
            Some(60.6808046),
            Some(11.3453802),
            Some(12.0),
            Some(40.0),
            Some(5.0),
            Some(50.0),
            false,
            Some("turn_right".into()),
            Some("nav_turn_right".into()),
            Some(320.0),
            Some("Kvikneveien".into()),
            false,
            Some("Car".into()),
            None,
            None,
            None,
            false,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            String::new(),
            sign.into(),
            "{}".into(),
            camera.into(),
            false,
            Some("2026-11-25T12:00:00Z".into()),
        );
        assert!(guidance.contains("\"code\":\"109\""));
        assert!(!guidance.contains("Camera"));
        assert_eq!(cluster_plugin_set_guidance_json(guidance), "OK");
        assert!(cluster_plugin_set_sink_kind(ClusterSinkKindFfi::UsbNdjson).starts_with("OK"));

        let tick = cluster_plugin_tick();
        assert_eq!(tick.kind, ClusterCallKind::Ok, "{}", tick.message);
        let pub_json = tick.last_publish_json.expect("publish");
        assert!(pub_json.contains("navi.cluster.v1"));
        assert!(pub_json.contains("109"));
        assert!(pub_json.contains("\"l_per_scandinavian_mil\":null"));
        assert!(!cluster_plugin_drain_usb_ndjson().is_empty());

        let cleared = cluster_assemble_guidance_json(
            true,
            Some(60.6808046),
            Some(11.3453802),
            None,
            Some(40.0),
            None,
            Some(50.0),
            false,
            Some("turn_right".into()),
            Some("nav_turn_right".into()),
            Some(320.0),
            None,
            false,
            None,
            None,
            None,
            None,
            false,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            String::new(),
            r#"{"phase":"hidden","code":"109"}"#.into(),
            "{}".into(),
            "{}".into(),
            false,
            None,
        );
        assert!(cleared.contains("\"warning\":null"));
        cluster_plugin_set_guidance_json(cleared);
        let tick = cluster_plugin_tick();
        assert_eq!(tick.kind, ClusterCallKind::Ok, "{}", tick.message);
        let pub_json = tick.last_publish_json.unwrap();
        assert!(pub_json.contains("\"warning\":null"), "{pub_json}");

        let children = r#"{"phase":"urgency","distance_m":67,"code":"142","icon_key":"no_sign_142","source":"children_proximity","label":"Children zone: Vallset"}"#;
        let g = cluster_assemble_guidance_json(
            true,
            Some(60.6811),
            Some(11.3422),
            None,
            Some(30.0),
            None,
            None,
            false,
            None,
            None,
            None,
            None,
            false,
            None,
            None,
            None,
            None,
            false,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            String::new(),
            "{}".into(),
            children.into(),
            camera.into(),
            false,
            None,
        );
        assert!(g.contains("142"));
        assert!(!g.contains("Camera"));
    }
}
