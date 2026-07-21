//! Integration test: Kongsvinger -> Lillehammer corridor (Norway).
//!
//! Run: `cargo test --test kongsvinger_lillehammer_integration -- --nocapture --ignored`

mod helpers;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use driver_break_core::config::{EcoConfig, RestConfig, SafetyConfig};
use driver_break_core::poi::PoiCategory;
use driver_break_core::routing::elevation::{
    bbox_to_tiles, DownloadControl, ElevationCache, ElevationDownloader, ElevationService,
};
use driver_break_core::routing::graph::{RouteGraph, RoutingProfile};
use driver_break_core::routing::rest::car_break_interval_hours;
use driver_break_core::storage::{ElevationJobStore, JobStatus, Storage};
use helpers::{
    car_required_breaks, compare_paths, haversine_m, nearest_node, path_edge_indices,
    route_metrics, sample_route_points, CombinedPoiIndex, PoiHit, TestReport,
};
use tokio::runtime::Runtime;

const START_LAT: f64 = 60.562_191_4;
const START_LON: f64 = 11.256_123_9;
const END_LAT: f64 = 61.851_250_0;
const END_LON: f64 = 10.233_842_0;

/// Real-world diesel consumption baseline (600 km / 40 L). Used only for fuel estimate
/// comparison — NOT fed into the eco-mode physics cost function.
const FUEL_L_PER_100KM: f64 = 6.67;

const AVG_SPEED_KMH: f64 = 90.0;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("integration-fixtures")
}

fn ensure_osm_extracts(dir: &Path) -> anyhow::Result<((PathBuf, PathBuf, PathBuf), f64)> {
    fs::create_dir_all(dir)?;
    let hedmark = dir.join("hedmark-latest.osm.pbf");
    let oppland = dir.join("oppland-latest.osm.pbf");
    let ostlandet = dir.join("ostlandet-latest.osm.pbf");
    let fr = "https://download.openstreetmap.fr/extracts/europe/norway";
    let mut download_s = 0.0;
    download_s += download_if_missing(&format!("{fr}/hedmark-latest.osm.pbf"), &hedmark)?;
    download_s += download_if_missing(&format!("{fr}/oppland-latest.osm.pbf"), &oppland)?;
    download_s += download_if_missing(
        "https://download.geofabrik.de/europe/norway/ostlandet-latest.osm.pbf",
        &ostlandet,
    )?;
    Ok(((hedmark, oppland, ostlandet), download_s))
}

fn download_if_missing(url: &str, dest: &Path) -> anyhow::Result<f64> {
    if dest.exists() && fs::metadata(dest)?.len() > 1_000_000 {
        println!("  [cache hit] {}", dest.display());
        return Ok(0.0);
    }
    println!("  [download] {url}");
    let t = Instant::now();
    let rt = Runtime::new()?;
    rt.block_on(async {
        let client = reqwest::Client::new();
        let resp = client.get(url).send().await?;
        if !resp.status().is_success() {
            anyhow::bail!("HTTP {} for {url}", resp.status());
        }
        let bytes = resp.bytes().await?;
        fs::write(dest, &bytes)?;
        Ok::<(), anyhow::Error>(())
    })?;
    let secs = t.elapsed().as_secs_f64();
    println!(
        "  [saved] {} ({} bytes) in {secs:.1}s",
        dest.display(),
        dest.metadata()?.len()
    );
    Ok(secs)
}

fn ensure_dem_tiles(data_dir: &Path, storage: &Storage) -> anyhow::Result<f64> {
    let bbox = [60.35, 9.95, 62.05, 11.65];
    let cache = ElevationCache::new(data_dir);
    let tiles = bbox_to_tiles(bbox);
    let have_all = tiles.iter().all(|t| cache.tile_exists(*t));
    if have_all {
        println!("  [cache hit] all {} DEM tiles present", tiles.len());
        return Ok(0.0);
    }

    println!(
        "  [dem] downloading {} tiles for bbox {:?}",
        tiles.len(),
        bbox
    );
    let t = Instant::now();
    let downloader = ElevationDownloader::new(storage.clone(), cache.clone());
    let job = downloader.queue_region(bbox)?;
    let rt = Runtime::new()?;
    let control = DownloadControl::default();
    let record = rt.block_on(downloader.run_job(job.id, &control))?;

    assert!(
        matches!(record.status, JobStatus::Completed),
        "elevation job stuck or failed: {:?}",
        record.status
    );

    let store = ElevationJobStore::new(storage);
    let (done, total) = store.progress(job.id)?;
    assert_eq!(done, total, "not all tiles completed");
    assert!(done > 0, "zero tiles downloaded");

    let mut found_on_disk = 0usize;
    for source in ["copernicus", "viewfinder", "srtm"] {
        let src_dir = data_dir.join(source);
        if src_dir.is_dir() {
            found_on_disk += fs::read_dir(src_dir)?.count();
        }
    }
    assert!(
        found_on_disk > 0,
        "no tiles under {{data_dir}}/{{copernicus|viewfinder|srtm}}/"
    );
    let secs = t.elapsed().as_secs_f64();
    println!("  [dem] job complete: {done}/{total} tiles, {found_on_disk} on-disk entries in {secs:.1}s");
    Ok(secs)
}

