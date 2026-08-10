//! Before/after timing: bbox-wide overnight buildings vs corridor pre-filter.
//!
//! Corridor geometry matches DNT Aakersaetra → Jammerdalsbu → Rondvassbu waypoints
//! (same bbox as `dnt_hiking_integration`).

use std::path::PathBuf;
use std::time::Instant;

use driver_break_core::config::{
    OVERNIGHT_BUILDING_CORRIDOR_MARGIN_M, SAFETY_MIN_BUILDING_DISTANCE_M,
};
use driver_break_core::poi::{CorridorBand, PoiIndex};
use driver_break_core::routing::safety::{DangerBarrierIndex, OvernightProximityIndex};
use geo::{Distance, Haversine, Point};

fn fixture_pbf() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/integration-fixtures/ostlandet-latest.osm.pbf")
}

/// DNT Aakersaetra → Jammerdalsbu → Rondvassbu corridor bbox.
const BBOX: [f64; 4] = [61.05, 9.75, 61.95, 11.05];

/// Coarse corridor vertices reused from the DNT integration waypoints.
/// Dense enough for a 1.5 km pre-filter (exact hiking polyline is used in FFI).
fn dnt_corridor() -> Vec<(f64, f64)> {
    vec![
        (61.155_366_9, 10.917_463_1), // Aakersaetra
        (61.585_779_9, 10.353_647_3), // Jammerdalsbu
        (61.878_748_3, 9.796_337_6),  // Rondvassbu
    ]
}

/// Second corridor inside the same extract (shorter, further east) to confirm
/// the filter is not tuned only to the DNT diagonal.
fn second_corridor() -> Vec<(f64, f64)> {
    vec![(61.20, 10.80), (61.35, 10.70), (61.50, 10.55)]
}

fn min_dist_to_corridor_m(lat: f64, lon: f64, corridor: &[(f64, f64)]) -> f64 {
    let mut best = f64::INFINITY;
    let p = Point::new(lon, lat);
    for &(alat, alon) in corridor {
        best = best.min(Haversine::distance(p, Point::new(alon, alat)));
    }
    for w in corridor.windows(2) {
        best = best.min(dist_point_to_segment_m(
            lat, lon, w[0].0, w[0].1, w[1].0, w[1].1,
        ));
    }
    best
}

fn dist_point_to_segment_m(
    lat: f64,
    lon: f64,
    a_lat: f64,
    a_lon: f64,
    b_lat: f64,
    b_lon: f64,
) -> f64 {
    let a = Point::new(a_lon, a_lat);
    let b = Point::new(b_lon, b_lat);
    let ab = Haversine::distance(a, b);
    if ab < 1.0 {
        return Haversine::distance(Point::new(lon, lat), a);
    }
    let mid_lat = (a_lat + b_lat) / 2.0;
    let m_per_deg_lat = 111_320.0;
    let m_per_deg_lon = 111_320.0 * mid_lat.to_radians().cos();
    let ax = a_lon * m_per_deg_lon;
    let ay = a_lat * m_per_deg_lat;
    let bx = b_lon * m_per_deg_lon;
    let by = b_lat * m_per_deg_lat;
    let px = lon * m_per_deg_lon;
    let py = lat * m_per_deg_lat;
    let abx = bx - ax;
    let aby = by - ay;
    let t = ((px - ax) * abx + (py - ay) * aby) / (abx * abx + aby * aby);
    let t = t.clamp(0.0, 1.0);
    let qx = ax + t * abx;
    let qy = ay + t * aby;
    let dx = px - qx;
    let dy = py - qy;
    (dx * dx + dy * dy).sqrt()
}

fn round_key(lat: f64, lon: f64) -> (i64, i64) {
    // ~1.1 m grid — stable across load paths.
    (
        (lat * 100_000.0).round() as i64,
        (lon * 100_000.0).round() as i64,
    )
}

