//! Right-to-roam camping: Android HostApi embedder + UniFFI session control.
//!
//! Runs the wasm guest under wasmtime with fuel / epoch / memory limits on a
//! worker thread. Misbehaviour disables the plugin for the session (fail closed).

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use chrono::{Datelike, Local};
use driver_break_core::admin_region_at;
use driver_break_core::config::{SafetyConfig, TravellerProfile};
use driver_break_core::routing::graph::RoutingProfile;
use driver_break_core::routing::indexed::{
    try_load_graph_for_plan_corridor, try_load_poi_barrier_for_plan_bbox,
};
use driver_break_core::routing::safety::OvernightProximityIndex;
use driver_break_core::storage::{ConfigStore, Storage};
use navi_right_to_roam_camping::{
    find_road_track_junctions, probe_along_tracks, CORRIDOR_SEED_RADIUS_M, DEFAULT_TRACK_WALK_M,
    OvernightSafety,
};
use navi_plugin_host::{
    cranelift_abi_supported, plugin_set_enabled, AdminRegionView, CallOutcome, Capability,
    ClockView, FilePluginKv, HostApi, LayerStatus, PluginEnableStore, PluginError,
    PluginHost, PluginKvStatus, PluginLimits, PoiWrite, Position, RouteDestinationView, RouteView,
    SafetyConfigView, TravelModeView, TravellerProfileView, VehicleProfileView, DEFAULT_MEMORY_BYTES,
};
use navi_right_to_roam_camping::on_camping_plugin_enable_changed;

use crate::TravelProfile;

const CAMPING_NAME: &str = "right_to_roam_camping";
const ENABLE_FILE: &str = "plugin_enable.json";
const KV_REL: &str = "plugin_kv/camping_night.json";
const PLUGINS_REL: &str = "plugins";

#[derive(uniffi::Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum CampingCallKind {
    Ok,
    FuelExhausted,
    Timeout,
    MemoryExceeded,
    Trap,
    Disabled,
    Unavailable,
    Error,
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct CampingCallResult {
    pub kind: CampingCallKind,
    pub message: String,
    /// Wall-clock milliseconds spent in the guest call (worker thread).
    pub elapsed_ms: u64,
    /// Result JSON from `rtr_suggest_result` when present.
    pub result_json: Option<String>,
    /// Guest linear-memory size after the last wasmtime call (0 if no guest ran).
    pub peak_guest_memory_bytes: u64,
}

struct Session {
    files_dir: PathBuf,
    data_dir: PathBuf,
    /// IANA timezone id from the Android device (e.g. `Europe/Oslo`).
    timezone: String,
    travel_profile: TravelProfile,
    professional_driver: bool,
    route_waypoints: Vec<[f64; 2]>,
    destination: Option<(f64, f64)>,
    /// Optional Y-M-D override for fire-window / night-store tests.
    clock_override: Option<(i32, u32, u32)>,
    /// Set when a sandbox limit or trap fires; cleared on re-enable.
    session_disable_reason: Option<String>,
    camping_host: Option<PluginHost>,
    /// Corridor job JSON keyed by waypoints + graph path + travel + clock.
    suggest_job_cache: Option<(String, String)>,
}

impl Session {
    fn enable_path(&self) -> PathBuf {
        self.files_dir.join(ENABLE_FILE)
    }

    fn kv_path(&self) -> PathBuf {
        self.files_dir.join(KV_REL)
    }

    fn plugins_root(&self) -> PathBuf {
        self.files_dir.join(PLUGINS_REL)
    }

    fn camping_dir(&self) -> PathBuf {
        self.plugins_root().join(CAMPING_NAME)
    }
}

static SESSION: OnceLock<Mutex<Option<Session>>> = OnceLock::new();

fn session_lock() -> &'static Mutex<Option<Session>> {
    SESSION.get_or_init(|| Mutex::new(None))
}

fn camping_policy() -> HashSet<Capability> {
    Capability::all().iter().copied().collect()
}

fn profile_to_travel_mode(p: TravelProfile) -> TravelModeView {
    match p {
        TravelProfile::Hiking | TravelProfile::Bicycle | TravelProfile::BicycleElectric => {
            TravelModeView::NonMotorised
        }
        TravelProfile::Car
        | TravelProfile::CarElectric
        | TravelProfile::Truck
        | TravelProfile::TruckElectric
        | TravelProfile::MobileHome
        | TravelProfile::Motorcycle
        | TravelProfile::MotorcycleElectric => TravelModeView::Motorised,
    }
}

fn routing_profile_for_travel(p: TravelProfile) -> RoutingProfile {
    if p == TravelProfile::Hiking {
        RoutingProfile::Foot
    } else {
        RoutingProfile::from(p.to_core())
    }
}

fn corridor_bbox_from_waypoints(waypoints: &[[f64; 2]]) -> Option<[f64; 4]> {
    if waypoints.is_empty() {
        return None;
    }
    let mut min_lat = f64::INFINITY;
    let mut max_lat = f64::NEG_INFINITY;
    let mut min_lon = f64::INFINITY;
    let mut max_lon = f64::NEG_INFINITY;
    for w in waypoints {
        min_lat = min_lat.min(w[0]);
        max_lat = max_lat.max(w[0]);
        min_lon = min_lon.min(w[1]);
        max_lon = max_lon.max(w[1]);
    }
    let span = (max_lat - min_lat).max(max_lon - min_lon);
    let pad = (span * 0.25).clamp(0.15, 0.55);
    Some([
        min_lat - pad,
        min_lon - pad,
        max_lat + pad,
        max_lon + pad,
    ])
}

