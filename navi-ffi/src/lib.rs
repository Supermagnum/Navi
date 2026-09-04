//! UniFFI surface for the Navi Android app and other foreign-language hosts.
//!
//! Two tiers of on-device checks:
//! - [`ffi_linkage_smoke_test`] — fast SMOKE (FFI + worker pool only; no routing).
//! - [`run_car_corridor_pipeline`] — real parse/build/reweight/POI/route against on-device data.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use driver_break_core::config::{
    EcoConfig, FmcsaHosParams, JurisdictionDrivingHoursPack, ProfilePoiRadii, ProfilePoiRadiiTable,
    RestConfig, SafetyConfig, VehicleLimits, HIKING_MAIN_BREAK_DISTANCE_KM,
    OVERNIGHT_BUILDING_CORRIDOR_MARGIN_M,
};
use driver_break_core::icons::{self, IconTheme};
use driver_break_core::poi::{rest_area_suitable_for_weekly, PoiCategory, PoiIndex, PoiRecord};
use driver_break_core::routing::elevation::{ElevationCache, ElevationService};
use driver_break_core::routing::graph::{
    apply_bike_suitability_from_pbf, apply_official_network_preference, apply_slow_road_preference,
    apply_surface_preference, apply_surface_quality_from_pbf, difficulty_notes_for_path,
    load_official_network_way_ids, load_or_build_reweighted, load_or_build_reweighted_bbox,
    load_pilgrim_route_way_ids, load_way_difficulty_tags, max_waypoint_snap_m, BikeCapability,
    OfficialNetworkKind, RoadLabelSticky, RoadNodeIndex, RouteGraph, RouteOptions, RoutingProfile,
    SnapTooFar, SurfaceRoutingMode,
};
use driver_break_core::routing::rest::car_break_interval_hours;
use driver_break_core::routing::safety::{
    check_overnight_candidate, DangerBarrierIndex, OvernightProximityIndex,
};
use driver_break_core::routing::workers::WorkerPoolPlan;
use driver_break_core::routing::{
    build_maneuvers, build_maneuvers_from_edges, build_sim_samples, build_sim_samples_from_edges,
    build_sim_samples_from_lat_lon, maneuvers_to_json, motor_path_minutes_from_edges,
    plan_hybrid_hiking_path, samples_to_json, HikingWaypoint, WetlandIndex, OFF_TRAIL_ADVISORY,
};
use driver_break_core::routing::{
    commit_truck_multi_day_plan, evaluate_fmcsa_trip, evaluate_truck_trip,
    hiking_samples_from_coords, max_daily_distance_km, motor_break_interval_km, motor_daily_budget,
    plan_fmcsa_multi_day, plan_hiking_multi_day, plan_motor_multi_day, plan_truck_multi_day,
    resolve_driving_hours_pack_at, truck_effective_break_parts, uses_motor_multi_day,
    uses_truck_rest, HikingMultiDayPlan, MotorMultiDayPlan, MotorOvernightCandidate,
    MotorOvernightKind, TruckMultiDayPlan, TruckOvernightKind, TruckOvernightRest,
    TruckRestCandidate, TruckRestFacility,
};
use driver_break_core::routing::{fixed_pace_minutes, motor_path_minutes, HIKING_MIN_PER_KM};
use osm4routing::NodeId;
use serde::Deserialize;
use serde_json::json;

uniffi::setup_scaffolding!();

fn ensure_native_logging() {
    #[cfg(target_os = "android")]
    {
        android_logger::init_once(
            android_logger::Config::default()
                .with_max_level(log::LevelFilter::Info)
                .with_tag("NaviNative"),
        );
    }
}

/// Initialize native logging so download progress appears in `adb logcat`
/// (tag `NaviNative`). Safe to call more than once. Also invoked automatically
/// from download FFI entry points.
#[uniffi::export]
pub fn init_native_logging() {
    ensure_native_logging();
    log::info!(target: "NaviDownload", "native logging ready");
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiDownloadProgress {
    pub units_done: u64,
    pub units_total: Option<u64>,
    pub percent: Option<u32>,
    pub label: String,
}

/// Snapshot of the in-flight download (region PBF or PMTiles extract) for UI polling.
#[uniffi::export]
pub fn download_progress_snapshot() -> FfiDownloadProgress {
    ffi_progress(driver_break_core::download::progress::snapshot_on(
        driver_break_core::download::progress::ProgressChannel::Download,
    ))
}

fn ffi_progress(s: driver_break_core::download::progress::Snapshot) -> FfiDownloadProgress {
    FfiDownloadProgress {
        units_done: s.units_done,
        units_total: s.units_total,
        percent: s.percent,
        label: s.label,
    }
}

/// Plan-only progress (PBF bbox / A*). Isolated from convert and cone.
#[uniffi::export]
pub fn plan_progress_snapshot() -> FfiDownloadProgress {
    ffi_progress(driver_break_core::download::progress::snapshot_on(
        driver_break_core::download::progress::ProgressChannel::Plan,
    ))
}

/// Indexed-maps convert progress. Isolated from plan and download.
#[uniffi::export]
pub fn convert_progress_snapshot() -> FfiDownloadProgress {
    ffi_progress(driver_break_core::download::progress::snapshot_on(
        driver_break_core::download::progress::ProgressChannel::Convert,
    ))
}

/// Clear the shared download progress snapshot.
#[uniffi::export]
pub fn download_progress_clear() {
    driver_break_core::download::progress::clear_on(
        driver_break_core::download::progress::ProgressChannel::Download,
    );
}

#[uniffi::export]
pub fn plan_progress_clear() {
    driver_break_core::download::progress::clear_on(
        driver_break_core::download::progress::ProgressChannel::Plan,
    );
}

#[uniffi::export]
pub fn convert_progress_clear() {
    driver_break_core::download::progress::clear_on(
        driver_break_core::download::progress::ProgressChannel::Convert,
    );
}

/// Pause background PBF convert / place-index while a foreground plan runs.
#[uniffi::export]
pub fn foreground_plan_enter() {
    driver_break_core::download::pbf_priority::enter();
}

#[uniffi::export]
pub fn foreground_plan_leave() {
    driver_break_core::download::pbf_priority::leave();
}

/// True while [`foreground_plan_enter`] is unmatched by leave (UI plan in flight).
#[uniffi::export]
pub fn foreground_plan_active() -> bool {
    driver_break_core::download::pbf_priority::foreground_plan_active()
}

/// Ask the in-flight UniFFI plan to stop at the next checkpoint.
/// Does not unwind JNI; [`plan_car_route`] / [`plan_hiking_route`] return
/// `FAIL: cancelled` once a blob/stage/A* check observes the flag.
#[uniffi::export]
pub fn cancel_in_flight_plan() {
    driver_break_core::download::plan_cancel::request_cancel();
}

/// True when the place-index SQLite file has at least one searchable row.
#[uniffi::export]
pub fn place_index_has_entries(index_db_path: String) -> bool {
    driver_break_core::search::NameIndex::has_entries(std::path::Path::new(&index_db_path))
}

/// Number of logical CPU cores detected on the host (routing worker autodetect input).
#[uniffi::export]
pub fn detected_parallelism() -> u32 {
    WorkerPoolPlan::detect().detected_cores as u32
}

/// Routing-tier worker count after reserving UI/audio headroom.
#[uniffi::export]
pub fn routing_worker_count() -> u32 {
    WorkerPoolPlan::detect().routing_workers as u32
}

/// Fast FFI/worker-pool smoke test. Does **not** validate routing.
///
/// Report always contains `TEST_KIND=SMOKE` and `DATA_SOURCE=none`.
#[uniffi::export]
pub fn ffi_linkage_smoke_test() -> String {
    let plan = WorkerPoolPlan::detect();
    format!(
        "TEST_KIND=SMOKE\n\
         DATA_SOURCE=none\n\
         note=FFI linkage + WorkerPoolPlan only; does not validate routing\n\
         detected_cores={}\n\
         routing_workers={}\n\
         reserved_for_ui_audio={}\n\
         PASS\n",
        plan.detected_cores, plan.routing_workers, plan.reserved_for_ui_audio
    )
}

fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6_378_100.0;
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

/// Snap `(lat,lon)` to a linked graph node within the profile snap budget.
fn nearest(graph: &RouteGraph, lat: f64, lon: f64) -> Result<(NodeId, f64), SnapTooFar> {
    graph.nearest_routable(lat, lon)
}

fn snap_network_noun(profile: RoutingProfile) -> &'static str {
    match profile {
        RoutingProfile::Foot => "walkable path",
        RoutingProfile::Bicycle => "cycle path",
        RoutingProfile::Car | RoutingProfile::Truck => "routable road",
    }
}

fn format_snap_too_far(label: &str, err: SnapTooFar, profile: RoutingProfile) -> String {
    format!(
        "{label} too far from any {} ({:.0} m > {:.0} m)",
        snap_network_noun(profile),
        err.nearest_m,
        err.max_m
    )
}

fn load_break_barriers(
    graph: &RouteGraph,
    pbf: &Path,
    bbox: [f64; 4],
) -> Result<DangerBarrierIndex, bool> {
    let mut barriers = DangerBarrierIndex::from_graph(graph);
    match DangerBarrierIndex::load_from_pbf_bbox(pbf, bbox) {
        Ok(extra) => {
            barriers.merge(extra);
            Ok(barriers)
        }
        Err(e) if driver_break_core::download::plan_cancel::is_cancel_err(&e) => Err(true),
        Err(e) => {
            log::warn!("danger barrier PBF load skipped: {e:#}");
            Ok(barriers)
        }
    }
}

fn car_required_breaks(driving_hours: f64, max_interval_hours: f64) -> u32 {
    if driving_hours <= max_interval_hours {
        0
    } else {
        (driving_hours / max_interval_hours).floor() as u32
    }
}

fn passat_eco() -> EcoConfig {
    EcoConfig {
        drag_coefficient: 0.28,
        frontal_area_m2: 2.2,
        mass_kg: 1500.0,
        ..EcoConfig::default()
    }
}

/// Profile-scoped eco physics for planning. Electric profiles get regen credit from
/// [`EcoConfig::for_profile`]; Car/CarElectric keep the Passat Cd/area/mass baseline.
/// Motorcycle uses [`motorcycle_eco_config`] (not the Passat overlay).
fn eco_for_travel_profile(profile: TravelProfile) -> EcoConfig {
    let mut eco = EcoConfig::for_profile(profile.to_core());
    match profile {
        TravelProfile::Car | TravelProfile::CarElectric => {
            eco.drag_coefficient = 0.28;
            eco.frontal_area_m2 = 2.2;
            eco.mass_kg = 1500.0;
        }
        TravelProfile::Motorcycle | TravelProfile::MotorcycleElectric => {
            // Already motorcycle-specific from for_profile; do not apply Passat.
        }
        TravelProfile::Bicycle | TravelProfile::BicycleElectric => {
            let tuned = driver_break_core::config::ebike_eco_config(matches!(
                profile,
                TravelProfile::BicycleElectric
            ));
            eco.drag_coefficient = tuned.drag_coefficient;
            eco.frontal_area_m2 = tuned.frontal_area_m2;
            eco.mass_kg = tuned.mass_kg;
            eco.rolling_resistance = tuned.rolling_resistance;
            eco.cruise_speed_m_s = tuned.cruise_speed_m_s;
            // Keep for_profile regen for BicycleElectric; plain Bicycle stays 0.
            if matches!(profile, TravelProfile::BicycleElectric) {
                eco.regen_efficiency = tuned.regen_efficiency;
            }
        }
        _ => {}
    }
    eco
}

fn ffi_vehicle_to_limits(v: &FfiVehicleLimits) -> Option<VehicleLimits> {
    if v.axle_weight_kg.is_none()
        && v.bogie_weight_kg.is_none()
        && v.height_m.is_none()
        && v.width_m.is_none()
        && v.length_m.is_none()
        && v.total_weight_kg.is_none()
    {
        return None;
    }
    Some(VehicleLimits {
        axle_weight_kg: v.axle_weight_kg,
        bogie_weight_kg: v.bogie_weight_kg,
        height_m: v.height_m,
        width_m: v.width_m,
        length_m: v.length_m,
        total_weight_kg: v.total_weight_kg,
    })
}

fn apply_network_pref_if_requested(
    graph: &mut RouteGraph,
    pbf: &Path,
    kind: OfficialNetworkKind,
    prefer: bool,
    report: &mut String,
) {
    apply_route_prefs_if_requested(graph, pbf, kind, prefer, false, report);
}

fn apply_route_prefs_if_requested(
    graph: &mut RouteGraph,
    pbf: &Path,
    kind: OfficialNetworkKind,
    prefer_official: bool,
    prefer_pilgrim: bool,
    report: &mut String,
) {
    if !prefer_official && !prefer_pilgrim {
        return;
    }
    let mut ways = std::collections::HashSet::new();
    if prefer_official {
        match load_official_network_way_ids(pbf, kind) {
            Ok(w) => {
                report.push_str(&format!(
                    "official_network_ways={}; prefer_official_networks=true\n",
                    w.len()
                ));
                ways.extend(w);
            }
            Err(e) if driver_break_core::download::plan_cancel::is_cancel_err(&e) => {
                return;
            }
            Err(e) => {
                report.push_str(&format!("WARN: official network load failed: {e:#}\n"));
            }
        }
    }
    if prefer_pilgrim {
        match load_pilgrim_route_way_ids(pbf) {
            Ok(w) => {
                report.push_str(&format!(
                    "pilgrim_route_ways={}; prefer_pilgrim_routes=true\n",
                    w.len()
                ));
                ways.extend(w);
            }
            Err(e) if driver_break_core::download::plan_cancel::is_cancel_err(&e) => {
                return;
            }
            Err(e) => {
                report.push_str(&format!("WARN: pilgrim route load failed: {e:#}\n"));
            }
        }
    }
    apply_official_network_preference(graph, &ways);
}

/// Download / ensure region data under `data_dir` via the in-app provisioner.
///
/// `pbf_url` is typically `http://10.0.2.2:<port>/...` from the emulator.
/// Optional `elevation_tar_url` seeds DEM tiles without live Copernicus fetch.
#[uniffi::export]
pub fn provision_region_data(
    data_dir: String,
    pbf_url: String,
    pbf_filename: String,
    elevation_tar_url: Option<String>,
) -> String {
    ensure_native_logging();
    let result = driver_break_core::routing::region::provision_region_with_elev_tar(
        Path::new(&data_dir),
        &pbf_url,
        &pbf_filename,
        elevation_tar_url.as_deref(),
    );
    match result {
        Ok(p) => format!(
            "TEST_KIND=PROVISION\n\
             DATA_SOURCE=download\n\
             pbf={}\n\
             elev_dir={}\n\
             cache_dir={}\n\
             osm_downloaded_bytes={}\n\
             dem_download_s={:.1}\n\
             PASS\n",
            p.pbf_path.display(),
            p.elev_dir.display(),
            p.cache_dir.display(),
            p.osm_downloaded_bytes,
            p.dem_download_s
        ),
        Err(e) => format!("TEST_KIND=PROVISION\nDATA_SOURCE=download\nFAIL: {e:#}\n"),
    }
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct CorridorRouteResult {
    pub report: String,
    pub distance_km: f64,
    /// Pre-departure duration estimate in minutes (before live GPS speed).
    ///
    /// Motor profiles: per-edge OSM `maxspeed` with highway-class fallback.
    /// Hiking / cycling callers may override using fixed pace on `distance_km`.
    pub eta_minutes: f64,
    pub cache_hit: bool,
    pub cold_build_s: f64,
    pub warm_load_s: f64,
    /// Encoded as "lon,lat;lon,lat;..." (MapLibre GeoJSON built on the Kotlin side).
    pub route_polyline: String,
    pub poi_lat: f64,
    pub poi_lon: f64,
    pub poi_name: String,
    pub poi_icon_key: String,
    /// JSON array of pause / overnight stops along the route:
    /// `[{"name","lat","lon","kind","icon"}]` where kind is `hut`, `tent`, or `amenity`.
    pub break_pois_json: String,
    /// JSON array of multi-day day cards (empty `"[]"` when single-day / unknown).
    /// Fields: day_index, date, start_km, end_km, distance_km, driving_hours,
    /// profile, rest_kind, rest_hours, rest_label, overnight_name, overnight_found,
    /// not_in_cab, compensation, is_final.
    pub days_json: String,
    /// Densified path samples for debug route simulation / live snap:
    /// `[{"lat","lon","cum_m","speed_kmh","highway","maxspeed_posted","street"?}]`.
    pub sim_samples_json: String,
    /// Turn / destination maneuvers along the path:
    /// `[{"lat","lon","cum_m","kind","street","roundabout_exit"}]`.
    pub maneuvers_json: String,
    /// Non-motorway share of planned path length (0–100). Motor: 100% minus
    /// motorway / motorway_link distance. Used by the avoid-motorways report; 0
    /// when no path was planned.
    pub priority_path_share_pct: f64,
    /// JSON array of route segments for map styling:
    /// `[{"kind":"on_trail"|"off_trail","polyline":"lon,lat;…","length_m":…}]`.
    pub route_segments_json: String,
    /// Non-empty when the route includes an off-trail terrain segment (advisory).
    pub off_trail_advisory: String,
}

fn empty_corridor(msg: String) -> CorridorRouteResult {
    CorridorRouteResult {
        report: msg,
        distance_km: 0.0,
        eta_minutes: 0.0,
        cache_hit: false,
        cold_build_s: 0.0,
        warm_load_s: 0.0,
        route_polyline: String::new(),
        poi_lat: 0.0,
        poi_lon: 0.0,
        poi_name: String::new(),
        poi_icon_key: String::new(),
        break_pois_json: String::from("[]"),
        days_json: String::from("[]"),
        sim_samples_json: String::from("[]"),
        maneuvers_json: String::from("[]"),
        priority_path_share_pct: 0.0,
        route_segments_json: String::from("[]"),
        off_trail_advisory: String::new(),
    }
}

/// When false (default), planners skip stage Instant samples and do not emit
/// `plan_duration_ms` / `ROUTE_PLAN_STAGES` into the report. Hosts turn this on
/// with the Diagnostic logging toggle ([`set_route_plan_timing_enabled`]).
static ROUTE_PLAN_TIMING_ENABLED: AtomicBool = AtomicBool::new(false);

/// Gate per-stage route-plan timing (matches Android Diagnostic logging toggle).
#[uniffi::export]
pub fn set_route_plan_timing_enabled(enabled: bool) {
    ROUTE_PLAN_TIMING_ENABLED.store(enabled, Ordering::Relaxed);
}

#[uniffi::export]
pub fn route_plan_timing_enabled() -> bool {
    ROUTE_PLAN_TIMING_ENABLED.load(Ordering::Relaxed)
}

/// Wall-clock stage timer for greppable `ROUTE_PLAN_STAGES` lines. No-op when
/// timing is disabled (no Instant samples, zero cost beyond one atomic load).
struct PlanStageTimer {
    enabled: bool,
    t0: Option<Instant>,
    mark: Option<Instant>,
}

impl PlanStageTimer {
    fn start() -> Self {
        let enabled = ROUTE_PLAN_TIMING_ENABLED.load(Ordering::Relaxed);
        if enabled {
            let now = Instant::now();
            Self {
                enabled: true,
                t0: Some(now),
                mark: Some(now),
            }
        } else {
            Self {
                enabled: false,
                t0: None,
                mark: None,
            }
        }
    }

    fn lap_ms(&mut self) -> u64 {
        if !self.enabled {
            return 0;
        }
        let mark = self.mark.expect("timer enabled");
        let ms = mark.elapsed().as_millis() as u64;
        self.mark = Some(Instant::now());
        ms
    }

    fn total_ms(&self) -> u64 {
        self.t0.map(|t| t.elapsed().as_millis() as u64).unwrap_or(0)
    }
}

fn append_route_plan_timing(report: &mut String, total_ms: u64, stages: &[(&str, u64)]) {
    if !ROUTE_PLAN_TIMING_ENABLED.load(Ordering::Relaxed) {
        return;
    }
    report.push_str(&format!("plan_duration_ms={total_ms}\n"));
    report.push_str("ROUTE_PLAN_STAGES |");
    for (k, v) in stages {
        report.push_str(&format!(" {k}={v}"));
    }
    report.push('\n');
}

fn plan_cancelled_result(
    mut report: String,
    timer: &PlanStageTimer,
    stages: &[(&str, u64)],
) -> CorridorRouteResult {
    report.push_str("FAIL: cancelled\ncancelled=true\n");
    append_route_plan_timing(&mut report, timer.total_ms(), stages);
    empty_corridor(report)
}

fn truck_rest_kind_key(kind: TruckOvernightKind) -> &'static str {
    match kind {
        TruckOvernightKind::DailyRegular => "daily_regular",
        TruckOvernightKind::DailyReduced => "daily_reduced",
        TruckOvernightKind::DailySplit => "daily_split",
        TruckOvernightKind::WeeklyRegular => "weekly_regular",
        TruckOvernightKind::WeeklyReduced => "weekly_reduced",
    }
}

fn truck_rest_label(o: &TruckOvernightRest) -> String {
    match o.kind {
        TruckOvernightKind::DailyRegular => format!("Daily rest {:.0} h", o.hours),
        TruckOvernightKind::DailyReduced => format!("Reduced daily rest {:.0} h", o.hours),
        TruckOvernightKind::DailySplit => {
            if let Some((a, b)) = o.split_parts {
                format!("Split daily rest {a:.0}+{b:.0} h")
            } else {
                format!("Split daily rest {:.0} h", o.hours)
            }
        }
        TruckOvernightKind::WeeklyRegular => format!("Weekly rest {:.0} h", o.hours),
        TruckOvernightKind::WeeklyReduced => format!("Reduced weekly rest {:.0} h", o.hours),
    }
}

fn truck_compensation_note(o: &TruckOvernightRest) -> String {
    if o.kind == TruckOvernightKind::WeeklyReduced {
        return "compensation due after reduced weekly rest".into();
    }
    o.notes
        .iter()
        .find(|n| n.to_lowercase().contains("compensat"))
        .cloned()
        .unwrap_or_default()
}

fn days_json_from_truck(plan: &TruckMultiDayPlan, profile: &str) -> String {
    let n = plan.days.len();
    let arr: Vec<serde_json::Value> = plan
        .days
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let is_final = i + 1 == n;
            let (
                rest_kind,
                rest_hours,
                rest_label,
                overnight_name,
                overnight_found,
                not_in_cab,
                compensation,
            ) = match &d.overnight {
                Some(o) => (
                    truck_rest_kind_key(o.kind).to_string(),
                    o.hours,
                    truck_rest_label(o),
                    o.name.clone().unwrap_or_default(),
                    o.poi_found,
                    o.not_in_cab,
                    truck_compensation_note(o),
                ),
                None => (
                    String::new(),
                    0.0,
                    String::new(),
                    String::new(),
                    false,
                    false,
                    String::new(),
                ),
            };
            json!({
                "day_index": d.day_index,
                "date": d.date,
                "start_km": d.start_km,
                "end_km": d.end_km,
                "distance_km": (d.end_km - d.start_km).max(0.0),
                "driving_hours": d.driving_hours,
                "profile": profile,
                "rest_kind": rest_kind,
                "rest_hours": rest_hours,
                "rest_label": rest_label,
                "overnight_name": overnight_name,
                "overnight_found": overnight_found,
                "not_in_cab": not_in_cab,
                "compensation": compensation,
                "is_final": is_final,
            })
        })
        .collect();
    serde_json::to_string(&arr).unwrap_or_else(|_| "[]".into())
}

fn days_json_from_hiking(plan: &HikingMultiDayPlan) -> String {
    let n = plan.days.len();
    let arr: Vec<serde_json::Value> = plan
        .days
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let is_final = i + 1 == n;
            let (rest_kind, rest_label, overnight_name, overnight_found, safety_reason) =
                match &d.overnight {
                    Some(o) => {
                        let reason = o.safety_reason.clone().unwrap_or_default();
                        let label = if o.safety_rejected && !reason.is_empty() {
                            reason.clone()
                        } else if o.is_network && o.membership_required {
                            "Network hut nearby (membership required)".to_string()
                        } else if o.is_network {
                            "Network hut overnight".to_string()
                        } else {
                            "Hut overnight".to_string()
                        };
                        (
                            if o.is_network { "network_hut" } else { "hut" }.to_string(),
                            label,
                            o.name.clone(),
                            !o.safety_rejected,
                            reason,
                        )
                    }
                    None => (
                        String::new(),
                        String::new(),
                        String::new(),
                        false,
                        String::new(),
                    ),
                };
            json!({
                "day_index": d.day_index,
                "date": "",
                "start_km": d.start_km,
                "end_km": d.end_km,
                "distance_km": d.distance_km,
                "driving_hours": 0.0,
                "profile": "hiking",
                "rest_kind": rest_kind,
                "rest_hours": 0.0,
                "rest_label": rest_label,
                "overnight_name": overnight_name,
                "overnight_found": overnight_found,
                "safety_rejected": d.overnight.as_ref().map(|o| o.safety_rejected).unwrap_or(false),
                "safety_reason": safety_reason,
                "membership_required": d
                    .overnight
                    .as_ref()
                    .map(|o| o.membership_required)
                    .unwrap_or(false),
                "not_in_cab": false,
                "compensation": "",
                "is_final": is_final,
            })
        })
        .collect();
    serde_json::to_string(&arr).unwrap_or_else(|_| "[]".into())
}

fn days_json_from_motor(plan: &MotorMultiDayPlan, profile: &str) -> String {
    let n = plan.days.len();
    let arr: Vec<serde_json::Value> = plan
        .days
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let is_final = i + 1 == n;
            let (rest_kind, rest_label, overnight_name, overnight_found) = match &d.overnight {
                Some(o) => {
                    let kind = match o.kind {
                        MotorOvernightKind::Lodging => "lodging",
                        MotorOvernightKind::Camping => "camping",
                        MotorOvernightKind::RestArea => "rest_area",
                        MotorOvernightKind::None => "none",
                    };
                    (
                        kind.to_string(),
                        match o.kind {
                            MotorOvernightKind::Lodging => "Lodging overnight",
                            MotorOvernightKind::Camping => "Camping overnight",
                            MotorOvernightKind::RestArea => "Rest-area overnight",
                            MotorOvernightKind::None => "Overnight (no POI)",
                        }
                        .to_string(),
                        o.name.clone().unwrap_or_default(),
                        o.poi_found,
                    )
                }
                None => (String::new(), String::new(), String::new(), false),
            };
            json!({
                "day_index": d.day_index,
                "date": "",
                "start_km": d.start_km,
                "end_km": d.end_km,
                "distance_km": d.distance_km,
                "driving_hours": d.driving_hours,
                "profile": profile,
                "rest_kind": rest_kind,
                "rest_hours": 0.0,
                "rest_label": rest_label,
                "overnight_name": overnight_name,
                "overnight_found": overnight_found,
                "not_in_cab": false,
                "compensation": "",
                "is_final": is_final,
            })
        })
        .collect();
    serde_json::to_string(&arr).unwrap_or_else(|_| "[]".into())
}

fn truck_overnight_break_pins(plan: &TruckMultiDayPlan) -> Vec<serde_json::Value> {
    let mut pins = Vec::new();
    for d in &plan.days {
        let Some(o) = &d.overnight else { continue };
        if !o.poi_found {
            continue;
        }
        let (Some(lat), Some(lon)) = (o.lat, o.lon) else {
            continue;
        };
        pins.push(json!({
            "name": o.name.clone().unwrap_or_else(|| "Truck rest".into()),
            "lat": lat,
            "lon": lon,
            "kind": "rest_area",
            "icon": "highway-rest_area",
            "icon_key": "highway-rest_area",
            "along_km": d.end_km,
            "overnight": true,
            "rest_kind": truck_rest_kind_key(o.kind),
            "not_in_cab": o.not_in_cab,
        }));
    }
    pins
}

fn merge_break_poi_pins(break_pois_json: &mut String, pins: Vec<serde_json::Value>) {
    if pins.is_empty() {
        return;
    }
    if let Ok(mut arr) = serde_json::from_str::<Vec<serde_json::Value>>(break_pois_json) {
        arr.extend(pins);
        if let Ok(s) = serde_json::to_string(&arr) {
            *break_pois_json = s;
        }
    }
}

fn fmcsa_break_interval_km(params: &FmcsaHosParams, dist_km: f64, eta_minutes: f64) -> f64 {
    let eta_h = (eta_minutes / 60.0).max(1e-6);
    let speed_kmh = (dist_km / eta_h).max(1.0);
    (speed_kmh * params.break_after_driving_hours).max(1.0)
}

fn travel_profile_report_key(profile: TravelProfile) -> &'static str {
    match profile {
        TravelProfile::Car => "car",
        TravelProfile::CarElectric => "car_electric",
        TravelProfile::Motorcycle => "motorcycle",
        TravelProfile::MotorcycleElectric => "motorcycle_electric",
        TravelProfile::Bicycle => "bicycle",
        TravelProfile::BicycleElectric => "bicycle_electric",
        TravelProfile::Hiking => "hiking",
        TravelProfile::Truck => "truck",
        TravelProfile::TruckElectric => "truck_electric",
        TravelProfile::MobileHome => "mobilehome",
    }
}

fn sample_polyline_km(polyline: &str) -> Vec<(f64, f64, f64)> {
    // Returns (lon, lat, cumulative_km)
    let mut out = Vec::new();
    let mut cum = 0.0;
    let mut prev: Option<(f64, f64)> = None;
    for part in polyline.split(';') {
        let bits: Vec<_> = part.split(',').collect();
        if bits.len() != 2 {
            continue;
        }
        let (Ok(lon), Ok(lat)) = (bits[0].parse::<f64>(), bits[1].parse::<f64>()) else {
            continue;
        };
        if let Some((plon, plat)) = prev {
            cum += haversine_m(plat, plon, lat, lon) / 1000.0;
        }
        out.push((lon, lat, cum));
        prev = Some((lon, lat));
    }
    out
}

fn interpolate_at_km(samples: &[(f64, f64, f64)], target_km: f64) -> (f64, f64) {
    if samples.is_empty() {
        return (0.0, 0.0);
    }
    if target_km <= samples[0].2 {
        return (samples[0].1, samples[0].0); // lat, lon
    }
    for w in samples.windows(2) {
        let (lon0, lat0, k0) = w[0];
        let (lon1, lat1, k1) = w[1];
        if target_km <= k1 {
            let t = if (k1 - k0).abs() < 1e-9 {
                0.0
            } else {
                (target_km - k0) / (k1 - k0)
            };
            let lat = lat0 + (lat1 - lat0) * t;
            let lon = lon0 + (lon1 - lon0) * t;
            return (lat, lon);
        }
    }
    let last = samples.last().unwrap();
    (last.1, last.0)
}

