//! Before/after timing: redundant overnight PBF scan vs POI+barrier reuse.
//!
//! Corridor bbox matches `dnt_hiking_integration` (Aakersaetra → Rondvassbu).

use std::path::PathBuf;
use std::time::Instant;

use driver_break_core::poi::PoiIndex;
use driver_break_core::routing::safety::{DangerBarrierIndex, OvernightProximityIndex};

fn fixture_pbf() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/integration-fixtures/ostlandet-latest.osm.pbf")
}

/// DNT Aakersaetra → Jammerdalsbu → Rondvassbu corridor bbox.
const BBOX: [f64; 4] = [61.05, 9.75, 61.95, 11.05];

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
    let poi = PoiIndex::load_from_pbf_bbox_with_overnight_buildings(&pbf, BBOX)
        .expect("poi+buildings");
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
        "OVERNIGHT_SCAN_BENCH corridor=Aakersaetra-Rondvassbu bbox={BBOX:?}\n\
         BEFORE_extra_overnight_raw_pbf_ms={before_extra_ms:.1} \
         buildings={} glaciers={}\n\
         AFTER_poi_plain_ms={poi_plain_ms:.1} \
         AFTER_poi_with_buildings_ms={poi_ms:.1} (overnight_buildings={}) \
         barriers_ms={barriers_ms:.1} (glacier_rings={}) \
         merge_only_ms={after_extra_ms:.1} merged_glaciers={}\n\
         SAVED_vs_poi_plain_plus_legacy≈{:.1} ms \
         (plain+legacy={:.1}; with_buildings+merge={:.1})",
        legacy.buildings.len(),
        legacy.glaciers.len(),
        poi.overnight_buildings().len(),
        barriers.glacier_ring_count(),
        merged.glaciers.len(),
        (poi_plain_ms + before_extra_ms) - (poi_ms + after_extra_ms),
        poi_plain_ms + before_extra_ms,
        poi_ms + after_extra_ms,
    );

    assert!(
        !poi.overnight_buildings().is_empty() || !merged.glaciers.is_empty(),
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
