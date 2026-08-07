//! Pack vs PBF wetland identity: same Soft/Hard classification and boardwalk
//! carve-out counters when hazards are applied to the same foot graph.

use std::path::PathBuf;
use std::time::Instant;

use driver_break_core::routing::graph::{RouteGraph, RoutingProfile};
use driver_break_core::routing::indexed::{
    convert_region_packs, load_wetland_pack, ConvertOptions, FlatWetlandPack,
};
use driver_break_core::routing::wetland::{tags_indicate_boardwalk, WetlandClass, WetlandIndex};

fn hedmark_pbf() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/integration-fixtures/hedmark-latest.osm.pbf");
    p.is_file().then_some(p)
}

#[test]
fn wetland_pack_matches_pbf_class_and_boardwalk_stats() {
    let Some(pbf) = hedmark_pbf() else {
        eprintln!("skip: hedmark PBF missing");
        return;
    };
    let dir = tempfile::tempdir().expect("tmpdir");
    let mut opts = ConvertOptions::new(dir.path(), &pbf);
    opts.profiles = vec![RoutingProfile::Foot];
    let report = convert_region_packs(&opts).expect("convert");
    assert!(report.wetland_rings > 0);

    // Trip-scale bbox around Esso Myklegård → Atnbrufossen.
    let bbox = [60.452_f64, 9.837, 62.248, 11.765];
    let t0 = Instant::now();
    let from_pbf = WetlandIndex::load_from_pbf_bbox(&pbf, bbox).expect("pbf wetlands");
    let pbf_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let wet_path = dir.path().join(&report.wetland_file);
    let t1 = Instant::now();
    let from_pack = load_wetland_pack(&wet_path, Some(bbox)).expect("pack wetlands");
    let pack_ms = t1.elapsed().as_secs_f64() * 1000.0;
    eprintln!("wetland_pbf_ms={pbf_ms:.1} wetland_pack_ms={pack_ms:.1}");
    assert_eq!(from_pbf.ring_count(), from_pack.ring_count());

    // Dense sample grid must agree on Soft/Hard/None.
    let mut lat = bbox[0];
    while lat <= bbox[2] + 1e-9 {
        let mut lon = bbox[1];
        while lon <= bbox[3] + 1e-9 {
            assert_eq!(
                from_pbf.class_at(lat, lon),
                from_pack.class_at(lat, lon),
                "class mismatch at {lat},{lon}"
            );
            lon += 0.2;
        }
        lat += 0.2;
    }

    let mut g_pbf =
        RouteGraph::build_from_pbf_bbox(&pbf, RoutingProfile::Foot, bbox).expect("foot graph pbf");
    let mut g_pack =
        RouteGraph::build_from_pbf_bbox(&pbf, RoutingProfile::Foot, bbox).expect("foot graph pack");
    let s_pbf = g_pbf.apply_wetland_hazards(&from_pbf);
    let s_pack = g_pack.apply_wetland_hazards(&from_pack);
    assert_eq!(s_pbf.soft_penalized, s_pack.soft_penalized);
    assert_eq!(s_pbf.hard_removed, s_pack.hard_removed);
    assert_eq!(s_pbf.boardwalk_kept, s_pack.boardwalk_kept);
    // Boardwalk carve-out is edge-tag based; if any hard wetland edges were kept,
    // those edges must still be marked boardwalk on the graph (pack path unchanged).
    if s_pack.boardwalk_kept > 0 {
        assert!(g_pack.edges.iter().any(|e| e.is_boardwalk_crossing));
    }
}

#[test]
fn boardwalk_tag_logic_unchanged() {
    assert!(tags_indicate_boardwalk(Some("boardwalk"), None));
    assert!(tags_indicate_boardwalk(None, Some("wood")));
    assert!(!tags_indicate_boardwalk(Some("yes"), None));
}

#[test]
fn flat_pack_preserves_hard_over_soft_precedence() {
    let idx = WetlandIndex::from_parts(vec![
        (
            WetlandClass::SoftAvoid,
            vec![
                [10.0, 60.0],
                [10.4, 60.0],
                [10.4, 60.4],
                [10.0, 60.4],
                [10.0, 60.0],
            ],
        ),
        (
            WetlandClass::HardAvoid,
            vec![
                [10.1, 60.1],
                [10.3, 60.1],
                [10.3, 60.3],
                [10.1, 60.3],
                [10.1, 60.1],
            ],
        ),
    ]);
    assert_eq!(idx.class_at(60.2, 10.2), Some(WetlandClass::HardAvoid));
    let pack = FlatWetlandPack::from_wetland_index(&idx);
    let back = pack.to_wetland_index(None);
    assert_eq!(back.class_at(60.2, 10.2), Some(WetlandClass::HardAvoid));
}