fn first_named<'a>(hits: &[&'a PoiRecord]) -> Option<&'a PoiRecord> {
    hits.iter()
        .copied()
        .find(|p| p.name.as_ref().is_some_and(|n| !n.trim().is_empty()))
        .or_else(|| hits.first().copied())
}

/// Path length along graph edges (metres).
fn path_length_m(graph: &RouteGraph, path: &[NodeId]) -> f64 {
    let mut m = 0.0;
    for w in path.windows(2) {
        if let Some(idx) = graph.edge_index(w[0], w[1]) {
            m += graph.edges[idx].length_m;
        }
    }
    m
}

/// True when `to` is reachable from `from` on the profile graph without ferries,
/// without crow-flies crossing a railway / major highway / river, and without a
/// large detour (e.g. around a lake).
fn reachable_without_barrier(
    graph: &RouteGraph,
    barriers: &DangerBarrierIndex,
    from_lat: f64,
    from_lon: f64,
    to_lat: f64,
    to_lon: f64,
) -> bool {
    let crow = haversine_m(from_lat, from_lon, to_lat, to_lon);
    if crow < 1.0 {
        return true;
    }
    // Railways, motorway/trunk, and rivers block straight-line access.
    if barriers.blocks_access(from_lat, from_lon, to_lat, to_lon) {
        return false;
    }
    let Ok((start, _)) = nearest(graph, from_lat, from_lon) else {
        return false;
    };
    let Ok((goal, _)) = nearest(graph, to_lat, to_lon) else {
        return false;
    };
    if start == goal {
        return true;
    }
    let opts = RouteOptions {
        avoid_ferries: true,
        // Hiking: do not treat motorway-grade walking as safe POI access.
        avoid_motorways: matches!(graph.profile(), RoutingProfile::Foot),
        ..RouteOptions::default()
    };
    let Some((path, _, _)) = graph.shortest_path_with_options(start, goal, false, &opts) else {
        return false;
    };
    let path_m = path_length_m(graph, &path);
    // Allow short absolute slack; reject lake-around detours (path >> crow-flies).
    const MAX_DETOUR_RATIO: f64 = 2.5;
    const SLACK_M: f64 = 800.0;
    path_m <= crow * MAX_DETOUR_RATIO + SLACK_M
}

fn sort_pois_near_sample(hits: &mut [&PoiRecord], lat: f64, lon: f64) {
    hits.sort_by(|a, b| {
        let da = haversine_m(lat, lon, a.lat, a.lon);
        let db = haversine_m(lat, lon, b.lat, b.lon);
        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
    });
}

/// Prefer POIs linked to the planned road/path/trail; else any POI reachable
/// without ferry/lake-scale detour or crow-flies across railway/highway/river;
/// else a synthetic stop on the sample.
///
/// Overnight / tent candidates are filtered with [`check_overnight_candidate`]
/// (building + glacier proximity) when `overnight` is provided.
fn pick_hiking_pause_at(
    poi: &PoiIndex,
    graph: &RouteGraph,
    barriers: &DangerBarrierIndex,
    route_link: &RoadNodeIndex,
    lat: f64,
    lon: f64,
    hut_radius_m: f64,
    overnight: Option<&(SafetyConfig, OvernightProximityIndex)>,
) -> (String, f64, f64, String, String) {
    let overnight_ok = |p: &PoiRecord| -> bool {
        let Some((safety, prox)) = overnight else {
            return true;
        };
        check_overnight_candidate(
            p.lat,
            p.lon,
            safety,
            p,
            &prox.buildings,
            &prox.glacier_rings,
        )
        .is_none()
    };
    let pick_hut = |p: &PoiRecord| -> (String, f64, f64, String, String) {
        let name = p
            .name
            .clone()
            .unwrap_or_else(|| format!("Hut {}", p.osm_id));
        (name, p.lat, p.lon, "hut".into(), p.icon_key.clone())
    };
    for cat in [
        PoiCategory::NetworkHut,
        PoiCategory::Cabin,
        PoiCategory::OvernightFacility,
    ] {
        let mut all: Vec<&PoiRecord> = poi.nearest(cat, lat, lon, hut_radius_m);
        sort_pois_near_sample(&mut all, lat, lon);
        let linked: Vec<&PoiRecord> = all
            .iter()
            .copied()
            .filter(|p| route_link.within_road_link(p.lat, p.lon) && overnight_ok(p))
            .collect();
        if let Some(p) = first_named(&linked) {
            return pick_hut(p);
        }
        let mut best_unnamed: Option<&PoiRecord> = None;
        for p in all {
            if !overnight_ok(p) {
                continue;
            }
            if route_link.within_road_link(p.lat, p.lon) {
                continue;
            }
            if route_link.distance_m(p.lat, p.lon) > 800.0 {
                continue;
            }
            if !reachable_without_barrier(graph, barriers, lat, lon, p.lat, p.lon) {
                continue;
            }
            if p.name.as_ref().is_some_and(|n| !n.trim().is_empty()) {
                return pick_hut(p);
            }
            if best_unnamed.is_none() {
                best_unnamed = Some(p);
            }
        }
        if let Some(p) = best_unnamed {
            return pick_hut(p);
        }
    }
    let mut tents: Vec<&PoiRecord> =
        poi.nearest(PoiCategory::TentSite, lat, lon, hut_radius_m * 1.5);
    sort_pois_near_sample(&mut tents, lat, lon);
    let linked: Vec<&PoiRecord> = tents
        .iter()
        .copied()
        .filter(|p| route_link.within_road_link(p.lat, p.lon) && overnight_ok(p))
        .collect();
    if let Some(p) = first_named(&linked) {
        let name = p.name.clone().unwrap_or_else(|| "Tent site".into());
        return (name, p.lat, p.lon, "tent".into(), p.icon_key.clone());
    }
    let mut best_unnamed: Option<&PoiRecord> = None;
    for p in tents {
        if !overnight_ok(p) {
            continue;
        }
        if route_link.within_road_link(p.lat, p.lon) {
            continue;
        }
        if route_link.distance_m(p.lat, p.lon) > 800.0 {
            continue;
        }
        if !reachable_without_barrier(graph, barriers, lat, lon, p.lat, p.lon) {
            continue;
        }
        if p.name.as_ref().is_some_and(|n| !n.trim().is_empty()) {
            let name = p.name.clone().unwrap_or_else(|| "Tent site".into());
            return (name, p.lat, p.lon, "tent".into(), p.icon_key.clone());
        }
        if best_unnamed.is_none() {
            best_unnamed = Some(p);
        }
    }
    if let Some(p) = best_unnamed {
        let name = p.name.clone().unwrap_or_else(|| "Tent site".into());
        return (name, p.lat, p.lon, "tent".into(), p.icon_key.clone());
    }
    // Synthetic corridor tent: reject if overnight filter forbids it at the sample.
    if let Some((safety, prox)) = overnight {
        let synthetic = PoiRecord {
            osm_id: 0,
            lat,
            lon,
            categories: vec![PoiCategory::TentSite],
            icon_key: "shelter".into(),
            tags: Default::default(),
            name: Some("Tent site".into()),
        };
        if let Some(reason) = check_overnight_candidate(
            lat,
            lon,
            safety,
            &synthetic,
            &prox.buildings,
            &prox.glacier_rings,
        ) {
            return (
                reason.user_message().to_string(),
                lat,
                lon,
                "tent".into(),
                "shelter".into(),
            );
        }
    }
    (
        "Tent site".into(),
        lat,
        lon,
        "tent".into(),
        "shelter".into(),
    )
}

/// Prefer amenities linked to the planned road; else reachable without ferry/
/// lake-scale detour or crow-flies across railway/highway/river; else synthetic.
/// When `require_road_link` is true (car / truck / mobile home / motorcycle), only
/// corridor-linked POIs are accepted — no off-road crow-flies fallback.
fn pick_motor_pause_at(
    poi: &PoiIndex,
    graph: &RouteGraph,
    barriers: &DangerBarrierIndex,
    route_link: &RoadNodeIndex,
    lat: f64,
    lon: f64,
    search_radius_m: f64,
    require_road_link: bool,
) -> (String, f64, f64, String, String) {
    let mut by_id: std::collections::HashMap<i64, &PoiRecord> = std::collections::HashMap::new();
    for cat in [
        PoiCategory::RestArea,
        PoiCategory::General,
        PoiCategory::Restroom,
        PoiCategory::OvernightFacility,
        PoiCategory::Cabin,
    ] {
        for p in poi.nearest(cat, lat, lon, search_radius_m) {
            by_id.entry(p.osm_id).or_insert(p);
        }
    }
    let mut all: Vec<&PoiRecord> = by_id.into_values().collect();
    // Prefer RestArea ahead of generic amenities at the same distance.
    all.sort_by(|a, b| {
        let ar = a.categories.contains(&PoiCategory::RestArea);
        let br = b.categories.contains(&PoiCategory::RestArea);
        match (ar, br) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        }
    });
    // Secondary sort by distance to sample (stable-ish with RestArea preference).
    let mut rest: Vec<&PoiRecord> = all
        .iter()
        .copied()
        .filter(|p| p.categories.contains(&PoiCategory::RestArea))
        .collect();
    let mut other: Vec<&PoiRecord> = all
        .iter()
        .copied()
        .filter(|p| !p.categories.contains(&PoiCategory::RestArea))
        .collect();
    sort_pois_near_sample(&mut rest, lat, lon);
    sort_pois_near_sample(&mut other, lat, lon);
    rest.append(&mut other);
    let all = rest;

    let pick = |p: &PoiRecord| -> (String, f64, f64, String, String) {
        let kind = if p.categories.contains(&PoiCategory::RestArea) {
            "rest_area"
        } else if p.categories.contains(&PoiCategory::Restroom) {
            "amenity"
        } else if p.categories.contains(&PoiCategory::OvernightFacility)
            || p.categories.contains(&PoiCategory::Cabin)
        {
            "hut"
        } else {
            "amenity"
        };
        let name = p
            .name
            .clone()
            .unwrap_or_else(|| format!("Stop {}", p.osm_id));
        (name, p.lat, p.lon, kind.into(), p.icon_key.clone())
    };

    let linked: Vec<&PoiRecord> = all
        .iter()
        .copied()
        .filter(|p| route_link.within_road_link(p.lat, p.lon))
        .collect();
    if let Some(p) = first_named(&linked) {
        return pick(p);
    }
    if require_road_link {
        // Motor profiles: POI must sit on the road network / planned corridor.
        if let Some(p) = linked.first().copied() {
            return pick(p);
        }
        return (
            "Rest stop".into(),
            lat,
            lon,
            "amenity".into(),
            "fuel".into(),
        );
    }
    let mut best_unnamed: Option<&PoiRecord> = None;
    for p in all {
        if route_link.within_road_link(p.lat, p.lon) {
            continue;
        }
        // Keep off-corridor fallbacks near the planned road — otherwise pins land
        // kilometres away from both the route overlay and the real stop.
        if route_link.distance_m(p.lat, p.lon) > 800.0 {
            continue;
        }
        if !reachable_without_barrier(graph, barriers, lat, lon, p.lat, p.lon) {
            continue;
        }
        if p.name.as_ref().is_some_and(|n| !n.trim().is_empty()) {
            return pick(p);
        }
        if best_unnamed.is_none() {
            best_unnamed = Some(p);
        }
    }
    if let Some(p) = best_unnamed {
        return pick(p);
    }
    (
        "Rest stop".into(),
        lat,
        lon,
        "amenity".into(),
        "fuel".into(),
    )
}

/// Break POIs prefer candidates linked to the planned route network
/// ([`RoadNodeIndex::MAX_LINK_M`]); otherwise fall back to POIs reachable
/// without ferry / lake-scale detours / crow-flies across dangerous barriers
/// unless `require_road_link` is set (motor profiles).
/// `hiking` selects hut vs motor categories.
fn build_break_pois_json(
    poi: &PoiIndex,
    polyline: &str,
    interval_km: f64,
    search_radius_m: f64,
    graph: &RouteGraph,
    barriers: &DangerBarrierIndex,
    route_path: &[NodeId],
    hiking: bool,
    overnight: Option<&(SafetyConfig, OvernightProximityIndex)>,
    require_road_link: bool,
) -> String {
    let samples = sample_polyline_km(polyline);
    if samples.len() < 2 {
        return "[]".into();
    }
    let route_link = RoadNodeIndex::from_path_nodes(graph, route_path);
    let total = samples.last().map(|s| s.2).unwrap_or(0.0);
    let mut stops = Vec::new();
    let mut next = interval_km;
    while next < total - 0.5 {
        let (lat, lon) = interpolate_at_km(&samples, next);
        let (name, plat, plon, kind, icon) = if hiking {
            pick_hiking_pause_at(
                poi,
                graph,
                barriers,
                &route_link,
                lat,
                lon,
                search_radius_m,
                overnight,
            )
        } else {
            pick_motor_pause_at(
                poi,
                graph,
                barriers,
                &route_link,
                lat,
                lon,
                search_radius_m,
                require_road_link,
            )
        };
        // Avoid stacking duplicate names within ~2 km.
        let dup = stops.iter().any(|s: &serde_json::Value| {
            let slat = s["lat"].as_f64().unwrap_or(0.0);
            let slon = s["lon"].as_f64().unwrap_or(0.0);
            haversine_m(plat, plon, slat, slon) < 2_000.0
        });
        if !dup {
            stops.push(json!({
                "name": name,
                "lat": plat,
                "lon": plon,
                "kind": kind,
                "icon": icon,
            }));
        }
        next += interval_km;
    }
    serde_json::to_string(&stops).unwrap_or_else(|_| "[]".into())
}

#[derive(Clone, Debug)]
struct HikingWp {
    name: String,
    lat: f64,
    lon: f64,
}

#[derive(Clone, Debug)]
struct HikingAutoVia {
    name: String,
    lat: f64,
    lon: f64,
    along_km: f64,
}

/// Relative extra length vs the containing user leg (paired with the cabin-radius floor).
const HIKING_AUTO_VIA_MAX_EXTRA_FRAC: f64 = 0.15;
/// Treat a candidate as the same place as a user waypoint within this distance.
const HIKING_AUTO_VIA_DEDUP_M: f64 = 500.0;

fn is_named_hiking_hut_pause(kind: &str, name: &str) -> bool {
    if kind != "hut" && kind != "network_hut" {
        return false;
    }
    let t = name.trim();
    if t.is_empty() {
        return false;
    }
    // Synthetic / unnamed OSM cabin labels from pick_hiking_pause_at.
    if t.starts_with("Hut ") && t[4..].chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    true
}

/// Extra path metres allowed when promoting a rast hut to a via.
/// Floor comes from the Drive POI cabin-radius slider; also allow 15% of the leg.
fn auto_via_extra_allowed_m(leg_m: f64, cabin_radius_m: f64) -> f64 {
    cabin_radius_m
        .max(0.0)
        .max(leg_m * HIKING_AUTO_VIA_MAX_EXTRA_FRAC)
}

/// Shortest-path length (m) between two snapped coordinates, or None if unreachable.
fn snapped_path_length_m(
    graph: &RouteGraph,
    a_lat: f64,
    a_lon: f64,
    b_lat: f64,
    b_lon: f64,
) -> Option<f64> {
    let (s, _) = nearest(graph, a_lat, a_lon).ok()?;
    let (g, _) = nearest(graph, b_lat, b_lon).ok()?;
    let (path, _, _) = graph.shortest_path(s, g, false)?;
    if path.len() < 2 {
        return Some(0.0);
    }
    Some(path_length_m(graph, &path))
}

/// On-trail-only path through waypoints (snap-bounded graph A*; no terrain).
fn hike_path_through_waypoints(
    graph: &RouteGraph,
    wps: &[HikingWp],
) -> Result<(Vec<NodeId>, f64, Vec<f64>), String> {
    if wps.len() < 2 {
        return Err("need at least start and end waypoints".into());
    }
    let mut full_path: Vec<NodeId> = Vec::new();
    let mut distance_m = 0.0;
    let mut cum_km = vec![0.0];
    let profile = graph.profile();
    for pair in wps.windows(2) {
        let (s, _) = nearest(graph, pair[0].lat, pair[0].lon).map_err(|e| {
            format_snap_too_far(&format!("waypoint \"{}\"", pair[0].name), e, profile)
        })?;
        let (g, _) = nearest(graph, pair[1].lat, pair[1].lon).map_err(|e| {
            format_snap_too_far(&format!("waypoint \"{}\"", pair[1].name), e, profile)
        })?;
        let Some((path, _, _cost)) = graph.shortest_path(s, g, false) else {
            if driver_break_core::download::plan_cancel::is_cancelled() {
                return Err("cancelled".into());
            }
            return Err(format!(
                "no foot route {} -> {}",
                pair[0].name, pair[1].name
            ));
        };
        if path.len() < 2 {
            return Err(format!(
                "zero-length leg {} -> {}",
                pair[0].name, pair[1].name
            ));
        }
        let mut leg_m = 0.0;
        for w in path.windows(2) {
            if let Some(idx) = graph.edge_index(w[0], w[1]) {
                leg_m += graph.edges[idx].length_m;
            }
        }
        distance_m += leg_m;
        cum_km.push(distance_m / 1000.0);
        if full_path.is_empty() {
            full_path.extend(path);
        } else {
            full_path.extend(path.into_iter().skip(1));
        }
    }
    Ok((full_path, distance_m, cum_km))
}

fn to_hiking_waypoints(wps: &[HikingWp]) -> Vec<HikingWaypoint> {
    wps.iter()
        .map(|w| HikingWaypoint {
            name: w.name.clone(),
            lat: w.lat,
            lon: w.lon,
        })
        .collect()
}

fn user_cum_km_from_hybrid(
    hybrid: &driver_break_core::routing::HybridHikingPath,
    wps: &[HikingWp],
) -> Vec<f64> {
    let mut cum = vec![0.0];
    if wps.len() < 2 {
        return cum;
    }
    let coords = hybrid.full_coords();
    if coords.is_empty() {
        return vec![0.0; wps.len()];
    }
    let mut along = 0.0;
    let mut ci = 0usize;
    for wp in wps.iter().skip(1) {
        let mut best_i = ci;
        let mut best_d = f64::INFINITY;
        for (i, &(lat, lon)) in coords.iter().enumerate().skip(ci) {
            let d = haversine_m(wp.lat, wp.lon, lat, lon);
            if d < best_d {
                best_d = d;
                best_i = i;
            }
        }
        for w in coords[ci..=best_i].windows(2) {
            along += haversine_m(w[0].0, w[0].1, w[1].0, w[1].1);
        }
        ci = best_i;
        cum.push(along / 1000.0);
    }
    cum
}

fn waypoint_dupes_auto_via(wps: &[HikingWp], name: &str, lat: f64, lon: f64) -> bool {
    wps.iter().any(|w| {
        w.name.eq_ignore_ascii_case(name)
            || haversine_m(w.lat, w.lon, lat, lon) < HIKING_AUTO_VIA_DEDUP_M
    })
}

/// Merge user waypoints with accepted auto-vias ordered by along-route km.
fn merge_hiking_waypoints_with_auto_vias(
    user: &[HikingWp],
    user_cum_km: &[f64],
    autos: &[HikingAutoVia],
) -> Vec<HikingWp> {
    debug_assert_eq!(user.len(), user_cum_km.len());
    let mut out: Vec<(f64, HikingWp)> = user
        .iter()
        .zip(user_cum_km.iter())
        .map(|(w, &km)| (km, w.clone()))
        .collect();
    for a in autos {
        if out.iter().any(|(_, w)| {
            w.name.eq_ignore_ascii_case(&a.name)
                || haversine_m(w.lat, w.lon, a.lat, a.lon) < HIKING_AUTO_VIA_DEDUP_M
        }) {
            continue;
        }
        out.push((
            a.along_km,
            HikingWp {
                name: a.name.clone(),
                lat: a.lat,
                lon: a.lon,
            },
        ));
    }
    out.sort_by(|a, b| {
        a.0.partial_cmp(&b.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.name.cmp(&b.1.name))
    });
    out.into_iter().map(|(_, w)| w).collect()
}

/// Named cabin / network hut candidates near a rast sample for auto-via promotion.
/// Prefers path-linked (or ≤800 m) candidates; otherwise allows up to
/// `lateral_max_m` (Drive cabin-radius slider) when the hut remains graph-reachable.
///
/// When `use_networked_cabins` is false, DNT/STF/… network huts are excluded from
/// auto-via candidacy (open cabins / overnight facilities remain eligible). This is
/// a geographic via filter only — it does not grant or imply hut access rights.
fn hiking_auto_via_hut_candidates_at(
    poi: &PoiIndex,
    graph: &RouteGraph,
    barriers: &DangerBarrierIndex,
    route_link: &RoadNodeIndex,
    lat: f64,
    lon: f64,
    search_radius_m: f64,
    lateral_max_m: f64,
    use_networked_cabins: bool,
) -> Vec<(String, f64, f64)> {
    let mut scored: Vec<(f64, f64, i64, String, f64, f64)> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let search_m = search_radius_m.max(500.0);
    let lateral_m = lateral_max_m.max(500.0);
    let categories: &[PoiCategory] = if use_networked_cabins {
        &[
            PoiCategory::NetworkHut,
            PoiCategory::Cabin,
            PoiCategory::OvernightFacility,
        ]
    } else {
        &[PoiCategory::Cabin, PoiCategory::OvernightFacility]
    };
    for cat in categories {
        for p in poi.nearest(*cat, lat, lon, search_m) {
            if !seen.insert(p.osm_id) {
                continue;
            }
            let is_network = p.categories.contains(&PoiCategory::NetworkHut);
            if is_network && !use_networked_cabins {
                continue;
            }
            let Some(name) = p.name.as_ref().map(|n| n.trim()).filter(|n| !n.is_empty()) else {
                continue;
            };
            if !is_named_hiking_hut_pause("hut", name) {
                continue;
            }
            let sample_dist = haversine_m(lat, lon, p.lat, p.lon);
            let lateral = route_link.distance_m(p.lat, p.lon);
            if lateral > lateral_m {
                continue;
            }
            let linked = route_link.within_road_link(p.lat, p.lon) || lateral <= 800.0;
            if !linked && !reachable_without_barrier(graph, barriers, lat, lon, p.lat, p.lon) {
                continue;
            }
            // Prefer nearer to the rast sample (not merely on-path), then network, then lateral.
            let rank = sample_dist
                + if is_network { 0.0 } else { 500.0 }
                + if linked { 0.0 } else { 200.0 };
            scored.push((rank, sample_dist, p.osm_id, name.to_string(), p.lat, p.lon));
        }
    }
    scored.sort_by(|a, b| {
        a.0.partial_cmp(&b.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    });
    scored
        .into_iter()
        .map(|(_, _, _, name, lat, lon)| (name, lat, lon))
        .collect()
}

/// Rast-interval named hut candidates promoted to vias when the detour fits the
/// Drive POI cabin-radius slider (lateral + absolute extra path).
fn collect_hiking_auto_vias(
    poi: &PoiIndex,
    graph: &RouteGraph,
    barriers: &DangerBarrierIndex,
    route_path: &[NodeId],
    polyline: &str,
    user_wps: &[HikingWp],
    user_cum_km: &[f64],
    cabin_radius_m: f64,
    search_radius_m: f64,
    use_networked_cabins: bool,
    report: &mut String,
) -> Vec<HikingAutoVia> {
    let samples = sample_polyline_km(polyline);
    if samples.len() < 2 || user_wps.len() < 2 || user_cum_km.len() != user_wps.len() {
        return Vec::new();
    }
    let route_link = RoadNodeIndex::from_path_nodes(graph, route_path);
    let total = samples.last().map(|s| s.2).unwrap_or(0.0);
    let search_m = search_radius_m.max(cabin_radius_m);
    let lateral_m = cabin_radius_m;
    report.push_str(&format!(
        "auto_via_limits: search_m={search_m:.0}; lateral_m={lateral_m:.0}; extra_floor_m={cabin_radius_m:.0}; extra_frac={HIKING_AUTO_VIA_MAX_EXTRA_FRAC}; use_networked_cabins={use_networked_cabins}\n"
    ));
    let mut autos = Vec::new();
    let mut next = HIKING_MAIN_BREAK_DISTANCE_KM;
    while next < total - 0.5 {
        let (lat, lon) = interpolate_at_km(&samples, next);
        let candidates = hiking_auto_via_hut_candidates_at(
            poi,
            graph,
            barriers,
            &route_link,
            lat,
            lon,
            search_m,
            lateral_m,
            use_networked_cabins,
        );

        // Containing user leg for detour check.
        let mut leg_i = 0usize;
        for i in 0..user_cum_km.len().saturating_sub(1) {
            if next <= user_cum_km[i + 1] + 1e-6 {
                leg_i = i;
                break;
            }
            leg_i = i;
        }
        let a = &user_wps[leg_i];
        let b = &user_wps[leg_i + 1];
        let Some(orig_m) = snapped_path_length_m(graph, a.lat, a.lon, b.lat, b.lon) else {
            next += HIKING_MAIN_BREAK_DISTANCE_KM;
            continue;
        };
        let allowed = auto_via_extra_allowed_m(orig_m, cabin_radius_m);

        for (name, plat, plon) in candidates {
            if waypoint_dupes_auto_via(user_wps, &name, plat, plon) {
                continue;
            }
            let dup_auto = autos.iter().any(|x: &HikingAutoVia| {
                x.name.eq_ignore_ascii_case(&name)
                    || haversine_m(x.lat, x.lon, plat, plon) < 2_000.0
            });
            if dup_auto {
                continue;
            }
            let Some(via_a) = snapped_path_length_m(graph, a.lat, a.lon, plat, plon) else {
                report.push_str(&format!(
                    "auto_via_skip={name}; reason=no_path_to_hut; along_km={next:.1}\n"
                ));
                continue;
            };
            let Some(via_b) = snapped_path_length_m(graph, plat, plon, b.lat, b.lon) else {
                report.push_str(&format!(
                    "auto_via_skip={name}; reason=no_path_from_hut; along_km={next:.1}\n"
                ));
                continue;
            };
            let extra = via_a + via_b - orig_m;
            if extra > allowed {
                report.push_str(&format!(
                    "auto_via_skip={name}; reason=detour; extra_m={extra:.0}; allowed_m={allowed:.0}; along_km={next:.1}\n"
                ));
                continue;
            }
            autos.push(HikingAutoVia {
                name,
                lat: plat,
                lon: plon,
                along_km: next,
            });
            break; // one via per rast sample
        }
        next += HIKING_MAIN_BREAK_DISTANCE_KM;
    }
    autos
}

/// Real on-device corridor pipeline (Espa -> Atnbrufossen).
///
/// Requires a real OSM `.pbf` already present (use [`provision_region_data`] first).
/// Runs the pipeline twice to assert graph-cache speedup. Report always includes
/// `TEST_KIND=REAL_PIPELINE` and `DATA_SOURCE=real_pbf`.
#[uniffi::export]
pub fn run_car_corridor_pipeline(
    pbf_path: String,
    elev_dir: String,
    cache_dir: String,
    break_interval_hours: f64,
) -> CorridorRouteResult {
    let empty = empty_corridor;

    let plan = WorkerPoolPlan::detect();
    WorkerPoolPlan::lower_current_thread_priority();
    let _ = plan.install_rayon_pool();

    let mut report = String::new();
    report.push_str("TEST_KIND=REAL_PIPELINE\nDATA_SOURCE=real_pbf\n");
    report.push_str(&format!(
        "detected_cores={}; routing_workers={}; reserved={}\nbreak_interval_hours={:.2}\n",
        plan.detected_cores, plan.routing_workers, plan.reserved_for_ui_audio, break_interval_hours
    ));

    let pbf = Path::new(pbf_path.trim());
    if pbf_path.trim().is_empty() || !pbf.is_file() {
        report.push_str(&format!(
            "FAIL: required OSM PBF missing or not a file: \"{pbf_path}\"\n"
        ));
        return empty(report);
    }
    let pbf_len = std::fs::metadata(pbf).map(|m| m.len()).unwrap_or(0);
    if pbf_len < 1_000_000 {
        report.push_str(&format!(
            "FAIL: PBF too small ({pbf_len} bytes); refusing empty/stub input\n"
        ));
        return empty(report);
    }
    report.push_str(&format!("pbf={}; pbf_bytes={}\n", pbf.display(), pbf_len));

    let elev = PathBuf::from(&elev_dir);
    let cache = PathBuf::from(&cache_dir);
    let _ = std::fs::create_dir_all(&cache);
    let eco = passat_eco();
    let elevation = ElevationService::new(ElevationCache::new(&elev));
    let _ = elevation.warm_bbox([60.35, 9.95, 62.05, 11.65]);

    // Phase 4 M5: `.navigph` removed. Warm path is indexed packs when present.
    let data_dir = pbf
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let t_cold = Instant::now();
    let (mut graph, pack_hit) = match driver_break_core::routing::indexed::try_load_graph_for_plan(
        &data_dir,
        pbf,
        RoutingProfile::Car,
    ) {
        Ok(g) => (g, true),
        Err(_) => {
            match load_or_build_reweighted(pbf, &cache, RoutingProfile::Car, &elevation, &eco) {
                Ok((g, _)) => (g, false),
                Err(e) => {
                    report.push_str(&format!("FAIL: cold graph build: {e:#}\n"));
                    return empty(report);
                }
            }
        }
    };
    let cold_s = t_cold.elapsed().as_secs_f64();
    report.push_str(&format!(
        "cold_build_s={cold_s:.2}; pack_hit={pack_hit}; nodes={}; edges={}\n",
        graph.nodes.len(),
        graph.edges.len()
    ));
    if graph.edges.is_empty() {
        report.push_str("FAIL: degenerate empty graph\n");
        return empty(report);
    }

    let t_warm = Instant::now();
    let (graph2, pack_hit2) = match driver_break_core::routing::indexed::try_load_graph_for_plan(
        &data_dir,
        pbf,
        RoutingProfile::Car,
    ) {
        Ok(g) => (g, true),
        Err(_) => {
            match load_or_build_reweighted(pbf, &cache, RoutingProfile::Car, &elevation, &eco) {
                Ok((g, _)) => (g, false),
                Err(e) => {
                    report.push_str(&format!("FAIL: warm graph load: {e:#}\n"));
                    return empty(report);
                }
            }
        }
    };
    let warm_s = t_warm.elapsed().as_secs_f64();
    report.push_str(&format!("warm_load_s={warm_s:.2}; pack_hit={pack_hit2}\n"));
    if pack_hit && pack_hit2 {
        if warm_s >= cold_s * 0.85 && cold_s > 2.0 {
            report.push_str(&format!(
                "FAIL: warm pack load ({warm_s:.1}s) not meaningfully faster than cold ({cold_s:.1}s)\n"
            ));
            return empty(report);
        }
    } else if !pack_hit {
        report.push_str(
            "NOTE: no indexed packs; `.navigph` deprecated — both passes rebuild from PBF\n",
        );
    }
    // Prefer the second load for routing.
    graph = graph2;
    let hit2 = pack_hit2;

    let start_lat = 60.562_191_4;
    let start_lon = 11.256_123_9;
    let end_lat = 61.851_250_0;
    let end_lon = 10.233_842_0;

    let (s, snap_start_m) = match nearest(&graph, start_lat, start_lon) {
        Ok(v) => v,
        Err(e) => {
            report.push_str(&format!(
                "FAIL: {}\n",
                format_snap_too_far("start", e, graph.profile())
            ));
            return empty(report);
        }
    };
    let (g, snap_end_m) = match nearest(&graph, end_lat, end_lon) {
        Ok(v) => v,
        Err(e) => {
            report.push_str(&format!(
                "FAIL: {}\n",
                format_snap_too_far("destination", e, graph.profile())
            ));
            return empty(report);
        }
    };
    report.push_str(&format!(
        "snap_start_m={snap_start_m:.0}; snap_end_m={snap_end_m:.0}; snap_max_m={:.0}\n",
        max_waypoint_snap_m(graph.profile())
    ));
    let Some((path, _, cost)) = graph.shortest_path(s, g, true) else {
        report.push_str("FAIL: no route\n");
        return empty(report);
    };
    if path.len() < 2 {
        report.push_str("FAIL: zero-length route\n");
        return empty(report);
    }

    let mut distance_m = 0.0;
    let mut polyline = String::new();
    for (i, w) in path.windows(2).enumerate() {
        if let Some(idx) = graph.edge_index(w[0], w[1]) {
            distance_m += graph.edges[idx].length_m;
        }
        let n = &graph.nodes[&w[0]];
        if i == 0 {
            polyline.push_str(&format!("{},{}", n.coord.x, n.coord.y));
        }
        let n1 = &graph.nodes[&w[1]];
        // Prefer full edge shape when present (junction-only path is a poor overlay).
        if let Some(idx) = graph.edge_index(w[0], w[1]) {
            for &(lon, lat) in &graph.edges[idx].shape {
                polyline.push_str(&format!(";{lon},{lat}"));
            }
        }
        polyline.push_str(&format!(";{},{}", n1.coord.x, n1.coord.y));
    }
    if let Some(last) = path.last() {
        let n = &graph.nodes[last];
        let tail = format!("{},{}", n.coord.x, n.coord.y);
        if !polyline.ends_with(&tail) {
            polyline.push_str(&format!(";{tail}"));
        }
    }

    let dist_km = distance_m / 1000.0;
    let eta_minutes = motor_path_minutes(&graph, &path);
    let avg_speed = 90.0;
    let duration_h = dist_km / avg_speed;
    let mut rest = RestConfig::default();
    rest.car.break_interval_min_hours = break_interval_hours;
    rest.car.break_interval_max_hours = break_interval_hours;
    let (_min_h, max_h) = car_break_interval_hours(&rest);
    let breaks = car_required_breaks(duration_h, max_h);
    let break_at_km = avg_speed * max_h;

    // POI index from same PBF — pick a fuel/amenity near mid-route for the map marker.
    let poi_index = match PoiIndex::load_from_pbf(pbf) {
        Ok(i) => i,
        Err(e) => {
            report.push_str(&format!("FAIL: POI index: {e:#}\n"));
            return empty(report);
        }
    };
    report.push_str(&format!("poi_records={}\n", poi_index.len()));
    let mid = path.get(path.len() / 2).and_then(|id| graph.nodes.get(id));
    let (mut poi_lat, mut poi_lon, mut poi_name, mut poi_icon) =
        (0.0, 0.0, String::from("none"), String::from("fuel"));
    if let Some(m) = mid {
        for cat in [
            PoiCategory::General,
            PoiCategory::Water,
            PoiCategory::Cabin,
            PoiCategory::Restroom,
        ] {
            let hits = poi_index.nearest(cat, m.coord.y, m.coord.x, 15_000.0);
            if let Some(p) = hits.first() {
                poi_lat = p.lat;
                poi_lon = p.lon;
                poi_name = p
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("POI {}", p.osm_id));
                poi_icon = p.icon_key.clone();
                break;
            }
        }
    }
    report.push_str(&format!(
        "map_poi={poi_name:?} ({poi_lat:.5},{poi_lon:.5}) icon={poi_icon}\n"
    ));

    let break_pois_json = build_break_pois_json(
        &poi_index,
        &polyline,
        break_at_km.max(15.0),
        12_000.0,
        &graph,
        &load_break_barriers(&graph, pbf, [60.35, 9.95, 62.05, 11.65])
            .unwrap_or_else(|_| DangerBarrierIndex::from_graph(&graph)),
        &path,
        false,
        None,
        true, // car corridor: require road-linked stops
    );
    report.push_str(&format!("break_pois={break_pois_json}\n"));

    report.push_str(&format!(
        "distance_km={dist_km:.2}; distance_m={distance_m:.0}; path_cost={cost:.0}; duration_h={duration_h:.2}; eta_min={eta_minutes:.1}\n\
         required_breaks={breaks}; break_at_km={break_at_km:.1}\n\
         route_points={}\n",
        path.len()
    ));

    // Corridor-class check: full Espa-Atnbrufossen is ~190 km; small extracts may be shorter
    // but must still produce a real multi-edge route.
    if dist_km < 5.0 {
        report.push_str(&format!(
            "FAIL: corridor distance too short ({dist_km:.1} km)\n"
        ));
        return empty(report);
    }
    if break_interval_hours <= 1.01 && duration_h > 1.5 && breaks < 1 {
        report.push_str(&format!(
            "FAIL: expected >=1 break with {break_interval_hours} h interval, got {breaks}\n"
        ));
        return empty(report);
    }

    report.push_str("PASS\n");
    let priority_path_share_pct = graph.non_motorway_share_pct(&path);
    report.push_str(&format!(
        "priority_path_share_pct={priority_path_share_pct:.2}; motorway_share_pct={:.2}\n",
        100.0 - priority_path_share_pct
    ));
    CorridorRouteResult {
        report,
        distance_km: dist_km,
        eta_minutes,
        cache_hit: hit2,
        cold_build_s: cold_s,
        warm_load_s: warm_s,
        route_polyline: polyline,
        poi_lat,
        poi_lon,
        poi_name,
        poi_icon_key: poi_icon,
        break_pois_json,
        days_json: String::from("[]"),
        sim_samples_json: String::from("[]"),
        maneuvers_json: String::from("[]"),
        priority_path_share_pct,
        route_segments_json: String::from("[]"),
        off_trail_advisory: String::new(),
    }
}