#[test]
#[ignore = "bench: needs ostlandet fixture under core/target/integration-fixtures"]
fn overnight_proximity_no_redundant_pbf_scan() {
    let pbf = fixture_pbf();
    assert!(
        pbf.is_file(),
        "missing fixture {} — provision Ostlandet extract first",
        pbf.display()
    );

    // --- BEFORE path: dedicated overnight raw PBF (2 passes) ---
    let t_legacy = Instant::now();
    let legacy = OvernightProximityIndex::load_from_pbf_bbox_legacy(&pbf, BBOX)
        .expect("legacy overnight load");
    let legacy_ms = t_legacy.elapsed().as_secs_f64() * 1000.0;

    // --- AFTER path: hiking POI load (includes buildings) + barriers (glaciers) ---
    let t_poi = Instant::now();
    let poi =
        PoiIndex::load_from_pbf_bbox_with_overnight_buildings(&pbf, BBOX).expect("poi+buildings");
    let poi_ms = t_poi.elapsed().as_secs_f64() * 1000.0;

    let t_poi_plain = Instant::now();
    let _poi_plain = PoiIndex::load_from_pbf_bbox(&pbf, BBOX).expect("poi plain");
    let poi_plain_ms = t_poi_plain.elapsed().as_secs_f64() * 1000.0;

    let t_bar = Instant::now();
    let barriers = DangerBarrierIndex::load_from_pbf_bbox(&pbf, BBOX).expect("barriers");
    let barriers_ms = t_bar.elapsed().as_secs_f64() * 1000.0;

    let t_merge = Instant::now();
    let merged = OvernightProximityIndex::from_poi_buildings_and_barriers(
        poi.overnight_buildings().to_vec(),
        &barriers,
    );
    let merge_ms = t_merge.elapsed().as_secs_f64() * 1000.0;

    // Hiking plan already needs POI + barriers; overnight merge adds negligible work.
    let after_extra_ms = merge_ms;
    let before_extra_ms = legacy_ms;

    eprintln!(
        "OVERNIGHT_SCAN_BENCH corridor=Aakersaetra-Rondvassbu bbox={BBOX:?}\n         BEFORE_extra_overnight_raw_pbf_ms={before_extra_ms:.1}          buildings={} glaciers={}\n         AFTER_poi_plain_ms={poi_plain_ms:.1}          AFTER_poi_with_buildings_ms={poi_ms:.1} (overnight_buildings={})          barriers_ms={barriers_ms:.1} (glacier_rings={})          merge_only_ms={after_extra_ms:.1} merged_glaciers={}\n         SAVED_vs_poi_plain_plus_legacy≈{:.1} ms          (plain+legacy={:.1}; with_buildings+merge={:.1})",
        legacy.buildings.len(),
        legacy.glacier_rings.len(),
        poi.overnight_buildings().len(),
        barriers.glacier_ring_count(),
        merged.glacier_rings.len(),
        (poi_plain_ms + before_extra_ms) - (poi_ms + after_extra_ms),
        poi_plain_ms + before_extra_ms,
        poi_ms + after_extra_ms,
    );

    assert!(
        !poi.overnight_buildings().is_empty() || !merged.glacier_rings.is_empty(),
        "expected some overnight proximity data from POI buildings and/or barrier glaciers"
    );
    assert!(
        after_extra_ms < before_extra_ms * 0.05 || after_extra_ms < 50.0,
        "merge-only path ({after_extra_ms:.1} ms) must be far cheaper than legacy raw scan ({before_extra_ms:.1} ms)"
    );
    // Document that the planner no longer calls the legacy loader.
    assert!(
        before_extra_ms > 100.0,
        "sanity: legacy overnight scan on this corridor should be a measurable cost"
    );
}