fn find_planning_pbf(data_dir: &Path) -> Option<PathBuf> {
    let mut fallback = None;
    if let Ok(entries) = fs::read_dir(data_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if !name.ends_with(".osm.pbf") {
                continue;
            }
            if name.contains("-latest.osm.pbf") {
                return Some(path);
            }
            if fallback.is_none() {
                fallback = Some(path);
            }
        }
    }
    fallback
}

fn load_overnight_geometry(
    data_dir: &Path,
    pbf: &Path,
    bbox: [f64; 4],
) -> (Vec<(f64, f64)>, Vec<Vec<[f64; 2]>>) {
    let Ok((poi, barriers)) = try_load_poi_barrier_for_plan_bbox(data_dir, pbf, Some(bbox)) else {
        return (Vec::new(), Vec::new());
    };
    let [min_lat, min_lon, max_lat, max_lon] = bbox;
    let buildings: Vec<(f64, f64)> = poi
        .overnight_buildings()
        .iter()
        .copied()
        .filter(|&(lat, lon)| lat >= min_lat && lat <= max_lat && lon >= min_lon && lon <= max_lon)
        .collect();
    let mut prox = OvernightProximityIndex::from_poi_buildings_and_barriers(buildings, &barriers);
    prox.glacier_rings.retain(|ring| {
        ring.iter().any(|p| {
            let (lon, lat) = (p[0], p[1]);
            lat >= min_lat && lat <= max_lat && lon >= min_lon && lon <= max_lon
        })
    });
    (prox.buildings, prox.glacier_rings)
}

fn profile_to_vehicle(p: TravelProfile, professional: bool) -> VehicleProfileView {
    let class = match p {
        TravelProfile::Car
        | TravelProfile::CarElectric
        | TravelProfile::Motorcycle
        | TravelProfile::MotorcycleElectric => "car",
        TravelProfile::MobileHome => "campervan_motorhome",
        TravelProfile::Truck | TravelProfile::TruckElectric => "hgv",
        TravelProfile::Hiking | TravelProfile::Bicycle | TravelProfile::BicycleElectric => {
            "unknown"
        }
    };
    VehicleProfileView {
        class: class.into(),
        gross_weight_kg: None,
        is_professional_driver_under_rest_rules: professional,
    }
}

fn load_safety(data_dir: &Path) -> Option<SafetyConfig> {
    let db = data_dir.join("navi.db");
    let storage = Storage::open(&db).ok()?;
    let store = ConfigStore::new(&storage);
    store.load_safety_config().ok()
}

fn load_traveller(data_dir: &Path) -> TravellerProfile {
    let db = data_dir.join("navi.db");
    let Ok(storage) = Storage::open(&db) else {
        return TravellerProfile::unknown();
    };
    ConfigStore::new(&storage)
        .load_traveller_profile()
        .unwrap_or_else(|_| TravellerProfile::unknown())
}

/// Real Android HostApi — every capability backed by ConfigStore / device / session.
struct AndroidCampingApi {
    data_dir: PathBuf,
    timezone: String,
    travel_profile: TravelProfile,
    professional_driver: bool,
    route_waypoints: Vec<[f64; 2]>,
    destination: Option<(f64, f64)>,
    clock_override: Option<(i32, u32, u32)>,
    kv: FilePluginKv,
    logs: Vec<String>,
}

impl AndroidCampingApi {
    fn open(session: &Session) -> Result<Self, String> {
        let kv = FilePluginKv::open(session.kv_path()).map_err(|e| e.to_string())?;
        Ok(Self {
            data_dir: session.data_dir.clone(),
            timezone: session.timezone.clone(),
            travel_profile: session.travel_profile,
            professional_driver: session.professional_driver,
            route_waypoints: session.route_waypoints.clone(),
            destination: session.destination,
            clock_override: session.clock_override,
            kv,
            logs: Vec::new(),
        })
    }
}

impl HostApi for AndroidCampingApi {
    fn position(&self) -> Option<Position> {
        self.route_waypoints.first().map(|w| Position {
            lat: w[0],
            lon: w[1],
        })
    }

    fn poi_query(&self, _lat: f64, _lon: f64, _radius_m: f64) -> Vec<PoiWrite> {
        Vec::new()
    }

    fn poi_write(&mut self, _poi: PoiWrite) -> Result<(), String> {
        Ok(())
    }

    fn log(&mut self, message: &str) {
        log::info!(target: "NaviCamping", "{message}");
        self.logs.push(message.to_string());
    }

    fn route_read(&self) -> RouteView {
        RouteView {
            waypoints: self.route_waypoints.clone(),
            junctions: Vec::new(),
        }
    }

    fn route_destination_read(&self) -> RouteDestinationView {
        match self.destination {
            Some((lat, lon)) => RouteDestinationView {
                lat: Some(lat),
                lon: Some(lon),
            },
            None => RouteDestinationView::default(),
        }
    }

    fn safety_config_read(&self) -> Option<SafetyConfigView> {
        let s = load_safety(&self.data_dir)?;
        Some(SafetyConfigView {
            min_building_distance_m: s.min_building_distance_m,
            min_glacier_distance_m: Some(s.min_glacier_distance_m),
        })
    }