/// Directory that holds `{stem}.navi-manifest.json` and indexed packs.
/// Empty string: the planning PBF's parent (hosts that co-locate extract + packs).
fn plan_pack_data_dir(pbf: &Path, data_dir: &str) -> PathBuf {
    let trimmed = data_dir.trim();
    if trimmed.is_empty() {
        pbf.parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    } else {
        PathBuf::from(trimmed)
    }
}

/// Plan a motor / bicycle route between two WGS84 points using a local OSM `.pbf`.
///
/// Always builds a **bbox-clipped** graph (`[min_lat,min_lon,max_lat,max_lon]` padded
/// around the endpoints) so truck / mobile-home / motorcycle / bicycle never load a
/// full Ostlandet extract into RAM. Hiking uses [`plan_hiking_route`] instead.
///
/// [`TravelProfile::Hiking`] is rejected (call [`plan_hiking_route`]).
///
/// `data_dir` is the app data directory for pack/manifest lookup. It is **not**
/// inferred from `pbf_path` (a fixture clone of the same extract must not send
/// lookup to a directory with no packs). Pass `""` only when the PBF already
/// lives next to the packs.
#[uniffi::export]
pub fn plan_car_route(
    pbf_path: String,
    elev_dir: String,
    cache_dir: String,
    start_lat: f64,
    start_lon: f64,
    end_lat: f64,
    end_lon: f64,
    use_eco: bool,
    profile: TravelProfile,
    avoid_motorways: bool,
    avoid_tolls: bool,
    avoid_ferries: bool,
    vehicle: FfiVehicleLimits,
    prefer_official_networks: bool,
    data_dir: String,
) -> CorridorRouteResult {
    plan_car_route_at(
        pbf_path,
        elev_dir,
        cache_dir,
        start_lat,
        start_lon,
        end_lat,
        end_lon,
        use_eco,
        profile,
        avoid_motorways,
        avoid_tolls,
        avoid_ferries,
        vehicle,
        prefer_official_networks,
        None,
        data_dir,
    )
}

/// Same as [`plan_car_route`], with optional local departure for seasonal closures.
///
/// `departure_local_iso` accepts `YYYY-MM-DDTHH:MM:SS` (no timezone). When `None`,
/// the planner uses the device local clock (same as [`plan_car_route`]).
#[uniffi::export]
pub fn plan_car_route_at(
    pbf_path: String,
    elev_dir: String,
    cache_dir: String,
    start_lat: f64,
    start_lon: f64,
    end_lat: f64,
    end_lon: f64,
    use_eco: bool,
    profile: TravelProfile,
    avoid_motorways: bool,
    avoid_tolls: bool,
    avoid_ferries: bool,
    vehicle: FfiVehicleLimits,
    prefer_official_networks: bool,
    departure_local_iso: Option<String>,
    data_dir: String,
) -> CorridorRouteResult {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ch = driver_break_core::download::progress::ChannelGuard::enter(
            driver_break_core::download::progress::ProgressChannel::Plan,
        );
        plan_car_route_inner(
            pbf_path,
            elev_dir,
            cache_dir,
            start_lat,
            start_lon,
            end_lat,
            end_lon,
            use_eco,
            profile,
            avoid_motorways,
            avoid_tolls,
            avoid_ferries,
            vehicle,
            prefer_official_networks,
            departure_local_iso,
            data_dir,
        )
    })) {
        Ok(result) => result,
        Err(_) => empty_corridor(
            "TEST_KIND=PLAN_CAR_ROUTE\nFAIL: native panic during plan_car_route\n".into(),
        ),
    }
}

fn parse_departure_local(iso: Option<&str>) -> Option<chrono::NaiveDateTime> {
    let s = iso?.trim();
    if s.is_empty() {
        return None;
    }
    chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S"))
        .ok()
}

fn plan_car_route_inner(
    pbf_path: String,
    elev_dir: String,
    cache_dir: String,
    start_lat: f64,
    start_lon: f64,
    end_lat: f64,
    end_lon: f64,
    use_eco: bool,
    profile: TravelProfile,
    avoid_motorways: bool,
    avoid_tolls: bool,
    avoid_ferries: bool,
    vehicle: FfiVehicleLimits,
    prefer_official_networks: bool,
    departure_local_iso: Option<String>,
    data_dir: String,
) -> CorridorRouteResult {
    let empty = empty_corridor;
    let _cancel_guard = driver_break_core::download::plan_cancel::begin_plan();

    if profile == TravelProfile::Hiking {
        return empty("TEST_KIND=PLAN_CAR_ROUTE\nFAIL: use plan_hiking_route for hiking\n".into());
    }

    let mut timer = PlanStageTimer::start();

    let routing_profile = RoutingProfile::from(profile.to_core());
    let plan = WorkerPoolPlan::detect();
    WorkerPoolPlan::lower_current_thread_priority();
    let _ = plan.install_rayon_pool();

    let vehicle_limits = ffi_vehicle_to_limits(&vehicle);
    let departure_local = parse_departure_local(departure_local_iso.as_deref());
    // Bicycle / e-bike: motorways are illegal or unsuitable — force avoid regardless of UI.
    let avoid_motorways = avoid_motorways
        || driver_break_core::routing::graph::profile_locks_avoid_motorways(routing_profile);
    let route_opts = RouteOptions {
        avoid_motorways,
        avoid_tolls,
        avoid_ferries,
        vehicle: vehicle_limits.clone(),
        departure_local,
    };

    let mut report = String::new();
    report.push_str("TEST_KIND=PLAN_CAR_ROUTE\nDATA_SOURCE=real_pbf\n");
    report.push_str(&format!(
        "profile={profile:?}; routing={routing_profile:?}; start={start_lat:.6},{start_lon:.6}; end={end_lat:.6},{end_lon:.6}; use_eco={use_eco}\n"
    ));
    report.push_str(&format!(
        "avoid_motorways={avoid_motorways}; avoid_tolls={avoid_tolls}; avoid_ferries={avoid_ferries}; vehicle_limits={}\n",
        vehicle_limits.is_some()
    ));
    report.push_str(&format!(
        "eco_regen={:.3}\n",
        eco_for_travel_profile(profile).regen_efficiency
    ));

    let pbf = Path::new(pbf_path.trim());
    if !pbf.is_file() {
        report.push_str(&format!("FAIL: PBF missing: \"{pbf_path}\"\n"));
        return empty(report);
    }
    let elev = PathBuf::from(&elev_dir);
    let cache = PathBuf::from(&cache_dir);
    let _ = std::fs::create_dir_all(&cache);
    let poi_radii = load_profile_poi_radii_near_cache(&cache)
        .for_profile(profile.to_core())
        .clone();
    report.push_str(&format!(
        "poi_search_radius_m={:.0}; cabin_radius_m={:.0}; network_hut_radius_m={:.0}; network_hut_pref_m={:.0}; require_road_link={}\n",
        poi_radii.search_radius_m,
        poi_radii.cabin_radius_m,
        poi_radii.network_hut_radius_m,
        poi_radii.network_hut_preference_radius_m,
        poi_radii.require_road_link
    ));
    let eco = eco_for_travel_profile(profile);
    let elevation = ElevationService::new(ElevationCache::new(&elev));
    let _ = elevation.warm_bbox([
        start_lat.min(end_lat) - 0.05,
        start_lon.min(end_lon) - 0.05,
        start_lat.max(end_lat) + 0.05,
        start_lon.max(end_lon) + 0.05,
    ]);

    // Clip to the trip bbox so we never load a full country graph into RAM
    // (that OOMs 4GB Automotive AVDs). Still reads the same region .pbf.
    // Pad scales with corridor span: a fixed 0.35° is enough for Ostlandet-scale
    // trips but clips long northbound legs (E6 swings west through Trondheim).
    let lat_span = (start_lat - end_lat).abs();
    let lon_span = (start_lon - end_lon).abs();
    let pad = (lat_span.max(lon_span) * 0.35).clamp(0.35, 2.5);
    let bbox = [
        start_lat.min(end_lat) - pad,
        start_lon.min(end_lon) - pad,
        start_lat.max(end_lat) + pad,
        start_lon.max(end_lon) + pad,
    ];
    report.push_str(&format!(
        "bbox={:.3},{:.3},{:.3},{:.3}; pad={pad:.2}\n",
        bbox[0], bbox[1], bbox[2], bbox[3]
    ));
    let profile_map_ms = timer.lap_ms();
    if driver_break_core::download::plan_cancel::is_cancelled() {
        return plan_cancelled_result(report, &timer, &[("profile_map_ms", profile_map_ms)]);
    }

    driver_break_core::download::progress::set(0, Some(5), "Planning route: building area graph…");
    let t_graph = Instant::now();
    let data_dir = plan_pack_data_dir(pbf, &data_dir);
    let pack_try = driver_break_core::routing::indexed::try_load_graph_for_plan_bbox(
        &data_dir,
        pbf,
        routing_profile,
        Some(bbox),
    );
    let _pause_bg = if pack_try.is_err() {
        Some(driver_break_core::download::ForegroundPlanGuard::acquire())
    } else {
        None
    };
    let (mut graph, cache_hit, pack_hit) = match pack_try {
        Ok(g) => (g, false, true),
        Err(_) => match load_or_build_reweighted_bbox(
            pbf,
            &data_dir,
            &cache,
            routing_profile,
            &elevation,
            &eco,
            bbox,
        ) {
            Ok((g, hit)) => (g, hit, false),
            Err(e) if driver_break_core::download::plan_cancel::is_cancel_err(&e) => {
                return plan_cancelled_result(
                    report,
                    &timer,
                    &[("profile_map_ms", profile_map_ms)],
                );
            }
            Err(e) => {
                report.push_str(&format!("FAIL: graph build: {e:#}\n"));
                return empty(report);
            }
        },
    };
    // Pack path skips load_or_build eco; apply DEM reweight when eco is on.
    if pack_hit && use_eco {
        graph.apply_eco_reweighting(&elevation, &eco);
    }
    let build_s = t_graph.elapsed().as_secs_f64();
    let graph_build_ms = timer.lap_ms();
    if driver_break_core::download::plan_cancel::is_cancelled() {
        return plan_cancelled_result(
            report,
            &timer,
            &[
                ("profile_map_ms", profile_map_ms),
                ("graph_build_ms", graph_build_ms),
            ],
        );
    }
    if (profile == TravelProfile::Bicycle || profile == TravelProfile::BicycleElectric)
        && prefer_official_networks
    {
        apply_network_pref_if_requested(
            &mut graph,
            pbf,
            OfficialNetworkKind::Cycling,
            true,
            &mut report,
        );
    }
    if profile == TravelProfile::Bicycle || profile == TravelProfile::BicycleElectric {
        apply_slow_road_preference(&mut graph);
        report.push_str("slow_road_preference=applied; profile=bicycle\n");
        let bike_cap =
            match driver_break_core::storage::Storage::open(routes_db(&data_dir.to_string_lossy()))
            {
                Ok(storage) => {
                    let store = driver_break_core::storage::ConfigStore::new(&storage);
                    BikeCapability::parse(
                        &store
                            .load_bike_capability()
                            .unwrap_or_else(|_| "trekking".to_string()),
                    )
                }
                Err(_) => BikeCapability::Trekking,
            };
        match apply_bike_suitability_from_pbf(&mut graph, pbf, bike_cap) {
            Ok(removed) => {
                report.push_str(&format!(
                    "bike_capability={}; bike_suitability_removed={removed}\n",
                    bike_cap.as_str()
                ));
            }
            Err(e) if driver_break_core::download::plan_cancel::is_cancel_err(&e) => {
                return plan_cancelled_result(
                    report,
                    &timer,
                    &[
                        ("profile_map_ms", profile_map_ms),
                        ("graph_build_ms", graph_build_ms),
                    ],
                );
            }
            Err(e) => report.push_str(&format!("bike_suitability_err={e}\n")),
        }
    }
    if matches!(routing_profile, RoutingProfile::Car | RoutingProfile::Truck) {
        let surface_mode =
            match driver_break_core::storage::Storage::open(routes_db(&data_dir.to_string_lossy()))
            {
                Ok(storage) => {
                    let store = driver_break_core::storage::ConfigStore::new(&storage);
                    SurfaceRoutingMode::parse(
                        &store
                            .load_surface_routing_mode()
                            .unwrap_or_else(|_| "car".to_string()),
                    )
                }
                Err(_) => SurfaceRoutingMode::Car,
            };
        graph.surface_routing_mode = surface_mode;
        let _ = apply_surface_quality_from_pbf(&mut graph, pbf);
        apply_surface_preference(&mut graph, surface_mode);
    }
    let network_pref_ms = timer.lap_ms();
    report.push_str(&format!(
        "build_s={build_s:.2}; cache_hit={cache_hit}; pack_hit={pack_hit}; nodes={}; edges={}\n",
        graph.nodes.len(),
        graph.edges.len()
    ));
    // Emit before snap/A*: winter OD on a seasonally closed mountain road can fail
    // snap, but the pack-hit graph still carries the closures we need to report.
    let seasonal_n = graph.seasonal_closure_excluded_in_graph(&route_opts);
    report.push_str(&format!("seasonal_closure_excluded_edges={seasonal_n}\n"));

    if driver_break_core::download::plan_cancel::is_cancelled() {
        return plan_cancelled_result(
            report,
            &timer,
            &[
                ("profile_map_ms", profile_map_ms),
                ("graph_build_ms", graph_build_ms),
                ("network_pref_ms", network_pref_ms),
            ],
        );
    }

    driver_break_core::download::progress::set(3, Some(5), "Planning route: finding path…");
    let (s, snap_start_m) = match nearest(&graph, start_lat, start_lon) {
        Ok(v) => v,
        Err(e) => {
            report.push_str(&format!(
                "FAIL: {}\n",
                format_snap_too_far("start", e, graph.profile())
            ));
            return empty(report);
        }
    };
    let (g, snap_end_m) = match nearest(&graph, end_lat, end_lon) {
        Ok(v) => v,
        Err(e) => {
            report.push_str(&format!(
                "FAIL: {}\n",
                format_snap_too_far("destination", e, graph.profile())
            ));
            return empty(report);
        }
    };
    {
        let sn = &graph.nodes[&s];
        let gn = &graph.nodes[&g];
        report.push_str(&format!(
            "snap_start={:.6},{:.6} dist_m={snap_start_m:.0}; snap_end={:.6},{:.6} dist_m={snap_end_m:.0}; snap_max_m={:.0}\n",
            sn.coord.y,
            sn.coord.x,
            gn.coord.y,
            gn.coord.x,
            max_waypoint_snap_m(graph.profile())
        ));
    }
    let Some((path, path_edges, cost)) =
        graph.shortest_path_with_options(s, g, use_eco, &route_opts)
    else {
        if driver_break_core::download::plan_cancel::is_cancelled() {
            return plan_cancelled_result(
                report,
                &timer,
                &[
                    ("profile_map_ms", profile_map_ms),
                    ("graph_build_ms", graph_build_ms),
                    ("network_pref_ms", network_pref_ms),
                ],
            );
        }
        report.push_str("FAIL: no route between snapped nodes\n");
        return empty(report);
    };
    if path.len() < 2 {
        report.push_str("FAIL: zero-length route\n");
        return empty(report);
    }
    let astar_ms = timer.lap_ms();

    let mut distance_m = 0.0;
    for &idx in &path_edges {
        distance_m += graph.edges[idx].length_m;
    }
    // Full OSM edge shape (not junction chords) so MapLibre follows the road.
    let polyline = graph.path_overlay_polyline_from_edges(&path_edges);

    let dist_km = distance_m / 1000.0;
    let eta_minutes = motor_path_minutes_from_edges(&graph, &path_edges);
    let sim_samples_json =
        samples_to_json(&build_sim_samples_from_edges(&graph, &path, &path_edges));
    let maneuvers_json = maneuvers_to_json(&build_maneuvers_from_edges(&graph, &path, &path_edges));
    let path_nodes = path.len();
    let polyline_ms = timer.lap_ms();
    driver_break_core::download::progress::set(4, Some(5), "Planning route: break stops…");
    // Clip POI load to the same trip bbox (never a full Ostlandet POI scan).
    let (poi_index, barriers, poi_pack_hit) =
        match driver_break_core::routing::indexed::try_load_poi_barrier_for_plan(&data_dir, pbf) {
            Ok((poi, barriers)) => {
                // Pack is region-wide; nearest queries already radius-limited.
                (poi, barriers, true)
            }
            Err(_) => {
                if driver_break_core::download::plan_cancel::is_cancelled() {
                    return plan_cancelled_result(
                        report,
                        &timer,
                        &[
                            ("profile_map_ms", profile_map_ms),
                            ("graph_build_ms", graph_build_ms),
                            ("network_pref_ms", network_pref_ms),
                            ("astar_ms", astar_ms),
                            ("polyline_ms", polyline_ms),
                        ],
                    );
                }
                let poi_index = match PoiIndex::load_from_pbf_bbox(pbf, bbox) {
                    Ok(idx) => idx,
                    Err(e) if driver_break_core::download::plan_cancel::is_cancel_err(&e) => {
                        return plan_cancelled_result(
                            report,
                            &timer,
                            &[
                                ("profile_map_ms", profile_map_ms),
                                ("graph_build_ms", graph_build_ms),
                                ("network_pref_ms", network_pref_ms),
                                ("astar_ms", astar_ms),
                                ("polyline_ms", polyline_ms),
                            ],
                        );
                    }
                    Err(_) => PoiIndex::new(),
                };
                let barriers = match load_break_barriers(&graph, pbf, bbox) {
                    Ok(b) => b,
                    Err(_) => {
                        return plan_cancelled_result(
                            report,
                            &timer,
                            &[
                                ("profile_map_ms", profile_map_ms),
                                ("graph_build_ms", graph_build_ms),
                                ("network_pref_ms", network_pref_ms),
                                ("astar_ms", astar_ms),
                                ("polyline_ms", polyline_ms),
                            ],
                        );
                    }
                };
                (poi_index, barriers, false)
            }
        };
    let poi_barrier_ms = timer.lap_ms();
    report.push_str(&format!("poi_pack_hit={poi_pack_hit}\n"));

    // Truck / TruckElectric: jurisdiction-keyed HOS (EC 561 or FMCSA).
    // MobileHome uses car soft break spacing (not commercial HGV legal tracking).
    let core_profile = profile.to_core();
    let mut rest = load_rest_config_near_cache(&cache);
    let mut break_interval_km = motor_break_interval_km(core_profile, &rest, dist_km, eta_minutes);
    let mut days_json = String::from("[]");
    let mut truck_overnight_pins: Vec<serde_json::Value> = Vec::new();
    if uses_truck_rest(core_profile) {
        let driving_h = eta_minutes / 60.0;
        let hos_pack = resolve_driving_hours_pack_at(start_lat, start_lon);
        report.push_str(&format!("hos_pack={}\n", hos_pack.as_report_key()));

        let mut history = load_truck_history_near_cache(&cache);
        let today = civil_today_utc();
        let week_id = iso_week_id_utc();
        let week_dates = driver_break_core::config::rolling_date_window(&today, 7);
        let fortnight_dates = driver_break_core::config::rolling_date_window(&today, 14);

        // RestArea / services candidates along the corridor for overnight matching
        // (detour-weighted + facility-tier preference).
        let samples = sample_polyline_km(&polyline);
        let overnight_link = RoadNodeIndex::from_path_nodes(&graph, &path);
        let mut candidates: Vec<TruckRestCandidate> = Vec::new();
        let mut seen_poi = std::collections::HashSet::new();
        for (i, (lat, lon, km)) in samples.iter().enumerate() {
            if i % 4 != 0 && i + 1 != samples.len() {
                continue;
            }
            for p in poi_index.nearest(PoiCategory::RestArea, *lat, *lon, poi_radii.search_radius_m)
            {
                if poi_radii.require_road_link && !overnight_link.within_road_link(p.lat, p.lon) {
                    continue;
                }
                if !seen_poi.insert(p.osm_id) {
                    continue;
                }
                let suitable_for_weekly = rest_area_suitable_for_weekly(&p.tags, &p.icon_key);
                let facility = match p.tags.get("highway").map(String::as_str) {
                    Some("services") => TruckRestFacility::Services,
                    Some("rest_area") => TruckRestFacility::RestArea,
                    _ => TruckRestFacility::HgvParking,
                };
                let detour_km = haversine_m(*lat, *lon, p.lat, p.lon) / 1000.0;
                candidates.push(TruckRestCandidate {
                    along_km: *km,
                    lat: p.lat,
                    lon: p.lon,
                    name: p
                        .name
                        .clone()
                        .unwrap_or_else(|| format!("Rest {}", p.osm_id)),
                    detour_km,
                    facility,
                    suitable_for_weekly,
                });
            }
        }

        match hos_pack {
            JurisdictionDrivingHoursPack::Ec561 => {
                let parts = truck_effective_break_parts(&rest.truck);
                report.push_str(&format!(
                    "truck_rest: break_after_h={:.2}; break_parts_min={parts:?}; daily_max_h={:.1}; weekly_max_h={:.1}; fortnightly_max_h={:.1}; break_interval_km={break_interval_km:.1}; driving_h={driving_h:.2}\n",
                    rest.truck.mandatory_break_after_hours,
                    rest.truck.max_daily_driving_hours,
                    rest.truck.max_weekly_driving_hours,
                    rest.truck.max_fortnightly_driving_hours,
                ));

                for d in driver_break_core::config::outstanding_weekly_rest_compensations(&history)
                {
                    report.push_str(&format!(
                        "truck_compensation: pending=true; reduced_on={}; shortfall_h={:.0}; compensate_by={}\n",
                        d.reduced_on_date, d.shortfall_hours, d.compensate_by_date
                    ));
                }
                let pending_n =
                    driver_break_core::config::outstanding_weekly_rest_compensations(&history)
                        .len();
                if pending_n == 0 {
                    report.push_str("truck_compensation: pending=0\n");
                } else {
                    report.push_str(&format!(
                        "truck_compensation_summary: pending_count={pending_n}\n"
                    ));
                }

                let multi = plan_truck_multi_day(
                    &rest.truck,
                    &history,
                    driving_h,
                    dist_km,
                    &today,
                    &week_id,
                    &candidates,
                    false,
                );
                days_json = days_json_from_truck(&multi, travel_profile_report_key(profile));
                truck_overnight_pins = truck_overnight_break_pins(&multi);
                if multi.multi_day {
                    report.push_str(&format!(
                        "truck_multi_day: days={}; total_driving_h={driving_h:.2}\n",
                        multi.days.len()
                    ));
                    for d in &multi.days {
                        report.push_str(&format!(
                            "truck_day: idx={}; date={}; start_km={:.1}; end_km={:.1}; driving_h={:.2}; extension={}\n",
                            d.day_index, d.date, d.start_km, d.end_km, d.driving_hours, d.used_daily_extension
                        ));
                        if let Some(o) = &d.overnight {
                            report.push_str(&format!(
                                "truck_overnight: kind={:?}; hours={:.1}; not_in_cab={}; poi_found={}; name={:?}; lat={:?}; lon={:?}\n",
                                o.kind, o.hours, o.not_in_cab, o.poi_found, o.name, o.lat, o.lon
                            ));
                            for n in &o.notes {
                                report.push_str(&format!("truck_overnight_note: {n}\n"));
                            }
                        }
                    }
                    let duty = evaluate_truck_trip(
                        &rest.truck,
                        &history,
                        driving_h,
                        &today,
                        &week_id,
                        &week_dates,
                        &fortnight_dates,
                    );
                    report.push_str(&format!(
                        "truck_duty: within_daily={}; within_weekly={}; within_fortnightly={}; allowed_daily_h={:.1}; weekly_rest_due={}; multi_day=true\n",
                        duty.within_daily,
                        duty.within_weekly,
                        duty.within_fortnightly,
                        duty.allowed_daily_hours,
                        duty.weekly_rest_due,
                    ));
                    for n in &duty.notes {
                        report.push_str(&format!("truck_duty_note: {n}\n"));
                    }
                    commit_truck_multi_day_plan(&mut rest.truck, &mut history, &multi, &week_id);
                    let pending_after =
                        driver_break_core::config::outstanding_weekly_rest_compensations(&history);
                    report.push_str(&format!(
                        "truck_compensation_after_commit: pending_count={}\n",
                        pending_after.len()
                    ));
                    for d in pending_after {
                        report.push_str(&format!(
                            "truck_compensation: pending=true; reduced_on={}; shortfall_h={:.0}; compensate_by={}\n",
                            d.reduced_on_date, d.shortfall_hours, d.compensate_by_date
                        ));
                    }
                } else {
                    let duty = evaluate_truck_trip(
                        &rest.truck,
                        &history,
                        driving_h,
                        &today,
                        &week_id,
                        &week_dates,
                        &fortnight_dates,
                    );
                    report.push_str(&format!(
                        "truck_duty: within_daily={}; within_weekly={}; within_fortnightly={}; allowed_daily_h={:.1}; weekly_rest_due={}; multi_day=false\n",
                        duty.within_daily,
                        duty.within_weekly,
                        duty.within_fortnightly,
                        duty.allowed_daily_hours,
                        duty.weekly_rest_due,
                    ));
                    for n in &duty.notes {
                        report.push_str(&format!("truck_duty_note: {n}\n"));
                    }
                    driver_break_core::routing::commit_truck_trip(
                        &mut rest.truck,
                        &mut history,
                        &duty,
                        &today,
                        &week_id,
                    );
                }
                save_rest_and_truck_history_near_cache(&cache, &rest, &history);
            }
            JurisdictionDrivingHoursPack::Fmcsa => {
                let fmcsa = FmcsaHosParams::default();
                break_interval_km = fmcsa_break_interval_km(&fmcsa, dist_km, eta_minutes);
                report.push_str(&format!(
                    "truck_rest: pack=fmcsa; break_after_h={:.2}; daily_max_h={:.1}; on_duty_window_h={:.1}; cycle={:.0}h/{}d; break_interval_km={break_interval_km:.1}; driving_h={driving_h:.2}\n",
                    fmcsa.break_after_driving_hours,
                    fmcsa.max_driving_hours,
                    fmcsa.on_duty_window_hours,
                    fmcsa.cycle_on_duty_hours,
                    fmcsa.cycle_days,
                ));
                let multi =
                    plan_fmcsa_multi_day(&fmcsa, &history, driving_h, dist_km, &today, &candidates);
                days_json = days_json_from_truck(&multi, travel_profile_report_key(profile));
                truck_overnight_pins = truck_overnight_break_pins(&multi);
                if multi.multi_day {
                    report.push_str(&format!(
                        "truck_multi_day: pack=fmcsa; days={}; total_driving_h={driving_h:.2}\n",
                        multi.days.len()
                    ));
                    for d in &multi.days {
                        report.push_str(&format!(
                            "truck_day: idx={}; date={}; start_km={:.1}; end_km={:.1}; driving_h={:.2}; extension={}\n",
                            d.day_index, d.date, d.start_km, d.end_km, d.driving_hours, d.used_daily_extension
                        ));
                        if let Some(o) = &d.overnight {
                            report.push_str(&format!(
                                "truck_overnight: kind={:?}; hours={:.1}; not_in_cab={}; poi_found={}; name={:?}; lat={:?}; lon={:?}\n",
                                o.kind, o.hours, o.not_in_cab, o.poi_found, o.name, o.lat, o.lon
                            ));
                            for n in &o.notes {
                                report.push_str(&format!("truck_overnight_note: {n}\n"));
                            }
                        }
                    }
                } else {
                    report.push_str("truck_multi_day: pack=fmcsa; days=1; multi_day=false\n");
                }
                let (within_daily, within_cycle, notes) =
                    evaluate_fmcsa_trip(&fmcsa, &history, driving_h, &today);
                report.push_str(&format!(
                    "truck_duty: pack=fmcsa; within_daily={within_daily}; within_cycle={within_cycle}; multi_day={}\n",
                    multi.multi_day
                ));
                for n in &notes {
                    report.push_str(&format!("truck_duty_note: {n}\n"));
                }
                // FMCSA duty history commit is not yet persisted (evaluate-only).
            }
            JurisdictionDrivingHoursPack::Unknown => {
                report.push_str(
                    "hos_pack_unknown: declining commercial driving-hours legal tracking — jurisdiction not recognized; no duty commit\n",
                );
                report.push_str(&format!(
                    "truck_rest: pack=unknown; break_interval_km={break_interval_km:.1}; driving_h={driving_h:.2} (informational spacing only)\n"
                ));
            }
        }
    } else {
        report.push_str(&format!(
            "motor_break_interval_km={break_interval_km:.1} (soft rest-interval hours → km via trip speed)\n"
        ));
    }
    let rest_branch_ms = timer.lap_ms();

    // Soft multi-day overnight for car / motorcycle / cycle / mobilehome (not truck).
    let mut motor_overnight_pins: Vec<serde_json::Value> = Vec::new();
    if uses_motor_multi_day(core_profile) {
        let driving_h = eta_minutes / 60.0;
        if let Some(budget) = motor_daily_budget(core_profile, &rest.car, &rest.cycling) {
            let samples = sample_polyline_km(&polyline);
            let overnight_link = RoadNodeIndex::from_path_nodes(&graph, &path);
            let mut candidates: Vec<MotorOvernightCandidate> = Vec::new();
            let mut seen_poi = std::collections::HashSet::new();
            for (i, (lat, lon, km)) in samples.iter().enumerate() {
                if i % 4 != 0 && i + 1 != samples.len() {
                    continue;
                }
                for cat in [
                    PoiCategory::Lodging,
                    PoiCategory::OvernightFacility,
                    PoiCategory::TentSite,
                    PoiCategory::Cabin,
                    PoiCategory::RestArea,
                ] {
                    for p in poi_index.nearest(cat, *lat, *lon, poi_radii.search_radius_m) {
                        if poi_radii.require_road_link
                            && !overnight_link.within_road_link(p.lat, p.lon)
                        {
                            continue;
                        }
                        if !seen_poi.insert(p.osm_id) {
                            continue;
                        }
                        let kind = if p.categories.contains(&PoiCategory::Lodging) {
                            MotorOvernightKind::Lodging
                        } else if p.categories.contains(&PoiCategory::TentSite)
                            || p.categories.contains(&PoiCategory::OvernightFacility)
                            || p.categories.contains(&PoiCategory::Cabin)
                        {
                            MotorOvernightKind::Camping
                        } else if p.categories.contains(&PoiCategory::RestArea) {
                            MotorOvernightKind::RestArea
                        } else {
                            continue;
                        };
                        candidates.push(MotorOvernightCandidate {
                            along_km: *km,
                            lat: p.lat,
                            lon: p.lon,
                            name: p
                                .name
                                .clone()
                                .unwrap_or_else(|| format!("Overnight {}", p.osm_id)),
                            kind,
                        });
                    }
                }
            }
            let multi = plan_motor_multi_day(budget, driving_h, dist_km, &candidates);
            if multi.multi_day || !multi.days.is_empty() {
                days_json = days_json_from_motor(&multi, travel_profile_report_key(profile));
            }
            if multi.multi_day {
                report.push_str(&format!(
                    "motor_multi_day: days={}; budget={:?}; total_driving_h={driving_h:.2}; total_km={dist_km:.1}\n",
                    multi.days.len(),
                    multi.budget
                ));
                for d in &multi.days {
                    report.push_str(&format!(
                        "motor_day: idx={}; start_km={:.1}; end_km={:.1}; driving_h={:.2}; distance_km={:.1}\n",
                        d.day_index, d.start_km, d.end_km, d.driving_hours, d.distance_km
                    ));
                    if let Some(o) = &d.overnight {
                        let kind_s = match o.kind {
                            MotorOvernightKind::Lodging => "lodging",
                            MotorOvernightKind::Camping => "camping",
                            MotorOvernightKind::RestArea => "rest_area",
                            MotorOvernightKind::None => "none",
                        };
                        report.push_str(&format!(
                            "motor_overnight: kind={kind_s}; poi_found={}; name={:?}; lat={:?}; lon={:?}\n",
                            o.poi_found, o.name, o.lat, o.lon
                        ));
                        for n in &o.notes {
                            report.push_str(&format!("motor_overnight_note: {n}\n"));
                        }
                        if o.poi_found {
                            if let (Some(lat), Some(lon)) = (o.lat, o.lon) {
                                let json_kind = match o.kind {
                                    MotorOvernightKind::Lodging => "lodging",
                                    MotorOvernightKind::Camping => "hut",
                                    MotorOvernightKind::RestArea => "rest_area",
                                    MotorOvernightKind::None => "amenity",
                                };
                                motor_overnight_pins.push(serde_json::json!({
                                    "name": o.name.clone().unwrap_or_else(|| "Overnight".into()),
                                    "lat": lat,
                                    "lon": lon,
                                    "kind": json_kind,
                                    "icon_key": match o.kind {
                                        MotorOvernightKind::Lodging => "tourism-hotel",
                                        MotorOvernightKind::Camping => "tourism-camp_site",
                                        MotorOvernightKind::RestArea => "highway-rest_area",
                                        MotorOvernightKind::None => "fuel",
                                    },
                                    "along_km": d.end_km,
                                }));
                            }
                        }
                    }
                }
            } else {
                report.push_str("motor_multi_day: days=1; multi_day=false\n");
            }
        }
    }
    let multiday_ms = timer.lap_ms();

    // Prefer stops on the planned road; fall back if reachable without danger barriers
    // unless the active profile requires road-linked POIs.
    let mut break_pois_json = build_break_pois_json(
        &poi_index,
        &polyline,
        break_interval_km,
        poi_radii.search_radius_m,
        &graph,
        &barriers,
        &path,
        false,
        None,
        poi_radii.require_road_link,
    );
    merge_break_poi_pins(&mut break_pois_json, motor_overnight_pins);
    merge_break_poi_pins(&mut break_pois_json, truck_overnight_pins);
    let pause_pins_ms = timer.lap_ms();
    // Difficulty metadata on cycling network ways (informational only).
    if (profile == TravelProfile::Bicycle || profile == TravelProfile::BicycleElectric)
        && prefer_official_networks
    {
        let way_ids: std::collections::HashSet<i64> = path
            .windows(2)
            .filter_map(|w| graph.edge_index(w[0], w[1]))
            .filter_map(|i| {
                graph.edges[i]
                    .id
                    .strip_suffix("-rev")
                    .unwrap_or(&graph.edges[i].id)
                    .split('-')
                    .next()
                    .and_then(|s| s.parse().ok())
            })
            .collect();
        if let Ok(tags) = load_way_difficulty_tags(pbf, &way_ids) {
            let notes = difficulty_notes_for_path(&graph, &path, &tags);
            if !notes.is_empty() {
                report.push_str(&format!("route_metadata={}\n", notes.join("; ")));
            }
        }
    }
    if profile == TravelProfile::BicycleElectric {
        let eco = eco_for_travel_profile(profile);
        let ebike = load_ebike_config_near_cache(&cache);
        let (range, climb, steep) = driver_break_core::routing::analyze_ebike_route(
            &graph, &path, &elevation, &eco, &ebike,
        );
        let path_max =
            driver_break_core::routing::path_max_climb_grade_pct(&graph, &path, &elevation);
        report.push_str(
            &driver_break_core::routing::format_ebike_route_report_with_path_grade(
                &range,
                &climb,
                &steep,
                Some(path_max),
            ),
        );
    }
    if profile == TravelProfile::CarElectric {
        let eco = eco_for_travel_profile(profile);
        let ev = load_ev_car_config_near_cache(&cache);
        let range =
            driver_break_core::routing::analyze_ev_car_route(&graph, &path, &elevation, &eco, &ev);
        report.push_str(&driver_break_core::routing::format_ev_car_route_report(
            &range,
        ));
    }
    if use_eco {
        let eco = eco_for_travel_profile(profile);
        let breakdown =
            driver_break_core::routing::path_eco_energy_breakdown(&graph, &path, &elevation, &eco);
        report
            .push_str(&driver_break_core::routing::format_eco_energy_breakdown_report(&breakdown));
    }
    let priority_path_share_pct = graph.non_motorway_share_pct(&path);
    report.push_str(&format!(
        "priority_path_share_pct={priority_path_share_pct:.2}; motorway_share_pct={:.2}\n",
        graph.motorway_share_pct(&path)
    ));
    let report_addons_ms = timer.lap_ms();
    let plan_duration_ms = timer.total_ms();
    append_route_plan_timing(
        &mut report,
        plan_duration_ms,
        &[
            ("profile_map_ms", profile_map_ms),
            ("graph_build_ms", graph_build_ms),
            ("network_pref_ms", network_pref_ms),
            ("astar_ms", astar_ms),
            ("polyline_ms", polyline_ms),
            ("poi_barrier_ms", poi_barrier_ms),
            ("rest_branch_ms", rest_branch_ms),
            ("multiday_ms", multiday_ms),
            ("pause_pins_ms", pause_pins_ms),
            ("report_addons_ms", report_addons_ms),
        ],
    );
    report.push_str(&format!(
        "distance_km={dist_km:.3}; eta_min={eta_minutes:.1}; path_nodes={path_nodes}; path_cost={cost:.0}; polyline_chars={}; break_pois={}\nPASS\n",
        polyline.len(),
        break_pois_json
    ));
    driver_break_core::download::progress::set(5, Some(5), "Planning route: done");

    CorridorRouteResult {
        report,
        distance_km: dist_km,
        eta_minutes,
        cache_hit,
        cold_build_s: build_s,
        warm_load_s: 0.0,
        route_polyline: polyline,
        poi_lat: end_lat,
        poi_lon: end_lon,
        poi_name: String::from("End"),
        poi_icon_key: String::from("fuel"),
        break_pois_json,
        days_json,
        sim_samples_json,
        maneuvers_json,
        priority_path_share_pct,
        route_segments_json: String::from("[]"),
        off_trail_advisory: String::new(),
    }
}

