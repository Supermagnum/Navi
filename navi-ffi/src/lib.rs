//! UniFFI surface for the Navi Android app and other foreign-language hosts.
//!
//! Two tiers of on-device checks:
//! - [`ffi_linkage_smoke_test`] — fast SMOKE (FFI + worker pool only; no routing).
//! - [`run_car_corridor_pipeline`] — real parse/build/reweight/POI/route against on-device data.

use std::path::{Path, PathBuf};
use std::time::Instant;

use driver_break_core::config::{
    EcoConfig, FmcsaHosParams, JurisdictionDrivingHoursPack, RestConfig, SafetyConfig,
    VehicleLimits, HIKING_MAIN_BREAK_DISTANCE_KM,
};
use driver_break_core::icons::{self, IconTheme};
use driver_break_core::poi::{rest_area_suitable_for_weekly, PoiCategory, PoiIndex, PoiRecord};
use driver_break_core::routing::elevation::{ElevationCache, ElevationService};
use driver_break_core::routing::graph::{
    apply_official_network_preference, difficulty_notes_for_path, load_official_network_way_ids,
    load_or_build_reweighted, load_or_build_reweighted_bbox, load_way_difficulty_tags,
    OfficialNetworkKind, RoadNodeIndex, RouteGraph, RouteOptions, RoutingProfile,
};
use driver_break_core::routing::rest::car_break_interval_hours;
use driver_break_core::routing::{
    commit_truck_multi_day_plan, evaluate_fmcsa_trip, evaluate_truck_trip,
    hiking_samples_from_coords, max_daily_distance_km, motor_break_interval_km,
    motor_daily_budget, plan_fmcsa_multi_day, plan_hiking_multi_day, plan_motor_multi_day,
    plan_truck_multi_day, resolve_driving_hours_pack_at, truck_effective_break_parts,
    uses_motor_multi_day, uses_truck_rest, HikingMultiDayPlan, MotorMultiDayPlan,
    MotorOvernightCandidate, MotorOvernightKind, TruckMultiDayPlan, TruckOvernightKind,
    TruckOvernightRest, TruckRestCandidate, TruckRestFacility,
};
use driver_break_core::routing::safety::{
    check_overnight_candidate, DangerBarrierIndex, OvernightProximityIndex,
};
use driver_break_core::routing::workers::WorkerPoolPlan;
use driver_break_core::routing::{fixed_pace_minutes, motor_path_minutes, HIKING_MIN_PER_KM};
use driver_break_core::routing::{
    build_maneuvers, build_sim_samples, maneuvers_to_json, samples_to_json,
};
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
    let s = driver_break_core::download::progress::snapshot();
    FfiDownloadProgress {
        units_done: s.units_done,
        units_total: s.units_total,
        percent: s.percent,
        label: s.label,
    }
}

