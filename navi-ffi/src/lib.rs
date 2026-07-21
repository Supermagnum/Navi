//! UniFFI surface for the Navi Android app and other foreign-language hosts.
//!
//! Two tiers of on-device checks:
//! - [`ffi_linkage_smoke_test`] — fast SMOKE (FFI + worker pool only; no routing).
//! - [`run_car_corridor_pipeline`] — real parse/build/reweight/POI/route against on-device data.

use std::path::{Path, PathBuf};
use std::time::Instant;

use driver_break_core::config::{EcoConfig, RestConfig};
use driver_break_core::icons::{self, IconTheme};
use driver_break_core::poi::{PoiCategory, PoiIndex};
use driver_break_core::routing::elevation::{ElevationCache, ElevationService};
use driver_break_core::routing::graph::{load_or_build_reweighted, RouteGraph, RoutingProfile};
use driver_break_core::routing::rest::car_break_interval_hours;
use driver_break_core::routing::workers::WorkerPoolPlan;
use osm4routing::NodeId;

uniffi::setup_scaffolding!();

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
    graph
        .nodes
        .values()
        .min_by(|a, b| {
            let da = haversine_m(lat, lon, a.coord.y, a.coord.x);
            let db = haversine_m(lat, lon, b.coord.y, b.coord.x);
            da.partial_cmp(&db).unwrap()
        })
        .map(|n| n.id)
        .expect("empty graph")
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
    pub cache_hit: bool,
    pub cold_build_s: f64,
    pub warm_load_s: f64,
    /// Encoded as "lon,lat;lon,lat;..." (MapLibre GeoJSON built on the Kotlin side).
    pub route_polyline: String,
    pub poi_lat: f64,
    pub poi_lon: f64,
    pub poi_name: String,
    pub poi_icon_key: String,
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
    let empty = |msg: String| CorridorRouteResult {
        report: msg,
        distance_km: 0.0,
        cache_hit: false,
        cold_build_s: 0.0,
        warm_load_s: 0.0,
        route_polyline: String::new(),
        poi_lat: 0.0,
        poi_lon: 0.0,
        poi_name: String::new(),
        poi_icon_key: String::new(),
    };

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

    report.push_str(&format!(
        "distance_km={dist_km:.2}; distance_m={distance_m:.0}; path_cost={cost:.0}; duration_h={duration_h:.2}\n\
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
        cache_hit: hit2,
        cold_build_s: cold_s,
        warm_load_s: warm_s,
        route_polyline: polyline,
        poi_lat,
        poi_lon,
        poi_name,
        poi_icon_key: poi_icon,
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
    pub height_m: Option<f64>,
    pub width_m: Option<f64>,
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
            height_m: None,
            width_m: None,
            total_weight_kg: None,
        };
    };
    let store = driver_break_core::storage::ConfigStore::new(&storage);
    let limits = store.load_vehicle_limits().unwrap_or_default();
    FfiVehicleLimits {
        axle_weight_kg: limits.axle_weight_kg,
        height_m: limits.height_m,
        width_m: limits.width_m,
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
            height_m: limits.height_m,
            width_m: limits.width_m,
            total_weight_kg: limits.total_weight_kg,
        })
        .is_ok()
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

/// Stub GPS fix for UI actions until Android fused location is wired.
#[uniffi::export]
pub fn last_gps_fix() -> FfiGpsFix {
    FfiGpsFix {
        lat: 61.2,
        lon: 10.7,
        available: true,
    }
}

/// Format a short validation blurb for avoid-major-roads / priority-path share.
#[uniffi::export]
pub fn format_avoid_major_report(avoid_major: bool, priority_path_share_pct: f64) -> String {
    if avoid_major {
        format!(
            "Avoid motorways/trunk/primary: ON\nPriority-path share on last plan: {priority_path_share_pct:.1}%"
        )
    } else {
        format!(
            "Avoid motorways/trunk/primary: OFF\nPriority-path share on last plan: {priority_path_share_pct:.1}%"
        )
    }
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
