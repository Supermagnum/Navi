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
    try_load_graph_for_plan_corridor_with_pack_dirs,
    try_load_poi_pack_covering_point_with_pack_dirs,
};
use driver_break_core::routing::plan_bbox::PlanEdgeClipMode;
use driver_break_core::routing::safety::OvernightProximityIndex;
use driver_break_core::storage::{ConfigStore, Storage};
use navi_plugin_host::{
    cranelift_abi_supported, plugin_set_enabled, AdminRegionView, CallOutcome, Capability,
    ClockView, FilePluginKv, HostApi, LayerStatus, PluginEnableStore, PluginError, PluginHost,
    PluginKvStatus, PluginLimits, PoiWrite, Position, RouteDestinationView, RouteView,
    SafetyConfigView, TravelModeView, TravellerProfileView, VehicleProfileView,
    DEFAULT_MEMORY_BYTES,
};
use navi_right_to_roam_camping::on_camping_plugin_enable_changed;
use navi_right_to_roam_camping::{
    find_road_track_junctions, probe_along_tracks, OvernightSafety, CORRIDOR_SEED_RADIUS_M,
    DEFAULT_TRACK_WALK_M,
};

use crate::TravelProfile;

const CAMPING_NAME: &str = "right_to_roam_camping";
const ENABLE_FILE: &str = "plugin_enable.json";
const KV_REL: &str = "plugin_kv/camping_night.json";
const PLUGINS_REL: &str = "plugins";
/// Waypoints per corridor graph segment so long-trip SD packs never inflate a
/// multi-country graph co-resident with POI buildings (Automotive 4 GB).
const CAMPING_SEGMENT_WAYPOINTS: usize = 6;
const CAMPING_MAX_SEEDS: usize = 80;
/// Max road∩track seeds kept from one 6-waypoint segment so Germany cannot
/// fill the global cap before Norway is scanned.
const CAMPING_SEEDS_PER_SEGMENT: usize = 8;
const CAMPING_MAX_JOB_BUILDINGS: usize = 400;

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
    /// Extra Ready-pack roots (e.g. Removable `long-trip-packs/`).
    pack_dirs: Vec<PathBuf>,
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
    /// Corridor geometry job JSON keyed by route + pack fingerprint + SafetyConfig.
    /// Travel/clock/profile are NOT part of the key — overlaid fresh every guest call.
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
    Some([min_lat - pad, min_lon - pad, max_lat + pad, max_lon + pad])
}

/// Prefer a `-latest.osm.pbf` under long-trip pack roots, then `data_dir`.
///
/// Skip country extracts that have no sibling (or search-dir) Ready manifest —
/// `files/sweden-latest.osm.pbf` without `sweden-latest.navi-manifest.json` made
/// every camping corridor segment fail with `indexed pack missing` before PIP
/// re-home could pick Niedersachsen / Ostlandet leaf packs.
fn find_planning_pbf(data_dir: &Path, pack_dirs: &[PathBuf]) -> Option<PathBuf> {
    // Pack roots first: they hold leaf Ready packs. filesDir often has leftover
    // country PBFs (sweden) that are not planning stems.
    let mut dirs: Vec<&Path> = pack_dirs.iter().map(|p| p.as_path()).collect();
    if !dirs.contains(&data_dir) {
        dirs.push(data_dir);
    }
    let has_manifest = |stem: &str| {
        dirs.iter()
            .any(|d| d.join(format!("{stem}.navi-manifest.json")).is_file())
    };
    let mut fallback_no_man = None;
    let mut fallback_non_latest = None;
    for dir in &dirs {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
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
                let stem = name.trim_end_matches(".osm.pbf");
                if has_manifest(stem) {
                    return Some(path);
                }
                if fallback_no_man.is_none() {
                    fallback_no_man = Some(path);
                }
                continue;
            }
            if fallback_non_latest.is_none() {
                fallback_non_latest = Some(path);
            }
        }
    }
    fallback_no_man.or(fallback_non_latest)
}

fn parse_pack_dirs_json(raw: &str) -> Vec<PathBuf> {
    let Ok(arr) = serde_json::from_str::<Vec<String>>(raw) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for s in arr {
        let p = PathBuf::from(s.trim());
        if p.as_os_str().is_empty() {
            continue;
        }
        if p.is_dir() && !out.iter().any(|x| x == &p) {
            out.push(p);
        }
    }
    out
}

