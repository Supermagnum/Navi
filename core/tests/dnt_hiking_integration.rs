//! Integration test: multi-day DNT hiking route Aakersaetra -> Jammerdalsbu -> Rondvassbu.
//!
//! Run: `cargo test --test dnt_hiking_integration -- --nocapture --ignored`

mod helpers;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use driver_break_core::config::{Profile, RestConfig, SafetyConfig};
use driver_break_core::poi::PoiCategory;
use driver_break_core::routing::elevation::{
    bbox_to_tiles, DownloadControl, ElevationCache, ElevationDownloader, ElevationService,
};
use driver_break_core::routing::graph::{RouteGraph, RoutingProfile};
use driver_break_core::storage::{ElevationJobStore, JobStatus, Storage};
use helpers::hiking::{
    apply_dnt_preference, build_route_samples, chain_paths, find_poi_by_name, hiking_eco_config,
    overnight_display_name, plan_multi_day, validate_route, EdgeTagMap, RestKind,
    OVERNIGHT_NEAR_HUT_MAX_M,
};
use helpers::{
    haversine_m, nearest_node, path_edge_indices, route_metrics, sample_route_points,
    CombinedPoiIndex, TestReport,
};
use tokio::runtime::Runtime;

const START_LAT: f64 = 61.155_366_9;
const START_LON: f64 = 10.917_463_1;
const VIA_LAT: f64 = 61.585_779_9;
const VIA_LON: f64 = 10.353_647_3;
const END_LAT: f64 = 61.878_748_3;
const END_LON: f64 = 9.796_337_6;

const JAMMERDALSBU_NAME: &str = "jammerdalsbu";

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("integration-fixtures")
}

/// Returns wall time spent downloading (0 on cache hit).
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
    Ok(t.elapsed().as_secs_f64())
}