    fn admin_region_read(&self, lat: f64, lon: f64) -> AdminRegionView {
        let r = admin_region_at(lat, lon);
        AdminRegionView {
            country_iso: r.country_iso,
            subdivision_iso: r.subdivision_iso,
        }
    }

    fn clock_read(&self) -> Option<ClockView> {
        // Always sample the device clock at call time (never a configure-time snapshot).
        let now = Local::now();
        let (year, month, day) = if let Some((y, m, d)) = self.clock_override {
            (y, m, d)
        } else {
            (now.year(), now.month(), now.day())
        };
        Some(ClockView {
            unix_secs: now.timestamp(),
            year,
            month,
            day,
            // Timezone id is refreshed on every suggest/isolation entry (see
            // camping_plugin_set_timezone / run_suggest timezone arg).
            timezone: self.timezone.clone(),
        })
    }

    fn plugin_kv_status(&self) -> PluginKvStatus {
        PluginKvStatus::Available
    }

    fn plugin_kv_get(&self, key: &str) -> Option<String> {
        self.kv.get(key)
    }

    fn plugin_kv_set(&mut self, key: &str, value: &str) -> Result<(), String> {
        self.kv.set(key, value).map_err(|e| e.to_string())
    }

    fn protected_area_query(&self, _lat: f64, _lon: f64) -> navi_plugin_host::ProtectedAreaQueryView {
        navi_plugin_host::ProtectedAreaQueryView {
            status: LayerStatus::Unknown,
            areas: Vec::new(),
        }
    }

    fn land_tenure_query(&self, _lat: f64, _lon: f64) -> navi_plugin_host::LandTenureView {
        navi_plugin_host::LandTenureView::default()
    }

    fn landcover_query(&self, _lat: f64, _lon: f64) -> navi_plugin_host::LandcoverQueryView {
        navi_plugin_host::LandcoverQueryView {
            status: LayerStatus::Unknown,
            class: None,
        }
    }

    fn travel_mode_read(&self) -> TravelModeView {
        profile_to_travel_mode(self.travel_profile)
    }

    fn vehicle_profile_read(&self) -> VehicleProfileView {
        profile_to_vehicle(self.travel_profile, self.professional_driver)
    }

    fn traveller_profile_read(&self) -> TravellerProfileView {
        let t = load_traveller(&self.data_dir);
        TravellerProfileView {
            residency_country: t.residency_country,
        }
    }
}

fn map_outcome(outcome: CallOutcome) -> (CampingCallKind, String) {
    match outcome {
        CallOutcome::Ok => (CampingCallKind::Ok, "ok".into()),
        CallOutcome::FuelExhausted => (
            CampingCallKind::FuelExhausted,
            "Camping plugin disabled for this session: fuel budget exceeded".into(),
        ),
        CallOutcome::Timeout => (
            CampingCallKind::Timeout,
            "Camping plugin disabled for this session: wall-clock timeout".into(),
        ),
        CallOutcome::MemoryExceeded => (
            CampingCallKind::MemoryExceeded,
            "Camping plugin disabled for this session: memory limit exceeded".into(),
        ),
    }
}

fn map_plugin_error(err: PluginError) -> (CampingCallKind, String) {
    match err {
        PluginError::FuelExhausted => map_outcome(CallOutcome::FuelExhausted),
        PluginError::Timeout => map_outcome(CallOutcome::Timeout),
        PluginError::MemoryExceeded => map_outcome(CallOutcome::MemoryExceeded),
        PluginError::Trap(msg) => (
            CampingCallKind::Trap,
            format!("Camping plugin disabled for this session: trap ({msg})"),
        ),
        PluginError::UnsupportedAbi(msg) => (
            CampingCallKind::Unavailable,
            format!("Camping plugin unavailable on this ABI: {msg}"),
        ),
        PluginError::CapabilityDenied(c) => (
            CampingCallKind::Error,
            format!("capability denied: {c}"),
        ),
        PluginError::Other(e) => (CampingCallKind::Error, format!("{e:#}")),
    }
}

fn ensure_camping_loaded(session: &mut Session) -> Result<(), String> {
    if session.camping_host.is_some() {
        return Ok(());
    }
    if !cranelift_abi_supported() {
        return Err(format!(
            "plugin unavailable on ABI {} (fail closed)",
            std::env::consts::ARCH
        ));
    }
    let dir = session.camping_dir();
    if !dir.join("plugin.json").is_file() || !dir.join("plugin.wasm").is_file() {
        return Err(format!(
            "camping plugin not installed under {}",
            dir.display()
        ));
    }
    let host = PluginHost::load_dir(
        &dir,
        &camping_policy(),
        PluginLimits {
            fuel: 500_000_000,
            timeout_ms: 60_000,
            memory_bytes: 32 * 1024 * 1024,
        },
    )
    .map_err(|e| e.to_string())?;
    session.camping_host = Some(host);
    Ok(())
}

/// Configure paths. Safe to call more than once (rebinds session).
#[uniffi::export]
pub fn camping_plugin_configure(files_dir: String, data_dir: String, timezone: String) {
    crate::init_native_logging();
    let mut guard = session_lock().lock().expect("camping session lock");
    *guard = Some(Session {
        files_dir: PathBuf::from(files_dir),
        data_dir: PathBuf::from(data_dir),
        timezone: if timezone.trim().is_empty() {
            "local".into()
        } else {
            timezone
        },
        travel_profile: TravelProfile::Hiking,
        professional_driver: false,
        route_waypoints: Vec::new(),
        destination: None,
        clock_override: None,
        session_disable_reason: None,
        camping_host: None,
        suggest_job_cache: None,
    });
    drop(guard);
    // Natural Earth country polygons are first-use expensive; do not pay that
    // on the suggest path (was ~10s of admin_region_at on Lillehammer).
    let _ = admin_region_at(61.11515, 10.46628);
}