/// Plan a hiking (foot) route through ordered waypoints.
///
/// `waypoints_json` is `[{"name","lat","lon"}, ...]` with at least two points
/// (start … vias … end). After a draft corridor, named rast-interval huts near
/// the path (same filters as pause pins) are promoted to vias and the path is
/// replanned once when the detour fits the Drive POI cabin-radius slider
/// (lateral + absolute extra path; also 15% of the containing user leg). Pause
/// stops prefer huts/cabins; otherwise camp pitches or a synthetic corridor
/// tent (never mountain peak names).
///
/// `data_dir` is the app data directory for pack/manifest lookup (same as
/// [`plan_car_route`]). Empty: PBF parent.
#[uniffi::export]
pub fn plan_hiking_route(
    pbf_path: String,
    elev_dir: String,
    cache_dir: String,
    waypoints_json: String,
    prefer_official_networks: bool,
    prefer_pilgrim_routes: bool,
    data_dir: String,
) -> CorridorRouteResult {
    let _ch = driver_break_core::download::progress::ChannelGuard::enter(
        driver_break_core::download::progress::ProgressChannel::Plan,
    );
    #[derive(Deserialize)]
    struct Wp {
        name: String,
        lat: f64,
        lon: f64,
    }

    let mut timer = PlanStageTimer::start();
    let _cancel_guard = driver_break_core::download::plan_cancel::begin_plan();
    let mut report = String::from("TEST_KIND=PLAN_HIKING_ROUTE\nDATA_SOURCE=real_pbf\n");
    report.push_str("profile=Hiking; use_eco=true\n");
    report.push_str(&format!(
        "prefer_official_networks={prefer_official_networks}; prefer_pilgrim_routes={prefer_pilgrim_routes}\n"
    ));
    report.push_str("avoid_motorways=true (locked for hiking)\n");
    let use_networked_cabins = load_use_networked_cabins_near_cache(&PathBuf::from(&cache_dir));
    report.push_str(&format!("use_networked_cabins={use_networked_cabins}\n"));
    let network_hut_member = load_network_hut_member_near_cache(&PathBuf::from(&cache_dir));
    report.push_str(&format!("network_hut_member={network_hut_member}\n"));
    let user_wps: Vec<HikingWp> = match serde_json::from_str::<Vec<Wp>>(&waypoints_json) {
        Ok(v) => v
            .into_iter()
            .map(|w| HikingWp {
                name: w.name,
                lat: w.lat,
                lon: w.lon,
            })
            .collect(),
        Err(e) => {
            report.push_str(&format!("FAIL: waypoints_json: {e}\n"));
            return empty_corridor(report);
        }
    };
    if user_wps.len() < 2 {
        report.push_str("FAIL: need at least start and end waypoints\n");
        return empty_corridor(report);
    }
    report.push_str(&format!("waypoints={}\n", user_wps.len()));

    let plan = WorkerPoolPlan::detect();
    WorkerPoolPlan::lower_current_thread_priority();
    let _ = plan.install_rayon_pool();

    let pbf = Path::new(pbf_path.trim());
    if !pbf.is_file() {
        report.push_str(&format!("FAIL: PBF missing: \"{pbf_path}\"\n"));
        return empty_corridor(report);
    }
    let elev = PathBuf::from(&elev_dir);
    let cache = PathBuf::from(&cache_dir);
    let _ = std::fs::create_dir_all(&cache);
    let poi_radii = load_profile_poi_radii_near_cache(&cache)
        .for_profile(driver_break_core::config::Profile::Hiking)
        .clone();
    report.push_str(&format!(
        "poi_search_radius_m={:.0}; cabin_radius_m={:.0}; network_hut_radius_m={:.0}; network_hut_pref_m={:.0}; require_road_link={}\n",
        poi_radii.search_radius_m,
        poi_radii.cabin_radius_m,
        poi_radii.network_hut_radius_m,
        poi_radii.network_hut_preference_radius_m,
        poi_radii.require_road_link
    ));
    let eco = eco_for_travel_profile(TravelProfile::Hiking);
    let elevation = ElevationService::new(ElevationCache::new(&elev));
    let min_lat = user_wps.iter().map(|w| w.lat).fold(f64::INFINITY, f64::min);
    let max_lat = user_wps
        .iter()
        .map(|w| w.lat)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_lon = user_wps.iter().map(|w| w.lon).fold(f64::INFINITY, f64::min);
    let max_lon = user_wps
        .iter()
        .map(|w| w.lon)
        .fold(f64::NEG_INFINITY, f64::max);
    // Clip to the trip bbox so we never load a full Ostlandet foot graph into RAM
    // (that OOMs 4GB Automotive AVDs during hiking plan). Same region .pbf.
    let span = (max_lat - min_lat).max(max_lon - min_lon);
    let pad = (span * 0.25).clamp(0.30, 0.55);
    let bbox = [min_lat - pad, min_lon - pad, max_lat + pad, max_lon + pad];
    report.push_str(&format!(
        "bbox={:.3},{:.3},{:.3},{:.3}; pad={pad:.2}\n",
        bbox[0], bbox[1], bbox[2], bbox[3]
    ));
    let _ = elevation.warm_bbox(bbox);
    let profile_map_ms = timer.lap_ms();
    if driver_break_core::download::plan_cancel::is_cancelled() {
        return plan_cancelled_result(report, &timer, &[("profile_map_ms", profile_map_ms)]);
    }

    driver_break_core::download::progress::set(
        0,
        Some(5),
        "Planning route: building hiking area graph…",
    );
    let t_graph = Instant::now();
    let data_dir = plan_pack_data_dir(pbf, &data_dir);
    let pack_try = driver_break_core::routing::indexed::try_load_graph_for_plan_bbox(
        &data_dir,
        pbf,
        RoutingProfile::Foot,
        Some(bbox),
    );
    let _pause_bg = if pack_try.is_err() {
        Some(driver_break_core::download::ForegroundPlanGuard::acquire())
    } else {
        None
    };
    let (mut graph, cache_hit, pack_hit) = match pack_try {
        Ok(g) => (g, false, true),
        Err(_) => match load_or_build_reweighted_bbox(
            pbf,
            &data_dir,
            &cache,
            RoutingProfile::Foot,
            &elevation,
            &eco,
            bbox,
        ) {
            Ok((g, hit)) => (g, hit, false),
            Err(e) if driver_break_core::download::plan_cancel::is_cancel_err(&e) => {
                return plan_cancelled_result(
                    report,
                    &timer,
                    &[("profile_map_ms", profile_map_ms)],
                );
            }
            Err(e) => {
                report.push_str(&format!("FAIL: foot graph build: {e:#}\n"));
                return empty_corridor(report);
            }
        },
    };
    let build_s = t_graph.elapsed().as_secs_f64();
    let graph_build_ms = timer.lap_ms();
    if driver_break_core::download::plan_cancel::is_cancelled() {
        return plan_cancelled_result(
            report,
            &timer,
            &[
                ("profile_map_ms", profile_map_ms),
                ("graph_build_ms", graph_build_ms),
            ],
        );
    }
    apply_route_prefs_if_requested(
        &mut graph,
        pbf,
        OfficialNetworkKind::Hiking,
        prefer_official_networks,
        prefer_pilgrim_routes,
        &mut report,
    );
    if driver_break_core::download::plan_cancel::is_cancelled() {
        return plan_cancelled_result(
            report,
            &timer,
            &[
                ("profile_map_ms", profile_map_ms),
                ("graph_build_ms", graph_build_ms),
            ],
        );
    }
    apply_slow_road_preference(&mut graph);
    report.push_str("slow_road_preference=applied; profile=hiking\n");
    let network_pref_ms = timer.lap_ms();
    report.push_str(&format!(
        "build_s={build_s:.2}; cache_hit={cache_hit}; pack_hit={pack_hit}; nodes={}; edges={}\n",
        graph.nodes.len(),
        graph.edges.len()
    ));

    if driver_break_core::download::plan_cancel::is_cancelled() {
        return plan_cancelled_result(
            report,
            &timer,
            &[
                ("profile_map_ms", profile_map_ms),
                ("graph_build_ms", graph_build_ms),
                ("network_pref_ms", network_pref_ms),
            ],
        );
    }

    // Prefer graph-first: if every leg snaps and connects on-trail, we still apply
    // wetlands then replan. If graph fails, refuse absurd crow-flies gaps before the
    // expensive wetland PBF scan (e.g. destination in open ocean).
    let graph_only_ok = hike_path_through_waypoints(&graph, &user_wps).is_ok();
    if driver_break_core::download::plan_cancel::is_cancelled() {
        return plan_cancelled_result(
            report,
            &timer,
            &[
                ("profile_map_ms", profile_map_ms),
                ("graph_build_ms", graph_build_ms),
                ("network_pref_ms", network_pref_ms),
            ],
        );
    }
    if !graph_only_ok {
        for pair in user_wps.windows(2) {
            let crow = haversine_m(pair[0].lat, pair[0].lon, pair[1].lat, pair[1].lon);
            if crow > driver_break_core::routing::TERRAIN_MAX_GAP_M {
                report.push_str(&format!(
                    "FAIL: no foot route {} -> {} (gap {:.0} m exceeds terrain limit {:.0} m)\n",
                    pair[0].name,
                    pair[1].name,
                    crow,
                    driver_break_core::routing::TERRAIN_MAX_GAP_M
                ));
                return empty_corridor(report);
            }
        }
    }

    let (wetlands, wetland_pack_hit) =
        match driver_break_core::routing::indexed::try_load_wetland_for_plan(
            &data_dir,
            pbf,
            Some(bbox),
        ) {
            Ok(w) => (w, true),
            Err(_) => match WetlandIndex::load_from_pbf_bbox(pbf, bbox) {
                Ok(w) => (w, false),
                Err(e) if driver_break_core::download::plan_cancel::is_cancel_err(&e) => {
                    return plan_cancelled_result(
                        report,
                        &timer,
                        &[
                            ("profile_map_ms", profile_map_ms),
                            ("graph_build_ms", graph_build_ms),
                            ("network_pref_ms", network_pref_ms),
                        ],
                    );
                }
                Err(e) => {
                    report.push_str(&format!("WARN: wetland index: {e:#}\n"));
                    (WetlandIndex::default(), false)
                }
            },
        };
    let wet_stats = graph.apply_wetland_hazards(&wetlands);
    report.push_str(&format!(
        "wetland_rings={}; wetland_soft_edges={}; wetland_hard_removed={}; wetland_boardwalk_kept={}; wetland_pack_hit={wetland_pack_hit}\n",
        wetlands.ring_count(),
        wet_stats.soft_penalized,
        wet_stats.hard_removed,
        wet_stats.boardwalk_kept
    ));
    let wetland_ms = timer.lap_ms();
    if driver_break_core::download::plan_cancel::is_cancelled() {
        return plan_cancelled_result(
            report,
            &timer,
            &[
                ("profile_map_ms", profile_map_ms),
                ("graph_build_ms", graph_build_ms),
                ("network_pref_ms", network_pref_ms),
                ("wetland_ms", wetland_ms),
            ],
        );
    }

    let mut hybrid = match plan_hybrid_hiking_path(
        &graph,
        &elevation,
        &wetlands,
        &eco,
        &to_hiking_waypoints(&user_wps),
    ) {
        Ok(h) => h,
        Err(e)
            if e.contains("cancelled")
                || driver_break_core::download::plan_cancel::is_cancelled() =>
        {
            return plan_cancelled_result(
                report,
                &timer,
                &[
                    ("profile_map_ms", profile_map_ms),
                    ("graph_build_ms", graph_build_ms),
                    ("network_pref_ms", network_pref_ms),
                    ("wetland_ms", wetland_ms),
                ],
            );
        }
        Err(e) => {
            report.push_str(&format!("FAIL: {e}\n"));
            return empty_corridor(report);
        }
    };
    let mut full_path = hybrid.path_nodes.clone();
    let mut distance_m = hybrid.distance_m;
    let user_cum_km = user_cum_km_from_hybrid(&hybrid, &user_wps);
    let mut polyline = hybrid.polyline_lon_lat();
    report.push_str(&format!(
        "route_mode={}; off_trail_m={:.0}\n",
        hybrid.route_mode(),
        hybrid.off_trail_m
    ));
    let hybrid_path_ms = timer.lap_ms();

    // Prefer indexed POI/barrier pack (v2 includes overnight buildings). Fall back
    // to the corridor PBF scan only when packs are missing/stale.
    let corridor_lat_lon = hybrid.full_coords();
    let (poi_index, barriers, poi_pack_hit, overnight_buildings_pack_hit) =
        match driver_break_core::routing::indexed::try_load_poi_barrier_for_plan(&data_dir, pbf) {
            Ok((mut idx, pack_barriers)) => {
                let had_buildings = !idx.overnight_buildings().is_empty();
                if had_buildings {
                    let band = driver_break_core::poi::CorridorBand::from_lat_lon(
                        &corridor_lat_lon,
                        OVERNIGHT_BUILDING_CORRIDOR_MARGIN_M,
                    );
                    if band.is_empty() {
                        // No corridor geometry — keep bbox buildings from pack.
                    } else {
                        idx.retain_overnight_buildings(|lat, lon| band.contains(lat, lon));
                    }
                }
                let mut barriers = DangerBarrierIndex::from_graph(&graph);
                barriers.merge(pack_barriers);
                (idx, barriers, true, had_buildings)
            }
            Err(_) => {
                let idx = match PoiIndex::load_from_pbf_bbox_with_overnight_buildings_near_corridor(
                    pbf,
                    bbox,
                    &corridor_lat_lon,
                    OVERNIGHT_BUILDING_CORRIDOR_MARGIN_M,
                ) {
                    Ok(i) => i,
                    Err(e) if driver_break_core::download::plan_cancel::is_cancel_err(&e) => {
                        return plan_cancelled_result(
                            report,
                            &timer,
                            &[
                                ("profile_map_ms", profile_map_ms),
                                ("graph_build_ms", graph_build_ms),
                                ("network_pref_ms", network_pref_ms),
                                ("wetland_ms", wetland_ms),
                                ("hybrid_path_ms", hybrid_path_ms),
                            ],
                        );
                    }
                    Err(e) => {
                        report.push_str(&format!("FAIL: POI index: {e:#}\n"));
                        return empty_corridor(report);
                    }
                };
                let barriers = match load_break_barriers(&graph, pbf, bbox) {
                    Ok(b) => b,
                    Err(_) => {
                        return plan_cancelled_result(
                            report,
                            &timer,
                            &[
                                ("profile_map_ms", profile_map_ms),
                                ("graph_build_ms", graph_build_ms),
                                ("network_pref_ms", network_pref_ms),
                                ("wetland_ms", wetland_ms),
                                ("hybrid_path_ms", hybrid_path_ms),
                            ],
                        );
                    }
                };
                (idx, barriers, false, false)
            }
        };
    report.push_str(&format!(
        "poi_pack_hit={poi_pack_hit}; overnight_buildings_pack_hit={overnight_buildings_pack_hit}\n"
    ));
    let mut safety = SafetyConfig::default();
    poi_radii.apply_to_safety(&mut safety);
    let overnight_prox = OvernightProximityIndex::from_poi_buildings_and_barriers(
        poi_index.overnight_buildings().to_vec(),
        &barriers,
    );
    let overnight_source = if overnight_buildings_pack_hit {
        "pack+corridor_filter"
    } else {
        "pbf_corridor"
    };
    report.push_str(&format!(
        "overnight_buildings={}; overnight_glaciers={}; overnight_source={overnight_source}; overnight_corridor_margin_m={OVERNIGHT_BUILDING_CORRIDOR_MARGIN_M:.0}\n",
        overnight_prox.buildings.len(),
        overnight_prox.glacier_rings.len()
    ));
    let overnight_ctx = (safety, overnight_prox);
    let poi_barrier_ms = timer.lap_ms();

    // Promote rast-interval named huts to vias and replan once so the corridor visits them.
    // Cabin / search radii from Drive POI slider set lateral + detour allowance.
    // Auto-vias require an on-trail draft path (skip when pure off-trail).
    let autos = if full_path.len() >= 2 {
        collect_hiking_auto_vias(
            &poi_index,
            &graph,
            &barriers,
            &full_path,
            &polyline,
            &user_wps,
            &user_cum_km,
            poi_radii.cabin_radius_m,
            poi_radii.search_radius_m,
            use_networked_cabins,
            &mut report,
        )
    } else {
        Vec::new()
    };
    let wps = if autos.is_empty() {
        report.push_str("auto_vias=0\n");
        user_wps.clone()
    } else {
        let names: Vec<&str> = autos.iter().map(|a| a.name.as_str()).collect();
        report.push_str(&format!(
            "auto_vias={}; names={}\n",
            autos.len(),
            names.join("|")
        ));
        let merged = merge_hiking_waypoints_with_auto_vias(&user_wps, &user_cum_km, &autos);
        match plan_hybrid_hiking_path(
            &graph,
            &elevation,
            &wetlands,
            &eco,
            &to_hiking_waypoints(&merged),
        ) {
            Ok(h) => {
                full_path = h.path_nodes.clone();
                distance_m = h.distance_m;
                polyline = h.polyline_lon_lat();
                hybrid = h;
                merged
            }
            Err(e) => {
                report.push_str(&format!("auto_via_replan_failed={e}; keeping_draft_path\n"));
                user_wps.clone()
            }
        }
    };
    let hut_via_ms = timer.lap_ms();
    let dist_km = distance_m / 1000.0;
    let route_segments_json = hybrid.segments_json();
    let mut off_trail_advisory = if hybrid.off_trail_m > 1.0 {
        report.push_str(&format!("ADVISORY: {OFF_TRAIL_ADVISORY}\n"));
        OFF_TRAIL_ADVISORY.to_string()
    } else {
        String::new()
    };
    // Informational only — DNT/OSM has no reliable winter closure tags.
    if let Some(winter) =
        driver_break_core::routing::dnt_winter::dnt_winter_advisory_for_month(None)
    {
        report.push_str(&format!("ADVISORY: {winter}\n"));
        if off_trail_advisory.is_empty() {
            off_trail_advisory = winter.to_string();
        } else {
            off_trail_advisory = format!("{off_trail_advisory} · {winter}");
        }
    }

    if prefer_official_networks {
        let way_ids: std::collections::HashSet<i64> = full_path
            .windows(2)
            .filter_map(|w| graph.edge_index(w[0], w[1]))
            .filter_map(|i| {
                graph.edges[i]
                    .id
                    .strip_suffix("-rev")
                    .unwrap_or(&graph.edges[i].id)
                    .split('-')
                    .next()
                    .and_then(|s| s.parse().ok())
            })
            .collect();
        if let Ok(tags) = load_way_difficulty_tags(pbf, &way_ids) {
            let notes = difficulty_notes_for_path(&graph, &full_path, &tags);
            if !notes.is_empty() {
                report.push_str(&format!("route_metadata={}\n", notes.join("; ")));
            }
        }
    }

    // Day-by-day multi-day overnight (mirrors truck/motor; same spirit as DNT helper).
    let rest = RestConfig::default();
    let max_daily =
        max_daily_distance_km(&rest, driver_break_core::config::Profile::Hiking).unwrap_or(40.0);
    let hike_coords = if full_path.len() >= 2 {
        graph.path_coords_lat_lon(&full_path)
    } else {
        hybrid.full_coords()
    };
    let hike_samples = hiking_samples_from_coords(&hike_coords);
    let multi = plan_hiking_multi_day(
        &hike_samples,
        max_daily,
        &overnight_ctx.0,
        &poi_index,
        &overnight_ctx.1,
        network_hut_member,
    );
    let days_json = days_json_from_hiking(&multi);
    let mut hiking_overnight_pins: Vec<serde_json::Value> = Vec::new();
    if multi.multi_day {
        report.push_str(&format!(
            "hiking_multi_day: days={}; max_daily_km={max_daily:.1}; total_km={dist_km:.1}\n",
            multi.days.len()
        ));
        for d in &multi.days {
            report.push_str(&format!(
                "hiking_day: idx={}; start_km={:.1}; end_km={:.1}; distance_km={:.1}; overnight_gap={}\n",
                d.day_index, d.start_km, d.end_km, d.distance_km, d.overnight_gap
            ));
            if let Some(o) = &d.overnight {
                report.push_str(&format!(
                    "hiking_overnight: name={:?}; network={}; membership_required={}; safety_rejected={}; safety_reason={:?}; dist_m={:.0}; lat={:.5}; lon={:.5}\n",
                    o.name, o.is_network, o.membership_required, o.safety_rejected, o.safety_reason, o.distance_from_target_m, o.lat, o.lon
                ));
                hiking_overnight_pins.push(json!({
                    "name": if o.safety_rejected {
                        o.safety_reason.clone().unwrap_or_else(|| o.name.clone())
                    } else {
                        o.name.clone()
                    },
                    "lat": o.lat,
                    "lon": o.lon,
                    "kind": if o.is_network { "network_hut" } else { "hut" },
                    "icon": "cabin",
                    "icon_key": o.icon_key,
                    "along_km": d.end_km,
                    "overnight": true,
                    "membership_required": o.membership_required,
                    "safety_rejected": o.safety_rejected,
                    "safety_reason": o.safety_reason,
                }));
            }
        }
    } else {
        report.push_str("hiking_multi_day: days=1; multi_day=false\n");
    }
    let multiday_ms = timer.lap_ms();
    // Hiking rast interval (~11.3 km); prefer path-linked huts, else reachable fallback.
    let mut break_pois_json = build_break_pois_json(
        &poi_index,
        &polyline,
        HIKING_MAIN_BREAK_DISTANCE_KM,
        poi_radii.search_radius_m.max(poi_radii.cabin_radius_m),
        &graph,
        &barriers,
        &full_path,
        true,
        Some(&overnight_ctx),
        poi_radii.require_road_link,
    );
    // Ensure hut vias/end (user + auto) appear as pause labels even if the interval skipped them.
    if let Ok(mut arr) = serde_json::from_str::<Vec<serde_json::Value>>(&break_pois_json) {
        for wp in &wps {
            let lower = wp.name.to_lowercase();
            if lower.contains("hytte")
                || lower.contains("hytta")
                || lower.contains("bu")
                || lower.contains("cabin")
                || lower.contains("rondvass")
            {
                let already = arr.iter().any(|s| {
                    s["name"]
                        .as_str()
                        .map(|n: &str| n.eq_ignore_ascii_case(&wp.name))
                        .unwrap_or(false)
                });
                if !already && !lower.contains("skolla") {
                    arr.push(json!({
                        "name": wp.name,
                        "lat": wp.lat,
                        "lon": wp.lon,
                        "kind": "hut",
                        "icon": "cabin",
                    }));
                }
            }
        }
        // Never label mountain peaks (e.g. Store Ramshøgda) as pause stops.
        arr.retain(|s| {
            let name = s["name"].as_str().unwrap_or("").to_lowercase();
            let kind = s["kind"].as_str().unwrap_or("");
            !(kind == "tent" && name.contains("ramsh"))
        });
        arr.extend(hiking_overnight_pins);
        break_pois_json = serde_json::to_string(&arr).unwrap_or(break_pois_json);
    }
    let pause_pins_ms = timer.lap_ms();

    let priority_path_share_pct = graph.non_motorway_share_pct(&full_path);
    report.push_str(&format!(
        "priority_path_share_pct={priority_path_share_pct:.2}\n"
    ));
    let end = wps.last().unwrap();
    // Hiking: fixed 16 min/km (no climb adjustment in this pass).
    let eta_minutes = fixed_pace_minutes(dist_km, HIKING_MIN_PER_KM);
    report.push_str(&format!(
        "distance_km={dist_km:.3}; eta_min={eta_minutes:.1}; path_nodes={}; break_pois={break_pois_json}\n",
        full_path.len()
    ));
    {
        let breakdown = driver_break_core::routing::path_eco_energy_breakdown(
            &graph, &full_path, &elevation, &eco,
        );
        report
            .push_str(&driver_break_core::routing::format_eco_energy_breakdown_report(&breakdown));
    }

    // Prefer graph densification; fall back to hiking-pace samples on the overlay
    // polyline so staged / coarse geometry still drives debug simulation.
    let mut sim_samples = build_sim_samples(&graph, &full_path);
    if sim_samples.len() < 2 {
        let hike_speed_kmh = 60.0 / HIKING_MIN_PER_KM;
        sim_samples = build_sim_samples_from_lat_lon(&hike_coords, hike_speed_kmh, Some("path"));
    }
    let sim_samples_json = samples_to_json(&sim_samples);
    let maneuvers_json = maneuvers_to_json(&build_maneuvers(&graph, &full_path));
    report.push_str(&format!("sim_samples={}\n", sim_samples.len()));
    let report_addons_ms = timer.lap_ms();
    let plan_duration_ms = timer.total_ms();
    append_route_plan_timing(
        &mut report,
        plan_duration_ms,
        &[
            ("profile_map_ms", profile_map_ms),
            ("graph_build_ms", graph_build_ms),
            ("network_pref_ms", network_pref_ms),
            ("wetland_ms", wetland_ms),
            ("hybrid_path_ms", hybrid_path_ms),
            ("poi_barrier_ms", poi_barrier_ms),
            ("hut_via_ms", hut_via_ms),
            ("multiday_ms", multiday_ms),
            ("pause_pins_ms", pause_pins_ms),
            ("report_addons_ms", report_addons_ms),
        ],
    );
    report.push_str("PASS\n");

    CorridorRouteResult {
        report,
        distance_km: dist_km,
        eta_minutes,
        cache_hit,
        cold_build_s: build_s,
        warm_load_s: 0.0,
        route_polyline: polyline,
        poi_lat: end.lat,
        poi_lon: end.lon,
        poi_name: end.name.clone(),
        poi_icon_key: String::from("cabin"),
        break_pois_json,
        days_json,
        sim_samples_json,
        maneuvers_json,
        priority_path_share_pct,
        route_segments_json,
        off_trail_advisory,
    }
}

