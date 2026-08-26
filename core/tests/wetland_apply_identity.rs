//! Apply-hazard identity using a wetland pack built from the same small extract.
//!
//! Fixture: `tests/fixtures/atnbrufossen-wetland.osm.pbf` (~1.5 MiB), cut from
//! Hedmark with `scripts/cut-corridor-extract.py` (Atnbrufossen bbox).

use std::path::PathBuf;

use driver_break_core::routing::graph::{RouteGraph, RoutingProfile};
use driver_break_core::routing::indexed::{
    load_wetland_pack, write_archive_atomic, FlatWetlandPack, Preamble, MAGIC_WETLAND,
    WETLAND_FORMAT_VERSION,
};
use driver_break_core::routing::wetland::WetlandIndex;
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
fn pack_and_pbf_wetland_apply_identical_counters() {
    let pbf = fixture_pbf();
    // Atnbrufossen (boardwalk-relevant soft/hard mix region).
    let bbox = [61.70_f64, 10.05, 61.95, 10.45];
    let from_pbf = WetlandIndex::load_from_pbf_bbox(&pbf, bbox).expect("pbf");
    assert!(
        from_pbf.ring_count() > 0,
        "fixture must contain wetland rings"
    );

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
    let from_pack = load_wetland_pack(&wet_path, Some(bbox)).expect("pack");
    assert_eq!(from_pbf.ring_count(), from_pack.ring_count());

    let mut g1 = RouteGraph::build_from_pbf_bbox(&pbf, RoutingProfile::Foot, bbox).expect("g1");
    let mut g2 = RouteGraph::build_from_pbf_bbox(&pbf, RoutingProfile::Foot, bbox).expect("g2");
    let s1 = g1.apply_wetland_hazards(&from_pbf);
    let s2 = g2.apply_wetland_hazards(&from_pack);
    assert_eq!(s1.soft_penalized, s2.soft_penalized, "soft");
    assert_eq!(s1.hard_removed, s2.hard_removed, "hard");
    assert_eq!(s1.boardwalk_kept, s2.boardwalk_kept, "boardwalk_kept");
    eprintln!(
        "identity soft={} hard={} boardwalk_kept={}",
        s1.soft_penalized, s1.hard_removed, s1.boardwalk_kept
    );
}