/// Install or replace a plugin directory under `filesDir/plugins/<name>/`.
/// `wasm_bytes` and `manifest_json` are built from source at APK build time
/// (assets) and copied here by the Android host — never committed binaries.
#[uniffi::export]
pub fn camping_plugin_install_guest(
    name: String,
    manifest_json: String,
    wasm_bytes: Vec<u8>,
) -> String {
    let mut guard = session_lock().lock().expect("camping session lock");
    let Some(session) = guard.as_mut() else {
        return "FAIL: not configured".into();
    };
    let dir = session.plugins_root().join(&name);
    if let Err(e) = fs::create_dir_all(&dir) {
        return format!("FAIL: mkdir: {e}");
    }
    if let Err(e) = fs::write(dir.join("plugin.json"), manifest_json.as_bytes()) {
        return format!("FAIL: write manifest: {e}");
    }
    if let Err(e) = fs::write(dir.join("plugin.wasm"), &wasm_bytes) {
        return format!("FAIL: write wasm: {e}");
    }
    if name == CAMPING_NAME {
        session.camping_host = None;
    }
    "OK".into()
}

/// Enable/disable via [`PluginEnableStore`] (default OFF). Disabling deletes the night store.
#[uniffi::export]
pub fn camping_plugin_set_enabled(enabled: bool) -> String {
    let mut guard = session_lock().lock().expect("camping session lock");
    let Some(session) = guard.as_mut() else {
        return "FAIL: not configured".into();
    };
    if let Err(e) = plugin_set_enabled(&session.enable_path(), CAMPING_NAME, enabled) {
        return format!("FAIL: enable store: {e:#}");
    }
    match on_camping_plugin_enable_changed(CAMPING_NAME, enabled, &session.kv_path()) {
        Ok(deleted) => {
            if !enabled {
                session.camping_host = None;
                session.session_disable_reason = None;
                if deleted {
                    "OK: disabled, night store deleted".into()
                } else {
                    "OK: disabled".into()
                }
            } else {
                session.session_disable_reason = None;
                "OK: enabled".into()
            }
        }
        Err(e) => format!("FAIL: night store hook: {e}"),
    }
}

#[uniffi::export]
pub fn camping_plugin_is_enabled() -> bool {
    let guard = session_lock().lock().expect("camping session lock");
    let Some(session) = guard.as_ref() else {
        return false;
    };
    PluginEnableStore::open(session.enable_path())
        .map(|s| s.is_enabled(CAMPING_NAME))
        .unwrap_or(false)
}

#[uniffi::export]
pub fn camping_plugin_session_disabled_reason() -> Option<String> {
    let guard = session_lock().lock().expect("camping session lock");
    guard
        .as_ref()
        .and_then(|s| s.session_disable_reason.clone())
}

/// Push live navigation context (closes production destination-null).
#[uniffi::export]
pub fn camping_plugin_set_nav_context(
    waypoints_json: String,
    dest_lat: Option<f64>,
    dest_lon: Option<f64>,
    profile: TravelProfile,
    professional_driver: bool,
) -> String {
    let mut guard = session_lock().lock().expect("camping session lock");
    let Some(session) = guard.as_mut() else {
        return "FAIL: not configured".into();
    };
    let waypoints: Vec<[f64; 2]> = match serde_json::from_str(&waypoints_json) {
        Ok(v) => v,
        Err(e) => return format!("FAIL: waypoints json: {e}"),
    };
    session.route_waypoints = waypoints;
    session.suggest_job_cache = None;
    session.destination = match (dest_lat, dest_lon) {
        (Some(lat), Some(lon)) => Some((lat, lon)),
        _ => None,
    };
    session.travel_profile = profile;
    session.professional_driver = professional_driver;
    "OK".into()
}

#[uniffi::export]
pub fn camping_plugin_set_clock_ymd(year: i32, month: u32, day: u32) {
    let mut guard = session_lock().lock().expect("camping session lock");
    if let Some(session) = guard.as_mut() {
        session.clock_override = Some((year, month, day));
        session.suggest_job_cache = None;
    }
}

