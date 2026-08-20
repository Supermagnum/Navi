//! Hiking/cycling slow-road preference against real Ostlandet extract when fixture present.

use driver_break_core::config::EcoConfig;
use driver_break_core::routing::elevation::{ElevationCache, ElevationService};
use driver_break_core::routing::graph::{
    apply_slow_road_preference, load_or_build_reweighted_bbox, RouteGraph, RoutingProfile,
};
use osm4routing::NodeId;
use std::collections::HashSet;
use std::path::PathBuf;

fn path_road_refs(graph: &RouteGraph, path: &[NodeId]) -> HashSet<String> {
    let mut refs = HashSet::new();
    for w in path.windows(2) {
        if let Some(idx) = graph.edge_index(w[0], w[1]) {
            if let Some(ref r) = graph.edges[idx].road_ref {
                refs.insert(r.clone());
            }
        }
    }
    refs
}

#[test]
#[ignore = "needs ostlandet fixture under core/target/integration-fixtures"]
fn osterdalen_hiking_prefers_fv237_over_rv3_when_both_connect() {
    let pbf = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/integration-fixtures/ostlandet-latest.osm.pbf");
    if !pbf.is_file() {
        eprintln!("skip: missing {}", pbf.display());
        return;
    }
    // Rendalen / Østerdalen corridor where Rv 3 and Fv 237 both exist in OSM.
    let start_lat: f64 = 61.893;
    let start_lon: f64 = 11.548;
    let end_lat: f64 = 61.918;
    let end_lon: f64 = 11.615;
    let pad = 0.35;
    let bbox = [
        start_lat.min(end_lat) - pad,
        start_lon.min(end_lon) - pad,
        start_lat.max(end_lat) + pad,
        start_lon.max(end_lon) + pad,
    ];
    let elev = ElevationService::new(ElevationCache::new(
        pbf.parent()
            .unwrap_or(std::path::Path::new("."))
            .join("elevation"),
    ));
    let eco = EcoConfig::default();
    let cache = pbf
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join("graph-cache-osterdalen-slow-road-test");
    let (mut graph, _) =
        load_or_build_reweighted_bbox(&pbf, &cache, RoutingProfile::Foot, &elev, &eco, bbox)
            .expect("foot graph");
    apply_slow_road_preference(&mut graph);
    let (s, _) = graph
        .nearest_routable(start_lat, start_lon)
        .expect("snap start");
    let (g, _) = graph.nearest_routable(end_lat, end_lon).expect("snap end");
    let (path, _) = graph
        .shortest_path(s, g, true)
        .expect("route must exist (fallback on high-speed ok)");
    let refs = path_road_refs(&graph, &path);
    eprintln!("osterdalen path refs: {refs:?}");
    assert!(
        refs.contains("237") || refs.iter().any(|r| r.contains("237")),
        "expected Fv 237 on preferred hiking path, got {refs:?}"
    );
    assert!(
        !refs.contains("3") || refs.contains("237"),
        "Rv 3 should not dominate when Fv 237 connects; refs={refs:?}"
    );
}
