//! Apply-hazard identity using a prebuilt wetland pack (no full convert).
//!
//! Run:
//!   WETLAND_PACK=/path/to/x.navi-wetland.rkyv \
//!   HEDMARK_PBF=/path/to/hedmark-latest.osm.pbf \
//!   cargo test -p driver-break-core --test wetland_apply_identity -- --nocapture

use std::path::PathBuf;

use driver_break_core::routing::graph::{RouteGraph, RoutingProfile};
use driver_break_core::routing::indexed::load_wetland_pack;
use driver_break_core::routing::wetland::WetlandIndex;

#[test]
fn pack_and_pbf_wetland_apply_identical_counters() {
    let pack = match std::env::var("WETLAND_PACK") {
        Ok(p) if PathBuf::from(&p).is_file() => PathBuf::from(p),
        _ => {
            let p = PathBuf::from("/tmp/navi_w4b_packs/hedmark-latest.navi-wetland.rkyv");
            if !p.is_file() {
                eprintln!("skip: set WETLAND_PACK or build /tmp/navi_w4b_packs");
                return;
            }
            p
        }
    };
    let pbf = match std::env::var("HEDMARK_PBF") {
        Ok(p) if PathBuf::from(&p).is_file() => PathBuf::from(p),
        _ => {
            let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("target/integration-fixtures/hedmark-latest.osm.pbf");
            if !p.is_file() {
                eprintln!("skip: hedmark PBF missing");
                return;
            }
            p
        }
    };

    // Smaller bbox around Atnbrufossen (boardwalk-relevant soft/hard mix region).
    let bbox = [61.70_f64, 10.05, 61.95, 10.45];
    let from_pbf = WetlandIndex::load_from_pbf_bbox(&pbf, bbox).expect("pbf");
    let from_pack = load_wetland_pack(&pack, Some(bbox)).expect("pack");
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