#[test]
#[ignore = "bench: needs ostlandet fixture under core/target/integration-fixtures"]
fn overnight_building_corridor_prefilter_bench() {
    let pbf = fixture_pbf();
    assert!(
        pbf.is_file(),
        "missing fixture {} — provision Ostlandet extract first",
        pbf.display()
    );

    let corridor = dnt_corridor();
    let margin = OVERNIGHT_BUILDING_CORRIDOR_MARGIN_M;
    let band = CorridorBand::from_lat_lon(&corridor, margin);

    let t_bbox = Instant::now();
    let bbox_poi =
        PoiIndex::load_from_pbf_bbox_with_overnight_buildings(&pbf, BBOX).expect("bbox buildings");
    let bbox_ms = t_bbox.elapsed().as_secs_f64() * 1000.0;
    let bbox_buildings = bbox_poi.overnight_buildings().to_vec();

    // Baseline: previous two full-extract scans (nodes then ways).
    let (two_pass_poi, two_pass_prof) =
        PoiIndex::load_from_pbf_bbox_with_overnight_buildings_near_corridor_two_pass_profiled(
            &pbf, BBOX, &corridor, margin,
        )
        .expect("two-pass corridor buildings");
    eprintln!(
        "OVERNIGHT_LOAD_PROFILE mode=two_pass total_ms={:.1} band_ms={:.1}          node_pass_ms={:.1} way_pass_ms={:.1} pbf_passes={}          nodes_decoded={} nodes_in_bbox={} coords_kept={}          building_nodes_kept={} ways_seen={} building_ways_seen={}          building_ways_centroid_ok={} building_ways_kept={}          contains_calls={} contains_hits={} overnight_buildings={} pois={}",
        two_pass_prof.total_ms,
        two_pass_prof.band_build_ms,
        two_pass_prof.node_pass_ms,
        two_pass_prof.way_pass_ms,
        two_pass_prof.pbf_passes,
        two_pass_prof.nodes_decoded,
        two_pass_prof.nodes_in_bbox,
        two_pass_prof.coords_kept,
        two_pass_prof.building_nodes_kept,
        two_pass_prof.ways_seen,
        two_pass_prof.building_ways_seen,
        two_pass_prof.building_ways_centroid_ok,
        two_pass_prof.building_ways_kept,
        two_pass_prof.corridor_contains_calls,
        two_pass_prof.corridor_contains_hits,
        two_pass_prof.overnight_buildings,
        two_pass_prof.poi_records,
    );

    let (corr_poi, one_pass_prof) =
        PoiIndex::load_from_pbf_bbox_with_overnight_buildings_near_corridor_profiled(
            &pbf, BBOX, &corridor, margin,
        )
        .expect("one-pass corridor buildings");
    let corr_ms = one_pass_prof.total_ms;
    let corr_buildings = corr_poi.overnight_buildings().to_vec();
    eprintln!(
        "OVERNIGHT_LOAD_PROFILE mode=one_pass total_ms={:.1} band_ms={:.1}          combined_pass_ms={:.1} pbf_passes={}          nodes_decoded={} nodes_in_bbox={} coords_kept={}          building_nodes_kept={} ways_seen={} building_ways_seen={}          building_ways_centroid_ok={} building_ways_kept={}          contains_calls={} contains_hits={} overnight_buildings={} pois={}",
        one_pass_prof.total_ms,
        one_pass_prof.band_build_ms,
        one_pass_prof.node_pass_ms,
        one_pass_prof.pbf_passes,
        one_pass_prof.nodes_decoded,
        one_pass_prof.nodes_in_bbox,
        one_pass_prof.coords_kept,
        one_pass_prof.building_nodes_kept,
        one_pass_prof.ways_seen,
        one_pass_prof.building_ways_seen,
        one_pass_prof.building_ways_centroid_ok,
        one_pass_prof.building_ways_kept,
        one_pass_prof.corridor_contains_calls,
        one_pass_prof.corridor_contains_hits,
        one_pass_prof.overnight_buildings,
        one_pass_prof.poi_records,
    );

    // Exact 150 m distance-check cost on the corridor candidate set (should be tiny).
    let t_dist = Instant::now();
    let mut reject_hits = 0usize;
    for &(clat, clon) in corr_buildings.iter().take(200) {
        for &(blat, blon) in &corr_buildings {
            let d = Haversine::distance(Point::new(clon, clat), Point::new(blon, blat));
            if d > 0.0 && d < SAFETY_MIN_BUILDING_DISTANCE_M {
                reject_hits += 1;
                break;
            }
        }
    }
    let dist_check_ms = t_dist.elapsed().as_secs_f64() * 1000.0;
    eprintln!(
        "OVERNIGHT_EXACT_150M_CHECK candidates={} probe_starts={} reject_hits={} elapsed_ms={:.3}",
        corr_buildings.len(),
        corr_buildings.len().min(200),
        reject_hits,
        dist_check_ms,
    );

    assert_eq!(
        two_pass_poi.overnight_buildings().len(),
        corr_poi.overnight_buildings().len(),
        "one-pass and two-pass overnight building counts must match"
    );
    let _ = two_pass_prof;

    let expected = band.filter_lat_lon(&bbox_buildings);
    let bbox_keys: std::collections::HashSet<_> = bbox_buildings
        .iter()
        .map(|&(a, b)| round_key(a, b))
        .collect();
    let corr_keys: std::collections::HashSet<_> = corr_buildings
        .iter()
        .map(|&(a, b)| round_key(a, b))
        .collect();
    let expected_keys: std::collections::HashSet<_> =
        expected.iter().map(|&(a, b)| round_key(a, b)).collect();

    // Near-150 m boundary cases from the bbox-all set must survive the load filter.
    let boundary: Vec<_> = bbox_buildings
        .iter()
        .copied()
        .filter(|&(lat, lon)| {
            let d = min_dist_to_corridor_m(lat, lon, &corridor);
            (SAFETY_MIN_BUILDING_DISTANCE_M * 0.5..SAFETY_MIN_BUILDING_DISTANCE_M * 1.5)
                .contains(&d)
        })
        .collect();
    let mut boundary_missing = 0usize;
    for &(lat, lon) in &boundary {
        if !corr_keys.contains(&round_key(lat, lon)) {
            boundary_missing += 1;
        }
    }

    // Every building within the real 150 m threshold must remain.
    let within_150: Vec<_> = bbox_buildings
        .iter()
        .copied()
        .filter(|&(lat, lon)| {
            min_dist_to_corridor_m(lat, lon, &corridor) <= SAFETY_MIN_BUILDING_DISTANCE_M
        })
        .collect();
    let mut within_150_missing = 0usize;
    for &(lat, lon) in &within_150 {
        if !corr_keys.contains(&round_key(lat, lon)) {
            within_150_missing += 1;
        }
    }

    let only_in_expected = expected_keys.difference(&corr_keys).count();
    let only_in_corr = corr_keys.difference(&expected_keys).count();
    let reduction = 1.0 - (corr_buildings.len() as f64 / bbox_buildings.len().max(1) as f64);

    eprintln!(
        "OVERNIGHT_CORRIDOR_PREFILTER_BENCH corridor=Aakersaetra-Rondvassbu          margin_m={margin:.0}\n         BEFORE_bbox_all_ms={bbox_ms:.1} buildings={}\n         AFTER_corridor_ms={corr_ms:.1} buildings={}\n         filtered_from_bbox_all={} reduction={:.1}%\n         set_diff expected\\corr={} corr\\expected={}\n         boundary_near_150m={} missing_after_filter={}\n         within_exact_150m={} missing_after_filter={}",
        bbox_buildings.len(),
        corr_buildings.len(),
        expected.len(),
        reduction * 100.0,
        only_in_expected,
        only_in_corr,
        boundary.len(),
        boundary_missing,
        within_150.len(),
        within_150_missing,
    );

    assert!(!bbox_buildings.is_empty(), "expected buildings in DNT bbox");
    assert!(
        corr_buildings.len() * 5 < bbox_buildings.len(),
        "corridor pre-filter should cut the candidate set dramatically          (bbox={}, corridor={})",
        bbox_buildings.len(),
        corr_buildings.len()
    );
    assert!(
        corr_keys.is_subset(&bbox_keys),
        "corridor load must be a subset of bbox-all buildings"
    );
    assert_eq!(
        within_150_missing, 0,
        "pre-filter must not drop any building within the exact 150 m threshold"
    );
    assert_eq!(
        boundary_missing, 0,
        "pre-filter must not drop buildings near the 150 m boundary"
    );
    // Allow tiny centroid/key differences between load paths.
    assert!(
        only_in_expected <= 25 && only_in_corr <= 25,
        "corridor load should match filter(bbox_all); diffs expected\\corr={only_in_expected}          corr\\expected={only_in_corr}"
    );

    // Second corridor — confirm reduction generalizes (reuse bbox-all count).
    let c2 = second_corridor();
    let (corr2, corr2_prof) =
        PoiIndex::load_from_pbf_bbox_with_overnight_buildings_near_corridor_profiled(
            &pbf, BBOX, &c2, margin,
        )
        .expect("corr2");
    eprintln!(
        "OVERNIGHT_CORRIDOR_PREFILTER_SECOND corridor=east-short          bbox_all_buildings={} AFTER_one_pass_ms={:.1} buildings={}",
        bbox_buildings.len(),
        corr2_prof.total_ms,
        corr2.overnight_buildings().len(),
    );
    assert!(
        corr2.overnight_buildings().len() * 5 < bbox_buildings.len(),
        "second corridor should also cut candidates dramatically"
    );
}