#[uniffi::export]
pub fn camping_plugin_clear_clock_override() {
    let mut guard = session_lock().lock().expect("camping session lock");
    if let Some(session) = guard.as_mut() {
        session.clock_override = None;
        session.suggest_job_cache = None;
    }
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct CampingClockSnapshot {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub timezone: String,
    pub unix_secs: i64,
}

/// Refresh the IANA timezone id used by the next `clock_read` (call before every suggest).
#[uniffi::export]
pub fn camping_plugin_set_timezone(timezone: String) {
    let mut guard = session_lock().lock().expect("camping session lock");
    if let Some(session) = guard.as_mut() {
        session.timezone = if timezone.trim().is_empty() {
            "local".into()
        } else {
            timezone
        };
    }
}

/// Peek the clock HostApi would supply right now (fresh Local date + current timezone id).
#[uniffi::export]
pub fn camping_plugin_peek_clock() -> Option<CampingClockSnapshot> {
    let guard = session_lock().lock().expect("camping session lock");
    let session = guard.as_ref()?;
    let api = AndroidCampingApi::open(session).ok()?;
    let c = api.clock_read()?;
    Some(CampingClockSnapshot {
        year: c.year,
        month: c.month,
        day: c.day,
        timezone: c.timezone,
        unix_secs: c.unix_secs,
    })
}

#[uniffi::export]
pub fn camping_plugin_set_residency_country(iso: Option<String>) -> String {
    let guard = session_lock().lock().expect("camping session lock");
    let Some(session) = guard.as_ref() else {
        return "FAIL: not configured".into();
    };
    let db = session.data_dir.join("navi.db");
    let Ok(storage) = Storage::open(&db) else {
        return "FAIL: open navi.db".into();
    };
    let store = ConfigStore::new(&storage);
    let profile = TravellerProfile::with_residency_country(iso.as_deref());
    match store.save_traveller_profile(&profile) {
        Ok(()) => "OK".into(),
        Err(e) => format!("FAIL: {e}"),
    }
}

/// Production evaluation backend. Always `wasmtime` — native `suggest_overnight`
/// is not called from navi-ffi (test crates only).
#[uniffi::export]
pub fn camping_plugin_evaluation_backend() -> String {
    "wasmtime".into()
}

fn travel_mode_job_str(p: TravelProfile) -> &'static str {
    match profile_to_travel_mode(p) {
        TravelModeView::Motorised => "motorised",
        TravelModeView::NonMotorised => "non_motorised",
        TravelModeView::Unknown => "unknown",
    }
}

fn vehicle_class_job_str(p: TravelProfile) -> &'static str {
    match p {
        TravelProfile::Car
        | TravelProfile::CarElectric
        | TravelProfile::Motorcycle
        | TravelProfile::MotorcycleElectric => "car",
        TravelProfile::MobileHome => "campervan_motorhome",
        TravelProfile::Truck | TravelProfile::TruckElectric => "hgv",
        TravelProfile::Hiking | TravelProfile::Bicycle | TravelProfile::BicycleElectric => {
            "unknown"
        }
    }
}

fn buildings_near_probes(
    buildings: &[(f64, f64)],
    probes: &[(f64, f64)],
    pad_deg: f64,
) -> Vec<[f64; 2]> {
    if probes.is_empty() {
        return Vec::new();
    }
    buildings
        .iter()
        .copied()
        .filter(|&(lat, lon)| {
            probes.iter().any(|&(plat, plon)| {
                (lat - plat).abs() <= pad_deg && (lon - plon).abs() <= pad_deg
            })
        })
        .map(|(lat, lon)| [lat, lon])
        .collect()
}

fn merge_guest_meta(
    result_json: Option<String>,
    timing: serde_json::Value,
    peak_guest_memory_bytes: u64,
) -> Option<String> {
    let mut v = result_json
        .as_deref()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    if let Some(obj) = v.as_object_mut() {
        obj.insert("via".into(), serde_json::Value::String("wasmtime".into()));
        obj.insert("timing_ms".into(), timing);
        obj.insert(
            "peak_guest_memory_bytes".into(),
            serde_json::Value::from(peak_guest_memory_bytes),
        );
    }
    Some(v.to_string())
}

/// Write suggest job, invoke guest, return result JSON.
/// `timezone` is the device IANA id at call time (must not be a configure-time cache).
#[uniffi::export]
pub fn camping_plugin_run_suggest(job_json: String, timezone: String) -> CampingCallResult {
    camping_plugin_set_timezone(timezone);
    run_camping_guest(Some(job_json))
}