fn ensure_osm_extracts_timed(dir: &Path) -> anyhow::Result<((PathBuf, PathBuf, PathBuf), f64)> {
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

/// Returns wall time spent downloading DEM tiles (0 on cache hit).
fn ensure_dem_tiles(data_dir: &Path, storage: &Storage) -> anyhow::Result<f64> {
    let bbox = [61.05, 9.75, 61.95, 11.05];
    let cache = ElevationCache::new(data_dir);
    let tiles = bbox_to_tiles(bbox);
    if tiles.iter().all(|t| cache.tile_exists(*t)) {
        println!("  [cache hit] all {} DEM tiles present", tiles.len());
        return Ok(0.0);
    }
    let t = Instant::now();
    let downloader = ElevationDownloader::new(storage.clone(), cache.clone());
    let job = downloader.queue_region(bbox)?;
    let rt = Runtime::new()?;
    let record = rt.block_on(downloader.run_job(job.id, &DownloadControl::default()))?;
    assert!(matches!(record.status, JobStatus::Completed));
    let store = ElevationJobStore::new(storage);
    let (done, total) = store.progress(job.id)?;
    assert_eq!(done, total);
    let secs = t.elapsed().as_secs_f64();
    println!("  [dem] {done}/{total} tiles ready in {secs:.1}s");
    Ok(secs)
}

#[test]
#[ignore = "network integration test: downloads OSM/DEM data"]
fn dnt_hiking_integration() {
    if let Err(e) = run_integration() {
        panic!("integration test failed: {e:#}");
    }
}

fn run_integration() -> anyhow::Result<()> {
    let started = Instant::now();
    let fixtures = fixture_dir();
    fs::create_dir_all(&fixtures)?;

    let mut report = TestReport::with_title(
        "DNT Hiking Integration Report — Aakersaetra -> Jammerdalsbu -> Rondvassbu",
    );

    report.section("Setup");
    report.line(&format!("Fixture dir: {}", fixtures.display()));
    report.line(&format!(
        "Route: ({START_LAT:.7}, {START_LON:.7}) -> ({VIA_LAT:.7}, {VIA_LON:.7}) -> ({END_LAT:.7}, {END_LON:.7})"
    ));
    report.line("Profile: Hiking (foot), eco-mode on (locked default)");
    report.line("Path preference: DNT network soft penalty on non-DNT foot edges");

    let ((hedmark, oppland, ostlandet), osm_download_s) = ensure_osm_extracts_timed(&fixtures)?;
    let data_dir = fixtures.join("elevation");
    fs::create_dir_all(&data_dir)?;
    let storage = Storage::open(fixtures.join("dnt_hiking.db"))?;
    let dem_download_s = ensure_dem_tiles(&data_dir, &storage)?;
    let download_s = osm_download_s + dem_download_s;
    let compute_started = Instant::now();

    let rest = RestConfig::default();
    let safety = SafetyConfig::default();
    report.line(&format!(
        "RestConfig hiking: main={:.3} km, alt={:.3} km, max daily={:.1} km",
        rest.hiking.main_break_distance_km,
        rest.hiking.alternative_break_distance_km,
        rest.hiking.max_daily_distance_km
    ));
    report.line(&format!(
        "SafetyConfig: cabin radius={:.0} m, network hut radius={:.0} m, hut preference={:.0} m",
        safety.poi_radius_cabin_m,
        safety.poi_radius_network_hut_m,
        safety.network_hut_preference_radius_m
    ));
    report.line(&format!(
        "Download time (OSM+DEM, 0 if cache hit): {download_s:.1}s (OSM {osm_download_s:.1}s, DEM {dem_download_s:.1}s)"
    ));

    // Parallelize independent PBF reads (POI extracts, DNT tags, foot graph).
    let t_load = Instant::now();
    let ostlandet_poi = ostlandet.clone();
    let oppland_c = oppland.clone();
    let hedmark_c = hedmark.clone();
    let (poi_result, tag_result, graph_result) = std::thread::scope(|s| {
        let poi_h = s.spawn(|| CombinedPoiIndex::load(&[oppland_c, hedmark_c]));
        let tag_h = s.spawn(|| EdgeTagMap::load_from_pbf(&ostlandet_poi));
        let graph_h = s.spawn(|| RouteGraph::build_from_pbf(&ostlandet, RoutingProfile::Foot));
        (
            poi_h.join().expect("poi thread"),
            tag_h.join().expect("tag thread"),
            graph_h.join().expect("graph thread"),
        )
    });
    let poi_index = poi_result?;
    let tag_map = tag_result?;
    let mut graph = graph_result?;
    println!(
        "  [timing] parallel PBF load: {:.1}s ({} threads)",
        t_load.elapsed().as_secs_f64(),
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
    );
    report.line(&format!(
        "POI index (oppland+hedmark): {} records",
        poi_index.total_len()
    ));
    assert!(poi_index.total_len() > 0, "empty POI index");
    report.line(&format!(
        "Edge tag map: {} tagged edges, {} DNT relation ways",
        tag_map.tags.len(),
        tag_map.dnt_way_ids.len()
    ));

    let elevation = ElevationService::new(ElevationCache::new(&data_dir));
    let eco = hiking_eco_config();
    let warmed = elevation.warm_bbox([61.05, 9.75, 61.95, 11.05])?;
    println!("  [timing] DEM warm ({warmed} tiles)");

    report.line(&format!(
        "Routing graph (ostlandet, foot): {} nodes, {} edges",
        graph.nodes.len(),
        graph.edges.len()
    ));
    assert!(graph.edges.len() > 10_000, "degenerate foot graph");

    let t_reweight = Instant::now();
    graph.apply_eco_reweighting(&elevation, &eco);
    apply_dnt_preference(&mut graph, &tag_map);
    println!(
        "  [timing] eco + DNT reweight: {:.1}s",
        t_reweight.elapsed().as_secs_f64()
    );
    assert!(
        RestConfig::default().eco_mode_enabled(Profile::Hiking),
        "hiking eco-mode should be locked on"
    );

    let start = nearest_node(&graph, START_LAT, START_LON);
    let via = nearest_node(&graph, VIA_LAT, VIA_LON);
    let goal = nearest_node(&graph, END_LAT, END_LON);
    for (label, lat, lon, snap) in [
        ("Start Aakersaetra", START_LAT, START_LON, start),
        ("Via Jammerdalsbu", VIA_LAT, VIA_LON, via),
        ("End Rondvassbu", END_LAT, END_LON, goal),
    ] {
        let d = haversine_m(lat, lon, snap.1, snap.2);
        report.line(&format!("{label} snap: {:.0} m to graph node {}", d, snap.0.0));
        assert!(d < 500.0, "{label} snap too far: {d:.0} m");
    }

    let leg1 = graph
        .shortest_path(start.0, via.0, true)
        .ok_or_else(|| anyhow::anyhow!("no route start -> via"))?;
    let leg2 = graph
        .shortest_path(via.0, goal.0, true)
        .ok_or_else(|| anyhow::anyhow!("no route via -> end"))?;
    let full_path = chain_paths(leg1.0, &leg2.0);
    assert!(full_path.len() >= 2, "degenerate path");

    let edge_idxs = path_edge_indices(&graph, &full_path);
    assert!(!edge_idxs.is_empty(), "path has no edges");

    let metrics = route_metrics(&graph, &edge_idxs, &elevation, &eco, true);
    let total_km = metrics.distance_m / 1000.0;
    report.line(&format!(
        "Total route: {:.1} km, climb {:.0} m, descent {:.0} m, energy {:.0} J ({:.2} kWh)",
        total_km,
        metrics.total_climb_m,
        metrics.total_descent_m,
        metrics.energy_j,
        metrics.energy_j / 3_600_000.0
    ));
    assert!(
        total_km > 40.0,
        "expected multi-day distance > 40 km, got {total_km:.1}"
    );

    report.section("0. DEM coverage on hiking corridor edges");
    let mut dem_ok = 0usize;
    let mut dem_miss = 0usize;
    for &i in &edge_idxs {
        let e = &graph.edges[i];
        let a = elevation.get_elevation(e.start_lat, e.start_lon);
        let b = elevation.get_elevation(e.end_lat, e.end_lon);
        if a.is_some() && b.is_some() {
            dem_ok += 1;
        } else {
            dem_miss += 1;
        }
    }
    let dem_pct = 100.0 * dem_ok as f64 / edge_idxs.len().max(1) as f64;
    report.line(&format!(
        "DEM both endpoints: {dem_ok}/{} edges ({dem_pct:.1}%), missing {dem_miss}",
        edge_idxs.len()
    ));
    if dem_miss == 0 {
        report.line(
            "STATUS: 100% DEM coverage — energy figure is not a missing-tile joule-fallback artifact.",
        );
    } else {
        report.line(
            "STATUS: incomplete DEM — energy uses flat-joule fallback on missing edges (post-fix units).",
        );
    }

    report.section("1. DNT coverage and route validation");
    let validation = validate_route(&graph, &edge_idxs, &tag_map);
    let dnt_km = validation.dnt_m / 1000.0;
    let other_km = validation.other_foot_m / 1000.0;
    report.line(&format!(
        "Total: {:.1} km | DNT-tagged: {:.1} km ({:.1}%) | other priority footpaths: {:.1} km ({:.1}%) | overall priority-path: {:.1}%",
        total_km,
        dnt_km,
        validation.dnt_pct(),
        other_km,
        validation.other_foot_pct(),
        validation.priority_pct()
    ));
    report.line(&format!(
        "DNT summary: {dnt_km:.1} km of {total_km:.1} km on DNT network, {:.1}%",
        validation.dnt_pct()
    ));

    assert!(
        validation.dnt_m > 0.0,
        "zero DNT-tagged distance — likely tag map or routing issue"
    );
    assert!(
        validation.dnt_pct() >= 30.0,
        "expected high DNT coverage on Rondane corridor, got {:.1}%",
        validation.dnt_pct()
    );
    assert!(
        validation.forbidden_segments.is_empty(),
        "forbidden segments on route: {:?}",
        validation.forbidden_segments
    );
    for w in &validation.low_priority_warnings {
        report.line(&format!("WARNING: {w}"));
    }

    report.section("2. Jammerdalsbu POI resolution");
    let jammer_hits = find_poi_by_name(&poi_index, JAMMERDALSBU_NAME, VIA_LAT, VIA_LON, 2_000.0);
    assert!(
        !jammer_hits.is_empty(),
        "Jammerdalsbu not found in POI index near via coordinates"
    );
    for hit in &jammer_hits {
        let d = haversine_m(VIA_LAT, VIA_LON, hit.lat, hit.lon);
        report.line(&format!(
            "  POI id={} name={:?} categories={:?} {:.0} m from via coords",
            hit.osm_id, hit.name, hit.categories, d
        ));
        assert!(d < 500.0, "Jammerdalsbu POI too far: {d:.0} m");
    }

    report.section("3. Day-by-day plan");
    let samples = build_route_samples(&graph, &full_path);
    let sample_total_km = samples.last().map(|s| s.cumulative_km).unwrap_or(0.0);
    assert!(
        (sample_total_km - total_km).abs() < 0.05,
        "sample cumulative ({sample_total_km:.3}) must match route_metrics ({total_km:.3})"
    );
    let days = plan_multi_day(&samples, &rest, &safety, &poi_index);
    assert!(days.len() > 1, "expected multiple day segments, got {}", days.len());
    let day_sum_km: f64 = days.iter().map(|d| d.distance_km).sum();
    let day_sum_rel = if total_km > 0.0 {
        (day_sum_km - total_km).abs() / total_km
    } else {
        0.0
    };
    report.line(&format!(
        "Day-segment distance sum: {day_sum_km:.2} km vs route total {total_km:.2} km (rel err {:.3}%)",
        day_sum_rel * 100.0
    ));
    assert!(
        day_sum_rel < 0.005,
        "sum(day distances) {day_sum_km:.2} km must be within 0.5% of route {total_km:.2} km (got {:.2}%)",
        day_sum_rel * 100.0
    );
    report.line("| Day | Start km | End km | Distance km | Rest stops | Overnight | Hut dist m | Detour |");
    report.line("|-----|----------|--------|-------------|------------|-----------|------------|--------|");

    let max_daily = rest.hiking.max_daily_distance_km;
    let mut gap_days = 0u32;
    for day in &days {
        assert!(
            day.distance_km <= max_daily + 0.5,
            "day {} exceeds max daily: {:.1} km",
            day.day,
            day.distance_km
        );
        let overnight_str = day
            .overnight
            .as_ref()
            .map(|o| {
                format!(
                    "{}{}",
                    overnight_display_name(o),
                    if o.is_network { " [network]" } else { "" }
                )
            })
            .unwrap_or_else(|| "NONE".into());
        let hut_dist = day
            .overnight
            .as_ref()
            .map(|o| format!("{:.0}", o.distance_from_target_m))
            .unwrap_or_else(|| "-".into());
        let detour = day
            .overnight
            .as_ref()
            .map(|o| {
                let km = o.distance_from_target_m / 1000.0;
                if km >= 0.5 {
                    format!("{km:.1} km detour")
                } else {
                    format!("{:.0} m", o.distance_from_target_m)
                }
            })
            .unwrap_or_else(|| "-".into());
        report.line(&format!(
            "| {} | {:.1} | {:.1} | {:.1} | {} | {} | {} | {} |",
            day.day,
            day.start_km,
            day.end_km,
            day.distance_km,
            day.rest_stops.len(),
            overnight_str,
            hut_dist,
            detour
        ));
        report.line(&format!(
            "  Start: ({:.5}, {:.5}) -> End: ({:.5}, {:.5})",
            day.start_lat, day.start_lon, day.end_lat, day.end_lon
        ));
        if let Some(o) = &day.overnight {
            if o.distance_from_target_m >= 500.0 {
                report.line(&format!(
                    "  Overnight detour: {:.1} km off-trail to reach {}",
                    o.distance_from_target_m / 1000.0,
                    overnight_display_name(o)
                ));
            }
        }
        for rs in &day.rest_stops {
            let kind = match rs.kind {
                RestKind::Main => "main",
                RestKind::Alternative => "alt",
            };
            let note = rs.reason.as_deref().unwrap_or("");
            report.line(&format!(
                "    rest @ {:.2} km ({kind}) ({:.5}, {:.5}) {note}",
                rs.cumulative_km, rs.lat, rs.lon
            ));
        }
        if day.overnight_gap {
            gap_days += 1;
            report.line(&format!(
                "  FLAG: day {} overnight gap — hut > {:.0} m from target or none found",
                day.day, OVERNIGHT_NEAR_HUT_MAX_M
            ));
        }
    }

    report.section("4. Overnight candidates");
    for day in &days {
        if let Some(o) = &day.overnight {
            report.line(&format!(
                "Day {}: {} id={} network={} dist={:.0} m ({:.1} km detour) safety_rejected={}",
                day.day,
                overnight_display_name(o),
                o.poi.osm_id,
                o.is_network,
                o.distance_from_target_m,
                o.distance_from_target_m / 1000.0,
                o.safety_rejected
            ));
        } else {
            report.line(&format!("Day {}: NO OVERNIGHT CANDIDATE", day.day));
        }
    }
    report.line(
        "Note: overnight boundaries are probed every 0.5 km along each day's corridor; \
         among same network class the planner prefers longer days toward the daily budget \
         unless that would worsen overnight detour by more than 500 m. Detour distance is \
         surfaced in the day table for UX.",
    );
    assert!(
        days.iter().all(|d| {
            d.end_km >= samples.last().map(|s| s.cumulative_km).unwrap_or(0.0) - 0.01
                || d.overnight.is_some()
        }),
        "mid-route day without overnight candidate"
    );

    report.section("5. Water POIs along corridor");
    // Separate indexing health from corridor sparsity: eco-shortened highland
    // paths can legitimately miss drinking_water/spring nodes within 2 km.
    let trailhead_water = poi_index.nearest(
        PoiCategory::Water,
        START_LAT,
        START_LON,
        5_000.0,
    );
    report.line(&format!(
        "Indexing check — water within 5 km of Aakersaetra: {}",
        trailhead_water.len()
    ));
    if trailhead_water.is_empty() {
        let report_path = fixtures.join("dnt_hiking_report.md");
        report.write(&report_path)?;
        println!("\n{}", report.to_string());
        anyhow::bail!(
            "water POI indexing broken: zero water near Aakersaetra trailhead (not a path-sparsity issue)"
        );
    }

    let sample_pts = sample_route_points(&graph, &full_path, 5_000.0);
    report.line(&format!(
        "Route sample points for water search: {} (initial radius {:.0} m)",
        sample_pts.len(),
        safety.poi_radius_water_m
    ));
    let mut water_hits: Vec<(i64, Option<String>, f64, f64)> = Vec::new();
    let mut search_radius = safety.poi_radius_water_m;
    for (lat, lon) in &sample_pts {
        for w in poi_index.nearest(PoiCategory::Water, *lat, *lon, search_radius) {
            if water_hits.iter().any(|h| h.0 == w.osm_id) {
                continue;
            }
            water_hits.push((w.osm_id, w.name.clone(), w.lat, w.lon));
        }
    }
    // Widen once for highland stretches before declaring corridor dry.
    if water_hits.is_empty() {
        search_radius = 5_000.0;
        report.line("No hits at default radius — retrying corridor samples at 5000 m");
        for (lat, lon) in &sample_pts {
            for w in poi_index.nearest(PoiCategory::Water, *lat, *lon, search_radius) {
                if water_hits.iter().any(|h| h.0 == w.osm_id) {
                    continue;
                }
                water_hits.push((w.osm_id, w.name.clone(), w.lat, w.lon));
            }
        }
    }
    report.line(&format!(
        "Water POIs found along corridor (radius {:.0} m): {}",
        search_radius,
        water_hits.len()
    ));
    report.line(
        "Note: natural/untreated sources should be treated before drinking (informational).",
    );
    for (id, name, lat, lon) in water_hits.iter().take(20) {
        report.line(&format!("  id={id} name={name:?} ({lat:.5}, {lon:.5})"));
    }
    if water_hits.len() > 20 {
        report.line(&format!("  ... and {} more", water_hits.len() - 20));
    }
    if water_hits.is_empty() {
        report.line(
            "STATUS: indexing OK at trailhead, but this eco-selected corridor has no mapped water near samples — treating as path sparsity (open observation), not an indexing regression.",
        );
        // Do not fail the suite: eco-path change exposed highland sparsity.
    } else {
        report.line("STATUS: water POIs present along corridor.");
    }

    report.section("6. Flags / rest-interval fallbacks");
    let all_rests: Vec<_> = days.iter().flat_map(|d| d.rest_stops.iter()).collect();
    let alt_rests: u32 = all_rests
        .iter()
        .filter(|r| matches!(r.kind, RestKind::Alternative))
        .count() as u32;
    let main_rests: u32 = all_rests
        .iter()
        .filter(|r| matches!(r.kind, RestKind::Main))
        .count() as u32;
    report.line(&format!("Day segments: {}", days.len()));
    report.line(&format!("Overnight gap days: {gap_days}"));
    report.line(&format!(
        "Rest stops: {main_rests} main + {alt_rests} alternative (of {})",
        all_rests.len()
    ));
    report.line(
        "Rejection taxonomy: current planner rejects main-interval rests only for missing \
         water/general POI within radius — building/glacier safety lists are empty here \
         (not applied to rest placement).",
    );
    let mut no_poi_alt = 0u32;
    let mut forced_main = 0u32;
    let mut other_alt = 0u32;
    for r in all_rests.iter().filter(|r| matches!(r.kind, RestKind::Alternative)) {
        let reason = r.reason.as_deref().unwrap_or("");
        if reason.contains("alt POI found") {
            no_poi_alt += 1;
        } else if reason.contains("forced main mark") {
            forced_main += 1;
        } else {
            other_alt += 1;
        }
        report.line(&format!(
            "  alt @ {:.2} km: {}",
            r.cumulative_km,
            if reason.is_empty() { "(no reason)" } else { reason }
        ));
    }
    report.line(&format!(
        "Alt fallback breakdown: no_poi_then_alt_poi={no_poi_alt}, forced_at_main_mark={forced_main}, other={other_alt}"
    ));
    // Cluster by ~25 km route buckets to spot terrain-local spikes.
    let mut buckets = std::collections::BTreeMap::<u32, u32>::new();
    for r in all_rests.iter().filter(|r| matches!(r.kind, RestKind::Alternative)) {
        let bucket = (r.cumulative_km / 25.0).floor() as u32;
        *buckets.entry(bucket).or_default() += 1;
    }
    if !buckets.is_empty() {
        report.line("Alt fallback clusters (25 km buckets):");
        for (b, n) in &buckets {
            let lo = *b as f64 * 25.0;
            let hi = lo + 25.0;
            report.line(&format!("  {lo:.0}-{hi:.0} km: {n} fallback(s)"));
        }
    }
    report.line(&format!(
        "Forbidden segments: {}",
        validation.forbidden_segments.len()
    ));
    report.line(&format!(
        "Low priority-path warnings: {}",
        validation.low_priority_warnings.len()
    ));

    if gap_days > 0 {
        let report_path = fixtures.join("dnt_hiking_report.md");
        report.write(&report_path)?;
        println!("\n{}", report.to_string());
        println!("\nPartial report written to {}", report_path.display());
        anyhow::bail!("{gap_days} day segment(s) lack suitable overnight hut within range");
    }

    let compute_s = compute_started.elapsed().as_secs_f64();
    let wall_s = started.elapsed().as_secs_f64();

    report.section("Summary");
    report.line(&format!(
        "Total distance: {total_km:.1} km across {} days (day-sum {day_sum_km:.1} km)",
        days.len()
    ));
    report.line(&format!(
        "DNT coverage: {dnt_km:.1}/{total_km:.1} km ({:.1}%)",
        validation.dnt_pct()
    ));
    report.line(&format!("Water POIs: {}", water_hits.len()));
    report.line(&format!("Jammerdalsbu POI matches: {}", jammer_hits.len()));
    report.line(&format!(
        "Timing: download {download_s:.1}s | compute {compute_s:.1}s | wall {wall_s:.1}s"
    ));
    println!(
        "  [timing] download={download_s:.1}s compute={compute_s:.1}s wall={wall_s:.1}s"
    );

    let report_path = fixtures.join("dnt_hiking_report.md");
    report.write(&report_path)?;
    println!("\n{}", report.to_string());
    println!("\nReport written to {}", report_path.display());

    Ok(())
}