/// Clear the shared download progress snapshot.
#[uniffi::export]
pub fn download_progress_clear() {
    driver_break_core::download::progress::clear();
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

fn nearest(graph: &RouteGraph, lat: f64, lon: f64) -> NodeId {
    // Prefer nodes that participate in the routing graph. Snapping to an
    // isolated POI/stub node yields "no route between snapped nodes".
    let linked = graph.nodes.values().filter(|n| graph.is_linked(n.id));
    let pool = {
        let v: Vec<_> = linked.collect();
        if v.is_empty() {
            graph.nodes.values().collect()
        } else {
            v
        }
    };
    pool.into_iter()
        .min_by(|a, b| {
            let da = haversine_m(lat, lon, a.coord.y, a.coord.x);
            let db = haversine_m(lat, lon, b.coord.y, b.coord.x);
            da.partial_cmp(&db).unwrap()
        })
        .map(|n| n.id)
        .expect("empty graph")
}

fn load_break_barriers(graph: &RouteGraph, pbf: &Path, bbox: [f64; 4]) -> DangerBarrierIndex {
    let mut barriers = DangerBarrierIndex::from_graph(graph);
    match DangerBarrierIndex::load_from_pbf_bbox(pbf, bbox) {
        Ok(extra) => barriers.merge(extra),
        Err(e) => {
            log::warn!("danger barrier PBF load skipped: {e:#}");
        }
    }
    barriers
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
/// [`EcoConfig::for_profile`]; car-like masses keep the Passat Cd/area/mass baseline.
fn eco_for_travel_profile(profile: TravelProfile) -> EcoConfig {
    let mut eco = EcoConfig::for_profile(profile.to_core());
    match profile {
        TravelProfile::Car
        | TravelProfile::CarElectric
        | TravelProfile::Motorcycle
        | TravelProfile::MotorcycleElectric => {
            eco.drag_coefficient = 0.28;
            eco.frontal_area_m2 = 2.2;
            eco.mass_kg = 1500.0;
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
    if !prefer {
        return;
    }
    match load_official_network_way_ids(pbf, kind) {
        Ok(ways) => {
            report.push_str(&format!(
                "official_network_ways={}; prefer_official_networks=true\n",
                ways.len()
            ));
            apply_official_network_preference(graph, &ways);
        }
        Err(e) => {
            report.push_str(&format!("WARN: official network load failed: {e:#}\n"));
        }
    }
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
    /// Densified path samples for debug route simulation:
    /// `[{"lat","lon","cum_m","speed_kmh","highway","maxspeed_posted"}]`.
    pub sim_samples_json: String,
    /// Turn / destination maneuvers along the path:
    /// `[{"lat","lon","cum_m","kind","street","roundabout_exit"}]`.
    pub maneuvers_json: String,
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
    }
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
            let (rest_kind, rest_hours, rest_label, overnight_name, overnight_found, not_in_cab, compensation) =
                match &d.overnight {
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
            let (rest_kind, rest_label, overnight_name, overnight_found) = match &d.overnight {
                Some(o) => (
                    if o.is_network {
                        "network_hut"
                    } else {
                        "hut"
                    }
                    .to_string(),
                    if o.is_network {
                        "Network hut overnight"
                    } else {
                        "Hut overnight"
                    }
                    .to_string(),
                    o.name.clone(),
                    !o.safety_rejected,
                ),
                None => (String::new(), String::new(), String::new(), false),
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
    let start = nearest(graph, from_lat, from_lon);
    let goal = nearest(graph, to_lat, to_lon);
    if start == goal {
        return true;
    }
    let opts = RouteOptions {
        avoid_ferries: true,
        // Hiking: do not treat major-road walking as safe access to a break POI.
        avoid_major_roads: matches!(graph.profile(), RoutingProfile::Foot),
        ..RouteOptions::default()
    };
    let Some((path, _)) = graph.shortest_path_with_options(start, goal, false, &opts) else {
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
            &prox.glaciers,
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
        let name = p
            .name
            .clone()
            .unwrap_or_else(|| "Tent site".into());
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
        let name = p
            .name
            .clone()
            .unwrap_or_else(|| "Tent site".into());
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
        if check_overnight_candidate(
            lat,
            lon,
            safety,
            &synthetic,
            &prox.buildings,
            &prox.glaciers,
        )
        .is_some()
        {
            // Still return a marker so the break interval has a stop, but label it
            // so the host can see the safety rejection in the report path.
            return (
                "Tent site (safety review)".into(),
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
fn pick_motor_pause_at(
    poi: &PoiIndex,
    graph: &RouteGraph,
    barriers: &DangerBarrierIndex,
    route_link: &RoadNodeIndex,
    lat: f64,
    lon: f64,
    search_radius_m: f64,
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
    let mut best_unnamed: Option<&PoiRecord> = None;
    for p in all {
        if route_link.within_road_link(p.lat, p.lon) {
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
/// without ferry / lake-scale detours / crow-flies across dangerous barriers.
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
                search_radius_m.min(5_000.0),
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
    report.push_str(&format!(
        "pbf={}; pbf_bytes={}\n",
        pbf.display(),
        pbf_len
    ));

    let elev = PathBuf::from(&elev_dir);
    let cache = PathBuf::from(&cache_dir);
    let _ = std::fs::create_dir_all(&cache);
    let eco = passat_eco();
    let elevation = ElevationService::new(ElevationCache::new(&elev));
    let _ = elevation.warm_bbox([60.35, 9.95, 62.05, 11.65]);

    // Cold path: remove any prior cache entry for this fingerprint by wiping cache dir contents
    // that match this run's stem — simpler: delete cache_dir files then build.
    if cache.is_dir() {
        if let Ok(rd) = std::fs::read_dir(&cache) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().and_then(|s| s.to_str()) == Some("navigph") {
                    let _ = std::fs::remove_file(&p);
                }
            }
        }
    }

    let t_cold = Instant::now();
    let (mut graph, hit1) = match load_or_build_reweighted(
        pbf,
        &cache,
        RoutingProfile::Car,
        &elevation,
        &eco,
    ) {
        Ok(v) => v,
        Err(e) => {
            report.push_str(&format!("FAIL: cold graph build: {e:#}\n"));
            return empty(report);
        }
    };
    let cold_s = t_cold.elapsed().as_secs_f64();
    if hit1 {
        report.push_str("WARN: unexpected cache hit on cold pass\n");
    }
    report.push_str(&format!(
        "cold_build_s={cold_s:.2}; cache_hit={hit1}; nodes={}; edges={}\n",
        graph.nodes.len(),
        graph.edges.len()
    ));
    if graph.edges.is_empty() {
        report.push_str("FAIL: degenerate empty graph\n");
        return empty(report);
    }

    let t_warm = Instant::now();
    let (graph2, hit2) = match load_or_build_reweighted(
        pbf,
        &cache,
        RoutingProfile::Car,
        &elevation,
        &eco,
    ) {
        Ok(v) => v,
        Err(e) => {
            report.push_str(&format!("FAIL: warm graph load: {e:#}\n"));
            return empty(report);
        }
    };
    let warm_s = t_warm.elapsed().as_secs_f64();
    report.push_str(&format!("warm_load_s={warm_s:.2}; cache_hit={hit2}\n"));
    if !hit2 {
        report.push_str("FAIL: expected cache hit on second load_or_build_reweighted\n");
        return empty(report);
    }
    if warm_s >= cold_s * 0.85 && cold_s > 2.0 {
        report.push_str(&format!(
            "FAIL: warm load ({warm_s:.1}s) not meaningfully faster than cold ({cold_s:.1}s)\n"
        ));
        return empty(report);
    }
    // Prefer the warm-loaded graph for routing (proves cache usable).
    graph = graph2;

    let start_lat = 60.562_191_4;
    let start_lon = 11.256_123_9;
    let end_lat = 61.851_250_0;
    let end_lon = 10.233_842_0;

    let s = nearest(&graph, start_lat, start_lon);
    let g = nearest(&graph, end_lat, end_lon);
    let Some((path, cost)) = graph.shortest_path(s, g, true) else {
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
        // Decimate for MapLibre overlay (every ~20th node + ends).
        if i % 20 == 0 || i + 1 == path.len().saturating_sub(1) {
            polyline.push_str(&format!(";{},{}", n1.coord.x, n1.coord.y));
        }
    }
    if let Some(last) = path.last() {
        let n = &graph.nodes[last];
        if !polyline.ends_with(&format!("{},{}", n.coord.x, n.coord.y)) {
            polyline.push_str(&format!(";{},{}", n.coord.x, n.coord.y));
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
        &load_break_barriers(&graph, pbf, [60.35, 9.95, 62.05, 11.65]),
        &path,
        false,
        None,
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
    }
}

/// Plan a motor / bicycle route between two WGS84 points using a local OSM `.pbf`.
///
/// Always builds a **bbox-clipped** graph (`[min_lat,min_lon,max_lat,max_lon]` padded
/// around the endpoints) so truck / mobile-home / motorcycle / bicycle never load a
/// full Ostlandet extract into RAM. Hiking uses [`plan_hiking_route`] instead.
///
/// [`TravelProfile::Hiking`] is rejected (call [`plan_hiking_route`]).
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
    avoid_major: bool,
    avoid_tolls: bool,
    avoid_ferries: bool,
    vehicle: FfiVehicleLimits,
    prefer_official_networks: bool,
) -> CorridorRouteResult {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
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
            avoid_major,
            avoid_tolls,
            avoid_ferries,
            vehicle,
            prefer_official_networks,
        )
    })) {
        Ok(result) => result,
        Err(_) => empty_corridor(
            "TEST_KIND=PLAN_CAR_ROUTE\nFAIL: native panic during plan_car_route\n".into(),
        ),
    }
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
    avoid_major: bool,
    avoid_tolls: bool,
    avoid_ferries: bool,
    vehicle: FfiVehicleLimits,
    prefer_official_networks: bool,
) -> CorridorRouteResult {
    let empty = empty_corridor;

    if profile == TravelProfile::Hiking {
        return empty(
            "TEST_KIND=PLAN_CAR_ROUTE\nFAIL: use plan_hiking_route for hiking\n".into(),
        );
    }

    let routing_profile = RoutingProfile::from(profile.to_core());
    let plan = WorkerPoolPlan::detect();
    WorkerPoolPlan::lower_current_thread_priority();
    let _ = plan.install_rayon_pool();

    let vehicle_limits = ffi_vehicle_to_limits(&vehicle);
    let route_opts = RouteOptions {
        avoid_major_roads: avoid_major,
        avoid_tolls,
        avoid_ferries,
        vehicle: vehicle_limits.clone(),
    };

    let mut report = String::new();
    report.push_str("TEST_KIND=PLAN_CAR_ROUTE\nDATA_SOURCE=real_pbf\n");
    report.push_str(&format!(
        "profile={profile:?}; routing={routing_profile:?}; start={start_lat:.6},{start_lon:.6}; end={end_lat:.6},{end_lon:.6}; use_eco={use_eco}\n"
    ));
    report.push_str(&format!(
        "avoid_major={avoid_major}; avoid_tolls={avoid_tolls}; avoid_ferries={avoid_ferries}; vehicle_limits={}\n",
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
    let eco = eco_for_travel_profile(profile);
    let elevation = ElevationService::new(ElevationCache::new(&elev));
    let _ = elevation.warm_bbox([
        start_lat.min(end_lat) - 0.05,
        start_lon.min(end_lon) - 0.05,
        start_lat.max(end_lat) + 0.05,
        start_lon.max(end_lon) + 0.05,
    ]);

    let t0 = Instant::now();
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
    driver_break_core::download::progress::set(
        0,
        Some(5),
        "Planning route: building area graph…",
    );
    let (mut graph, cache_hit) = match load_or_build_reweighted_bbox(
        pbf,
        &cache,
        routing_profile,
        &elevation,
        &eco,
        bbox,
    ) {
        Ok(v) => v,
        Err(e) => {
            report.push_str(&format!("FAIL: graph build: {e:#}\n"));
            return empty(report);
        }
    };
    if profile == TravelProfile::Bicycle && prefer_official_networks {
        apply_network_pref_if_requested(
            &mut graph,
            pbf,
            OfficialNetworkKind::Cycling,
            true,
            &mut report,
        );
    }
    let build_s = t0.elapsed().as_secs_f64();
    report.push_str(&format!(
        "build_s={build_s:.2}; cache_hit={cache_hit}; nodes={}; edges={}\n",
        graph.nodes.len(),
        graph.edges.len()
    ));

    driver_break_core::download::progress::set(3, Some(5), "Planning route: finding path…");
    let s = nearest(&graph, start_lat, start_lon);
    let g = nearest(&graph, end_lat, end_lon);
    {
        let sn = &graph.nodes[&s];
        let gn = &graph.nodes[&g];
        let snap_start_m = haversine_m(start_lat, start_lon, sn.coord.y, sn.coord.x);
        let snap_end_m = haversine_m(end_lat, end_lon, gn.coord.y, gn.coord.x);
        report.push_str(&format!(
            "snap_start={:.6},{:.6} dist_m={snap_start_m:.0}; snap_end={:.6},{:.6} dist_m={snap_end_m:.0}\n",
            sn.coord.y, sn.coord.x, gn.coord.y, gn.coord.x
        ));
    }
    let Some((path, cost)) = graph.shortest_path_with_options(s, g, use_eco, &route_opts) else {
        report.push_str("FAIL: no route between snapped nodes\n");
        return empty(report);
    };
    if path.len() < 2 {
        report.push_str("FAIL: zero-length route\n");
        return empty(report);
    }

    let mut distance_m = 0.0;
    let mut polyline = String::new();
    // Keep denser geometry on short urban trips so MapLibre does not look like
    // a single chord (corridor pipeline decimates every 20th node).
    let stride = if path.len() < 80 { 1 } else { 5 };
    for (i, w) in path.windows(2).enumerate() {
        if let Some(idx) = graph.edge_index(w[0], w[1]) {
            distance_m += graph.edges[idx].length_m;
        }
        let n0 = &graph.nodes[&w[0]];
        if i == 0 {
            polyline.push_str(&format!("{},{}", n0.coord.x, n0.coord.y));
        }
        let n1 = &graph.nodes[&w[1]];
        if i % stride == 0 || i + 1 == path.len().saturating_sub(1) {
            polyline.push_str(&format!(";{},{}", n1.coord.x, n1.coord.y));
        }
    }
    if let Some(last) = path.last() {
        let n = &graph.nodes[last];
        let tail = format!("{},{}", n.coord.x, n.coord.y);
        if !polyline.ends_with(&tail) {
            polyline.push(';');
            polyline.push_str(&tail);
        }
    }

    let dist_km = distance_m / 1000.0;
    let eta_minutes = motor_path_minutes(&graph, &path);
    let sim_samples_json = samples_to_json(&build_sim_samples(&graph, &path));
    let maneuvers_json = maneuvers_to_json(&build_maneuvers(&graph, &path));
    let path_nodes = path.len();
    driver_break_core::download::progress::set(4, Some(5), "Planning route: break stops…");
    // Clip POI load to the same trip bbox (never a full Ostlandet POI scan).
    let poi_index = PoiIndex::load_from_pbf_bbox(pbf, bbox).unwrap_or_else(|_| PoiIndex::new());
    let barriers = load_break_barriers(&graph, pbf, bbox);

    // Truck / TruckElectric: jurisdiction-keyed HOS (EC 561 or FMCSA).
    // MobileHome uses car soft break spacing (not commercial HGV legal tracking).
    let core_profile = profile.to_core();
    let mut rest = load_rest_config_near_cache(&cache);
    let mut break_interval_km =
        motor_break_interval_km(core_profile, &rest, dist_km, eta_minutes);
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
        let mut candidates: Vec<TruckRestCandidate> = Vec::new();
        let mut seen_poi = std::collections::HashSet::new();
        for (i, (lat, lon, km)) in samples.iter().enumerate() {
            if i % 4 != 0 && i + 1 != samples.len() {
                continue;
            }
            for p in poi_index.nearest(PoiCategory::RestArea, *lat, *lon, 20_000.0) {
                if !seen_poi.insert(p.osm_id) {
                    continue;
                }
                let suitable_for_weekly =
                    rest_area_suitable_for_weekly(&p.tags, &p.icon_key);
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

                for d in driver_break_core::config::outstanding_weekly_rest_compensations(&history) {
                    report.push_str(&format!(
                        "truck_compensation: pending=true; reduced_on={}; shortfall_h={:.0}; compensate_by={}\n",
                        d.reduced_on_date, d.shortfall_hours, d.compensate_by_date
                    ));
                }
                let pending_n =
                    driver_break_core::config::outstanding_weekly_rest_compensations(&history).len();
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
                let multi = plan_fmcsa_multi_day(
                    &fmcsa,
                    &history,
                    driving_h,
                    dist_km,
                    &today,
                    &candidates,
                );
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
            "motor_break_interval_km={break_interval_km:.1} (legacy / car-style heuristic)\n"
        ));
    }

    // Soft multi-day overnight for car / motorcycle / cycle / mobilehome (not truck).
    let mut motor_overnight_pins: Vec<serde_json::Value> = Vec::new();
    if uses_motor_multi_day(core_profile) {
        let driving_h = eta_minutes / 60.0;
        if let Some(budget) = motor_daily_budget(core_profile, &rest.car, &rest.cycling) {
            let samples = sample_polyline_km(&polyline);
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
                    for p in poi_index.nearest(cat, *lat, *lon, 20_000.0) {
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

    // Prefer stops on the planned road; fall back if reachable without danger barriers.
    let mut break_pois_json = build_break_pois_json(
        &poi_index,
        &polyline,
        break_interval_km,
        5_000.0,
        &graph,
        &barriers,
        &path,
        false,
        None,
    );
    merge_break_poi_pins(&mut break_pois_json, motor_overnight_pins);
    merge_break_poi_pins(&mut break_pois_json, truck_overnight_pins);
    // Difficulty metadata on cycling network ways (informational only).
    if profile == TravelProfile::Bicycle && prefer_official_networks {
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
    }
}

/// Plan a hiking (foot) route through ordered waypoints.
///
/// `waypoints_json` is `[{"name","lat","lon"}, ...]` with at least two points
/// (start … vias … end). Pause stops prefer huts/cabins; otherwise camp pitches
/// or a synthetic corridor tent (never mountain peak names).
#[uniffi::export]
pub fn plan_hiking_route(
    pbf_path: String,
    elev_dir: String,
    cache_dir: String,
    waypoints_json: String,
    prefer_official_networks: bool,
) -> CorridorRouteResult {
    #[derive(Deserialize)]
    struct Wp {
        name: String,
        lat: f64,
        lon: f64,
    }

    let mut report = String::from("TEST_KIND=PLAN_HIKING_ROUTE\nDATA_SOURCE=real_pbf\n");
    report.push_str(&format!(
        "prefer_official_networks={prefer_official_networks}\n"
    ));
    let wps: Vec<Wp> = match serde_json::from_str(&waypoints_json) {
        Ok(v) => v,
        Err(e) => {
            report.push_str(&format!("FAIL: waypoints_json: {e}\n"));
            return empty_corridor(report);
        }
    };
    if wps.len() < 2 {
        report.push_str("FAIL: need at least start and end waypoints\n");
        return empty_corridor(report);
    }
    report.push_str(&format!("waypoints={}\n", wps.len()));

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
    let eco = eco_for_travel_profile(TravelProfile::Hiking);
    let elevation = ElevationService::new(ElevationCache::new(&elev));
    let min_lat = wps.iter().map(|w| w.lat).fold(f64::INFINITY, f64::min);
    let max_lat = wps.iter().map(|w| w.lat).fold(f64::NEG_INFINITY, f64::max);
    let min_lon = wps.iter().map(|w| w.lon).fold(f64::INFINITY, f64::min);
    let max_lon = wps.iter().map(|w| w.lon).fold(f64::NEG_INFINITY, f64::max);
    // Clip to the trip bbox so we never load a full Ostlandet foot graph into RAM
    // (that OOMs 4GB Automotive AVDs during hiking plan). Same region .pbf.
    let span = (max_lat - min_lat).max(max_lon - min_lon);
    let pad = (span * 0.25).clamp(0.30, 0.55);
    let bbox = [
        min_lat - pad,
        min_lon - pad,
        max_lat + pad,
        max_lon + pad,
    ];
    report.push_str(&format!(
        "bbox={:.3},{:.3},{:.3},{:.3}; pad={pad:.2}\n",
        bbox[0], bbox[1], bbox[2], bbox[3]
    ));
    let _ = elevation.warm_bbox(bbox);

    let t0 = Instant::now();
    driver_break_core::download::progress::set(
        0,
        Some(5),
        "Planning route: building hiking area graph…",
    );
    let (mut graph, cache_hit) = match load_or_build_reweighted_bbox(
        pbf,
        &cache,
        RoutingProfile::Foot,
        &elevation,
        &eco,
        bbox,
    ) {
        Ok(v) => v,
        Err(e) => {
            report.push_str(&format!("FAIL: foot graph build: {e:#}\n"));
            return empty_corridor(report);
        }
    };
    apply_network_pref_if_requested(
        &mut graph,
        pbf,
        OfficialNetworkKind::Hiking,
        prefer_official_networks,
        &mut report,
    );
    let build_s = t0.elapsed().as_secs_f64();
    report.push_str(&format!(
        "build_s={build_s:.2}; cache_hit={cache_hit}; nodes={}; edges={}\n",
        graph.nodes.len(),
        graph.edges.len()
    ));

    let mut full_path: Vec<NodeId> = Vec::new();
    let mut distance_m = 0.0;
    for pair in wps.windows(2) {
        let s = nearest(&graph, pair[0].lat, pair[0].lon);
        let g = nearest(&graph, pair[1].lat, pair[1].lon);
        let Some((path, _cost)) = graph.shortest_path(s, g, false) else {
            report.push_str(&format!(
                "FAIL: no foot route {} -> {}\n",
                pair[0].name, pair[1].name
            ));
            return empty_corridor(report);
        };
        if path.len() < 2 {
            report.push_str(&format!(
                "FAIL: zero-length leg {} -> {}\n",
                pair[0].name, pair[1].name
            ));
            return empty_corridor(report);
        }
        for w in path.windows(2) {
            if let Some(idx) = graph.edge_index(w[0], w[1]) {
                distance_m += graph.edges[idx].length_m;
            }
        }
        if full_path.is_empty() {
            full_path.extend(path);
        } else {
            full_path.extend(path.into_iter().skip(1));
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

    let mut polyline = String::new();
    let stride = if full_path.len() < 120 { 1 } else { 8 };
    for (i, id) in full_path.iter().enumerate() {
        let n = &graph.nodes[id];
        if i == 0 {
            polyline.push_str(&format!("{},{}", n.coord.x, n.coord.y));
        } else if i % stride == 0 || i + 1 == full_path.len() {
            polyline.push_str(&format!(";{},{}", n.coord.x, n.coord.y));
        }
    }

    let dist_km = distance_m / 1000.0;
    // Clip POI load to the same trip bbox (never a full Ostlandet POI scan).
    let poi_index = match PoiIndex::load_from_pbf_bbox_with_overnight_buildings(pbf, bbox) {
        Ok(i) => i,
        Err(e) => {
            report.push_str(&format!("FAIL: POI index: {e:#}\n"));
            return empty_corridor(report);
        }
    };
    let barriers = load_break_barriers(&graph, pbf, bbox);
    let safety = SafetyConfig::default();
    // Buildings from the POI load; glaciers from the barrier index already built
    // for break access — no extra overnight PBF scan.
    let overnight_prox = OvernightProximityIndex::from_poi_buildings_and_barriers(
        poi_index.overnight_buildings().to_vec(),
        &barriers,
    );
    report.push_str(&format!(
        "overnight_buildings={}; overnight_glaciers={}; overnight_source=poi+barriers\n",
        overnight_prox.buildings.len(),
        overnight_prox.glaciers.len()
    ));
    let overnight_ctx = (safety, overnight_prox);
    // Day-by-day multi-day overnight (mirrors truck/motor; same spirit as DNT helper).
    let rest = RestConfig::default();
    let max_daily = max_daily_distance_km(&rest, driver_break_core::config::Profile::Hiking)
        .unwrap_or(40.0);
    let hike_coords: Vec<(f64, f64)> = full_path
        .iter()
        .enumerate()
        .filter(|(i, _)| *i % stride == 0 || *i + 1 == full_path.len())
        .map(|(_, id)| {
            let n = &graph.nodes[id];
            (n.coord.y, n.coord.x)
        })
        .collect();
    let hike_samples = hiking_samples_from_coords(&hike_coords);
    let multi = plan_hiking_multi_day(
        &hike_samples,
        max_daily,
        &overnight_ctx.0,
        &poi_index,
        &overnight_ctx.1,
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
                    "hiking_overnight: name={:?}; network={}; safety_rejected={}; dist_m={:.0}; lat={:.5}; lon={:.5}\n",
                    o.name, o.is_network, o.safety_rejected, o.distance_from_target_m, o.lat, o.lon
                ));
                hiking_overnight_pins.push(json!({
                    "name": o.name,
                    "lat": o.lat,
                    "lon": o.lon,
                    "kind": if o.is_network { "network_hut" } else { "hut" },
                    "icon": "cabin",
                    "icon_key": o.icon_key,
                    "along_km": d.end_km,
                    "overnight": true,
                    "safety_rejected": o.safety_rejected,
                }));
            }
        }
    } else {
        report.push_str("hiking_multi_day: days=1; multi_day=false\n");
    }
    // Hiking rast interval (~11.3 km); prefer path-linked huts, else reachable fallback.
    let mut break_pois_json = build_break_pois_json(
        &poi_index,
        &polyline,
        HIKING_MAIN_BREAK_DISTANCE_KM,
        15_000.0,
        &graph,
        &barriers,
        &full_path,
        true,
        Some(&overnight_ctx),
    );
    // Ensure hut vias/end appear as pause labels even if the interval skipped them.
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
                    // Skip pure start road addresses; keep hut vias/end.
                    if lower.contains("harland")
                        || lower.contains("eldå")
                        || lower.contains("elda")
                        || lower.contains("rondvass")
                    {
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

    let end = wps.last().unwrap();
    // Hiking: fixed 16 min/km (no climb adjustment in this pass).
    let eta_minutes = fixed_pace_minutes(dist_km, HIKING_MIN_PER_KM);
    report.push_str(&format!(
        "distance_km={dist_km:.3}; eta_min={eta_minutes:.1}; path_nodes={}; break_pois={break_pois_json}\nPASS\n",
        full_path.len()
    ));

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
        sim_samples_json: String::from("[]"),
        maneuvers_json: String::from("[]"),
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
    run_car_corridor_pipeline(pbf_path, elev_dir, cache.display().to_string(), break_interval_hours)
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
    // Reuse existing index if non-trivial.
    if db.is_file() {
        if let Ok(meta) = std::fs::metadata(db) {
            if meta.len() > 10_000 {
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

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiFuelConfig {
    pub tank_capacity_l: Option<f64>,
    pub fuel_added_l: Option<f64>,
    pub prefer_liters: bool,
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiGpsFix {
    pub lat: f64,
    pub lon: f64,
    pub available: bool,
}

fn routes_db(data_dir: &str) -> PathBuf {
    Path::new(data_dir).join("navi.db")
}

#[uniffi::export]
pub fn list_saved_routes(data_dir: String) -> Vec<FfiSavedRoute> {
    let Ok(storage) = driver_break_core::storage::Storage::open(&routes_db(&data_dir)) else {
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
        })
        .collect()
}

#[uniffi::export]
pub fn delete_saved_route(data_dir: String, id: String) -> bool {
    let Ok(storage) = driver_break_core::storage::Storage::open(&routes_db(&data_dir)) else {
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
    let Ok(storage) = driver_break_core::storage::Storage::open(&routes_db(&data_dir)) else {
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

fn chrono_like_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".into())
}

#[uniffi::export]
pub fn load_vehicle_limits(data_dir: String) -> FfiVehicleLimits {
    let Ok(storage) = driver_break_core::storage::Storage::open(&routes_db(&data_dir)) else {
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
    let Ok(storage) = driver_break_core::storage::Storage::open(&routes_db(&data_dir)) else {
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
    let Ok(storage) = driver_break_core::storage::Storage::open(&routes_db(&data_dir)) else {
        return false;
    };
    let store = driver_break_core::storage::ConfigStore::new(&storage);
    store.load_prefer_official_networks().unwrap_or(false)
}

#[uniffi::export]
pub fn save_prefer_official_networks(data_dir: String, prefer: bool) -> bool {
    let Ok(storage) = driver_break_core::storage::Storage::open(&routes_db(&data_dir)) else {
        return false;
    };
    let store = driver_break_core::storage::ConfigStore::new(&storage);
    store.save_prefer_official_networks(prefer).is_ok()
}

#[uniffi::export]
pub fn load_car_rest_settings(data_dir: String) -> FfiCarRestSettings {
    let default = driver_break_core::config::CarRestParams::default();
    let fallback = FfiCarRestSettings {
        break_interval_hours: default.break_interval_min_hours,
        rest_duration_minutes: default.break_duration_min_minutes,
        eco_mode_enabled: default.eco_mode_enabled,
    };
    let Ok(storage) = driver_break_core::storage::Storage::open(&routes_db(&data_dir)) else {
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
    let Ok(storage) = driver_break_core::storage::Storage::open(&routes_db(&data_dir)) else {
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

fn truck_settings_from_params(t: &driver_break_core::config::TruckRestParams) -> FfiTruckRestSettings {
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
    let Ok(storage) = driver_break_core::storage::Storage::open(&routes_db(&data_dir)) else {
        return fallback;
    };
    let store = driver_break_core::storage::ConfigStore::new(&storage);
    let rest = store.load_rest_config().unwrap_or_default();
    truck_settings_from_params(&rest.truck)
}

#[uniffi::export]
pub fn save_truck_rest_settings(data_dir: String, settings: FfiTruckRestSettings) -> bool {
    let Ok(storage) = driver_break_core::storage::Storage::open(&routes_db(&data_dir)) else {
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
    let Ok(storage) = driver_break_core::storage::Storage::open(&routes_db(&data_dir)) else {
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

fn load_truck_history_near_cache(
    cache: &Path,
) -> driver_break_core::config::TruckDrivingHistory {
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
    let Ok(storage) = driver_break_core::storage::Storage::open(&routes_db(&data_dir)) else {
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
    let Ok(storage) = driver_break_core::storage::Storage::open(&routes_db(&data_dir)) else {
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

/// Sample on-disk DEM elevation (meters) at a WGS84 point, or null if no tile.
#[uniffi::export]
pub fn elevation_at(elev_dir: String, lat: f64, lon: f64) -> Option<f64> {
    let elev = ElevationService::new(ElevationCache::new(Path::new(&elev_dir)));
    elev.get_elevation(lat, lon)
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
        })
    })
}

/// Push the device LocationManager / fused fix into the native layer.
#[uniffi::export]
pub fn update_gps_fix(lat: f64, lon: f64, available: bool) {
    if let Ok(mut g) = gps_fix_slot().lock() {
        *g = FfiGpsFix {
            lat,
            lon,
            available,
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
        })
}

/// Format a short validation blurb for avoid-major / toll / ferry preferences.
#[uniffi::export]
pub fn format_avoid_major_report(avoid_major: bool, priority_path_share_pct: f64) -> String {
    format_route_avoidance_report(avoid_major, false, false, priority_path_share_pct)
}

/// Extended avoidance report (motorways + tolls + ferries). Defaults for toll/ferry: off.
#[uniffi::export]
pub fn format_route_avoidance_report(
    avoid_major: bool,
    avoid_tolls: bool,
    avoid_ferries: bool,
    priority_path_share_pct: f64,
) -> String {
    let opts = driver_break_core::RouteOptions {
        avoid_major_roads: avoid_major,
        avoid_tolls,
        avoid_ferries,
        vehicle: None,
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
    let Ok(Some(meta)) =
        driver_break_core::routing::RegionExtractMeta::load(Path::new(&data_dir))
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
        self.inner.lock().expect("track store lock").expire(now_unix)
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
use std::sync::{Mutex, OnceLock};

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
        map.entry(job_id.clone())
            .or_insert_with(DownloadControl::default)
            .clone()
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