/// Backward-compatible wrapper: real pipeline without separate cache dir (uses elev parent).
#[uniffi::export]
pub fn run_car_corridor_smoke_test(
    pbf_path: String,
    elev_dir: String,
    break_interval_hours: f64,
) -> String {
    let cache = PathBuf::from(&elev_dir)
        .parent()
        .map(|p| p.join("graph-cache"))
        .unwrap_or_else(|| PathBuf::from("graph-cache"));
    let _ = std::fs::create_dir_all(&cache);
    run_car_corridor_pipeline(
        pbf_path,
        elev_dir,
        cache.display().to_string(),
        break_interval_hours,
    )
    .report
}

#[derive(uniffi::Enum, Debug, Clone, Copy)]
pub enum FfiIconTheme {
    Day,
    Night,
}

fn map_theme(theme: FfiIconTheme) -> IconTheme {
    match theme {
        FfiIconTheme::Day => IconTheme::Day,
        FfiIconTheme::Night => IconTheme::Night,
    }
}

/// Rasterize a semantic icon to PNG bytes (real usvg/resvg pipeline).
#[uniffi::export]
pub fn rasterize_icon_png(
    key: String,
    theme: FfiIconTheme,
    width: u32,
    height: u32,
    bundled_dir: String,
) -> Vec<u8> {
    icons::rasterize_key_png(
        &key,
        map_theme(theme),
        width,
        height,
        None,
        Path::new(&bundled_dir),
    )
    .unwrap_or_default()
}

/// Rasterize and return a short status string for instrumented tests.
#[uniffi::export]
pub fn rasterize_icon_check(
    key: String,
    theme: FfiIconTheme,
    width: u32,
    height: u32,
    bundled_dir: String,
) -> String {
    match icons::rasterize_key(
        &key,
        map_theme(theme),
        width,
        height,
        None,
        Path::new(&bundled_dir),
    ) {
        Ok(rgba) => {
            let expected = (width as usize) * (height as usize) * 4;
            let nonzero = rgba.iter().filter(|b| **b != 0).count();
            if rgba.len() != expected {
                format!(
                    "TEST_KIND=ICON_RASTER\nFAIL: len {} != expected {expected}\n",
                    rgba.len()
                )
            } else if nonzero == 0 {
                format!("TEST_KIND=ICON_RASTER\nFAIL: all-zero bitmap for key={key}\n")
            } else {
                format!(
                    "TEST_KIND=ICON_RASTER\nDATA_SOURCE=real_svg\nkey={key}\n\
                     width={width}; height={height}; rgba_bytes={}; nonzero_bytes={nonzero}\nPASS\n",
                    rgba.len()
                )
            }
        }
        Err(e) => format!("TEST_KIND=ICON_RASTER\nFAIL: {e:#}\n"),
    }
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct PlaceHit {
    pub osm_id: i64,
    pub name: String,
    pub kind: String,
    pub lat: f64,
    pub lon: f64,
    /// Named hamlet / neighbourhood / locality containing or nearest the place.
    pub sub_area: String,
    /// Containing municipality (kommune), from OSM admin_level 6–8 polygons.
    pub municipality: String,
}

/// Build or open the offline FTS name index for a region PBF.
/// Returns number of indexed named features (0 on failure; check report string).
#[uniffi::export]
pub fn ensure_place_index(pbf_path: String, index_db_path: String) -> String {
    let pbf = Path::new(&pbf_path);
    if !pbf.is_file() {
        return format!("FAIL: PBF missing: {pbf_path}\n");
    }
    let db = Path::new(&index_db_path);
    if let Some(parent) = db.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // Reuse existing index if non-trivial and built with area-context schema.
    if db.is_file() {
        if let Ok(meta) = std::fs::metadata(db) {
            if meta.len() > 10_000 && driver_break_core::search::NameIndex::is_current_schema(db) {
                return format!("PASS\ncache_hit=true\nindex_db={index_db_path}\n");
            }
        }
    }
    match driver_break_core::search::NameIndex::open(db) {
        Ok(mut idx) => match idx.load_from_pbf(pbf) {
            Ok(n) => format!("PASS\ncache_hit=false\nindexed={n}\nindex_db={index_db_path}\n"),
            Err(e) => format!("FAIL: index load: {e:#}\n"),
        },
        Err(e) => format!("FAIL: open index: {e}\n"),
    }
}

/// Build preprocess-once indexed map packs next to a region PBF (graph + POI/barrier).
///
/// Writes `{stem}.navi-graph-*.rkyv`, `{stem}.navi-poi-barrier.rkyv`, and
/// `{stem}.navi-manifest.json` under `data_dir` (defaults to the PBF parent).
/// Safe to call after download / for migration rebuild from local PBF.
#[uniffi::export]
pub fn ensure_indexed_maps(pbf_path: String, data_dir: String, elev_dir: Option<String>) -> String {
    use driver_break_core::routing::graph::RoutingProfile;
    use driver_break_core::routing::indexed::{
        convert_region_packs, manifest_path, ConvertOptions, NaviManifest, PackStatus,
    };

    let pbf = PathBuf::from(&pbf_path);
    if !pbf.is_file() {
        return format!("FAIL: PBF missing: {pbf_path}\n");
    }
    let data = if data_dir.is_empty() {
        pbf.parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
    } else {
        PathBuf::from(&data_dir)
    };
    let _ = std::fs::create_dir_all(&data);

    let stem = pbf
        .file_name()
        .and_then(|s| s.to_str())
        .map(|name| {
            name.strip_suffix(".osm.pbf")
                .or_else(|| name.strip_suffix(".pbf"))
                .unwrap_or(name)
                .to_string()
        })
        .unwrap_or_else(|| "region".into());
    let man_path = manifest_path(&data, &stem);
    if man_path.is_file() {
        if let Ok(man) = NaviManifest::load(&man_path) {
            // Ready requires graph + POI/barrier v2 + wetland (tiled or monolith).
            // Packs built before wetland tiling / overnight buildings regenerate.
            if man.status_for_pbf(&data, &pbf) == PackStatus::Ready {
                return format!("PASS\ncache_hit=true\nmanifest={}\n", man_path.display());
            }
        }
    }

    ensure_native_logging();
    let mut opts = ConvertOptions::new(&data, &pbf);
    opts.elev_dir = elev_dir.map(PathBuf::from);
    // Motor + hiking covers the shared planning profiles for v1.
    opts.profiles = vec![
        RoutingProfile::Car,
        RoutingProfile::Truck,
        RoutingProfile::Foot,
        RoutingProfile::Bicycle,
    ];
    match convert_region_packs(&opts) {
        Ok(r) => format!(
            "PASS\ncache_hit=false\nconvert_ms={:.1}\nnodes={}\nedges={}\npois={}\nbarrier_segs={}\nwetland_rings={}\ngraph_tiles={}\npeak_rss_mb={:.1}\nmanifest={}\n",
            r.convert_ms,
            r.nodes,
            r.edges,
            r.pois,
            r.barrier_segs,
            r.wetland_rings,
            r.graph_tiles,
            r.peak_rss_mb,
            r.manifest_file
        ),
        Err(e)
            if driver_break_core::routing::region_lock::is_convert_in_progress_err(&e) =>
        {
            "PASS\nskipped=convert_in_progress\n".to_string()
        }
        Err(e) => format!("FAIL: indexed convert: {e:#}\n"),
    }
}

/// Whether region indexed packs are ready for `pbf_path` under `data_dir`.
#[uniffi::export]
pub fn indexed_maps_status(pbf_path: String, data_dir: String) -> String {
    use driver_break_core::routing::indexed::{manifest_path, NaviManifest, PackStatus};

    let pbf = PathBuf::from(&pbf_path);
    let data = if data_dir.is_empty() {
        pbf.parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
    } else {
        PathBuf::from(data_dir)
    };
    let stem = pbf
        .file_name()
        .and_then(|s| s.to_str())
        .map(|name| {
            name.strip_suffix(".osm.pbf")
                .or_else(|| name.strip_suffix(".pbf"))
                .unwrap_or(name)
                .to_string()
        })
        .unwrap_or_else(|| "region".into());
    let man_path = manifest_path(&data, &stem);
    if !man_path.is_file() {
        return "missing\n".into();
    }
    match NaviManifest::load(&man_path) {
        Ok(man) => {
            let packed = match driver_break_core::routing::indexed::fingerprint_pbf_for_packs(
                &data, &pbf, &man,
            ) {
                Ok(p) => p,
                Err(_) => return "missing\n".into(),
            };
            match man.status_for_pbf(&data, &packed) {
                PackStatus::Ready => "ready\n".into(),
                PackStatus::Missing => "missing\n".into(),
                PackStatus::StalePbf => "stale_pbf\n".into(),
                PackStatus::VersionMismatch => "version_mismatch\n".into(),
            }
        }
        Err(e) => format!("error: {e:#}\n"),
    }
}

/// Water-source POI hit sampled along a planned route polyline.
#[derive(uniffi::Record, Debug, Clone)]
pub struct WaterPoiAlongRoute {
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub sample_km: f64,
    pub dist_m: f64,
}

/// Sample a route polyline every `sample_step_km` and collect unique water POIs
/// within `radius_m` of each sample (indexed pack preferred, PBF fallback).
#[uniffi::export]
pub fn water_pois_along_polyline(
    data_dir: String,
    pbf_path: String,
    polyline: String,
    sample_step_km: f64,
    radius_m: f64,
) -> Vec<WaterPoiAlongRoute> {
    let data_dir = PathBuf::from(data_dir);
    let pbf = PathBuf::from(pbf_path);
    let poi_index =
        match driver_break_core::routing::indexed::try_load_poi_barrier_for_plan(&data_dir, &pbf) {
            Ok((poi, _)) => poi,
            Err(_) => match PoiIndex::load_from_pbf(&pbf) {
                Ok(i) => i,
                Err(_) => return Vec::new(),
            },
        };
    let samples = sample_polyline_km(&polyline);
    if samples.len() < 2 {
        return Vec::new();
    }
    let total = samples.last().map(|s| s.2).unwrap_or(0.0);
    let step = sample_step_km.max(1.0);
    let mut seen = std::collections::HashSet::<i64>::new();
    let mut out = Vec::new();
    let mut km = 0.0;
    while km <= total + 0.01 {
        let (lat, lon) = interpolate_at_km(&samples, km);
        for w in poi_index.nearest(PoiCategory::Water, lat, lon, radius_m) {
            if !seen.insert(w.osm_id) {
                continue;
            }
            let name = w
                .name
                .clone()
                .unwrap_or_else(|| format!("water:{}", w.osm_id));
            let dist = haversine_m(lat, lon, w.lat, w.lon);
            out.push(WaterPoiAlongRoute {
                name,
                lat: w.lat,
                lon: w.lon,
                sample_km: km,
                dist_m: dist,
            });
        }
        km += step;
    }
    out
}

/// Offline place / address-style name search (FTS5 prefix).
#[uniffi::export]
pub fn search_places(index_db_path: String, query: String, limit: u32) -> Vec<PlaceHit> {
    let Ok(idx) = driver_break_core::search::NameIndex::open(Path::new(&index_db_path)) else {
        return Vec::new();
    };
    let Ok(hits) = idx.search(&query, limit as usize) else {
        return Vec::new();
    };
    hits.into_iter()
        .map(|h| PlaceHit {
            osm_id: h.osm_id,
            name: h.name,
            kind: h.kind,
            lat: h.lat,
            lon: h.lon,
            sub_area: h.sub_area,
            municipality: h.municipality,
        })
        .collect()
}

/// Place-index hits near a GPS fix (for idle “Currently on …” without a route).
#[uniffi::export]
pub fn nearby_places(
    index_db_path: String,
    lat: f64,
    lon: f64,
    radius_m: f64,
    limit: u32,
) -> Vec<PlaceHit> {
    let Ok(idx) = driver_break_core::search::NameIndex::open(Path::new(&index_db_path)) else {
        return Vec::new();
    };
    let Ok(hits) = idx.nearby(lat, lon, radius_m, limit as usize) else {
        return Vec::new();
    };
    hits.into_iter()
        .map(|h| PlaceHit {
            osm_id: h.osm_id,
            name: h.name,
            kind: h.kind,
            lat: h.lat,
            lon: h.lon,
            sub_area: h.sub_area,
            municipality: h.municipality,
        })
        .collect()
}

#[derive(uniffi::Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum TravelProfile {
    Car,
    CarElectric,
    Truck,
    TruckElectric,
    MobileHome,
    Bicycle,
    /// Battery-assisted cycle / pedelec (primary chip; battery + climb specs).
    BicycleElectric,
    Hiking,
    Motorcycle,
    MotorcycleElectric,
}

impl TravelProfile {
    fn to_core(self) -> driver_break_core::config::Profile {
        match self {
            Self::Car => driver_break_core::config::Profile::Car,
            Self::CarElectric => driver_break_core::config::Profile::CarElectric,
            Self::Truck => driver_break_core::config::Profile::Truck,
            Self::TruckElectric => driver_break_core::config::Profile::TruckElectric,
            Self::MobileHome => driver_break_core::config::Profile::MobileHome,
            Self::Bicycle => driver_break_core::config::Profile::Cycling,
            Self::BicycleElectric => driver_break_core::config::Profile::CyclingElectric,
            Self::Hiking => driver_break_core::config::Profile::Hiking,
            Self::Motorcycle => driver_break_core::config::Profile::Motorcycle,
            Self::MotorcycleElectric => driver_break_core::config::Profile::MotorcycleElectric,
        }
    }
}

/// Whether this profile appears as a primary travel-mode chip.
#[uniffi::export]
pub fn travel_profile_menu_focus(profile: TravelProfile) -> bool {
    profile.to_core().menu_focus()
}

/// Whether eco-mode is user-toggleable for this profile (Hiking/Cycling default on / locked).
#[uniffi::export]
pub fn eco_mode_toggleable(profile: TravelProfile) -> bool {
    profile.to_core().eco_mode_user_toggle()
}

/// Whether avoid-motorways is forced on and locked for this profile (bike / e-bike / hiking).
#[uniffi::export]
pub fn travel_profile_locks_avoid_motorways(profile: TravelProfile) -> bool {
    profile.to_core().locks_avoid_motorways()
}

/// Default eco enabled for profile.
#[uniffi::export]
pub fn eco_mode_default(profile: TravelProfile) -> bool {
    profile.to_core().eco_mode_default()
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiSavedRoute {
    pub id: String,
    pub start_name: String,
    pub end_name: String,
    pub profile: String,
    pub created_at: String,
    pub start_lat: f64,
    pub start_lon: f64,
    pub end_lat: f64,
    pub end_lon: f64,
    pub last_break_lat: Option<f64>,
    pub last_break_lon: Option<f64>,
    pub summary_json: String,
    /// JSON array of via waypoints: `[{"name","lat","lon"}, ...]`.
    pub via_json: String,
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiVehicleLimits {
    pub axle_weight_kg: Option<f64>,
    pub bogie_weight_kg: Option<f64>,
    pub height_m: Option<f64>,
    pub width_m: Option<f64>,
    pub length_m: Option<f64>,
    pub total_weight_kg: Option<f64>,
}

/// Car rest / break settings. Edits persist as the profile default (not trip-only).
#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiCarRestSettings {
    /// Desired hours between breaks (stored as both min and max interval).
    pub break_interval_hours: f64,
    /// Desired break duration in minutes (stored as both min and max duration).
    pub rest_duration_minutes: u32,
    pub eco_mode_enabled: bool,
}

/// Per-profile POI search radii (metres) and road-link policy.
#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiProfilePoiRadii {
    pub search_radius_m: f64,
    pub cabin_radius_m: f64,
    pub network_hut_radius_m: f64,
    pub network_hut_preference_radius_m: f64,
    /// When true, pause / overnight POIs must be linked to the road network.
    pub require_road_link: bool,
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiFuelConfig {
    pub tank_capacity_l: Option<f64>,
    pub fuel_added_l: Option<f64>,
    pub prefer_liters: bool,
}

/// Electric Cycle (e-bike) vehicle specs. Legal assist caps are not enforced.
#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiEbikeConfig {
    pub battery_capacity_wh: Option<f64>,
    pub motor_torque_nm: Option<f64>,
    /// Wheel diameter in inches (20 / 26 / 27.5 / 29 or custom).
    pub wheel_diameter_in: Option<f64>,
}

/// Electric Car pack capacity (kWh). Climbing-capability is not modeled for cars.
#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiEvCarConfig {
    pub battery_capacity_kwh: Option<f64>,
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiGpsFix {
    pub lat: f64,
    pub lon: f64,
    pub available: bool,
    /// Device-reported speed in km/h when known (`Location.hasSpeed()` / gpsd).
    /// `None` when the provider did not supply speed.
    pub speed_kmh: Option<f64>,
}

fn routes_db(data_dir: &str) -> PathBuf {
    Path::new(data_dir).join("navi.db")
}

#[uniffi::export]
pub fn list_saved_routes(data_dir: String) -> Vec<FfiSavedRoute> {
    let Ok(storage) = driver_break_core::storage::Storage::open(routes_db(&data_dir)) else {
        return Vec::new();
    };
    let store = driver_break_core::search::RouteStore::new(&storage);
    let Ok(rows) = store.list() else {
        return Vec::new();
    };
    rows.into_iter()
        .map(|r| FfiSavedRoute {
            id: r.id,
            start_name: r.start_name.unwrap_or_default(),
            end_name: r.end_name.unwrap_or_default(),
            profile: r.profile,
            created_at: r.created_at,
            start_lat: r.start_lat,
            start_lon: r.start_lon,
            end_lat: r.end_lat,
            end_lon: r.end_lon,
            last_break_lat: r.last_break_lat,
            last_break_lon: r.last_break_lon,
            summary_json: r.summary_json,
            via_json: r.via_json,
        })
        .collect()
}

#[uniffi::export]
pub fn delete_saved_route(data_dir: String, id: String) -> bool {
    let Ok(storage) = driver_break_core::storage::Storage::open(routes_db(&data_dir)) else {
        return false;
    };
    driver_break_core::search::RouteStore::new(&storage)
        .delete(&id)
        .is_ok()
}

#[uniffi::export]
pub fn save_named_route(
    data_dir: String,
    start_lat: f64,
    start_lon: f64,
    start_name: String,
    end_lat: f64,
    end_lon: f64,
    end_name: String,
    via_json: String,
    profile: String,
    summary_json: String,
) -> String {
    let Ok(storage) = driver_break_core::storage::Storage::open(routes_db(&data_dir)) else {
        return "FAIL: open db".into();
    };
    let id = uuid::Uuid::new_v4().to_string();
    let route = driver_break_core::search::SavedRoute {
        id: id.clone(),
        start_lat,
        start_lon,
        start_name: Some(start_name),
        end_lat,
        end_lon,
        end_name: Some(end_name),
        via_json,
        profile,
        vehicle_json: "{}".into(),
        summary_json,
        created_at: chrono_like_now(),
        last_break_lat: None,
        last_break_lon: None,
        last_overnight_lat: None,
        last_overnight_lon: None,
    };
    match driver_break_core::search::RouteStore::new(&storage).insert(&route) {
        Ok(()) => format!("PASS\nid={id}\n"),
        Err(e) => format!("FAIL: {e}"),
    }
}

/// Serialize a planned corridor to GPX 1.1.
///
/// * `rte_json` — full route-point list including start and end:
///   `[{"name","lat","lon"}, …]` (same shape as saved `via_json`, but must include
///   the endpoints when used standalone).
/// * `route_polyline` — Navi corridor string `"lon,lat;lon,lat;…"`.
///
/// Returns GPX XML on success, or a string starting with `FAIL:`.
#[uniffi::export]
pub fn route_to_gpx(
    name: String,
    time_iso: String,
    rte_json: String,
    route_polyline: String,
) -> String {
    let route_points = driver_break_core::export::parse_via_json(&rte_json);
    if route_points.len() < 2 {
        return "FAIL: rte_json needs at least start and end points".into();
    }
    let track = driver_break_core::export::parse_route_polyline(&route_polyline);
    let name_opt = {
        let t = name.trim();
        if t.is_empty() {
            None
        } else {
            Some(t)
        }
    };
    let time_opt = {
        let t = time_iso.trim();
        if t.is_empty() {
            None
        } else {
            Some(t)
        }
    };
    driver_break_core::export::to_gpx(name_opt, time_opt, &route_points, &track)
}

