//! Host probe: count car edges whose midpoint is in Sweden inside a Langflon
//! window of the Ostlandet Geofabrik extract. Gated on `OSTLANDET_PBF`.

use driver_break_core::routing::elevation::country_iso_at;
use driver_break_core::routing::graph::{RouteGraph, RoutingProfile};
use std::path::PathBuf;

#[test]
#[ignore = "needs OSTLANDET_PBF; run for Phase 1 border-spill measurement"]
fn ostlandet_pbf_contains_sweden_midpoint_edges_near_langflon() {
    let pbf = std::env::var("OSTLANDET_PBF")
        .map(PathBuf::from)
        .expect("OSTLANDET_PBF");
    assert!(pbf.is_file(), "missing {pbf:?}");
    // Window around Långflon (SE) that sits inside the Ostlandet catalog bbox.
    let bbox = [61.85, 12.20, 61.95, 12.35];
    let graph =
        RouteGraph::build_from_pbf_bbox(&pbf, RoutingProfile::Car, bbox).expect("bbox build");
    let mut se = 0usize;
    let mut no = 0usize;
    let mut other = 0usize;
    for e in &graph.edges {
        let mid_lat = (e.start_lat + e.end_lat) * 0.5;
        let mid_lon = (e.start_lon + e.end_lon) * 0.5;
        match country_iso_at(mid_lat, mid_lon) {
            Some("se") => se += 1,
            Some("no") => no += 1,
            _ => other += 1,
        }
    }
    eprintln!(
        "langflon_window edges={} se={} no={} other={}",
        se + no + other,
        se,
        no,
        other
    );
    assert!(
        se > 0,
        "expected Sweden-midpoint edges in ostlandet extract near Langflon; se={se} no={no}"
    );
}
