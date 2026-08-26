//! Pack vs PBF wetland identity: same Soft/Hard classification and boardwalk
//! carve-out counters when hazards are applied to the same foot graph.
//!
//! Builds the wetland archive directly (not via region convert): tiled Ostlandet/
//! Hedmark converts intentionally skip wetland emission for 4GB-class RAM margin.
//!
//! Fixture: `tests/fixtures/atnbrufossen-wetland.osm.pbf` (~1.5 MiB), cut from
//! Hedmark with `scripts/cut-corridor-extract.py` (Atnbrufossen bbox).

use std::path::PathBuf;
use std::time::Instant;

use driver_break_core::routing::graph::{RouteGraph, RoutingProfile};
use driver_break_core::routing::indexed::{
    load_wetland_pack, write_archive_atomic, FlatWetlandPack, Preamble, MAGIC_WETLAND,
    WETLAND_FORMAT_VERSION,
};
use driver_break_core::routing::wetland::{tags_indicate_boardwalk, WetlandClass, WetlandIndex};
use rkyv::rancor::Error as RkyvError;

fn fixture_pbf() -> PathBuf {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/atnbrufossen-wetland.osm.pbf");
    assert!(
        p.is_file(),
        "missing checked-in fixture {} — regenerate with scripts/cut-corridor-extract.py",
        p.display()
    );
    p
}

#[test]
fn wetland_pack_matches_pbf_class_and_boardwalk_stats() {
    let pbf = fixture_pbf();
    // Atnbrufossen (same clip as the checked-in extract).
    let bbox = [61.70_f64, 10.05, 61.95, 10.45];
    let t0 = Instant::now();
    let from_pbf = WetlandIndex::load_from_pbf_bbox(&pbf, bbox).expect("pbf wetlands");
    let pbf_ms = t0.elapsed().as_secs_f64() * 1000.0;
    assert!(from_pbf.ring_count() > 0);

    let dir = tempfile::tempdir().expect("tmpdir");
    let wet_path = dir.path().join("atnbrufossen.navi-wetland.rkyv");
    let wet_pack = FlatWetlandPack::from_wetland_index(&from_pbf);
    let bytes = rkyv::to_bytes::<RkyvError>(&wet_pack).expect("serialize wetland");
    write_archive_atomic(
        &wet_path,
        Preamble::new(MAGIC_WETLAND, WETLAND_FORMAT_VERSION),
        bytes.as_ref(),
    )
    .expect("write wetland");

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