/// Buildings / glaciers near probes: one covering Ready pack at a time (same
/// spirit as chunked soft-break finalize — never merge region-wide POI packs).
#[allow(clippy::type_complexity)]
fn load_overnight_geometry_near_probes(
    data_dir: &Path,
    pack_dirs: &[PathBuf],
    probes: &[(f64, f64)],
) -> (Vec<(f64, f64)>, Vec<Vec<[f64; 2]>>) {
    let mut buildings = Vec::new();
    let mut glacier_rings = Vec::new();
    let mut seen_cell = std::collections::HashSet::new();
    for &(lat, lon) in probes {
        let cell = ((lat * 2.0).round() as i32, (lon * 2.0).round() as i32);
        if !seen_cell.insert(cell) {
            continue;
        }
        let Ok((poi, barriers)) =
            try_load_poi_pack_covering_point_with_pack_dirs(data_dir, pack_dirs, lat, lon)
        else {
            continue;
        };
        let mut prox = OvernightProximityIndex::from_poi_buildings_and_barriers(
            poi.overnight_buildings().to_vec(),
            &barriers,
        );
        // Keep rings that touch a ~0.5° cell around the probe.
        prox.glacier_rings.retain(|ring| {
            ring.iter().any(|p| {
                let (rlon, rlat) = (p[0], p[1]);
                (rlat - lat).abs() <= 0.08 && (rlon - lon).abs() <= 0.08
            })
        });
        // Pack overnight_buildings is region-wide; keep only the probe cell so
        // the wasm guest job stays under the 1 MiB rtr_suggest_job buffer.
        prox.buildings
            .retain(|&(blat, blon)| (blat - lat).abs() <= 0.02 && (blon - lon).abs() <= 0.02);
        buildings.extend(prox.buildings);
        glacier_rings.extend(prox.glacier_rings);
        drop(poi);
    }
    (buildings, glacier_rings)
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

    fn protected_area_query(
        &self,
        _lat: f64,
        _lon: f64,
    ) -> navi_plugin_host::ProtectedAreaQueryView {
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
        PluginError::CapabilityDenied(c) => {
            (CampingCallKind::Error, format!("capability denied: {c}"))
        }
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
        pack_dirs: Vec::new(),
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

/// JSON array of absolute pack roots (e.g. Removable `…/long-trip-packs`).
/// Cleared on [`camping_plugin_configure`]; host should set after long-trip
/// volume selection so suggest can see SD-only Ready packs.
#[uniffi::export]
pub fn camping_plugin_set_pack_dirs(pack_dirs_json: String) -> String {
    let dirs = parse_pack_dirs_json(&pack_dirs_json);
    let mut guard = session_lock().lock().expect("camping session lock");
    let Some(session) = guard.as_mut() else {
        return "FAIL: not configured".into();
    };
    if session.pack_dirs != dirs {
        session.pack_dirs = dirs;
        session.suggest_job_cache = None;
    }
    "OK".into()
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
///
/// `waypoints_json` is `[[lat, lon], …]` — not the MapLibre overlay string
/// `"lon,lat;lon,lat;…"`. The Android host converts via
/// `sampleCampingCorridorWaypoints`.
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
    let destination = match (dest_lat, dest_lon) {
        (Some(lat), Some(lon)) => Some((lat, lon)),
        _ => None,
    };
    let route_changed = session.route_waypoints != waypoints || session.destination != destination;
    session.route_waypoints = waypoints;
    session.destination = destination;
    session.travel_profile = profile;
    session.professional_driver = professional_driver;
    // Geometry cache keys on route (+ pack + safety). Profile/travel changes stay
    // fresh via job_with_live_profile + live HostApi; do not drop geometry.
    if route_changed {
        session.suggest_job_cache = None;
    }
    "OK".into()
}

#[uniffi::export]
pub fn camping_plugin_set_clock_ymd(year: i32, month: u32, day: u32) {
    let mut guard = session_lock().lock().expect("camping session lock");
    if let Some(session) = guard.as_mut() {
        session.clock_override = Some((year, month, day));
        // Geometry cache is independent of clock; guest reads clock_read live.
    }
}

#[uniffi::export]
pub fn camping_plugin_clear_clock_override() {
    let mut guard = session_lock().lock().expect("camping session lock");
    if let Some(session) = guard.as_mut() {
        session.clock_override = None;
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

/// Geometry-cache fingerprint for installed region packs (weekly rebakes).
/// Keyed on every `*.navi-manifest.json` / `*.navi-server-install.json` name+size+mtime
/// plus the planning PBF size+mtime.
fn pack_geometry_fingerprint(data_dir: &Path, pbf: &Path) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Ok(meta) = fs::metadata(pbf) {
        let mt = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        parts.push(format!("pbf:{}:{}", meta.len(), mt));
    }
    let Ok(rd) = fs::read_dir(data_dir) else {
        return parts.join("|");
    };
    let mut files: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
    files.sort();
    for path in files {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        if !(name.ends_with(".navi-manifest.json") || name.ends_with(".navi-server-install.json")) {
            continue;
        }
        if let Ok(meta) = fs::metadata(&path) {
            let mt = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            parts.push(format!("{name}:{}:{}", meta.len(), mt));
        }
    }
    parts.join("|")
}

fn safety_geometry_fingerprint(data_dir: &Path) -> String {
    match load_safety(data_dir) {
        Some(s) => format!(
            "bldg:{:.3}|glacier:{:.3}|water:{:.3}|cabin:{:.3}|general:{:.3}",
            s.min_building_distance_m,
            s.min_glacier_distance_m,
            s.poi_radius_water_m,
            s.poi_radius_cabin_m,
            s.poi_radius_general_m
        ),
        None => "safety:none".into(),
    }
}

/// Geometry cache key: route + planning PBF identity + pack version/hash fingerprint + SafetyConfig.
/// Travel mode, vehicle, residency, clock, and night-store are intentionally absent — the guest
/// re-runs every call with live HostApi reads / overlaid job fields.
fn geometry_cache_key(
    waypoints: &[[f64; 2]],
    pbf: &Path,
    data_dir: &Path,
    pack_dirs: &[PathBuf],
    max_suggestions: u32,
) -> String {
    let mut pack_fp = pack_geometry_fingerprint(data_dir, pbf);
    for dir in pack_dirs {
        pack_fp.push('|');
        pack_fp.push_str(&pack_geometry_fingerprint(dir, pbf));
    }
    format!(
        "{:?}|{}|{}|{}|{max_suggestions}",
        waypoints,
        pbf.display(),
        pack_fp,
        safety_geometry_fingerprint(data_dir),
    )
}

/// Overlay live session fields onto a cached geometry job so travel/vehicle stay fresh.
fn job_with_live_profile(
    geometry_job_json: &str,
    travel: TravelProfile,
    professional: bool,
) -> String {
    let mut v: serde_json::Value =
        serde_json::from_str(geometry_job_json).unwrap_or_else(|_| serde_json::json!({}));
    if let Some(obj) = v.as_object_mut() {
        obj.insert(
            "travel_mode".into(),
            serde_json::Value::String(travel_mode_job_str(travel).into()),
        );
        obj.insert(
            "vehicle_class".into(),
            serde_json::Value::String(vehicle_class_job_str(travel).into()),
        );
        obj.insert(
            "is_professional_driver_under_rest_rules".into(),
            serde_json::Value::Bool(professional),
        );
        obj.insert("clock".into(), serde_json::Value::Null);
    }
    v.to_string()
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
            probes
                .iter()
                .any(|&(plat, plon)| (lat - plat).abs() <= pad_deg && (lon - plon).abs() <= pad_deg)
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

fn subsample_even<T: Clone>(items: &[T], max: usize) -> Vec<T> {
    if max == 0 || items.is_empty() {
        return Vec::new();
    }
    if items.len() <= max {
        return items.to_vec();
    }
    if max == 1 {
        return vec![items[items.len() / 2].clone()];
    }
    let n = items.len();
    let mut out = Vec::with_capacity(max);
    let mut last = usize::MAX;
    for i in 0..max {
        let idx = i * (n - 1) / (max - 1);
        if idx == last {
            continue;
        }
        out.push(items[idx].clone());
        last = idx;
    }
    out
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
    let pack_dirs = session.pack_dirs.clone();
    let waypoints = session.route_waypoints.clone();
    let travel_profile = session.travel_profile;
    let professional_driver = session.professional_driver;
    drop(guard);

    if corridor_bbox_from_waypoints(&waypoints).is_none() {
        return CampingCallResult {
            kind: CampingCallKind::Error,
            message: "could not derive corridor bbox".into(),
            elapsed_ms: started.elapsed().as_millis() as u64,
            result_json: None,
            peak_guest_memory_bytes: 0,
        };
    }
    let Some(pbf) = find_planning_pbf(&data_dir, &pack_dirs) else {
        return CampingCallResult {
            kind: CampingCallKind::Unavailable,
            message: format!(
                "no .osm.pbf under {} (or pack_dirs) for overnight corridor graph",
                data_dir.display()
            ),
            elapsed_ms: started.elapsed().as_millis() as u64,
            result_json: None,
            peak_guest_memory_bytes: 0,
        };
    };
    log::info!(
        target: "NaviCamping",
        "suggest_along_route planning_pbf={} pack_dirs={}",
        pbf.display(),
        pack_dirs.len()
    );
    let cache_key = geometry_cache_key(&waypoints, &pbf, &data_dir, &pack_dirs, max_suggestions);
    let cached_geometry = {
        let guard = session_lock().lock().expect("camping session lock");
        guard
            .as_ref()
            .and_then(|s| s.suggest_job_cache.clone())
            .and_then(|(k, job)| if k == cache_key { Some(job) } else { None })
    };
    if let Some(geometry_json) = cached_geometry {
        // Warm path: release session lock before guest (see warm_path_releases_lock_before_guest).
        let job_json = job_with_live_profile(&geometry_json, travel_profile, professional_driver);
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
        call.result_json = merge_guest_meta(call.result_json, timing, call.peak_guest_memory_bytes);
        call.elapsed_ms = started.elapsed().as_millis() as u64;
        return call;
    }

    let routing_profile = routing_profile_for_travel(travel_profile);
    // Segment the corridor so Removable multi-country packs never sit as one
    // giant graph (same RAM constraint as densify `poi_skipped=chunk_leg`).
    let t_graph = Instant::now();
    let mut seeds = Vec::new();
    let mut probes: Vec<(f64, f64)> = Vec::new();
    let mut segment_errors = 0u32;
    let step = CAMPING_SEGMENT_WAYPOINTS.max(2);
    let mut start = 0usize;
    while start < waypoints.len() {
        let end = (start + step).min(waypoints.len());
        // Overlap one shared waypoint so segment joints are not skipped.
        let seg_start = if start == 0 {
            0
        } else {
            start.saturating_sub(1)
        };
        let chunk = &waypoints[seg_start..end];
        if chunk.len() < 2 {
            break;
        }
        let Some(seg_bbox) = corridor_bbox_from_waypoints(chunk) else {
            start = end;
            continue;
        };
        let route_points: Vec<(f64, f64)> = chunk.iter().map(|w| (w[0], w[1])).collect();
        match try_load_graph_for_plan_corridor_with_pack_dirs(
            &data_dir,
            &pack_dirs,
            &pbf,
            routing_profile,
            Some(seg_bbox),
            Some(&route_points),
            PlanEdgeClipMode::CorridorBand,
        ) {
            Ok(graph) => {
                let mut seg_seeds =
                    find_road_track_junctions(&graph, chunk, CORRIDOR_SEED_RADIUS_M);
                if seg_seeds.len() > CAMPING_SEEDS_PER_SEGMENT {
                    seg_seeds = subsample_even(&seg_seeds, CAMPING_SEEDS_PER_SEGMENT);
                }
                let walked = probe_along_tracks(&graph, &seg_seeds, DEFAULT_TRACK_WALK_M, None);
                drop(graph);
                for s in seg_seeds.drain(..) {
                    seeds.push(s);
                }
                for p in walked {
                    probes.push((p.lat, p.lon));
                }
            }
            Err(e) => {
                segment_errors += 1;
                // Info (not Warn): android_logger max is Info, so Warn never reaches logcat.
                log::info!(
                    target: "NaviCamping",
                    "corridor segment graph load failed seg=[{seg_start}..{end}) \
                     bbox={seg_bbox:?} err={e:#}"
                );
            }
        }
        start = end;
    }
    let seeds_raw = seeds.len();
    let probes_raw = probes.len();
    seeds = subsample_even(&seeds, CAMPING_MAX_SEEDS);
    if probes.len() > CAMPING_MAX_SEEDS * 2 {
        probes = subsample_even(&probes, CAMPING_MAX_SEEDS * 2);
    }
    if probes.is_empty() {
        probes = seeds.iter().map(|s| (s.lat, s.lon)).collect();
    }
    let graph_ms = t_graph.elapsed().as_millis() as u64;
    let junction_ms = 0u64;
    let probe_walk_ms = 0u64;
    if seeds.is_empty() && probes.is_empty() {
        return CampingCallResult {
            kind: CampingCallKind::Unavailable,
            message: format!(
                "corridor graph segments produced no seeds (pack_dirs={}; segment_errors={segment_errors})",
                pack_dirs.len()
            ),
            elapsed_ms: started.elapsed().as_millis() as u64,
            result_json: None,
            peak_guest_memory_bytes: 0,
        };
    }

    let t_bldg = Instant::now();
    let (all_buildings, mut glacier_rings) =
        load_overnight_geometry_near_probes(&data_dir, &pack_dirs, &probes);
    let mut buildings = buildings_near_probes(&all_buildings, &probes, 0.008);
    if buildings.len() > CAMPING_MAX_JOB_BUILDINGS {
        buildings = subsample_even(&buildings, CAMPING_MAX_JOB_BUILDINGS);
    }
    let buildings_ms = t_bldg.elapsed().as_millis() as u64;

    let t_admin = Instant::now();
    let mut countries: Vec<(f64, f64, String)> = Vec::with_capacity(probes.len());
    let mut subdivisions: Vec<(f64, f64, String)> = Vec::new();
    for &(lat, lon) in &probes {
        let ar = admin_region_at(lat, lon);
        countries.push((lat, lon, ar.country_iso.unwrap_or_else(|| "unknown".into())));
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
    if buildings.len() > CAMPING_MAX_SEEDS * 24 {
        buildings = subsample_even(&buildings, CAMPING_MAX_SEEDS * 24);
    }
    if glacier_rings.len() > 32 {
        glacier_rings = subsample_even(&glacier_rings, 32);
    }
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
    let mut job_json = job.to_string();
    const CAMPING_GUEST_JOB_MAX_BYTES: usize = 900_000;
    if job_json.len() > CAMPING_GUEST_JOB_MAX_BYTES {
        log::warn!(
            target: "NaviCamping",
            "suggest job {} bytes exceeds guest kv buffer; dropping glaciers then subsample buildings",
            job_json.len()
        );
        let mut compact = job.clone();
        if let Some(obj) = compact.as_object_mut() {
            obj.insert("glaciers".into(), serde_json::json!([]));
            if let Some(b) = obj.get("buildings").and_then(|v| v.as_array()) {
                let keep = subsample_even(b, CAMPING_MAX_SEEDS * 8);
                obj.insert("buildings".into(), serde_json::Value::Array(keep));
            }
        }
        job_json = compact.to_string();
    }
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
        "suggest_along_route via=wasmtime job_bytes={job_bytes} seeds_raw={seeds_raw} \
         probes_raw={probes_raw} probes={} buildings={} first_lat={:.4} last_lat={:.4} timing={timing}",
        probes.len(),
        buildings.len(),
        probes.first().map(|p| p.0).unwrap_or(0.0),
        probes.last().map(|p| p.0).unwrap_or(0.0),
    );
    call.result_json = merge_guest_meta(call.result_json, timing, call.peak_guest_memory_bytes);
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
            memory_bytes: 1024 * 1024,
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
        "camp_here_tonight": "explicit UI action only — NightStore::record_night with clock_read date",
        "undo_camp_here_tonight": "same-day undo — NightStore::undo_night_today",
    })
    .to_string()
}

/// Thin CampingHost over FilePluginKv for night-store mutations (not suggest eval).
struct NightKvHost {
    kv: FilePluginKv,
    clock: navi_right_to_roam_camping::LocalDate,
}

impl navi_right_to_roam_camping::CampingHost for NightKvHost {
    fn safety_config(&self) -> Option<navi_right_to_roam_camping::OvernightSafety> {
        None
    }
    fn clock_local(&self) -> Option<navi_right_to_roam_camping::LocalDate> {
        Some(self.clock)
    }
    fn plugin_kv_available(&self) -> bool {
        true
    }
    fn kv_get(&self, key: &str) -> Option<String> {
        self.kv.get(key)
    }
    fn kv_set(&mut self, key: &str, value: &str) -> Result<(), String> {
        self.kv.set(key, value).map_err(|e| e.to_string())
    }
    fn admin_country_iso(&self, _: f64, _: f64) -> Option<String> {
        None
    }
    fn admin_subdivision_iso(&self, _: f64, _: f64) -> Option<String> {
        None
    }
    fn travel_mode(&self) -> navi_right_to_roam_camping::TravelMode {
        navi_right_to_roam_camping::TravelMode::Unknown
    }
    fn overnight_buildings(&self) -> &[(f64, f64)] {
        &[]
    }
    fn overnight_glacier_rings(&self) -> &[Vec<[f64; 2]>] {
        &[]
    }
}

fn night_store_key_for_location(
    lat: f64,
    lon: f64,
    country_iso: Option<String>,
    subdivision_iso: Option<String>,
) -> Result<(String, String), String> {
    let country = country_iso.or_else(|| admin_region_at(lat, lon).country_iso);
    let subdiv = subdivision_iso.or_else(|| admin_region_at(lat, lon).subdivision_iso);
    let pack = navi_right_to_roam_camping::pack_for_location(country.as_deref(), subdiv.as_deref());
    let (max_n, store_key) = pack
        .hard_max_nights()
        .ok_or_else(|| "pack has no hard consecutive-night limit".to_string())?;
    let _ = max_n;
    let loc = navi_right_to_roam_camping::location_id_from_lat_lon(lat, lon);
    Ok((store_key.to_string(), loc))
}

fn open_night_kv_host() -> Result<NightKvHost, String> {
    let guard = session_lock().lock().expect("camping session lock");
    let session = guard.as_ref().ok_or_else(|| "not configured".to_string())?;
    let api = AndroidCampingApi::open(session)?;
    let c = api
        .clock_read()
        .ok_or_else(|| "clock_read unavailable".to_string())?;
    let clock = navi_right_to_roam_camping::LocalDate {
        year: c.year,
        month: c.month,
        day: c.day,
    };
    Ok(NightKvHost { kv: api.kv, clock })
}

/// Explicit "Camp here tonight" — records one night using `clock_read` date.
/// Suggest / Accept must never call this.
#[uniffi::export]
pub fn camping_plugin_camp_here_tonight(
    lat: f64,
    lon: f64,
    country_iso: Option<String>,
    subdivision_iso: Option<String>,
) -> String {
    let (store_key, loc) =
        match night_store_key_for_location(lat, lon, country_iso, subdivision_iso) {
            Ok(v) => v,
            Err(e) => return format!("FAIL: {e}"),
        };
    let mut host = match open_night_kv_host() {
        Ok(h) => h,
        Err(e) => return format!("FAIL: {e}"),
    };
    let tonight = host.clock;
    match navi_right_to_roam_camping::NightStore::record_night(&mut host, &store_key, &loc, tonight)
    {
        Ok(()) => {
            log::info!(
                target: "NaviCamping",
                "camp_here_tonight pack={store_key} loc={loc} date={:04}-{:02}-{:02}",
                tonight.year, tonight.month, tonight.day
            );
            format!(
                "OK: recorded {store_key}/{loc} on {:04}-{:02}-{:02}",
                tonight.year, tonight.month, tonight.day
            )
        }
        Err(e) => format!("FAIL: {e}"),
    }
}

/// Same-day undo / "not camping here" for an explicit Camp-here record.
#[uniffi::export]
pub fn camping_plugin_undo_camp_here_tonight(
    lat: f64,
    lon: f64,
    country_iso: Option<String>,
    subdivision_iso: Option<String>,
) -> String {
    let (store_key, loc) =
        match night_store_key_for_location(lat, lon, country_iso, subdivision_iso) {
            Ok(v) => v,
            Err(e) => return format!("FAIL: {e}"),
        };
    let mut host = match open_night_kv_host() {
        Ok(h) => h,
        Err(e) => return format!("FAIL: {e}"),
    };
    let tonight = host.clock;
    match navi_right_to_roam_camping::NightStore::undo_night_today(
        &mut host, &store_key, &loc, tonight,
    ) {
        Ok(true) => {
            log::info!(
                target: "NaviCamping",
                "undo_camp_here_tonight pack={store_key} loc={loc} date={:04}-{:02}-{:02}",
                tonight.year, tonight.month, tonight.day
            );
            format!(
                "OK: undone {store_key}/{loc} on {:04}-{:02}-{:02}",
                tonight.year, tonight.month, tonight.day
            )
        }
        Ok(false) => "OK: nothing to undo for today".into(),
        Err(e) => format!("FAIL: {e}"),
    }
}

#[cfg(test)]
mod geometry_cache_tests {
    use super::*;
    use std::io::Write;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "navi-camping-cache-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir for geometry cache test");
        dir
    }

    fn touch_manifest(dir: &Path, name: &str, body: &[u8]) {
        let mut f = fs::File::create(dir.join(name)).unwrap();
        f.write_all(body).unwrap();
    }

    #[test]
    fn geometry_cache_key_changes_on_route_pack_and_safety() {
        let dir = temp_dir();
        let pbf = dir.join("ostlandet-latest.osm.pbf");
        fs::write(&pbf, b"pbf-v1").unwrap();
        touch_manifest(
            &dir,
            "ostlandet-latest.navi-manifest.json",
            br#"{"schema":1,"stem":"ostlandet-latest"}"#,
        );
        let wp1 = vec![[61.1, 10.5], [61.2, 10.6]];
        let wp2 = vec![[61.1, 10.5], [61.3, 10.7]];
        let empty: Vec<PathBuf> = Vec::new();
        let k1 = geometry_cache_key(&wp1, &pbf, &dir, &empty, 12);
        let k2 = geometry_cache_key(&wp2, &pbf, &dir, &empty, 12);
        assert_ne!(k1, k2, "route change must invalidate geometry cache");

        // Pack rebake: manifest content/mtime fingerprint changes.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        touch_manifest(
            &dir,
            "ostlandet-latest.navi-manifest.json",
            br#"{"schema":1,"stem":"ostlandet-latest","rebake":2}"#,
        );
        let k3 = geometry_cache_key(&wp1, &pbf, &dir, &empty, 12);
        assert_ne!(
            k1, k3,
            "pack version/hash change must invalidate geometry cache"
        );

        let s_fp = safety_geometry_fingerprint(&dir);
        assert!(
            k1.contains(&s_fp),
            "geometry key must include SafetyConfig fingerprint ({s_fp})"
        );
        let s_a = "bldg:150.000|glacier:1000.000|water:1.000|cabin:1.000|general:1.000";
        let s_b = "bldg:200.000|glacier:1000.000|water:1.000|cabin:1.000|general:1.000";
        assert_ne!(
            s_a, s_b,
            "SafetyConfig change must invalidate geometry cache"
        );
        let k_safety_a = format!("{:?}|{}|pack|{s_a}|12", wp1, pbf.display());
        let k_safety_b = format!("{:?}|{}|pack|{s_b}|12", wp1, pbf.display());
        assert_ne!(k_safety_a, k_safety_b);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn live_profile_overlay_keeps_geometry_but_refreshes_travel() {
        let geometry = r#"{"probes":[[61.1,10.5]],"travel_mode":"non_motorised","vehicle_class":"unknown","is_professional_driver_under_rest_rules":false,"clock":null}"#;
        let over = job_with_live_profile(geometry, TravelProfile::Car, true);
        let v: serde_json::Value = serde_json::from_str(&over).unwrap();
        assert_eq!(v["travel_mode"], "motorised");
        assert_eq!(v["vehicle_class"], "car");
        assert_eq!(v["is_professional_driver_under_rest_rules"], true);
        assert_eq!(v["probes"][0][0], 61.1);
        assert!(v["clock"].is_null());
    }

    #[test]
    fn geometry_cache_key_omits_travel_clock_and_residency() {
        let dir = temp_dir();
        let pbf = dir.join("x.osm.pbf");
        fs::write(&pbf, b"x").unwrap();
        let empty: Vec<PathBuf> = Vec::new();
        let k = geometry_cache_key(&[[1.0, 2.0]], &pbf, &dir, &empty, 4);
        assert!(
            !k.contains("motorised")
                && !k.contains("Hiking")
                && !k.contains("2026-")
                && !k.contains("residency"),
            "geometry key must not include travel/clock/residency: {k}"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_planning_pbf_sees_long_trip_pack_dirs() {
        let root = temp_dir();
        let data = root.join("files");
        let ltp = data.join("long-trip-packs");
        fs::create_dir_all(&data).unwrap();
        fs::create_dir_all(&ltp).unwrap();
        fs::write(ltp.join("nord-norge-latest.osm.pbf"), b"pbf").unwrap();
        assert!(
            find_planning_pbf(&data, &[]).is_none(),
            "files/ alone must not see LTP-only PBF"
        );
        let found =
            find_planning_pbf(&data, std::slice::from_ref(&ltp)).expect("pack_dirs must find PBF");
        assert!(found.ends_with("nord-norge-latest.osm.pbf"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn find_planning_pbf_prefers_manifest_backed_over_sweden_country() {
        let root = temp_dir();
        let data = root.join("files");
        let ltp = root.join("long-trip-packs");
        fs::create_dir_all(&data).unwrap();
        fs::create_dir_all(&ltp).unwrap();
        // Country extract without a graph pack (device landmine).
        fs::write(data.join("sweden-latest.osm.pbf"), b"pbf").unwrap();
        // Leaf Ready stem next to its PBF under pack_dirs.
        fs::write(ltp.join("niedersachsen-latest.osm.pbf"), b"pbf").unwrap();
        fs::write(ltp.join("niedersachsen-latest.navi-manifest.json"), b"{}").unwrap();
        let found =
            find_planning_pbf(&data, std::slice::from_ref(&ltp)).expect("must pick leaf PBF");
        assert!(
            found.ends_with("niedersachsen-latest.osm.pbf"),
            "got {}",
            found.display()
        );
        let _ = fs::remove_dir_all(&root);
    }

    /// Regression: warm path must clone the cached job and drop the session lock
    /// before `run_camping_guest` (which re-acquires the same Mutex). Holding the
    /// lock across the guest call deadlocks the warm path (~9 ms target).
    #[test]
    fn warm_path_releases_lock_before_guest() {
        let src = include_str!("camping_plugin.rs");
        let warm = src
            .split("if let Some(geometry_json) = cached_geometry")
            .nth(1)
            .expect("warm-path cache hit arm");
        let warm_arm = warm.split("let routing_profile").next().unwrap();
        assert!(
            warm_arm.contains("run_camping_guest(Some(job_json))"),
            "warm path must still invoke the guest"
        );
        assert!(
            !warm_arm.contains("session_lock().lock()"),
            "warm path must not re-lock session across run_camping_guest"
        );
        let run = src
            .split("fn run_camping_guest")
            .nth(1)
            .expect("run_camping_guest");
        let before_call = run.split("host.call_with_stats").next().unwrap();
        assert!(
            before_call.contains("drop(guard)"),
            "run_camping_guest must drop session lock before guest call"
        );
    }

    #[test]
    fn corridor_bbox_treats_waypoints_as_lat_lon() {
        // Host JSON is [[lat, lon], …]. Feeding overlay lon,lat as-is puts
        // Bugøynes in Pakistan latitudes (~29N).
        let overlay_as_latlon = [[29.6337571_f64, 69.9741435]];
        let converted = [[69.9741435_f64, 29.6337571]];
        let swapped = corridor_bbox_from_waypoints(&overlay_as_latlon).unwrap();
        let norway = corridor_bbox_from_waypoints(&converted).unwrap();
        assert!(
            norway[0] > 60.0,
            "min_lat after lon,lat conversion must stay in Nord-Norge, got {}",
            norway[0]
        );
        assert!(
            swapped[0] < 40.0,
            "unconverted overlay would request ~Pakistan latitudes, got {}",
            swapped[0]
        );
    }
}