fn build_routing_graph(path: &Path) -> anyhow::Result<RouteGraph> {
    RouteGraph::build_from_pbf(path, RoutingProfile::Car).map_err(|e| {
        anyhow::anyhow!("graph build failed for {}: {e}", path.display())
    })
}

fn passat_eco_config() -> EcoConfig {
    EcoConfig {
        drag_coefficient: 0.28,
        frontal_area_m2: 2.2,
        mass_kg: 1500.0,
        ..EcoConfig::default()
    }
}

#[test]
#[ignore = "network integration test: downloads OSM/DEM data"]
fn kongsvinger_lillehammer_integration() {
    if let Err(e) = run_integration() {
        panic!("integration test failed: {e:#}");
    }
}

fn run_integration() -> anyhow::Result<()> {
    let started = Instant::now();
    let fixtures = fixture_dir();
    fs::create_dir_all(&fixtures)?;

    let mut report = TestReport::new();
    report.section("Setup");
    report.line(&format!("Fixture dir: {}", fixtures.display()));
    report.line(&format!(
        "Route: ({START_LAT:.7}, {START_LON:.7}) -> ({END_LAT:.7}, {END_LON:.7})"
    ));
    report.line("Vehicle: VW Passat B8 diesel — Cd=0.28, A=2.2 m², mass=1500 kg");
    report.line(&format!(
        "Fuel baseline (adaptive-learning seed only, NOT eco physics): {FUEL_L_PER_100KM} L/100km"
    ));
    report.line(
        "Eco-mode uses physics model (Cd, A, mass, delta_h) — independent of the 6.67 L/100km figure.",
    );

    let ((hedmark, oppland, ostlandet), osm_download_s) = ensure_osm_extracts(&fixtures)?;
    let data_dir = fixtures.join("elevation");
    fs::create_dir_all(&data_dir)?;
    let db_path = fixtures.join("integration.db");
    let storage = Storage::open(&db_path)?;
    let dem_download_s = ensure_dem_tiles(&data_dir, &storage)?;
    let download_s = osm_download_s + dem_download_s;
    let compute_started = Instant::now();
    report.line(&format!(
        "Download time (OSM+DEM, 0 if cache hit): {download_s:.1}s (OSM {osm_download_s:.1}s, DEM {dem_download_s:.1}s)"
    ));

    // Geofabrik ostlandet for routable graph (OSM.fr county extracts have border node gaps).
    let graph_opl = build_routing_graph(&ostlandet)?;

    let start = nearest_node(&graph_opl, START_LAT, START_LON);
    let goal = nearest_node(&graph_opl, END_LAT, END_LON);
    report.line(&format!(
        "Routing graph: ostlandet ({} nodes, {} edges)",
        graph_opl.nodes.len(),
        graph_opl.edges.len()
    ));
    report.line(&format!(
        "Start snap: node {} at {:.0} m; goal snap: node {} at {:.0} m",
        start.0.0,
        haversine_m(START_LAT, START_LON, start.1, start.2),
        goal.0.0,
        haversine_m(END_LAT, END_LON, goal.1, goal.2)
    ));
    assert!(
        haversine_m(START_LAT, START_LON, start.1, start.2) < 15_000.0,
        "start snap too far from Kongsvinger"
    );
    assert!(
        haversine_m(END_LAT, END_LON, goal.1, goal.2) < 15_000.0,
        "goal snap too far from Lillehammer"
    );
    let elevation = ElevationService::new(ElevationCache::new(&data_dir));
    let eco = passat_eco_config();

    // --- Test 1: with elevation awareness ---
    report.section("Test 1 — With elevation awareness");
    let mut graph_elev = build_routing_graph(&ostlandet)?;
    graph_elev.apply_eco_reweighting(&elevation, &eco);
    let path1 = graph_elev
        .shortest_path(start.0, goal.0, true)
        .ok_or_else(|| anyhow::anyhow!("no route found (elevation-aware)"))?;
    assert!(path1.0.len() >= 2, "zero-length route");
    assert!(path1.1 > 1000.0, "route cost suspiciously low");

    let edge_idxs = path_edge_indices(&graph_elev, &path1.0);
    assert!(!edge_idxs.is_empty(), "path has no edges");
    let eco_some = edge_idxs
        .iter()
        .filter(|&&i| graph_elev.edges[i].eco_weight.is_some())
        .count();
    let eco_share = eco_some as f64 / edge_idxs.len() as f64;
    report.line(&format!(
        "Edges on route: {}, eco_weight Some: {} ({:.1}%)",
        edge_idxs.len(),
        eco_some,
        eco_share * 100.0
    ));
    assert!(
        eco_share >= 0.25,
        "expected meaningful eco_weight coverage, got {:.1}%",
        eco_share * 100.0
    );

    let metrics1 = route_metrics(&graph_elev, &edge_idxs, &elevation, &eco, true);
    assert!(
        metrics1.total_climb_m > 0.0,
        "expected climb through Gudbrandsdalen"
    );
    report.log_route_metrics("Elevation-aware", &metrics1, path1.1);

    // --- Test 2: without elevation awareness ---
    report.section("Test 2 — Without elevation awareness");
    let graph_flat = build_routing_graph(&ostlandet)?;
    let path2 = graph_flat
        .shortest_path(start.0, goal.0, false)
        .ok_or_else(|| anyhow::anyhow!("no route found (flat)"))?;
    let edge_idxs2 = path_edge_indices(&graph_flat, &path2.0);
    assert!(
        edge_idxs2
            .iter()
            .all(|&i| graph_flat.edges[i].eco_weight.is_none()),
        "expected eco_weight None without reweighting"
    );

    let metrics2 = route_metrics(&graph_flat, &edge_idxs2, &elevation, &eco, false);
    report.log_route_metrics("Flat-weight", &metrics2, path2.1);

    let same_path = compare_paths(&path1.0, &path2.0);
    report.line(&format!(
        "Path identical: {same_path} (distance delta {:.1} m, cost delta {:.0})",
        (metrics1.distance_m - metrics2.distance_m).abs(),
        (path1.1 - path2.1).abs()
    ));
    report.line(&format!(
        "Energy cost (physics eco sum): {:.0} J vs flat base_weight sum {:.0} J",
        metrics1.energy_j, metrics2.flat_weight_sum
    ));

    let dist_km = metrics1.distance_m / 1000.0;
    let fuel_l = dist_km * FUEL_L_PER_100KM / 100.0;
    let duration_h = dist_km / AVG_SPEED_KMH;
    report.line(&format!(
        "Estimated fuel at {FUEL_L_PER_100KM} L/100km baseline: {fuel_l:.2} L for {dist_km:.1} km"
    ));
    report.line(&format!(
        "Estimated duration at {AVG_SPEED_KMH} km/h: {:.2} h ({:.0} min)",
        duration_h,
        duration_h * 60.0
    ));

    // --- Test 3: POI awareness ---
    report.section("Test 3 — POI awareness");
    let safety = SafetyConfig::default();
    let mut poi_paths = vec![hedmark.clone()];
    if oppland.exists() {
        poi_paths.push(oppland.clone());
    }
    let poi_index = CombinedPoiIndex::load(&poi_paths)?;
    report.line(&format!("POI index size: {} records", poi_index.total_len()));

    let sample_pts = sample_route_points(&graph_elev, &path1.0, 10_000.0);
    assert!(!sample_pts.is_empty(), "no sample points along route");

    let categories = [
        (PoiCategory::Water, safety.poi_radius_water_m),
        (PoiCategory::General, safety.poi_radius_general_m),
        (PoiCategory::Restroom, safety.restroom_radius_m()),
        (PoiCategory::Cabin, safety.poi_radius_cabin_m),
    ];

    let mut all_hits: Vec<PoiHit> = Vec::new();
    for (cat, radius) in categories {
        for (lat, lon) in &sample_pts {
            for poi in poi_index.nearest(cat, *lat, *lon, radius) {
                let d = haversine_m(*lat, *lon, poi.lat, poi.lon);
                if all_hits.iter().any(|h| h.osm_id == poi.osm_id) {
                    continue;
                }
                all_hits.push(PoiHit {
                    osm_id: poi.osm_id,
                    name: poi.name.clone(),
                    category: cat,
                    lat: poi.lat,
                    lon: poi.lon,
                    distance_from_sample_m: d,
                    icon_key: poi.icon_key.clone(),
                });
            }
        }
    }
    all_hits.sort_by(|a, b| {
        a.distance_from_sample_m
            .partial_cmp(&b.distance_from_sample_m)
            .unwrap()
    });

    report.line(&format!("POIs found along corridor: {}", all_hits.len()));
    for hit in all_hits.iter().take(15) {
        report.line(&format!(
            "  {:?} id={} {:?} {:.0}m — {}",
            hit.category,
            hit.osm_id,
            hit.name,
            hit.distance_from_sample_m,
            hit.icon_key
        ));
    }
    if all_hits.len() > 15 {
        report.line(&format!("  ... and {} more", all_hits.len() - 15));
    }
    assert!(
        !all_hits.is_empty(),
        "zero POIs along ~{dist_km:.0} km corridor — likely indexing/query bug"
    );

    // --- Rest-stop: Part 3 override — 1 hour break interval ---
    report.section("Rest-stop parameter override (Car, 1 hour interval)");
    let mut rest = RestConfig::default();
    rest.car.break_interval_min_hours = 1.0;
    rest.car.break_interval_max_hours = 1.0;
    let (min_h, max_h) = car_break_interval_hours(&rest);
    let breaks = car_required_breaks(duration_h, max_h);
    // Expected count: floor(driving_hours / break_interval_hours).
    // For this corridor ~2.12 h at 90 km/h with a 1.0 h interval → floor(2.12/1.0) = 2.
    assert!(
        (duration_h - 2.12).abs() < 0.35,
        "duration changed materially ({duration_h:.2} h); re-check expected break count"
    );
    assert_eq!(
        breaks, 2,
        "with 1 h interval on ~{duration_h:.2} h drive, expected exactly floor(duration/interval)=2 breaks, got {breaks}"
    );
    report.line(&format!(
        "Break count with 1 h interval: {breaks} (expected 2 = floor({duration_h:.2}/1.0); first break at the 1 h driving mark)"
    ));
    assert!((min_h - 1.0).abs() < 1e-9 && (max_h - 1.0).abs() < 1e-9);
    report.line(&format!(
        "Car break interval override: {min_h}-{max_h} h, driving time {duration_h:.2} h -> {breaks} required breaks (= floor(duration/interval))"
    ));

    // Place the break near the 1-hour driving mark along the eco route (~AVG_SPEED_KMH).
    let break_at_km = AVG_SPEED_KMH * max_h;
    let break_samples = sample_route_points(&graph_elev, &path1.0, 1_000.0);
    let mut along = 0.0_f64;
    let mut break_lat = START_LAT;
    let mut break_lon = START_LON;
    for w in break_samples.windows(2) {
        let seg = haversine_m(w[0].0, w[0].1, w[1].0, w[1].1) / 1000.0;
        if along + seg >= break_at_km {
            let frac = ((break_at_km - along) / seg).clamp(0.0, 1.0);
            break_lat = w[0].0 + (w[1].0 - w[0].0) * frac;
            break_lon = w[0].1 + (w[1].1 - w[0].1) * frac;
            along = break_at_km;
            break;
        }
        along += seg;
        break_lat = w[1].0;
        break_lon = w[1].1;
    }
    report.line(&format!(
        "Planned break at ~{break_at_km:.1} km / {max_h:.1} h driving mark: ({break_lat:.5}, {break_lon:.5})"
    ));
    assert!(
        break_at_km > 10.0 && break_at_km < dist_km - 10.0,
        "break placement degenerate at start/end: {break_at_km:.1} km of {dist_km:.1}"
    );
    assert!(
        (along - break_at_km).abs() < 5.0 || along >= break_at_km - 1.0,
        "break not near 1-hour mark"
    );

    report.section("Summary");
    report.line(&format!(
        "Test 1 distance: {:.1} km, climb {:.0} m, descent {:.0} m, energy {:.0} J",
        metrics1.distance_m / 1000.0,
        metrics1.total_climb_m,
        metrics1.total_descent_m,
        metrics1.energy_j
    ));
    report.line(&format!(
        "Test 2 distance: {:.1} km, flat cost {:.0}",
        metrics2.distance_m / 1000.0,
        metrics2.flat_weight_sum
    ));
    let compute_s = compute_started.elapsed().as_secs_f64();
    let wall_s = started.elapsed().as_secs_f64();
    report.line(&format!("Paths differ: {}", !same_path));
    report.line(&format!("POI hits: {}", all_hits.len()));
    report.line(&format!(
        "Timing: download {download_s:.1}s | compute {compute_s:.1}s | wall {wall_s:.1}s"
    ));
    println!(
        "  [timing] download={download_s:.1}s compute={compute_s:.1}s wall={wall_s:.1}s"
    );

    let report_path = fixtures.join("kongsvinger_lillehammer_report.md");
    report.write(&report_path)?;
    println!("\n{}", report.to_string());
    println!("\nReport written to {}", report_path.display());

    Ok(())
}