/// Discover overnight spots along the live nav corridor.
/// Host: graph load, road∩track junctions, probe walk, nearby buildings.
/// Guest: every rule, filter, and card decision via wasmtime.
#[uniffi::export]
pub fn camping_plugin_suggest_along_route(max_suggestions: u32) -> CampingCallResult {
    let started = Instant::now();
    let guard = session_lock().lock().expect("camping session lock");
    let Some(session) = guard.as_ref() else {
        return CampingCallResult {
            kind: CampingCallKind::Error,
            message: "not configured".into(),
            elapsed_ms: 0,
            result_json: None,
            peak_guest_memory_bytes: 0,
        };
    };
    if let Some(reason) = &session.session_disable_reason {
        return CampingCallResult {
            kind: CampingCallKind::Disabled,
            message: reason.clone(),
            elapsed_ms: 0,
            result_json: None,
            peak_guest_memory_bytes: 0,
        };
    }
    if !PluginEnableStore::open(session.enable_path())
        .map(|s| s.is_enabled(CAMPING_NAME))
        .unwrap_or(false)
    {
        return CampingCallResult {
            kind: CampingCallKind::Disabled,
            message: "camping plugin is disabled (default OFF)".into(),
            elapsed_ms: 0,
            result_json: None,
            peak_guest_memory_bytes: 0,
        };
    }
    if session.route_waypoints.is_empty() {
        return CampingCallResult {
            kind: CampingCallKind::Error,
            message: "route_waypoints empty — call camping_plugin_set_nav_context first".into(),
            elapsed_ms: 0,
            result_json: None,
            peak_guest_memory_bytes: 0,
        };
    }
    let data_dir = session.data_dir.clone();
    let waypoints = session.route_waypoints.clone();
    let travel_profile = session.travel_profile;
    let professional_driver = session.professional_driver;
    let clock_key = session.clock_override;
    drop(guard);

    let Some(bbox) = corridor_bbox_from_waypoints(&waypoints) else {
        return CampingCallResult {
            kind: CampingCallKind::Error,
            message: "could not derive corridor bbox".into(),
            elapsed_ms: started.elapsed().as_millis() as u64,
            result_json: None,
            peak_guest_memory_bytes: 0,
        };
    };
    let Some(pbf) = find_planning_pbf(&data_dir) else {
        return CampingCallResult {
            kind: CampingCallKind::Unavailable,
            message: format!(
                "no .osm.pbf under {} for overnight corridor graph",
                data_dir.display()
            ),
            elapsed_ms: started.elapsed().as_millis() as u64,
            result_json: None,
            peak_guest_memory_bytes: 0,
        };
    };
    let cache_key = format!(
        "{:?}|{}|{:?}|{}|{}|{max_suggestions}",
        waypoints,
        pbf.display(),
        travel_profile,
        professional_driver,
        clock_key
            .map(|(y, m, d)| format!("{y:04}-{m:02}-{d:02}"))
            .unwrap_or_default()
    );
    let cached_job = {
        let guard = session_lock().lock().expect("camping session lock");
        guard
            .as_ref()
            .and_then(|s| s.suggest_job_cache.clone())
            .and_then(|(k, job)| if k == cache_key { Some(job) } else { None })
    };
    if let Some(job_json) = cached_job {
        let job_bytes = job_json.len();
        let t_guest = Instant::now();
        let mut call = run_camping_guest(Some(job_json));
        let guest_ms = t_guest.elapsed().as_millis() as u64;
        let timing = serde_json::json!({
            "graph": 0,
            "junctions": 0,
            "probe_walk": 0,
            "buildings": 0,
            "admin": 0,
            "serialize": 0,
            "guest": guest_ms,
            "cache_hit": true,
            "total": started.elapsed().as_millis() as u64,
        });
        log::info!(
            target: "NaviCamping",
            "suggest_along_route via=wasmtime cache_hit job_bytes={job_bytes} timing={timing}"
        );
        call.result_json = merge_guest_meta(
            call.result_json,
            timing,
            call.peak_guest_memory_bytes,
        );
        call.elapsed_ms = started.elapsed().as_millis() as u64;
        return call;
    }

    let routing_profile = routing_profile_for_travel(travel_profile);
    let route_points: Vec<(f64, f64)> = waypoints.iter().map(|w| (w[0], w[1])).collect();
    let t_graph = Instant::now();
    let graph = match try_load_graph_for_plan_corridor(
        &data_dir,
        &pbf,
        routing_profile,
        Some(bbox),
        Some(&route_points),
    ) {
        Ok(g) => g,
        Err(e) => {
            return CampingCallResult {
                kind: CampingCallKind::Unavailable,
                message: format!("corridor graph load failed: {e:?}"),
                elapsed_ms: started.elapsed().as_millis() as u64,
                result_json: None,
                peak_guest_memory_bytes: 0,
            };
        }
    };
    let graph_ms = t_graph.elapsed().as_millis() as u64;

    let t_junc = Instant::now();
    let mut seeds = find_road_track_junctions(&graph, &waypoints, CORRIDOR_SEED_RADIUS_M);
    seeds.truncate(80);
    let junction_ms = t_junc.elapsed().as_millis() as u64;

    let t_walk = Instant::now();
    let walked = probe_along_tracks(&graph, &seeds, DEFAULT_TRACK_WALK_M, None);
    let mut probes: Vec<(f64, f64)> = walked.iter().map(|p| (p.lat, p.lon)).collect();
    if probes.is_empty() {
        probes = seeds.iter().map(|s| (s.lat, s.lon)).collect();
    }
    let probe_walk_ms = t_walk.elapsed().as_millis() as u64;

    let t_bldg = Instant::now();
    let (all_buildings, glacier_rings) = load_overnight_geometry(&data_dir, &pbf, bbox);
    let buildings = buildings_near_probes(&all_buildings, &probes, 0.008);
    let buildings_ms = t_bldg.elapsed().as_millis() as u64;

    let t_admin = Instant::now();
    let mut countries: Vec<(f64, f64, String)> = Vec::with_capacity(probes.len());
    let mut subdivisions: Vec<(f64, f64, String)> = Vec::new();
    for &(lat, lon) in &probes {
        let ar = admin_region_at(lat, lon);
        countries.push((
            lat,
            lon,
            ar.country_iso.unwrap_or_else(|| "unknown".into()),
        ));
        if let Some(iso) = ar.subdivision_iso {
            subdivisions.push((lat, lon, iso));
        }
    }
    let admin_ms = t_admin.elapsed().as_millis() as u64;

    let safety: OvernightSafety = (&load_safety(&data_dir).unwrap_or_default()).into();
    let max = if max_suggestions == 0 {
        None
    } else {
        Some(max_suggestions as usize)
    };

    let t_ser = Instant::now();
    let job = serde_json::json!({
        "probes": probes.iter().map(|&(la, lo)| [la, lo]).collect::<Vec<_>>(),
        "max_suggestions": max,
        "buildings": buildings,
        "glaciers": glacier_rings,
        "safety": safety,
        "clock": serde_json::Value::Null,
        "kv_ok": true,
        "countries": countries,
        "subdivisions": subdivisions,
        "travel_mode": travel_mode_job_str(travel_profile),
        "vehicle_class": vehicle_class_job_str(travel_profile),
        "is_professional_driver_under_rest_rules": professional_driver,
    });
    let job_json = job.to_string();
    let job_bytes = job_json.len();
    let serialize_ms = t_ser.elapsed().as_millis() as u64;
    if let Some(session) = session_lock()
        .lock()
        .expect("camping session lock")
        .as_mut()
    {
        session.suggest_job_cache = Some((cache_key, job_json.clone()));
    }

    let t_guest = Instant::now();
    let mut call = run_camping_guest(Some(job_json));
    let guest_ms = t_guest.elapsed().as_millis() as u64;
    let timing = serde_json::json!({
        "graph": graph_ms,
        "junctions": junction_ms,
        "probe_walk": probe_walk_ms,
        "buildings": buildings_ms,
        "admin": admin_ms,
        "serialize": serialize_ms,
        "guest": guest_ms,
        "cache_hit": false,
        "total": started.elapsed().as_millis() as u64,
    });
    log::info!(
        target: "NaviCamping",
        "suggest_along_route via=wasmtime job_bytes={job_bytes} probes={} buildings={} timing={timing}",
        probes.len(),
        buildings.len()
    );
    call.result_json = merge_guest_meta(
        call.result_json,
        timing,
        call.peak_guest_memory_bytes,
    );
    call.elapsed_ms = started.elapsed().as_millis() as u64;
    call
}