/// Look up a saved route, rebuild `<rte>` from stored waypoints, and serialize GPX
/// using a caller-supplied replan polyline (Option A — geometry is not stored in DB).
///
/// Returns GPX XML on success, or a string starting with `FAIL:`.
#[uniffi::export]
pub fn export_saved_route_gpx(
    data_dir: String,
    route_id: String,
    route_polyline: String,
) -> String {
    let Ok(storage) = driver_break_core::storage::Storage::open(routes_db(&data_dir)) else {
        return "FAIL: open db".into();
    };
    let store = driver_break_core::search::RouteStore::new(&storage);
    let Ok(Some(route)) = store.get(&route_id) else {
        return "FAIL: saved route not found".into();
    };
    if route_polyline.trim().is_empty() {
        return "FAIL: empty route polyline (replan before export)".into();
    }
    let route_points = driver_break_core::export::route_points_from_saved(
        route.start_lat,
        route.start_lon,
        route.start_name.as_deref(),
        route.end_lat,
        route.end_lon,
        route.end_name.as_deref(),
        &route.via_json,
    );
    let track = driver_break_core::export::parse_route_polyline(&route_polyline);
    if track.is_empty() {
        return "FAIL: could not parse route polyline".into();
    }
    let name = format!(
        "{} -> {}",
        route.start_name.as_deref().unwrap_or("Start"),
        route.end_name.as_deref().unwrap_or("Destination"),
    );
    let time = route.created_at.trim();
    let time_opt = if time.is_empty() { None } else { Some(time) };
    driver_break_core::export::to_gpx(Some(&name), time_opt, &route_points, &track)
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiSavedPlace {
    pub id: String,
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub kind: String,
    pub created_at: String,
}

#[uniffi::export]
pub fn list_saved_places(data_dir: String) -> Vec<FfiSavedPlace> {
    let Ok(storage) = driver_break_core::storage::Storage::open(routes_db(&data_dir)) else {
        return Vec::new();
    };
    let store = driver_break_core::search::PlaceStore::new(&storage);
    let Ok(rows) = store.list() else {
        return Vec::new();
    };
    rows.into_iter()
        .map(|p| FfiSavedPlace {
            id: p.id,
            name: p.name,
            lat: p.lat,
            lon: p.lon,
            kind: p.kind,
            created_at: p.created_at,
        })
        .collect()
}

#[uniffi::export]
pub fn save_named_place(
    data_dir: String,
    name: String,
    lat: f64,
    lon: f64,
    kind: String,
) -> String {
    let Ok(storage) = driver_break_core::storage::Storage::open(routes_db(&data_dir)) else {
        return "FAIL: open db".into();
    };
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return "FAIL: empty name".into();
    }
    let id = uuid::Uuid::new_v4().to_string();
    let place = driver_break_core::search::SavedPlace {
        id: id.clone(),
        name: trimmed.to_string(),
        lat,
        lon,
        kind,
        created_at: chrono_like_now(),
    };
    match driver_break_core::search::PlaceStore::new(&storage).insert(&place) {
        Ok(()) => format!("PASS\nid={id}\n"),
        Err(e) => format!("FAIL: {e}"),
    }
}

#[uniffi::export]
pub fn rename_saved_place(data_dir: String, id: String, name: String) -> bool {
    let Ok(storage) = driver_break_core::storage::Storage::open(routes_db(&data_dir)) else {
        return false;
    };
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return false;
    }
    driver_break_core::search::PlaceStore::new(&storage)
        .rename(&id, trimmed)
        .unwrap_or(false)
}

#[uniffi::export]
pub fn delete_saved_place(data_dir: String, id: String) -> bool {
    let Ok(storage) = driver_break_core::storage::Storage::open(routes_db(&data_dir)) else {
        return false;
    };
    driver_break_core::search::PlaceStore::new(&storage)
        .delete(&id)
        .is_ok()
}

fn chrono_like_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".into())
}

#[uniffi::export]
pub fn load_vehicle_limits(data_dir: String) -> FfiVehicleLimits {
    let Ok(storage) = driver_break_core::storage::Storage::open(routes_db(&data_dir)) else {
        return FfiVehicleLimits {
            axle_weight_kg: None,
            bogie_weight_kg: None,
            height_m: None,
            width_m: None,
            length_m: None,
            total_weight_kg: None,
        };
    };
    let store = driver_break_core::storage::ConfigStore::new(&storage);
    let limits = store.load_vehicle_limits().unwrap_or_default();
    FfiVehicleLimits {
        axle_weight_kg: limits.axle_weight_kg,
        bogie_weight_kg: limits.bogie_weight_kg,
        height_m: limits.height_m,
        width_m: limits.width_m,
        length_m: limits.length_m,
        total_weight_kg: limits.total_weight_kg,
    }
}

#[uniffi::export]
pub fn save_vehicle_limits(data_dir: String, limits: FfiVehicleLimits) -> bool {
    let Ok(storage) = driver_break_core::storage::Storage::open(routes_db(&data_dir)) else {
        return false;
    };
    let store = driver_break_core::storage::ConfigStore::new(&storage);
    store
        .save_vehicle_limits(&driver_break_core::config::VehicleLimits {
            axle_weight_kg: limits.axle_weight_kg,
            bogie_weight_kg: limits.bogie_weight_kg,
            height_m: limits.height_m,
            width_m: limits.width_m,
            length_m: limits.length_m,
            total_weight_kg: limits.total_weight_kg,
        })
        .is_ok()
}

/// Soft preference for official hiking/cycling route networks (default off).
#[uniffi::export]
pub fn load_prefer_official_networks(data_dir: String) -> bool {
    let Ok(storage) = driver_break_core::storage::Storage::open(routes_db(&data_dir)) else {
        return false;
    };
    let store = driver_break_core::storage::ConfigStore::new(&storage);
    store.load_prefer_official_networks().unwrap_or(false)
}

#[uniffi::export]
pub fn save_prefer_official_networks(data_dir: String, prefer: bool) -> bool {
    let Ok(storage) = driver_break_core::storage::Storage::open(routes_db(&data_dir)) else {
        return false;
    };
    let store = driver_break_core::storage::ConfigStore::new(&storage);
    store.save_prefer_official_networks(prefer).is_ok()
}

/// Soft preference for pilgrim routes (default off).
#[uniffi::export]
pub fn load_prefer_pilgrim_routes(data_dir: String) -> bool {
    let Ok(storage) = driver_break_core::storage::Storage::open(routes_db(&data_dir)) else {
        return false;
    };
    let store = driver_break_core::storage::ConfigStore::new(&storage);
    store.load_prefer_pilgrim_routes().unwrap_or(false)
}

#[uniffi::export]
pub fn save_prefer_pilgrim_routes(data_dir: String, prefer: bool) -> bool {
    let Ok(storage) = driver_break_core::storage::Storage::open(routes_db(&data_dir)) else {
        return false;
    };
    let store = driver_break_core::storage::ConfigStore::new(&storage);
    store.save_prefer_pilgrim_routes(prefer).is_ok()
}

/// Allow networked (DNT/STF/…) huts as hiking auto-via waypoints (default off).
/// Geographic via only — does not imply membership or right of entry.
#[uniffi::export]
pub fn load_use_networked_cabins(data_dir: String) -> bool {
    let Ok(storage) = driver_break_core::storage::Storage::open(routes_db(&data_dir)) else {
        return false;
    };
    let store = driver_break_core::storage::ConfigStore::new(&storage);
    store.load_use_networked_cabins().unwrap_or(false)
}

#[uniffi::export]
pub fn save_use_networked_cabins(data_dir: String, prefer: bool) -> bool {
    let Ok(storage) = driver_break_core::storage::Storage::open(routes_db(&data_dir)) else {
        return false;
    };
    let store = driver_break_core::storage::ConfigStore::new(&storage);
    store.save_use_networked_cabins(prefer).is_ok()
}

/// Bicycle / electric-cycle terrain capability: `road`, `trekking`, or `mountain`.
#[uniffi::export]
pub fn load_bike_capability(data_dir: String) -> String {
    let Ok(storage) = driver_break_core::storage::Storage::open(routes_db(&data_dir)) else {
        return BikeCapability::Trekking.as_str().to_string();
    };
    let store = driver_break_core::storage::ConfigStore::new(&storage);
    store
        .load_bike_capability()
        .unwrap_or_else(|_| BikeCapability::Trekking.as_str().to_string())
}

#[uniffi::export]
pub fn save_bike_capability(data_dir: String, capability: String) -> bool {
    let Ok(storage) = driver_break_core::storage::Storage::open(routes_db(&data_dir)) else {
        return false;
    };
    let store = driver_break_core::storage::ConfigStore::new(&storage);
    let cap = BikeCapability::parse(&capability);
    store.save_bike_capability(cap.as_str()).is_ok()
}

/// Motor surface routing strictness: `car` (default) or `offroad` / `4x4`.
#[uniffi::export]
pub fn load_surface_routing_mode(data_dir: String) -> String {
    let Ok(storage) = driver_break_core::storage::Storage::open(routes_db(&data_dir)) else {
        return SurfaceRoutingMode::Car.as_str().to_string();
    };
    let store = driver_break_core::storage::ConfigStore::new(&storage);
    store
        .load_surface_routing_mode()
        .unwrap_or_else(|_| SurfaceRoutingMode::Car.as_str().to_string())
}

#[uniffi::export]
pub fn save_surface_routing_mode(data_dir: String, mode: String) -> bool {
    let Ok(storage) = driver_break_core::storage::Storage::open(routes_db(&data_dir)) else {
        return false;
    };
    let store = driver_break_core::storage::ConfigStore::new(&storage);
    let parsed = SurfaceRoutingMode::parse(&mode);
    store.save_surface_routing_mode(parsed.as_str()).is_ok()
}

/// Load whether the user is a DNT/STF/… network hut member (default false).
/// Gates overnight-stay preference only — not auto-via waypoints.
#[uniffi::export]
pub fn load_network_hut_member(data_dir: String) -> bool {
    let Ok(storage) = driver_break_core::storage::Storage::open(routes_db(&data_dir)) else {
        return false;
    };
    let store = driver_break_core::storage::ConfigStore::new(&storage);
    store.load_network_hut_member().unwrap_or(false)
}

#[uniffi::export]
pub fn save_network_hut_member(data_dir: String, is_member: bool) -> bool {
    let Ok(storage) = driver_break_core::storage::Storage::open(routes_db(&data_dir)) else {
        return false;
    };
    let store = driver_break_core::storage::ConfigStore::new(&storage);
    store.save_network_hut_member(is_member).is_ok()
}

fn ffi_from_poi_radii(r: &ProfilePoiRadii) -> FfiProfilePoiRadii {
    FfiProfilePoiRadii {
        search_radius_m: r.search_radius_m,
        cabin_radius_m: r.cabin_radius_m,
        network_hut_radius_m: r.network_hut_radius_m,
        network_hut_preference_radius_m: r.network_hut_preference_radius_m,
        require_road_link: r.require_road_link,
    }
}

/// Load POI search radii for the active travel profile (persisted per profile).
#[uniffi::export]
pub fn load_profile_poi_radii(data_dir: String, profile: TravelProfile) -> FfiProfilePoiRadii {
    let fallback =
        ffi_from_poi_radii(ProfilePoiRadiiTable::default().for_profile(profile.to_core()));
    let Ok(storage) = driver_break_core::storage::Storage::open(routes_db(&data_dir)) else {
        return fallback;
    };
    let table = driver_break_core::storage::ConfigStore::new(&storage)
        .load_profile_poi_radii()
        .unwrap_or_default();
    ffi_from_poi_radii(table.for_profile(profile.to_core()))
}

/// Save POI search radii for the active travel profile.
#[uniffi::export]
pub fn save_profile_poi_radii(
    data_dir: String,
    profile: TravelProfile,
    settings: FfiProfilePoiRadii,
) -> bool {
    let Ok(storage) = driver_break_core::storage::Storage::open(routes_db(&data_dir)) else {
        return false;
    };
    let store = driver_break_core::storage::ConfigStore::new(&storage);
    let mut table = store.load_profile_poi_radii().unwrap_or_default();
    *table.for_profile_mut(profile.to_core()) = ProfilePoiRadii {
        search_radius_m: settings.search_radius_m,
        cabin_radius_m: settings.cabin_radius_m,
        network_hut_radius_m: settings.network_hut_radius_m,
        network_hut_preference_radius_m: settings.network_hut_preference_radius_m,
        require_road_link: settings.require_road_link,
    }
    .sanitized();
    store.save_profile_poi_radii(&table).is_ok()
}

#[uniffi::export]
pub fn load_car_rest_settings(data_dir: String) -> FfiCarRestSettings {
    let default = driver_break_core::config::CarRestParams::default();
    let fallback = FfiCarRestSettings {
        break_interval_hours: default.break_interval_min_hours,
        rest_duration_minutes: default.break_duration_min_minutes,
        eco_mode_enabled: default.eco_mode_enabled,
    };
    let Ok(storage) = driver_break_core::storage::Storage::open(routes_db(&data_dir)) else {
        return fallback;
    };
    let store = driver_break_core::storage::ConfigStore::new(&storage);
    let rest = store.load_rest_config().unwrap_or_default();
    FfiCarRestSettings {
        break_interval_hours: rest.car.break_interval_min_hours,
        rest_duration_minutes: rest.car.break_duration_min_minutes,
        eco_mode_enabled: rest.car.eco_mode_enabled,
    }
}

/// Persist car break interval / rest duration as the default RestConfig (not a one-trip override).
#[uniffi::export]
pub fn save_car_rest_settings(data_dir: String, settings: FfiCarRestSettings) -> bool {
    let Ok(storage) = driver_break_core::storage::Storage::open(routes_db(&data_dir)) else {
        return false;
    };
    let store = driver_break_core::storage::ConfigStore::new(&storage);
    let mut rest = store.load_rest_config().unwrap_or_default();
    let hours = settings.break_interval_hours.clamp(1.0, 12.0);
    let mins = settings.rest_duration_minutes.clamp(5, 120);
    rest.car.break_interval_min_hours = hours;
    rest.car.break_interval_max_hours = hours;
    rest.car.break_duration_min_minutes = mins;
    rest.car.break_duration_max_minutes = mins;
    rest.car.eco_mode_enabled = settings.eco_mode_enabled;
    store.save_rest_config(&rest).is_ok()
}

/// Truck / mobile-home EC 561/2006 rest settings (persisted on `RestConfig.truck`).
#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiTruckRestSettings {
    pub mandatory_break_after_hours: f64,
    pub break_duration_minutes: u32,
    pub prefer_split_break: bool,
    pub max_daily_driving_hours: f64,
    pub max_daily_driving_extended_hours: f64,
    pub max_daily_extensions_per_week: u32,
    pub max_weekly_driving_hours: f64,
    pub max_fortnightly_driving_hours: f64,
    pub exceptional_extension_armed: bool,
    pub eco_mode_enabled: bool,
}

fn truck_settings_from_params(
    t: &driver_break_core::config::TruckRestParams,
) -> FfiTruckRestSettings {
    FfiTruckRestSettings {
        mandatory_break_after_hours: t.mandatory_break_after_hours,
        break_duration_minutes: t.break_duration_minutes,
        prefer_split_break: t.prefer_split_break,
        max_daily_driving_hours: t.max_daily_driving_hours,
        max_daily_driving_extended_hours: t.max_daily_driving_extended_hours,
        max_daily_extensions_per_week: t.max_daily_extensions_per_week,
        max_weekly_driving_hours: t.max_weekly_driving_hours,
        max_fortnightly_driving_hours: t.max_fortnightly_driving_hours,
        exceptional_extension_armed: t.exceptional_extension_armed,
        eco_mode_enabled: t.eco_mode_enabled,
    }
}

#[uniffi::export]
pub fn load_truck_rest_settings(data_dir: String) -> FfiTruckRestSettings {
    let default = driver_break_core::config::TruckRestParams::default();
    let fallback = truck_settings_from_params(&default);
    let Ok(storage) = driver_break_core::storage::Storage::open(routes_db(&data_dir)) else {
        return fallback;
    };
    let store = driver_break_core::storage::ConfigStore::new(&storage);
    let rest = store.load_rest_config().unwrap_or_default();
    truck_settings_from_params(&rest.truck)
}

#[uniffi::export]
pub fn save_truck_rest_settings(data_dir: String, settings: FfiTruckRestSettings) -> bool {
    let Ok(storage) = driver_break_core::storage::Storage::open(routes_db(&data_dir)) else {
        return false;
    };
    let store = driver_break_core::storage::ConfigStore::new(&storage);
    let mut rest = store.load_rest_config().unwrap_or_default();
    rest.truck.mandatory_break_after_hours = settings.mandatory_break_after_hours.clamp(1.0, 6.0);
    rest.truck.break_duration_minutes = settings.break_duration_minutes.clamp(15, 90);
    rest.truck.prefer_split_break = settings.prefer_split_break;
    rest.truck.max_daily_driving_hours = settings.max_daily_driving_hours.clamp(1.0, 15.0);
    rest.truck.max_daily_driving_extended_hours =
        settings.max_daily_driving_extended_hours.clamp(1.0, 15.0);
    rest.truck.max_daily_extensions_per_week = settings.max_daily_extensions_per_week.min(7);
    rest.truck.max_weekly_driving_hours = settings.max_weekly_driving_hours.clamp(1.0, 80.0);
    rest.truck.max_fortnightly_driving_hours =
        settings.max_fortnightly_driving_hours.clamp(1.0, 120.0);
    rest.truck.exceptional_extension_armed = settings.exceptional_extension_armed;
    rest.truck.eco_mode_enabled = settings.eco_mode_enabled;
    store.save_rest_config(&rest).is_ok()
}

/// Arm / disarm the +1 h exceptional extension (explicit opt-in; not a silent default).
#[uniffi::export]
pub fn set_truck_exceptional_extension_armed(data_dir: String, armed: bool) -> bool {
    let Ok(storage) = driver_break_core::storage::Storage::open(routes_db(&data_dir)) else {
        return false;
    };
    let store = driver_break_core::storage::ConfigStore::new(&storage);
    let mut rest = store.load_rest_config().unwrap_or_default();
    rest.truck.exceptional_extension_armed = armed;
    store.save_rest_config(&rest).is_ok()
}

fn load_rest_config_near_cache(cache: &Path) -> RestConfig {
    let data_dir = cache.parent().unwrap_or(cache);
    let Ok(storage) = driver_break_core::storage::Storage::open(data_dir.join("navi.db")) else {
        return RestConfig::default();
    };
    driver_break_core::storage::ConfigStore::new(&storage)
        .load_rest_config()
        .unwrap_or_default()
}

fn load_profile_poi_radii_near_cache(cache: &Path) -> ProfilePoiRadiiTable {
    let data_dir = cache.parent().unwrap_or(cache);
    let Ok(storage) = driver_break_core::storage::Storage::open(data_dir.join("navi.db")) else {
        return ProfilePoiRadiiTable::default();
    };
    driver_break_core::storage::ConfigStore::new(&storage)
        .load_profile_poi_radii()
        .unwrap_or_default()
}

fn load_use_networked_cabins_near_cache(cache: &Path) -> bool {
    let data_dir = cache.parent().unwrap_or(cache);
    let Ok(storage) = driver_break_core::storage::Storage::open(data_dir.join("navi.db")) else {
        return false;
    };
    driver_break_core::storage::ConfigStore::new(&storage)
        .load_use_networked_cabins()
        .unwrap_or(false)
}

fn load_network_hut_member_near_cache(cache: &Path) -> bool {
    let data_dir = cache.parent().unwrap_or(cache);
    let Ok(storage) = driver_break_core::storage::Storage::open(data_dir.join("navi.db")) else {
        return false;
    };
    driver_break_core::storage::ConfigStore::new(&storage)
        .load_network_hut_member()
        .unwrap_or(false)
}

fn load_ebike_config_near_cache(cache: &Path) -> driver_break_core::config::EbikeConfig {
    let data_dir = cache.parent().unwrap_or(cache);
    let Ok(storage) = driver_break_core::storage::Storage::open(data_dir.join("navi.db")) else {
        return driver_break_core::config::EbikeConfig::default();
    };
    driver_break_core::storage::ConfigStore::new(&storage)
        .load_ebike_config()
        .unwrap_or_default()
}

fn load_ev_car_config_near_cache(cache: &Path) -> driver_break_core::config::EvCarConfig {
    let data_dir = cache.parent().unwrap_or(cache);
    let Ok(storage) = driver_break_core::storage::Storage::open(data_dir.join("navi.db")) else {
        return driver_break_core::config::EvCarConfig::default();
    };
    driver_break_core::storage::ConfigStore::new(&storage)
        .load_ev_car_config()
        .unwrap_or_default()
}

fn load_truck_history_near_cache(cache: &Path) -> driver_break_core::config::TruckDrivingHistory {
    let data_dir = cache.parent().unwrap_or(cache);
    let Ok(storage) = driver_break_core::storage::Storage::open(data_dir.join("navi.db")) else {
        return driver_break_core::config::TruckDrivingHistory::default();
    };
    driver_break_core::storage::ConfigStore::new(&storage)
        .load_truck_driving_history()
        .unwrap_or_default()
}

fn save_rest_and_truck_history_near_cache(
    cache: &Path,
    rest: &RestConfig,
    history: &driver_break_core::config::TruckDrivingHistory,
) {
    let data_dir = cache.parent().unwrap_or(cache);
    let Ok(storage) = driver_break_core::storage::Storage::open(data_dir.join("navi.db")) else {
        return;
    };
    let store = driver_break_core::storage::ConfigStore::new(&storage);
    let _ = store.save_rest_config(rest);
    let _ = store.save_truck_driving_history(history);
}

/// UTC civil date `YYYY-MM-DD` from Unix days (Howard Hinnant civil_from_days).
fn civil_today_utc() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let z = (secs / 86_400) as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

fn iso_week_id_utc() -> String {
    let today = civil_today_utc();
    let y: i32 = today[0..4].parse().unwrap_or(2026);
    let m: u32 = today[5..7].parse().unwrap_or(1);
    let d: u32 = today[8..10].parse().unwrap_or(1);
    static CUM: [u32; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let doy = CUM[(m.saturating_sub(1) as usize).min(11)] + d;
    let week = ((doy.saturating_sub(1)) / 7 + 1).min(53);
    format!("{y:04}-W{week:02}")
}

#[uniffi::export]
pub fn load_fuel_config(data_dir: String) -> FfiFuelConfig {
    let Ok(storage) = driver_break_core::storage::Storage::open(routes_db(&data_dir)) else {
        return FfiFuelConfig {
            tank_capacity_l: None,
            fuel_added_l: None,
            prefer_liters: true,
        };
    };
    let store = driver_break_core::storage::ConfigStore::new(&storage);
    let fuel = store.load_fuel_config().unwrap_or_default();
    FfiFuelConfig {
        tank_capacity_l: fuel.tank_capacity_l,
        fuel_added_l: fuel.fuel_added_l,
        prefer_liters: fuel.prefer_liters,
    }
}

#[uniffi::export]
pub fn save_fuel_config(data_dir: String, config: FfiFuelConfig) -> bool {
    let Ok(storage) = driver_break_core::storage::Storage::open(routes_db(&data_dir)) else {
        return false;
    };
    let store = driver_break_core::storage::ConfigStore::new(&storage);
    store
        .save_fuel_config(&driver_break_core::config::FuelConfig {
            tank_capacity_l: config.tank_capacity_l,
            fuel_added_l: config.fuel_added_l,
            prefer_liters: config.prefer_liters,
        })
        .is_ok()
}

#[uniffi::export]
pub fn load_ebike_config(data_dir: String) -> FfiEbikeConfig {
    let defaults = driver_break_core::config::EbikeConfig::default();
    let Ok(storage) = driver_break_core::storage::Storage::open(routes_db(&data_dir)) else {
        return FfiEbikeConfig {
            battery_capacity_wh: defaults.battery_capacity_wh,
            motor_torque_nm: defaults.motor_torque_nm,
            wheel_diameter_in: defaults.wheel_diameter_in,
        };
    };
    let store = driver_break_core::storage::ConfigStore::new(&storage);
    let cfg = store.load_ebike_config().unwrap_or(defaults);
    FfiEbikeConfig {
        battery_capacity_wh: cfg.battery_capacity_wh,
        motor_torque_nm: cfg.motor_torque_nm,
        wheel_diameter_in: cfg.wheel_diameter_in,
    }
}

#[uniffi::export]
pub fn save_ebike_config(data_dir: String, config: FfiEbikeConfig) -> bool {
    let Ok(storage) = driver_break_core::storage::Storage::open(routes_db(&data_dir)) else {
        return false;
    };
    let store = driver_break_core::storage::ConfigStore::new(&storage);
    store
        .save_ebike_config(&driver_break_core::config::EbikeConfig {
            battery_capacity_wh: config.battery_capacity_wh,
            motor_torque_nm: config.motor_torque_nm,
            wheel_diameter_in: config.wheel_diameter_in,
        })
        .is_ok()
}

#[uniffi::export]
pub fn load_ev_car_config(data_dir: String) -> FfiEvCarConfig {
    let defaults = driver_break_core::config::EvCarConfig::default();
    let Ok(storage) = driver_break_core::storage::Storage::open(routes_db(&data_dir)) else {
        return FfiEvCarConfig {
            battery_capacity_kwh: defaults.battery_capacity_kwh,
        };
    };
    let store = driver_break_core::storage::ConfigStore::new(&storage);
    let cfg = store.load_ev_car_config().unwrap_or(defaults);
    FfiEvCarConfig {
        battery_capacity_kwh: cfg.battery_capacity_kwh,
    }
}

#[uniffi::export]
pub fn save_ev_car_config(data_dir: String, config: FfiEvCarConfig) -> bool {
    let Ok(storage) = driver_break_core::storage::Storage::open(routes_db(&data_dir)) else {
        return false;
    };
    let store = driver_break_core::storage::ConfigStore::new(&storage);
    store
        .save_ev_car_config(&driver_break_core::config::EvCarConfig {
            battery_capacity_kwh: config.battery_capacity_kwh,
        })
        .is_ok()
}

/// Process-wide DEM cache so HUD `elevation_at` does not re-inflate GeoTIFF
/// tiles on every GPS fix.
fn hud_elevation_service(elev_dir: &Path) -> ElevationService {
    static SLOT: OnceLock<Mutex<std::collections::HashMap<PathBuf, ElevationCache>>> =
        OnceLock::new();
    let map = SLOT.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    let mut guard = map.lock().unwrap_or_else(|e| e.into_inner());
    let cache = guard
        .entry(elev_dir.to_path_buf())
        .or_insert_with(|| ElevationCache::new(elev_dir))
        .clone();
    ElevationService::new(cache)
}

/// Sample on-disk DEM elevation (meters) at a WGS84 point, or null if no tile.
#[uniffi::export]
pub fn elevation_at(elev_dir: String, lat: f64, lon: f64) -> Option<f64> {
    hud_elevation_service(Path::new(&elev_dir)).get_elevation(lat, lon)
}

/// Last GPS fix pushed from the Android host ([`update_gps_fix`]).
///
/// Rust cannot call Android LocationManager. The host must push each fused /
/// GPS update here; [`last_gps_fix`] then returns that value. Until the first
/// push, `available` is false (not a demo coordinate).
fn gps_fix_slot() -> &'static std::sync::Mutex<FfiGpsFix> {
    static SLOT: std::sync::OnceLock<std::sync::Mutex<FfiGpsFix>> = std::sync::OnceLock::new();
    SLOT.get_or_init(|| {
        std::sync::Mutex::new(FfiGpsFix {
            lat: 0.0,
            lon: 0.0,
            available: false,
            speed_kmh: None,
        })
    })
}

/// Push the device LocationManager / fused fix into the native layer.
///
/// `speed_kmh` is optional: pass the provider speed converted to km/h
/// (Android `Location.speed` is m/s → × 3.6), or `None` when unknown.
#[uniffi::export]
pub fn update_gps_fix(lat: f64, lon: f64, available: bool, speed_kmh: Option<f64>) {
    if let Ok(mut g) = gps_fix_slot().lock() {
        *g = FfiGpsFix {
            lat,
            lon,
            available,
            speed_kmh: speed_kmh.filter(|v| v.is_finite() && *v >= 0.0),
        };
    }
}

/// Last GPS fix from [`update_gps_fix`] (Android LocationManager is the source
/// of truth; this mirror exists for hosts/tests that read via UniFFI).
#[uniffi::export]
pub fn last_gps_fix() -> FfiGpsFix {
    gps_fix_slot()
        .lock()
        .map(|g| g.clone())
        .unwrap_or(FfiGpsFix {
            lat: 0.0,
            lon: 0.0,
            available: false,
            speed_kmh: None,
        })
}

/// Live GPS speed (km/h) from the last [`update_gps_fix`], or `None` when the
/// host has not pushed a fix / did not supply speed.
#[uniffi::export]
pub fn current_speed_kmh() -> Option<f64> {
    let fix = last_gps_fix();
    if !fix.available {
        return None;
    }
    fix.speed_kmh
}

/// Positive when `speed_kmh` exceeds `limit_kmh`; `None` if either value is
/// missing/non-finite. Convenience for HUD overspeed chrome (optional).
#[uniffi::export]
pub fn overspeed_delta_kmh(speed_kmh: Option<f64>, limit_kmh: Option<f64>) -> Option<f64> {
    let s = speed_kmh.filter(|v| v.is_finite())?;
    let lim = limit_kmh.filter(|v| v.is_finite() && *v > 0.0)?;
    Some(s - lim)
}

/// Resolve applicable road speed limit (km/h): `maxspeed:conditional` at local
/// now, else posted `maxspeed`, else highway-class ETA fallback.
#[uniffi::export]
pub fn resolve_speed_limit_kmh(
    posted_kmh: Option<f64>,
    maxspeed_conditional: Option<String>,
    highway: Option<String>,
) -> f64 {
    driver_break_core::routing::speed_camera::applicable_limit_or_fallback_kmh(
        posted_kmh,
        maxspeed_conditional.as_deref(),
        highway.as_deref(),
        None,
    )
}

/// Format a short validation blurb for avoid-motorways / toll / ferry preferences.
#[uniffi::export]
pub fn format_avoid_motorways_report(
    avoid_motorways: bool,
    priority_path_share_pct: f64,
) -> String {
    format_route_avoidance_report(avoid_motorways, false, false, priority_path_share_pct)
}

/// Extended avoidance report (motorways + tolls + ferries). Defaults for toll/ferry: off.
#[uniffi::export]
pub fn format_route_avoidance_report(
    avoid_motorways: bool,
    avoid_tolls: bool,
    avoid_ferries: bool,
    priority_path_share_pct: f64,
) -> String {
    let opts = driver_break_core::RouteOptions {
        avoid_motorways,
        avoid_tolls,
        avoid_ferries,
        vehicle: None,
        departure_local: None,
    };
    driver_break_core::format_route_avoidance_report(&opts, 0, priority_path_share_pct)
}

/// Approach-box phase for a distance (shared with voice guidance timing).
#[uniffi::export]
pub fn approach_phase_for_distance(active: bool, distance_m: f64) -> String {
    let g = driver_break_core::NavGuidance {
        active,
        kind: driver_break_core::ManeuverKind::Unknown,
        distance_m,
        next_street: None,
        roundabout_exit: None,
    };
    match g.phase() {
        driver_break_core::ApproachPhase::Hidden => "hidden".into(),
        driver_break_core::ApproachPhase::Appear => "appear".into(),
        driver_break_core::ApproachPhase::Urgency => "urgency".into(),
    }
}

/// Format approach distance for display (metric vs imperial).
#[uniffi::export]
pub fn format_approach_distance(distance_m: f64, prefer_metric: bool) -> String {
    driver_break_core::format_distance_m(distance_m, prefer_metric)
}