fn run_camping_guest(job_json: Option<String>) -> CampingCallResult {
    let mut guard = session_lock().lock().expect("camping session lock");
    let Some(session) = guard.as_mut() else {
        return CampingCallResult {
            kind: CampingCallKind::Error,
            message: "not configured".into(),
            elapsed_ms: 0,
            result_json: None,
            peak_guest_memory_bytes: 0,
        };
    };
    if let Some(reason) = &session.session_disable_reason {
        return CampingCallResult {
            kind: CampingCallKind::Disabled,
            message: reason.clone(),
            elapsed_ms: 0,
            result_json: None,
            peak_guest_memory_bytes: 0,
        };
    }
    if !PluginEnableStore::open(session.enable_path())
        .map(|s| s.is_enabled(CAMPING_NAME))
        .unwrap_or(false)
    {
        return CampingCallResult {
            kind: CampingCallKind::Disabled,
            message: "camping plugin is disabled (default OFF)".into(),
            elapsed_ms: 0,
            result_json: None,
            peak_guest_memory_bytes: 0,
        };
    }
    if let Err(e) = ensure_camping_loaded(session) {
        return CampingCallResult {
            kind: CampingCallKind::Unavailable,
            message: e,
            elapsed_ms: 0,
            result_json: None,
            peak_guest_memory_bytes: 0,
        };
    }

    let api = match AndroidCampingApi::open(session) {
        Ok(mut api) => {
            if let Some(job) = job_json {
                if let Err(e) = api.plugin_kv_set("rtr_suggest_job", &job) {
                    return CampingCallResult {
                        kind: CampingCallKind::Error,
                        message: e,
                        elapsed_ms: 0,
                        result_json: None,
                        peak_guest_memory_bytes: 0,
                    };
                }
            }
            api
        }
        Err(e) => {
            return CampingCallResult {
                kind: CampingCallKind::Error,
                message: e,
                elapsed_ms: 0,
                result_json: None,
                peak_guest_memory_bytes: 0,
            };
        }
    };

    // Take host out so we can move a clone of Engine state via PluginHost::call on a thread.
    // PluginHost is not Clone; call inline under the lock is unsafe for UI — release lock.
    let host = session.camping_host.take().expect("loaded above");
    let files_dir = session.files_dir.clone();
    let data_dir = session.data_dir.clone();
    let timezone = session.timezone.clone();
    let travel_profile = session.travel_profile;
    let professional_driver = session.professional_driver;
    let route_waypoints = session.route_waypoints.clone();
    let destination = session.destination;
    let clock_override = session.clock_override;
    drop(guard);

    let started = Instant::now();
    // Kotlin callers must use Dispatchers.Default / a background thread; the
    // guest itself is killed by fuel / epoch / memory if it misbehaves.
    let (outcome, stats) = match host.call_with_stats(Box::new(api)) {
        Ok(pair) => pair,
        Err(err) => {
            let elapsed_ms = started.elapsed().as_millis() as u64;
            let mut guard = session_lock().lock().expect("camping session lock");
            let Some(session) = guard.as_mut() else {
                let (kind, message) = map_plugin_error(err);
                return CampingCallResult {
                    kind,
                    message,
                    elapsed_ms,
                    result_json: None,
                    peak_guest_memory_bytes: 0,
                };
            };
            session.camping_host = Some(host);
            let (kind, message) = map_plugin_error(err);
            if matches!(
                kind,
                CampingCallKind::Trap
                    | CampingCallKind::FuelExhausted
                    | CampingCallKind::Timeout
                    | CampingCallKind::MemoryExceeded
            ) {
                session.session_disable_reason = Some(message.clone());
                session.camping_host = None;
            }
            return CampingCallResult {
                kind,
                message,
                elapsed_ms,
                result_json: None,
                peak_guest_memory_bytes: 0,
            };
        }
    };
    let elapsed_ms = started.elapsed().as_millis() as u64;
    let peak_mem = stats.memory_bytes;
    let result_json = FilePluginKv::open(files_dir.join(KV_REL))
        .ok()
        .and_then(|kv| kv.get("rtr_suggest_result"));
    let _ = (
        data_dir,
        timezone,
        travel_profile,
        professional_driver,
        route_waypoints,
        destination,
        clock_override,
    );

    let mut guard = session_lock().lock().expect("camping session lock");
    let Some(session) = guard.as_mut() else {
        return CampingCallResult {
            kind: CampingCallKind::Error,
            message: "session cleared during call".into(),
            elapsed_ms,
            result_json: None,
            peak_guest_memory_bytes: peak_mem,
        };
    };
    session.camping_host = Some(host);

    match outcome {
        CallOutcome::Ok => {
            let json = merge_guest_meta(
                result_json,
                serde_json::json!({ "guest": elapsed_ms }),
                peak_mem,
            );
            CampingCallResult {
                kind: CampingCallKind::Ok,
                message: "ok".into(),
                elapsed_ms,
                result_json: json,
                peak_guest_memory_bytes: peak_mem,
            }
        }
        other => {
            let (kind, message) = map_outcome(other);
            session.session_disable_reason = Some(message.clone());
            session.camping_host = None;
            CampingCallResult {
                kind,
                message,
                elapsed_ms,
                result_json: None,
                peak_guest_memory_bytes: peak_mem,
            }
        }
    }
}

/// Run a staged isolation guest (`busy_loop`, `trap_guest`, `memory_bomb`) under
/// the same sandbox. Used by instrumented emulator tests. Never crashes the app.
#[uniffi::export]
pub fn camping_plugin_run_isolation_guest(name: String) -> CampingCallResult {
    let mut guard = session_lock().lock().expect("camping session lock");
    let Some(session) = guard.as_mut() else {
        return CampingCallResult {
            kind: CampingCallKind::Error,
            message: "not configured".into(),
            elapsed_ms: 0,
            result_json: None,
            peak_guest_memory_bytes: 0,
        };
    };
    if !cranelift_abi_supported() {
        return CampingCallResult {
            kind: CampingCallKind::Unavailable,
            message: format!(
                "plugin unavailable on ABI {} (fail closed)",
                std::env::consts::ARCH
            ),
            elapsed_ms: 0,
            result_json: None,
            peak_guest_memory_bytes: 0,
        };
    }
    let dir = session.plugins_root().join(&name);
    let limits = match name.as_str() {
        "busy_loop" => PluginLimits {
            fuel: 50_000,
            timeout_ms: 80,
            memory_bytes: DEFAULT_MEMORY_BYTES,
        },
        "memory_bomb" => PluginLimits {
            fuel: 50_000_000,
            timeout_ms: 2_000,
            memory_bytes: 1 * 1024 * 1024,
        },
        _ => PluginLimits::default(),
    };
    let host = match PluginHost::load_dir(&dir, &camping_policy(), limits) {
        Ok(h) => h,
        Err(e) => {
            let (kind, message) = map_plugin_error(e);
            return CampingCallResult {
                kind,
                message,
                elapsed_ms: 0,
                result_json: None,
                peak_guest_memory_bytes: 0,
            };
        }
    };
    let api = match AndroidCampingApi::open(session) {
        Ok(a) => a,
        Err(e) => {
            return CampingCallResult {
                kind: CampingCallKind::Error,
                message: e,
                elapsed_ms: 0,
                result_json: None,
                peak_guest_memory_bytes: 0,
            };
        }
    };
    drop(guard);

    let started = Instant::now();
    let (outcome, stats) = match host.call_with_stats(Box::new(api)) {
        Ok(pair) => pair,
        Err(e) => {
            let elapsed_ms = started.elapsed().as_millis() as u64;
            let (kind, message) = map_plugin_error(e);
            return CampingCallResult {
                kind,
                message,
                elapsed_ms,
                result_json: None,
                peak_guest_memory_bytes: 0,
            };
        }
    };
    let elapsed_ms = started.elapsed().as_millis() as u64;
    let (kind, message) = map_outcome(outcome);
    CampingCallResult {
        kind,
        message,
        elapsed_ms,
        result_json: None,
        peak_guest_memory_bytes: stats.memory_bytes,
    }
}

/// Capability source map for diagnostics / Phase 5a report verification.
#[uniffi::export]
pub fn camping_plugin_capability_sources_json() -> String {
    serde_json::json!({
        "safety_config_read": "ConfigStore via data_dir/navi.db",
        "clock_read": "device Local::now() Y-M-D at every guest call + IANA timezone refreshed via camping_plugin_set_timezone / run_suggest",
        "plugin_kv": "filesDir/plugin_kv/camping_night.json (FilePluginKv)",
        "admin_region_read": "driver_break_core::admin_region_at",
        "travel_mode_read": "active TravelProfile from camping_plugin_set_nav_context",
        "vehicle_profile_read": "TravelProfile class + professional_driver flag",
        "traveller_profile_read": "ConfigStore traveller_profile (residency)",
        "route_read": "live nav waypoints from camping_plugin_set_nav_context",
        "route_destination_read": "live nav destination from camping_plugin_set_nav_context",
        "protected_area_query": "LayerStatus::Unknown until ingest lands",
        "land_tenure_query": "unknown until PAD-US / Crown ingest",
        "landcover_query": "LayerStatus::Unknown until ingest lands",
        "enable": "PluginEnableStore at filesDir/plugin_enable.json (default OFF)",
        "weather_datex": "untouched (MapHudPrefs)",
    })
    .to_string()
}