/// Locked approach thresholds (meters).
#[uniffi::export]
pub fn approach_appear_m() -> f64 {
    driver_break_core::APPROACH_APPEAR_M
}

#[uniffi::export]
pub fn approach_urgency_m() -> f64 {
    driver_break_core::APPROACH_URGENCY_M
}

#[uniffi::export]
pub fn approach_hide_m() -> f64 {
    driver_break_core::APPROACH_HIDE_M
}

/// True when speed-camera display may be offered (NO/UK opt-in jurisdictions).
#[uniffi::export]
pub fn speed_camera_jurisdiction_allows(lat: f64, lon: f64) -> bool {
    use driver_break_core::routing::speed_camera::{
        resolve_speed_camera_jurisdiction_at, SpeedCameraJurisdiction,
    };
    resolve_speed_camera_jurisdiction_at(lat, lon) == SpeedCameraJurisdiction::AllowedOptIn
}

/// Load speed cameras from a PBF as JSON (point + average-speed records).
#[uniffi::export]
pub fn load_speed_cameras_json(pbf_path: String) -> String {
    match driver_break_core::routing::speed_camera::load_speed_cameras_from_pbf(&pbf_path) {
        Ok(cams) => {
            let rows: Vec<serde_json::Value> = cams
                .into_iter()
                .map(|c| {
                    serde_json::json!({
                        "osm_id": c.osm_id,
                        "lat": c.lat,
                        "lon": c.lon,
                        "kind": match c.kind {
                            driver_break_core::routing::speed_camera::SpeedCameraKind::Point => "point",
                            driver_break_core::routing::speed_camera::SpeedCameraKind::AverageSpeed => "average_speed",
                        },
                        "maxspeed_kmh": c.maxspeed_kmh,
                        "maxspeed_conditional": c.maxspeed_conditional,
                        "zone_from_lat": c.zone_from_lat,
                        "zone_from_lon": c.zone_from_lon,
                        "zone_to_lat": c.zone_to_lat,
                        "zone_to_lon": c.zone_to_lon,
                        "zone_length_m": c.zone_length_m,
                    })
                })
                .collect();
            serde_json::to_string(&rows).unwrap_or_else(|_| "[]".into())
        }
        Err(e) => format!(
            r#"{{"error":{}}}"#,
            serde_json::to_string(&e.to_string()).unwrap_or_default()
        ),
    }
}

/// Nearest speed-camera warning JSON for live HUD (empty object when none).
#[uniffi::export]
pub fn nearest_speed_camera_warning_json(
    cameras_json: String,
    lat: f64,
    lon: f64,
    opted_in: bool,
) -> String {
    use driver_break_core::routing::speed_camera::{
        nearest_speed_camera_warning, SpeedCameraKind, SpeedCameraRecord,
    };
    let Ok(raw) = serde_json::from_str::<Vec<serde_json::Value>>(&cameras_json) else {
        return "{}".into();
    };
    let mut cams = Vec::new();
    for v in raw {
        let kind = match v.get("kind").and_then(|x| x.as_str()).unwrap_or("point") {
            "average_speed" => SpeedCameraKind::AverageSpeed,
            _ => SpeedCameraKind::Point,
        };
        cams.push(SpeedCameraRecord {
            osm_id: v.get("osm_id").and_then(|x| x.as_i64()).unwrap_or(0),
            lat: v.get("lat").and_then(|x| x.as_f64()).unwrap_or(0.0),
            lon: v.get("lon").and_then(|x| x.as_f64()).unwrap_or(0.0),
            kind,
            maxspeed_kmh: v.get("maxspeed_kmh").and_then(|x| x.as_f64()),
            maxspeed_conditional: v
                .get("maxspeed_conditional")
                .and_then(|x| x.as_str())
                .map(|s| s.to_string()),
            zone_from_lat: v.get("zone_from_lat").and_then(|x| x.as_f64()),
            zone_from_lon: v.get("zone_from_lon").and_then(|x| x.as_f64()),
            zone_to_lat: v.get("zone_to_lat").and_then(|x| x.as_f64()),
            zone_to_lon: v.get("zone_to_lon").and_then(|x| x.as_f64()),
            zone_length_m: v.get("zone_length_m").and_then(|x| x.as_f64()),
        });
    }
    let Some(w) = nearest_speed_camera_warning(&cams, lat, lon, opted_in, None) else {
        return "{}".into();
    };
    let phase = match w.phase {
        driver_break_core::ApproachPhase::Hidden => "hidden",
        driver_break_core::ApproachPhase::Appear => "appear",
        driver_break_core::ApproachPhase::Urgency => "urgency",
    };
    serde_json::json!({
        "kind": match w.kind {
            SpeedCameraKind::Point => "point",
            SpeedCameraKind::AverageSpeed => "average_speed",
        },
        "phase": phase,
        "distance_m": w.distance_m,
        "applicable_limit_kmh": w.applicable_limit_kmh,
        "zone_remaining_m": w.zone_remaining_m,
        "zone_time_budget_s": w.zone_time_budget_s,
        "label": w.label,
    })
    .to_string()
}

/// True when Norwegian road-sign warnings may be shown at `(lat, lon)`.
#[uniffi::export]
pub fn road_sign_jurisdiction_allows(lat: f64, lon: f64) -> bool {
    use driver_break_core::routing::road_sign::{
        resolve_road_sign_jurisdiction_at, RoadSignJurisdiction,
    };
    resolve_road_sign_jurisdiction_at(lat, lon) == RoadSignJurisdiction::Norway
}

/// Load catalogue-matched road signs from a region PBF as JSON.
#[uniffi::export]
pub fn load_road_signs_json(pbf_path: String) -> String {
    use driver_break_core::routing::road_sign::{load_catalog, load_road_signs_from_pbf};
    let catalog = match load_catalog() {
        Ok(c) => c,
        Err(e) => {
            return format!(
                r#"{{"error":{}}}"#,
                serde_json::to_string(&e.to_string()).unwrap_or_default()
            );
        }
    };
    match load_road_signs_from_pbf(&pbf_path, &catalog) {
        Ok(signs) => {
            let rows: Vec<serde_json::Value> = signs
                .into_iter()
                .map(|s| {
                    serde_json::json!({
                        "osm_id": s.osm_id,
                        "lat": s.lat,
                        "lon": s.lon,
                        "icon_key": s.icon_key,
                        "code": s.code,
                        "name_en": s.name_en,
                        "traffic_sign_raw": s.traffic_sign_raw,
                    })
                })
                .collect();
            serde_json::to_string(&rows).unwrap_or_else(|_| "[]".into())
        }
        Err(e) => format!(
            r#"{{"error":{}}}"#,
            serde_json::to_string(&e.to_string()).unwrap_or_default()
        ),
    }
}

/// Nearest road-sign warning JSON for live HUD (empty object when none).
#[uniffi::export]
pub fn nearest_road_sign_warning_json(signs_json: String, lat: f64, lon: f64) -> String {
    use driver_break_core::routing::road_sign::{nearest_road_sign_warning, RoadSignRecord};
    let Ok(raw) = serde_json::from_str::<Vec<serde_json::Value>>(&signs_json) else {
        return "{}".into();
    };
    let mut signs = Vec::new();
    for v in raw {
        signs.push(RoadSignRecord {
            osm_id: v.get("osm_id").and_then(|x| x.as_i64()).unwrap_or(0),
            lat: v.get("lat").and_then(|x| x.as_f64()).unwrap_or(0.0),
            lon: v.get("lon").and_then(|x| x.as_f64()).unwrap_or(0.0),
            icon_key: v
                .get("icon_key")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string(),
            code: v
                .get("code")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string(),
            name_en: v
                .get("name_en")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string(),
            traffic_sign_raw: v
                .get("traffic_sign_raw")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string(),
        });
    }
    let Some(w) = nearest_road_sign_warning(&signs, lat, lon) else {
        return "{}".into();
    };
    let phase = match w.phase {
        driver_break_core::ApproachPhase::Hidden => "hidden",
        driver_break_core::ApproachPhase::Appear => "appear",
        driver_break_core::ApproachPhase::Urgency => "urgency",
    };
    serde_json::json!({
        "phase": phase,
        "distance_m": w.distance_m,
        "icon_key": w.icon_key,
        "code": w.code,
        "name_en": w.name_en,
        "label": w.label,
    })
    .to_string()
}

struct LiveHazardStore {
    pbf_key: String,
    full: driver_break_core::routing::live_hazard::LiveHazardIndex,
    window_key: String,
    window: driver_break_core::routing::live_hazard::LiveHazardIndex,
}

static LIVE_HAZARD_STORE: Mutex<Option<LiveHazardStore>> = Mutex::new(None);

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiLiveHazardLoadStats {
    pub signs: u32,
    pub children: u32,
    pub cameras: u32,
    pub bumps: u32,
    pub compact_json_utf8: u64,
    pub cone_m: f64,
}

/// Parse compact hazard points once into the native cache (signs, children centroids,
/// cameras, speed bumps). Window refresh reuses the road_label_near cell bbox.
#[uniffi::export]
pub fn ensure_live_hazards_loaded(pbf_path: String) -> FfiLiveHazardLoadStats {
    use driver_break_core::routing::live_hazard::{LiveHazardIndex, LIVE_HAZARD_CONE_M};
    let key = pbf_path.clone();
    {
        let guard = LIVE_HAZARD_STORE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(store) = guard.as_ref() {
            if store.pbf_key == key {
                return FfiLiveHazardLoadStats {
                    signs: store.full.signs.len() as u32,
                    children: store.full.children.len() as u32,
                    cameras: store.full.cameras.len() as u32,
                    bumps: store.full.bumps.len() as u32,
                    compact_json_utf8: store.full.estimated_compact_json_utf8() as u64,
                    cone_m: LIVE_HAZARD_CONE_M,
                };
            }
        }
    }
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        LiveHazardIndex::load_from_pbf(&pbf_path)
    })) {
        Ok(Ok((full, stats))) => {
            let empty = LiveHazardIndex::default();
            let out = FfiLiveHazardLoadStats {
                signs: stats.signs as u32,
                children: stats.children as u32,
                cameras: stats.cameras as u32,
                bumps: stats.bumps as u32,
                compact_json_utf8: stats.compact_json_utf8 as u64,
                cone_m: LIVE_HAZARD_CONE_M,
            };
            if let Ok(mut guard) = LIVE_HAZARD_STORE.lock() {
                *guard = Some(LiveHazardStore {
                    pbf_key: key,
                    full,
                    window_key: String::new(),
                    window: empty,
                });
            }
            log::info!(
                target: "NaviNative",
                "live_hazards loaded signs={} children={} cameras={} bumps={}",
                out.signs, out.children, out.cameras, out.bumps
            );
            out
        }
        Ok(Err(e)) => {
            log::error!(target: "NaviNative", "live_hazards load error: {e:#}");
            FfiLiveHazardLoadStats {
                signs: 0,
                children: 0,
                cameras: 0,
                bumps: 0,
                compact_json_utf8: 0,
                cone_m: LIVE_HAZARD_CONE_M,
            }
        }
        Err(_) => {
            log::error!(target: "NaviNative", "live_hazards load panicked");
            FfiLiveHazardLoadStats {
                signs: 0,
                children: 0,
                cameras: 0,
                bumps: 0,
                compact_json_utf8: 0,
                cone_m: LIVE_HAZARD_CONE_M,
            }
        }
    }
}

/// Build the native compact cache from already-loaded JSON layers (parse once).
/// Prefer this over [`ensure_live_hazards_loaded`] when the host already scanned the PBF.
#[uniffi::export]
pub fn live_hazards_ingest_from_json(
    pbf_key: String,
    signs_json: String,
    cameras_json: String,
    children_json: String,
    bumps_json: String,
) -> FfiLiveHazardLoadStats {
    use driver_break_core::routing::live_hazard::{
        ChildrenCentroid, LiveHazardIndex, SpeedBumpPoint, LIVE_HAZARD_CONE_M,
    };
    use driver_break_core::routing::road_sign::RoadSignRecord;
    use driver_break_core::routing::speed_camera::{SpeedCameraKind, SpeedCameraRecord};

    let signs: Vec<RoadSignRecord> = serde_json::from_str::<Vec<serde_json::Value>>(&signs_json)
        .unwrap_or_default()
        .into_iter()
        .map(|v| RoadSignRecord {
            osm_id: v.get("osm_id").and_then(|x| x.as_i64()).unwrap_or(0),
            lat: v.get("lat").and_then(|x| x.as_f64()).unwrap_or(0.0),
            lon: v.get("lon").and_then(|x| x.as_f64()).unwrap_or(0.0),
            icon_key: v
                .get("icon_key")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string(),
            code: v
                .get("code")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string(),
            name_en: v
                .get("name_en")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string(),
            traffic_sign_raw: v
                .get("traffic_sign_raw")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string(),
        })
        .collect();

    let cameras: Vec<SpeedCameraRecord> =
        serde_json::from_str::<Vec<serde_json::Value>>(&cameras_json)
            .unwrap_or_default()
            .into_iter()
            .map(|v| {
                let kind = match v.get("kind").and_then(|x| x.as_str()).unwrap_or("point") {
                    "average_speed" => SpeedCameraKind::AverageSpeed,
                    _ => SpeedCameraKind::Point,
                };
                SpeedCameraRecord {
                    osm_id: v.get("osm_id").and_then(|x| x.as_i64()).unwrap_or(0),
                    lat: v.get("lat").and_then(|x| x.as_f64()).unwrap_or(0.0),
                    lon: v.get("lon").and_then(|x| x.as_f64()).unwrap_or(0.0),
                    kind,
                    maxspeed_kmh: v.get("maxspeed_kmh").and_then(|x| x.as_f64()),
                    maxspeed_conditional: v
                        .get("maxspeed_conditional")
                        .and_then(|x| x.as_str())
                        .map(|s| s.to_string()),
                    zone_from_lat: v.get("zone_from_lat").and_then(|x| x.as_f64()),
                    zone_from_lon: v.get("zone_from_lon").and_then(|x| x.as_f64()),
                    zone_to_lat: v.get("zone_to_lat").and_then(|x| x.as_f64()),
                    zone_to_lon: v.get("zone_to_lon").and_then(|x| x.as_f64()),
                    zone_length_m: v.get("zone_length_m").and_then(|x| x.as_f64()),
                }
            })
            .collect();

    let children: Vec<ChildrenCentroid> =
        serde_json::from_str::<Vec<serde_json::Value>>(&children_json)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|v| {
                let category = match v.get("category").and_then(|x| x.as_str())? {
                    "school" => "school",
                    "kindergarten" => "kindergarten",
                    "playground" => "playground",
                    _ => return None,
                };
                Some(ChildrenCentroid {
                    osm_id: v.get("osm_id").and_then(|x| x.as_i64()).unwrap_or(0),
                    lat: v.get("lat").and_then(|x| x.as_f64()).unwrap_or(0.0),
                    lon: v.get("lon").and_then(|x| x.as_f64()).unwrap_or(0.0),
                    name: v
                        .get("name")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string(),
                    category,
                })
            })
            .collect();

    let bumps: Vec<SpeedBumpPoint> = serde_json::from_str::<Vec<serde_json::Value>>(&bumps_json)
        .unwrap_or_default()
        .into_iter()
        .map(|v| SpeedBumpPoint {
            osm_id: v.get("osm_id").and_then(|x| x.as_i64()).unwrap_or(0),
            lat: v.get("lat").and_then(|x| x.as_f64()).unwrap_or(0.0),
            lon: v.get("lon").and_then(|x| x.as_f64()).unwrap_or(0.0),
            calming: v
                .get("calming")
                .and_then(|x| x.as_str())
                .unwrap_or("hump")
                .to_string(),
        })
        .collect();

    let full = LiveHazardIndex {
        signs,
        children,
        cameras,
        bumps,
    };
    let out = FfiLiveHazardLoadStats {
        signs: full.signs.len() as u32,
        children: full.children.len() as u32,
        cameras: full.cameras.len() as u32,
        bumps: full.bumps.len() as u32,
        compact_json_utf8: full.estimated_compact_json_utf8() as u64,
        cone_m: LIVE_HAZARD_CONE_M,
    };
    if let Ok(mut guard) = LIVE_HAZARD_STORE.lock() {
        *guard = Some(LiveHazardStore {
            pbf_key,
            full,
            window_key: String::new(),
            window: LiveHazardIndex::default(),
        });
    }
    out
}

/// Load speed-bump / hump / table nodes as compact JSON.
#[uniffi::export]
pub fn load_speed_bumps_json(pbf_path: String) -> String {
    match driver_break_core::routing::live_hazard::load_speed_bumps_json(&pbf_path) {
        Ok(s) => s,
        Err(e) => format!(
            r#"{{"error":{}}}"#,
            serde_json::to_string(&e.to_string()).unwrap_or_default()
        ),
    }
}

/// Road-sign JSON from the native compact cache (empty if not loaded).
#[uniffi::export]
pub fn live_hazards_road_signs_json() -> String {
    let guard = LIVE_HAZARD_STORE.lock().unwrap_or_else(|e| e.into_inner());
    match guard.as_ref() {
        Some(s) => s.full.signs_json(),
        None => "[]".into(),
    }
}

/// Children-centroid JSON from the native compact cache.
#[uniffi::export]
pub fn live_hazards_children_json() -> String {
    let guard = LIVE_HAZARD_STORE.lock().unwrap_or_else(|e| e.into_inner());
    match guard.as_ref() {
        Some(s) => s.full.children_json(),
        None => "[]".into(),
    }
}

/// Speed-camera JSON from the native compact cache.
#[uniffi::export]
pub fn live_hazards_speed_cameras_json() -> String {
    use driver_break_core::routing::speed_camera::SpeedCameraKind;
    let guard = LIVE_HAZARD_STORE.lock().unwrap_or_else(|e| e.into_inner());
    let Some(s) = guard.as_ref() else {
        return "[]".into();
    };
    let rows: Vec<_> = s
        .full
        .cameras
        .iter()
        .map(|c| {
            serde_json::json!({
                "osm_id": c.osm_id,
                "lat": c.lat,
                "lon": c.lon,
                "kind": match c.kind {
                    SpeedCameraKind::Point => "point",
                    SpeedCameraKind::AverageSpeed => "average_speed",
                },
                "maxspeed_kmh": c.maxspeed_kmh,
                "maxspeed_conditional": c.maxspeed_conditional,
                "zone_from_lat": c.zone_from_lat,
                "zone_from_lon": c.zone_from_lon,
                "zone_to_lat": c.zone_to_lat,
                "zone_to_lon": c.zone_to_lon,
                "zone_length_m": c.zone_length_m,
            })
        })
        .collect();
    serde_json::to_string(&rows).unwrap_or_else(|_| "[]".into())
}

/// Hard-coded live cone radius (metres). Distinct from the 200 m route corridor.
#[uniffi::export]
pub fn live_hazard_cone_m() -> f64 {
    driver_break_core::routing::live_hazard::LIVE_HAZARD_CONE_M
}

fn with_live_hazard_window<R>(
    lat: f64,
    lon: f64,
    f: impl FnOnce(&driver_break_core::routing::live_hazard::LiveHazardIndex) -> R,
) -> Option<R> {
    use driver_break_core::routing::live_hazard::live_hazard_cell_key;
    let mut guard = LIVE_HAZARD_STORE.lock().ok()?;
    let store = guard.as_mut()?;
    let key = live_hazard_cell_key(lat, lon);
    if store.window_key != key {
        store.window = store.full.windowed(lat, lon);
        store.window_key = key;
    }
    Some(f(&store.window))
}

fn road_sign_warning_to_json(w: &driver_break_core::routing::road_sign::RoadSignWarning) -> String {
    use driver_break_core::ApproachPhase;
    let phase = match w.phase {
        ApproachPhase::Hidden => "hidden",
        ApproachPhase::Appear => "appear",
        ApproachPhase::Urgency => "urgency",
    };
    serde_json::json!({
        "phase": phase,
        "distance_m": w.distance_m,
        "icon_key": w.icon_key,
        "code": w.code,
        "name_en": w.name_en,
        "label": w.label,
        "cone_m": driver_break_core::routing::live_hazard::LIVE_HAZARD_CONE_M,
        "source": "live_cone",
    })
    .to_string()
}

/// Route-independent 300 m heading-cone road-sign / bump / children warning.
/// `heading_deg` null = isotropic disk within the cone radius.
#[uniffi::export]
pub fn live_hazard_cone_road_sign_warning_json(
    lat: f64,
    lon: f64,
    heading_deg: Option<f64>,
) -> String {
    use driver_break_core::routing::live_hazard::{
        live_sign_and_children, nearest_live_sign_style_warning,
    };
    let Some((sign_opt, children, w_opt)) = with_live_hazard_window(lat, lon, |window| {
        let (sign_opt, children) = live_sign_and_children(window, lat, lon, heading_deg);
        let w_opt = nearest_live_sign_style_warning(window, lat, lon, heading_deg);
        (sign_opt, children, w_opt)
    }) else {
        return "{}".into();
    };
    let Some(w) = w_opt else {
        return "{}".into();
    };
    let mut v: serde_json::Value =
        serde_json::from_str(&road_sign_warning_to_json(&w)).unwrap_or(serde_json::json!({}));
    if let Some((_, cat)) = children {
        let tagged_142 = sign_opt.as_ref().map(|s| s.code.as_str()) == Some("142");
        if w.code == "142" && !tagged_142 {
            v["source"] = serde_json::json!("children_proximity");
            v["category"] = serde_json::json!(cat);
        }
    }
    v.to_string()
}

/// Children proximity only (source=`children_proximity`) inside the live cone.
#[uniffi::export]
pub fn live_hazard_cone_children_warning_json(
    lat: f64,
    lon: f64,
    heading_deg: Option<f64>,
) -> String {
    use driver_break_core::routing::live_hazard::nearest_live_children_warning;
    let Some(hit) = with_live_hazard_window(lat, lon, |window| {
        nearest_live_children_warning(window, lat, lon, heading_deg)
    }) else {
        return "{}".into();
    };
    let Some((w, cat)) = hit else {
        return "{}".into();
    };
    let mut v: serde_json::Value =
        serde_json::from_str(&road_sign_warning_to_json(&w)).unwrap_or(serde_json::json!({}));
    v["source"] = serde_json::json!("children_proximity");
    v["category"] = serde_json::json!(cat);
    v.to_string()
}

/// Speed-camera warning inside the live cone (same jurisdiction / opt-in gates).
#[uniffi::export]
pub fn live_hazard_cone_speed_camera_warning_json(
    lat: f64,
    lon: f64,
    heading_deg: Option<f64>,
    opted_in: bool,
) -> String {
    use driver_break_core::routing::live_hazard::nearest_live_speed_camera_warning;
    use driver_break_core::ApproachPhase;
    let Some(w_opt) = with_live_hazard_window(lat, lon, |window| {
        nearest_live_speed_camera_warning(window, lat, lon, heading_deg, opted_in)
    }) else {
        return "{}".into();
    };
    let Some(w) = w_opt else {
        return "{}".into();
    };
    let phase = match w.phase {
        ApproachPhase::Hidden => "hidden",
        ApproachPhase::Appear => "appear",
        ApproachPhase::Urgency => "urgency",
    };
    serde_json::json!({
        "kind": match w.kind {
            driver_break_core::routing::speed_camera::SpeedCameraKind::Point => "point",
            driver_break_core::routing::speed_camera::SpeedCameraKind::AverageSpeed => "average_speed",
        },
        "phase": phase,
        "distance_m": w.distance_m,
        "applicable_limit_kmh": w.applicable_limit_kmh,
        "zone_remaining_m": w.zone_remaining_m,
        "zone_time_budget_s": w.zone_time_budget_s,
        "label": w.label,
        "cone_m": driver_break_core::routing::live_hazard::LIVE_HAZARD_CONE_M,
        "source": "live_cone",
    })
    .to_string()
}

/// Upcoming speed limit on the existing road_label cell graph within the 300 m cone.
/// No separate speed-limit dataset — reuses [`road_near_info`]'s graph cache.
#[uniffi::export]
pub fn live_speed_limit_cone_json(
    pbf_path: String,
    cache_dir: String,
    elev_dir: String,
    lat: f64,
    lon: f64,
    heading_deg: Option<f64>,
    profile: TravelProfile,
    current_limit_kmh: Option<f64>,
) -> String {
    let _ch = driver_break_core::download::progress::ChannelGuard::enter(
        driver_break_core::download::progress::ProgressChannel::Cone,
    );
    use driver_break_core::routing::live_hazard::{
        live_speed_limit_in_cone, speed_limit_cone_as_sign_warning, LIVE_HAZARD_CONE_M,
    };
    // Ensure the cell graph is warm via the same path as idle street/limit.
    let _ = road_near_info(pbf_path, cache_dir, elev_dir, lat, lon, profile, 80.0);
    let guard = match ROAD_LABEL_GRAPH.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    let Some(cache) = guard.as_ref() else {
        return "{}".into();
    };
    let Some(hit) = live_speed_limit_in_cone(&cache.graph, lat, lon, heading_deg) else {
        return "{}".into();
    };
    let mut out = serde_json::json!({
        "distance_m": hit.distance_m,
        "speed_limit_kmh": hit.speed_limit_kmh,
        "highway": hit.highway,
        "maxspeed_posted": hit.maxspeed_posted,
        "cone_m": LIVE_HAZARD_CONE_M,
        "source": "live_cone_graph",
    });
    if let Some(w) = speed_limit_cone_as_sign_warning(&hit, current_limit_kmh) {
        out["road_sign"] =
            serde_json::from_str(&road_sign_warning_to_json(&w)).unwrap_or(serde_json::json!({}));
    }
    out.to_string()
}

/// Densify a lat/lon polyline into simulation samples (no route plan required).
/// `coords_json`: `[[lat,lon],…]`.
#[uniffi::export]
pub fn sim_samples_json_from_lat_lon(coords_json: String, speed_kmh: f64) -> String {
    let Ok(raw) = serde_json::from_str::<Vec<serde_json::Value>>(&coords_json) else {
        return "[]".into();
    };
    let coords: Vec<(f64, f64)> = raw
        .iter()
        .filter_map(|v| {
            if let Some(arr) = v.as_array() {
                if arr.len() >= 2 {
                    return Some((arr[0].as_f64()?, arr[1].as_f64()?));
                }
            }
            Some((v.get("lat")?.as_f64()?, v.get("lon")?.as_f64()?))
        })
        .collect();
    let samples = build_sim_samples_from_lat_lon(&coords, speed_kmh.max(1.0), Some("residential"));
    samples_to_json(&samples)
}

/// Load child-zone POIs as **centroids only** (nodes + way centroids — no way vertices).
#[uniffi::export]
pub fn load_school_pois_json(pbf_path: String) -> String {
    match driver_break_core::routing::live_hazard::load_children_centroids_json(&pbf_path) {
        Ok(s) => s,
        Err(e) => format!(
            r#"{{"error":{}}}"#,
            serde_json::to_string(&e.to_string()).unwrap_or_default()
        ),
    }
}

/// Keep school POIs within `margin_m` of the route corridor (`sim_samples_json`).
#[uniffi::export]
pub fn schools_near_route_corridor_json(
    schools_json: String,
    sim_samples_json: String,
    margin_m: f64,
) -> String {
    use driver_break_core::CorridorBand;
    let Ok(schools) = serde_json::from_str::<Vec<serde_json::Value>>(&schools_json) else {
        return "[]".into();
    };
    let Ok(samples) = serde_json::from_str::<Vec<serde_json::Value>>(&sim_samples_json) else {
        return "[]".into();
    };
    let coords: Vec<(f64, f64)> = samples
        .iter()
        .filter_map(|s| Some((s.get("lat")?.as_f64()?, s.get("lon")?.as_f64()?)))
        .collect();
    if coords.len() < 2 {
        return "[]".into();
    }
    let band = CorridorBand::from_lat_lon(&coords, margin_m.max(1.0));
    let filtered: Vec<serde_json::Value> = schools
        .into_iter()
        .filter(|v| {
            let lat = v.get("lat").and_then(|x| x.as_f64()).unwrap_or(0.0);
            let lon = v.get("lon").and_then(|x| x.as_f64()).unwrap_or(0.0);
            band.contains(lat, lon)
        })
        .collect();
    serde_json::to_string(&filtered).unwrap_or_else(|_| "[]".into())
}

/// Nearest children-zone proximity fallback warning JSON (empty object when none).
/// Picks the single closest POI across school/kindergarten/playground — no stacked warnings.
#[uniffi::export]
pub fn nearest_school_proximity_warning_json(schools_json: String, lat: f64, lon: f64) -> String {
    use driver_break_core::{
        ApproachPhase, APPROACH_APPEAR_M, APPROACH_HIDE_M, APPROACH_URGENCY_M,
    };

    let Ok(pois) = serde_json::from_str::<Vec<serde_json::Value>>(&schools_json) else {
        return "{}".into();
    };
    let mut best: Option<(serde_json::Value, f64)> = None;
    for poi in pois {
        let plat = poi.get("lat").and_then(|x| x.as_f64()).unwrap_or(0.0);
        let plon = poi.get("lon").and_then(|x| x.as_f64()).unwrap_or(0.0);
        let d = driver_break_core::haversine_km(lat, lon, plat, plon) * 1_000.0;
        match &best {
            Some((_, bd)) if d >= *bd => {}
            _ => best = Some((poi, d)),
        }
    }
    let Some((poi, d)) = best else {
        return "{}".into();
    };
    let phase = if !d.is_finite() || d > APPROACH_APPEAR_M || d <= APPROACH_HIDE_M {
        ApproachPhase::Hidden
    } else if d <= APPROACH_URGENCY_M {
        ApproachPhase::Urgency
    } else {
        ApproachPhase::Appear
    };
    if phase == ApproachPhase::Hidden {
        return "{}".into();
    }
    let phase_s = match phase {
        ApproachPhase::Hidden => "hidden",
        ApproachPhase::Appear => "appear",
        ApproachPhase::Urgency => "urgency",
    };
    let poi_name = poi
        .get("name")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let category = poi
        .get("category")
        .and_then(|x| x.as_str())
        .unwrap_or("school");
    let osm_id = poi.get("osm_id").and_then(|x| x.as_i64()).unwrap_or(0);
    serde_json::json!({
        "phase": phase_s,
        "distance_m": d,
        "icon_key": "no_sign_142",
        "code": "142",
        "name_en": "Children",
        "label": if poi_name.is_empty() { "Children ahead".to_string() } else { format!("Children zone: {poi_name}") },
        "source": "children_proximity",
        "category": category,
        "poi_osm_id": osm_id,
        "school_osm_id": osm_id,
        "poi_name": poi_name,
        "school_name": poi_name,
    })
    .to_string()
}

/// Human highway-class label when OSM name/ref are missing (never a raw tag).
#[uniffi::export]
pub fn highway_class_display_label(highway: Option<String>) -> String {
    driver_break_core::routing::eta::highway_class_display_label(highway.as_deref()).to_string()
}

/// Current-road HUD label: `name`, else `ref`, else highway-class display label.
#[uniffi::export]
pub fn format_current_road_label(
    name: Option<String>,
    road_ref: Option<String>,
    highway: Option<String>,
) -> String {
    driver_break_core::current_road_label(name.as_deref(), road_ref.as_deref(), highway.as_deref())
}

/// Grid cell size (degrees) for idle-GPS road-label graph caches (~5.5 km at mid-latitudes).
const ROAD_LABEL_CELL_DEG: f64 = 0.05;
/// Extra cells of pad around the GPS cell so edges near the border stay in-graph.
const ROAD_LABEL_PAD_CELLS: f64 = 1.0;

struct RoadLabelGraphCache {
    key: String,
    graph: RouteGraph,
    sticky: RoadLabelSticky,
}

static ROAD_LABEL_GRAPH: Mutex<Option<RoadLabelGraphCache>> = Mutex::new(None);

fn road_label_bbox(lat: f64, lon: f64) -> [f64; 4] {
    let cell = ROAD_LABEL_CELL_DEG;
    let pad = ROAD_LABEL_PAD_CELLS * cell;
    let i = (lat / cell).floor();
    let j = (lon / cell).floor();
    [
        i * cell - pad,
        j * cell - pad,
        (i + 1.0) * cell + pad,
        (j + 1.0) * cell + pad,
    ]
}

fn road_label_cache_key(profile: RoutingProfile, bbox: [f64; 4]) -> String {
    format!(
        "{:?}_{:.2}_{:.2}_{:.2}_{:.2}",
        profile, bbox[0], bbox[1], bbox[2], bbox[3]
    )
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiRoadNearInfo {
    /// Street label (name → ref → highway class), empty when no edge in range.
    pub label: String,
    /// Applicable limit km/h (conditional → posted → highway fallback); 0 when no edge.
    pub speed_limit_kmh: f64,
    pub highway: Option<String>,
    /// True when a base OSM `maxspeed` tag was present on the locked edge.
    pub maxspeed_posted: bool,
    /// True when a matching `maxspeed:conditional` window is active now.
    pub limit_from_conditional: bool,
}

fn empty_road_near_info() -> FfiRoadNearInfo {
    FfiRoadNearInfo {
        label: String::new(),
        speed_limit_kmh: 0.0,
        highway: None,
        maxspeed_posted: false,
        limit_from_conditional: false,
    }
}

fn road_near_info_from_sticky(
    sticky: &mut RoadLabelSticky,
    graph: &RouteGraph,
    lat: f64,
    lon: f64,
    max_m: f64,
) -> FfiRoadNearInfo {
    let Some(hit) = sticky.update_hit(graph, lat, lon, max_m) else {
        return empty_road_near_info();
    };
    let speed_limit_kmh = hit.speed_limit_kmh_at(None);
    let limit_from_conditional = hit.limit_from_conditional_at(None);
    let maxspeed_posted = hit
        .maxspeed_kmh
        .filter(|v| v.is_finite() && *v > 0.0)
        .is_some();
    FfiRoadNearInfo {
        label: hit.label,
        speed_limit_kmh,
        highway: hit.highway,
        maxspeed_posted,
        limit_from_conditional,
    }
}

/// Nearest OSM way at `(lat, lon)` for idle GPS: label + applicable speed limit.
///
/// Shares sticky hysteresis with [`road_label_near`] (same graph cache) so the
/// speed-limit value does not flip-flop near parallel roads.
#[uniffi::export]
pub fn road_near_info(
    pbf_path: String,
    cache_dir: String,
    elev_dir: String,
    lat: f64,
    lon: f64,
    profile: TravelProfile,
    max_m: f64,
) -> FfiRoadNearInfo {
    let _ch = driver_break_core::download::progress::ChannelGuard::enter(
        driver_break_core::download::progress::ProgressChannel::Cone,
    );
    if !lat.is_finite() || !lon.is_finite() {
        return empty_road_near_info();
    }
    let pbf = Path::new(pbf_path.trim());
    if !pbf.is_file() {
        return empty_road_near_info();
    }
    let routing_profile = if profile == TravelProfile::Hiking {
        RoutingProfile::Foot
    } else {
        RoutingProfile::from(profile.to_core())
    };
    let bbox = road_label_bbox(lat, lon);
    let key = road_label_cache_key(routing_profile, bbox);
    let max_m = max_m.max(1.0);
    {
        let mut guard = ROAD_LABEL_GRAPH.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(cached) = guard.as_mut() {
            if cached.key == key {
                return road_near_info_from_sticky(
                    &mut cached.sticky,
                    &cached.graph,
                    lat,
                    lon,
                    max_m,
                );
            }
        }
    }
    let elev = PathBuf::from(&elev_dir);
    let cache = PathBuf::from(&cache_dir);
    let _ = std::fs::create_dir_all(&cache);
    let eco = eco_for_travel_profile(profile);
    let elevation = ElevationService::new(ElevationCache::new(&elev));
    let data_dir = pbf
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let (graph, _) = match driver_break_core::routing::indexed::try_load_graph_for_plan_bbox(
        &data_dir,
        pbf,
        routing_profile,
        Some(bbox),
    ) {
        Ok(g) => (g, true),
        Err(_) => match load_or_build_reweighted_bbox(
            pbf,
            &data_dir,
            &cache,
            routing_profile,
            &elevation,
            &eco,
            bbox,
        ) {
            Ok(v) => v,
            Err(_) => return empty_road_near_info(),
        },
    };
    let mut sticky = RoadLabelSticky::new();
    let info = road_near_info_from_sticky(&mut sticky, &graph, lat, lon, max_m);
    if let Ok(mut guard) = ROAD_LABEL_GRAPH.lock() {
        *guard = Some(RoadLabelGraphCache { key, graph, sticky });
    }
    info
}

/// Nearest OSM way label at `(lat, lon)` for idle GPS (no planned corridor).
///
/// Loads a small bbox-clipped routing graph (cached under `cache_dir`), then
/// snaps to the nearest edge within `max_m` using full edge shape + sticky
/// hysteresis (sustained margin before switching parallel roads). Prefer this
/// over place-index address voting at junctions. Empty string when no edge is
/// close enough or inputs are missing.
///
/// Prefer [`road_near_info`] when the HUD also needs the speed limit.
#[uniffi::export]
pub fn road_label_near(
    pbf_path: String,
    cache_dir: String,
    elev_dir: String,
    lat: f64,
    lon: f64,
    profile: TravelProfile,
    max_m: f64,
) -> String {
    road_near_info(pbf_path, cache_dir, elev_dir, lat, lon, profile, max_m).label
}

/// Applicable speed limit (km/h) for the road under the last GPS fix, using the
/// same sticky nearest-edge path as [`road_near_info`]. `None` when GPS is
/// unavailable or no edge is in range.
#[uniffi::export]
pub fn current_speed_limit_kmh(
    pbf_path: String,
    cache_dir: String,
    elev_dir: String,
    profile: TravelProfile,
    max_m: f64,
) -> Option<f64> {
    let fix = last_gps_fix();
    if !fix.available {
        return None;
    }
    let info = road_near_info(
        pbf_path, cache_dir, elev_dir, fix.lat, fix.lon, profile, max_m,
    );
    if info.label.is_empty() && info.speed_limit_kmh <= 0.0 {
        return None;
    }
    Some(info.speed_limit_kmh)
}

/// Canonical Geofabrik `-latest.osm.pbf` URL for a region path
/// (e.g. `europe/norway/ostlandet`). Prefer this over host-side URL string
/// interpolation so Android and core share one builder.
#[uniffi::export]
pub fn geofabrik_latest_pbf_url(geofabrik_region: String) -> String {
    driver_break_core::routing::geofabrik_latest_pbf_url(geofabrik_region.trim().trim_matches('/'))
}

/// Canonical Geofabrik `{region}-updates` base URL (no trailing slash).
#[uniffi::export]
pub fn geofabrik_updates_base_url(geofabrik_region: String) -> String {
    driver_break_core::routing::geofabrik_updates_base(geofabrik_region.trim().trim_matches('/'))
}

/// Bind a Geofabrik region to the local extract (required before update checks).
#[uniffi::export]
pub fn bind_geofabrik_region(
    data_dir: String,
    geofabrik_region: String,
    pbf_filename: String,
    local_sequence: Option<u64>,
) -> String {
    match driver_break_core::routing::bind_geofabrik_extract(
        Path::new(&data_dir),
        &geofabrik_region,
        &pbf_filename,
        local_sequence,
        None,
    ) {
        Ok(meta) => format!(
            "PASS\nregion={}\npbf={}\nsequence={:?}\nUSER_VISIBLE=true\n",
            meta.geofabrik_region, meta.pbf_filename, meta.local_sequence
        ),
        Err(e) => format!("FAIL: {e:#}\n"),
    }
}

/// Opt-in OSM update check against Geofabrik (never downloads map data by itself).
#[uniffi::export]
pub fn check_osm_updates(data_dir: String) -> String {
    match driver_break_core::routing::check_for_updates(Path::new(&data_dir)) {
        Ok(plan) => {
            let body = driver_break_core::routing::format_update_plan(&plan);
            format!("USER_VISIBLE=true\n{body}")
        }
        Err(e) => format!("FAIL: {e:#}\nUSER_VISIBLE=true\n"),
    }
}

/// Apply the pending plan from the last check (user-confirmed).
#[uniffi::export]
pub fn apply_osm_update(data_dir: String) -> String {
    match driver_break_core::routing::apply_pending_update(Path::new(&data_dir)) {
        Ok(r) => r.report,
        Err(e) => format!("FAIL: {e:#}\nUSER_VISIBLE=true\n"),
    }
}

/// Weekly reminder opt-in (surfaces a check prompt; never auto-applies).
#[uniffi::export]
pub fn set_osm_weekly_reminder(data_dir: String, enabled: bool) -> bool {
    driver_break_core::routing::set_weekly_reminder_opt_in(Path::new(&data_dir), enabled).is_ok()
}

#[uniffi::export]
pub fn osm_weekly_reminder_due(data_dir: String) -> bool {
    let Ok(Some(meta)) = driver_break_core::routing::RegionExtractMeta::load(Path::new(&data_dir))
    else {
        return false;
    };
    driver_break_core::routing::weekly_reminder_due(&meta)
}

#[uniffi::export]
pub fn osm_update_staleness_days() -> u64 {
    driver_break_core::routing::STALENESS_FULL_REDOWNLOAD_DAYS
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiTrackStation {
    pub id: String,
    pub lat: f64,
    pub lon: f64,
    pub symbol_table: String,
    pub symbol_code: String,
    pub symbol_key: String,
    pub last_heard_unix: u64,
    pub comment: String,
}

#[derive(uniffi::Object)]
pub struct FfiTrackStore {
    inner: std::sync::Mutex<driver_break_core::tracks::TrackStore>,
}

#[uniffi::export]
impl FfiTrackStore {
    #[uniffi::constructor]
    pub fn new(timeout_s: u64, range_km: f64) -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self {
            inner: std::sync::Mutex::new(driver_break_core::tracks::TrackStore::new(
                timeout_s, range_km,
            )),
        })
    }

    /// Upsert by id. Returns "created" or "updated". Never duplicates.
    pub fn upsert(
        &self,
        id: String,
        lat: f64,
        lon: f64,
        symbol_table: String,
        symbol_code: String,
        symbol_key: String,
        last_heard_unix: u64,
        comment: String,
    ) -> String {
        let station = driver_break_core::tracks::TrackStation {
            id,
            lat,
            lon,
            symbol_table,
            symbol_code,
            symbol_key,
            last_heard_unix,
            comment,
        };
        let mut guard = self.inner.lock().expect("track store lock");
        match guard.upsert(station) {
            driver_break_core::tracks::UpsertOutcome::Created => "created".into(),
            driver_break_core::tracks::UpsertOutcome::Updated => "updated".into(),
        }
    }

    pub fn expire(&self, now_unix: u64) -> Vec<String> {
        self.inner
            .lock()
            .expect("track store lock")
            .expire(now_unix)
    }

    pub fn visible(&self, center_lat: f64, center_lon: f64) -> Vec<FfiTrackStation> {
        self.inner
            .lock()
            .expect("track store lock")
            .visible(center_lat, center_lon)
            .into_iter()
            .map(|s| FfiTrackStation {
                id: s.id.clone(),
                lat: s.lat,
                lon: s.lon,
                symbol_table: s.symbol_table.clone(),
                symbol_code: s.symbol_code.clone(),
                symbol_key: s.symbol_key.clone(),
                last_heard_unix: s.last_heard_unix,
                comment: s.comment.clone(),
            })
            .collect()
    }

    pub fn all(&self) -> Vec<FfiTrackStation> {
        self.inner
            .lock()
            .expect("track store lock")
            .all()
            .into_iter()
            .map(|s| FfiTrackStation {
                id: s.id.clone(),
                lat: s.lat,
                lon: s.lon,
                symbol_table: s.symbol_table.clone(),
                symbol_code: s.symbol_code.clone(),
                symbol_key: s.symbol_key.clone(),
                last_heard_unix: s.last_heard_unix,
                comment: s.comment.clone(),
            })
            .collect()
    }

    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> u32 {
        self.inner.lock().expect("track store lock").len() as u32
    }

    pub fn timeout_s(&self) -> u64 {
        self.inner.lock().expect("track store lock").timeout_s()
    }

    pub fn range_km(&self) -> f64 {
        self.inner.lock().expect("track store lock").range_km()
    }
}

#[uniffi::export]
pub fn station_timeout_max_s() -> u64 {
    driver_break_core::tracks::STATION_TIMEOUT_MAX_S
}

#[uniffi::export]
pub fn display_range_min_km() -> f64 {
    driver_break_core::tracks::DISPLAY_RANGE_MIN_KM
}

#[uniffi::export]
pub fn display_range_max_km() -> f64 {
    driver_break_core::tracks::DISPLAY_RANGE_MAX_KM
}

#[uniffi::export]
pub fn offset_lat_lon_m(lat: f64, lon: f64, east_m: f64, north_m: f64) -> Vec<f64> {
    let (a, b) = driver_break_core::tracks::offset_lat_lon(lat, lon, east_m, north_m);
    vec![a, b]
}

#[uniffi::export]
pub fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    driver_break_core::tracks::haversine_km(lat1, lon1, lat2, lon2)
}

// --- Offline PMTiles basemap downloads ---------------------------------------

use std::collections::HashMap;

use driver_break_core::download::DownloadControl;
use driver_break_core::routing::basemap::{
    default_pmtiles_planet_url, geofabrik_path_to_region_key, region_bbox, PmtilesDownloader,
    PROTOMAPS_PLANET_FALLBACK_URL,
};
use driver_break_core::storage::{PmtilesJobStatus, Storage};

fn pmtiles_controls() -> &'static Mutex<HashMap<String, DownloadControl>> {
    static CONTROLS: OnceLock<Mutex<HashMap<String, DownloadControl>>> = OnceLock::new();
    CONTROLS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn pmtiles_db(data_dir: &Path) -> Result<Storage, String> {
    let db = data_dir.join("navi.db");
    if let Some(parent) = db.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    Storage::open(&db).map_err(|e| e.to_string())
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiPmtilesJob {
    pub id: String,
    pub region_key: String,
    pub url: String,
    pub local_path: String,
    pub bytes_received: u64,
    pub total_bytes: Option<u64>,
    pub status: String,
    pub paused: bool,
    pub min_lat: Option<f64>,
    pub min_lon: Option<f64>,
    pub max_lat: Option<f64>,
    pub max_lon: Option<f64>,
}

fn map_pmtiles_job(j: driver_break_core::storage::PmtilesJobRecord) -> FfiPmtilesJob {
    let status = match j.status {
        PmtilesJobStatus::Pending => "pending",
        PmtilesJobStatus::Running => "running",
        PmtilesJobStatus::Paused => "paused",
        PmtilesJobStatus::Completed => "completed",
        PmtilesJobStatus::Cancelled => "cancelled",
        PmtilesJobStatus::Failed => "failed",
    };
    FfiPmtilesJob {
        id: j.id.to_string(),
        region_key: j.region_key,
        url: j.url,
        local_path: j.local_path,
        bytes_received: j.bytes_received,
        total_bytes: j.total_bytes,
        status: status.to_string(),
        paused: j.paused,
        min_lat: j.min_lat,
        min_lon: j.min_lon,
        max_lat: j.max_lat,
        max_lon: j.max_lon,
    }
}

#[uniffi::export]
pub fn pmtiles_default_base_url() -> String {
    default_pmtiles_planet_url()
}

/// Current Protomaps public planet URL (resolved from builds metadata when online).
#[uniffi::export]
pub fn pmtiles_planet_url() -> String {
    default_pmtiles_planet_url()
}

#[uniffi::export]
pub fn pmtiles_fallback_planet_url() -> String {
    PROTOMAPS_PLANET_FALLBACK_URL.to_string()
}

#[uniffi::export]
pub fn pmtiles_region_key(geofabrik_path: String) -> String {
    geofabrik_path_to_region_key(&geofabrik_path)
}

#[uniffi::export]
pub fn pmtiles_region_bbox(geofabrik_path: String) -> Option<Vec<f64>> {
    region_bbox(&geofabrik_path).map(|b| b.to_vec())
}

/// Most-specific known Geofabrik path covering `(lat, lon)`, or empty when unknown.
#[uniffi::export]
pub fn suggest_geofabrik_path(lat: f64, lon: f64) -> String {
    driver_break_core::routing::suggest_geofabrik_path_for_point(lat, lon)
        .unwrap_or("")
        .to_string()
}

/// Whether any of the comma-separated Geofabrik paths' bboxes cover `(lat, lon)`.
#[uniffi::export]
pub fn regions_cover_point(geofabrik_paths_csv: String, lat: f64, lon: f64) -> bool {
    let paths: Vec<&str> = geofabrik_paths_csv
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    driver_break_core::routing::point_covered_by_regions(lat, lon, &paths)
}

/// Map a PBF filename/stem (`ostlandet-latest.osm.pbf`) to a Geofabrik path.
#[uniffi::export]
pub fn geofabrik_path_for_pbf_name(pbf_name: String) -> String {
    driver_break_core::routing::pbf_stem_to_geofabrik_path(&pbf_name).unwrap_or_default()
}

/// Queue a Mapterhorn DEM extract for the same Geofabrik bbox as the basemap.
/// Writes `{region_key}_dem.pmtiles` beside the vector extract.
#[uniffi::export]
pub fn pmtiles_queue_dem_region(data_dir: String, geofabrik_path: String) -> FfiPmtilesJob {
    let empty = |msg: &str| FfiPmtilesJob {
        id: String::new(),
        region_key: String::new(),
        url: String::new(),
        local_path: String::new(),
        bytes_received: 0,
        total_bytes: None,
        status: format!("failed:{msg}"),
        paused: false,
        min_lat: None,
        min_lon: None,
        max_lat: None,
        max_lon: None,
    };
    let storage = match pmtiles_db(Path::new(&data_dir)) {
        Ok(s) => s,
        Err(e) => return empty(&e),
    };
    let bbox = match region_bbox(&geofabrik_path) {
        Some(b) => b,
        None => return empty("no bbox for geofabrik path"),
    };
    let base_key = geofabrik_path_to_region_key(&geofabrik_path);
    let dem_key = format!("{base_key}_dem");
    let dl = PmtilesDownloader::new(storage, PathBuf::from(&data_dir));
    const MAPTERHORN_PLANET: &str = "https://download.mapterhorn.com/planet.pmtiles";
    match dl.queue_url(&dem_key, MAPTERHORN_PLANET, Some(bbox)) {
        Ok(job) => {
            let control = DownloadControl::default();
            pmtiles_controls()
                .lock()
                .expect("pmtiles controls")
                .insert(job.id.to_string(), control);
            map_pmtiles_job(job.record)
        }
        Err(e) => empty(&e.to_string()),
    }
}

/// Queue a PMTiles download for the selected Geofabrik path.
#[uniffi::export]
pub fn pmtiles_queue_region(
    data_dir: String,
    geofabrik_path: String,
    base_url: Option<String>,
) -> FfiPmtilesJob {
    let empty = |msg: &str| FfiPmtilesJob {
        id: String::new(),
        region_key: String::new(),
        url: String::new(),
        local_path: String::new(),
        bytes_received: 0,
        total_bytes: None,
        status: format!("failed:{msg}"),
        paused: false,
        min_lat: None,
        min_lon: None,
        max_lat: None,
        max_lon: None,
    };
    let storage = match pmtiles_db(Path::new(&data_dir)) {
        Ok(s) => s,
        Err(e) => return empty(&e),
    };
    let dl = PmtilesDownloader::new(storage, PathBuf::from(&data_dir));
    let planet = base_url
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(default_pmtiles_planet_url);
    match dl.queue_geofabrik_region(&geofabrik_path, Some(planet.as_str())) {
        Ok(job) => {
            let control = DownloadControl::default();
            pmtiles_controls()
                .lock()
                .expect("pmtiles controls")
                .insert(job.id.to_string(), control);
            map_pmtiles_job(job.record)
        }
        Err(e) => empty(&e.to_string()),
    }
}

/// Run (or continue) a queued PMTiles job on the calling thread until terminal status.
#[uniffi::export]
pub fn pmtiles_run_job(data_dir: String, job_id: String) -> FfiPmtilesJob {
    ensure_native_logging();
    log::info!(
        target: "NaviDownload",
        "[NaviDownload] pmtiles_run_job start job_id={job_id} data_dir={data_dir}"
    );
    let empty = |msg: &str| FfiPmtilesJob {
        id: job_id.clone(),
        region_key: String::new(),
        url: String::new(),
        local_path: String::new(),
        bytes_received: 0,
        total_bytes: None,
        status: format!("failed:{msg}"),
        paused: false,
        min_lat: None,
        min_lon: None,
        max_lat: None,
        max_lon: None,
    };
    let uuid = match uuid::Uuid::parse_str(&job_id) {
        Ok(u) => u,
        Err(_) => return empty("invalid job id"),
    };
    let storage = match pmtiles_db(Path::new(&data_dir)) {
        Ok(s) => s,
        Err(e) => return empty(&e),
    };
    let dl = PmtilesDownloader::new(storage, PathBuf::from(&data_dir));
    let control = {
        let mut map = pmtiles_controls().lock().expect("pmtiles controls");
        map.entry(job_id.clone()).or_default().clone()
    };
    // Do not call control.reset() here: Resume must only clear the pause flag via
    // pmtiles_resume_job. Resetting would race a still-running extract and start a
    // second job that deletes the partial file. Fresh controls are inserted at queue.
    match dl.run_job_blocking(uuid, &control) {
        Ok(rec) => map_pmtiles_job(rec),
        Err(e) => empty(&e.to_string()),
    }
}

#[uniffi::export]
pub fn pmtiles_pause_job(job_id: String) {
    if let Some(c) = pmtiles_controls()
        .lock()
        .expect("pmtiles controls")
        .get(&job_id)
    {
        c.pause();
    }
}

#[uniffi::export]
pub fn pmtiles_resume_job(job_id: String) {
    if let Some(c) = pmtiles_controls()
        .lock()
        .expect("pmtiles controls")
        .get(&job_id)
    {
        c.resume();
    }
}

#[uniffi::export]
pub fn pmtiles_cancel_job(job_id: String) {
    if let Some(c) = pmtiles_controls()
        .lock()
        .expect("pmtiles controls")
        .get(&job_id)
    {
        c.cancel();
    }
}

#[uniffi::export]
pub fn pmtiles_get_job(data_dir: String, job_id: String) -> Option<FfiPmtilesJob> {
    let uuid = uuid::Uuid::parse_str(&job_id).ok()?;
    let storage = pmtiles_db(Path::new(&data_dir)).ok()?;
    let dl = PmtilesDownloader::new(storage, PathBuf::from(&data_dir));
    dl.get_job(uuid).ok().flatten().map(map_pmtiles_job)
}

#[uniffi::export]
pub fn pmtiles_list_jobs(data_dir: String) -> Vec<FfiPmtilesJob> {
    let storage = match pmtiles_db(Path::new(&data_dir)) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let dl = PmtilesDownloader::new(storage, PathBuf::from(&data_dir));
    dl.list_jobs()
        .unwrap_or_default()
        .into_iter()
        .map(map_pmtiles_job)
        .collect()
}

/// Completed PMTiles extracts whose stored bbox covers (lat, lon).
#[uniffi::export]
pub fn pmtiles_list_covering(data_dir: String, lat: f64, lon: f64) -> Vec<FfiPmtilesJob> {
    let storage = match pmtiles_db(Path::new(&data_dir)) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let dl = PmtilesDownloader::new(storage, PathBuf::from(&data_dir));
    dl.list_completed_covering(lat, lon)
        .unwrap_or_default()
        .into_iter()
        .map(map_pmtiles_job)
        .collect()
}

#[uniffi::export]
pub fn pmtiles_delete_job(data_dir: String, job_id: String) -> bool {
    let uuid = match uuid::Uuid::parse_str(&job_id) {
        Ok(u) => u,
        Err(_) => return false,
    };
    let storage = match pmtiles_db(Path::new(&data_dir)) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let dl = PmtilesDownloader::new(storage, PathBuf::from(&data_dir));
    dl.delete_job(uuid).is_ok()
}

/// Read one tile from a local PMTiles archive (raw bytes, decompressed).
/// Used by the in-app DEM HTTP loopback so MapLibre can apply terrarium encoding
/// via `tiles` + TileSet (PMTiles `url` sources still drop style encoding on Native).
#[uniffi::export]
pub fn pmtiles_get_tile(path: String, z: u8, x: u32, y: u32) -> Option<Vec<u8>> {
    match driver_break_core::routing::basemap::read_pmtiles_tile(Path::new(&path), z, x, y) {
        Ok(bytes) => bytes,
        Err(e) => {
            log::warn!("pmtiles_get_tile({path}, {z}/{x}/{y}): {e:#}");
            None
        }
    }
}

#[cfg(test)]
mod hiking_auto_via_tests {
    use super::*;

    #[test]
    fn named_hut_pause_filter() {
        assert!(is_named_hiking_hut_pause("hut", "Veslefjellbua"));
        assert!(is_named_hiking_hut_pause("network_hut", "Eldåbu"));
        assert!(!is_named_hiking_hut_pause("tent", "Veslefjellbua"));
        assert!(!is_named_hiking_hut_pause("hut", "Hut 12345"));
        assert!(!is_named_hiking_hut_pause("hut", "  "));
    }

    #[test]
    fn detour_allowance_uses_max_of_cabin_radius_and_frac() {
        // Hiking slider floor 10.5 km dominates a short leg.
        assert!((auto_via_extra_allowed_m(1_000.0, 10_500.0) - 10_500.0).abs() < 1e-9);
        // Long leg: 15% of 100 km exceeds a 10.5 km cabin radius.
        assert!((auto_via_extra_allowed_m(100_000.0, 10_500.0) - 15_000.0).abs() < 1e-9);
        // Raising the slider raises the absolute floor.
        assert!((auto_via_extra_allowed_m(1_000.0, 20_000.0) - 20_000.0).abs() < 1e-9);
    }

    #[test]
    fn merge_inserts_auto_via_between_user_waypoints() {
        let user = vec![
            HikingWp {
                name: "Skolla".into(),
                lat: 61.24,
                lon: 10.82,
            },
            HikingWp {
                name: "Eldåbu".into(),
                lat: 61.76,
                lon: 9.98,
            },
            HikingWp {
                name: "Rondvassbu".into(),
                lat: 61.88,
                lon: 9.80,
            },
        ];
        let cum = vec![0.0, 40.0, 55.0];
        let autos = vec![HikingAutoVia {
            name: "Veslefjellbua".into(),
            lat: 61.70,
            lon: 10.05,
            along_km: 35.0,
        }];
        let merged = merge_hiking_waypoints_with_auto_vias(&user, &cum, &autos);
        assert_eq!(merged.len(), 4);
        assert_eq!(merged[0].name, "Skolla");
        assert_eq!(merged[1].name, "Veslefjellbua");
        assert_eq!(merged[2].name, "Eldåbu");
        assert_eq!(merged[3].name, "Rondvassbu");
    }

    #[test]
    fn merge_dedupes_auto_via_near_user_waypoint() {
        let user = vec![
            HikingWp {
                name: "A".into(),
                lat: 61.0,
                lon: 10.0,
            },
            HikingWp {
                name: "Eldåbu".into(),
                lat: 61.756,
                lon: 9.979,
            },
        ];
        let cum = vec![0.0, 20.0];
        let autos = vec![HikingAutoVia {
            name: "Eldabu hut".into(),
            lat: 61.7561,
            lon: 9.9791,
            along_km: 19.5,
        }];
        let merged = merge_hiking_waypoints_with_auto_vias(&user, &cum, &autos);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[1].name, "Eldåbu");
    }
}

#[cfg(test)]
mod gps_speed_ffi_tests {
    use super::*;

    #[test]
    fn current_speed_tracks_update_gps_fix() {
        update_gps_fix(60.85, 11.0, true, Some(72.5));
        assert!((current_speed_kmh().unwrap() - 72.5).abs() < 1e-9);
        let fix = last_gps_fix();
        assert!(fix.available);
        assert!((fix.speed_kmh.unwrap() - 72.5).abs() < 1e-9);

        update_gps_fix(60.85, 11.0, true, None);
        assert!(current_speed_kmh().is_none());
    }

    #[test]
    fn resolve_speed_limit_uses_conditional_and_fallback() {
        let night_cond = Some("50 @ (Mo-Fr 00:00-06:00)".to_string());
        // Without live clock control here, base posted still wins when the window
        // does not match — posted 80 must be returned for a mid-day default path
        // through applicable_limit_or_fallback (conditional only when OH matches).
        let day = resolve_speed_limit_kmh(Some(80.0), night_cond.clone(), Some("primary".into()));
        assert!(day == 80.0 || day == 50.0, "got {day}");

        let fallback = resolve_speed_limit_kmh(None, None, Some("residential".into()));
        assert!((fallback - 40.0).abs() < 1e-9);

        let delta = overspeed_delta_kmh(Some(95.0), Some(80.0)).unwrap();
        assert!((delta - 15.0).abs() < 1e-9);
        assert!(overspeed_delta_kmh(None, Some(80.0)).is_none());
    }
}
