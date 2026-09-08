//! Hamar → Lillestrøm toll policy OD (Ostlandet extract).
//!
//! Chosen after Lillehammer→Imsroa failed the Allow control: that OD's shortest
//! path used Birkebeinervegen (way/117994302) which is untolled in the 2026-07-21
//! PBF. This pair follows the E6 motorway toll spine (Svartelva bru /
//! Espatunnelen and neighbours tagged `toll=yes` in the extract).
//!
//! Control: `TollPolicy::Allow` must use at least one toll edge.
//! Subject: `TollPolicy::NeverUse` must still return a route with zero toll edges.
//!
//! Coords: Hamar ~60.7945, 11.0680; Lillestrøm ~59.9155, 11.2170.
//!
//! Run:
//! `cargo test -p driver-break-core --test hamar_lillestrom_toll -- --ignored --nocapture`

use std::path::PathBuf;

use driver_break_core::config::EcoConfig;
use driver_break_core::routing::elevation::{ElevationCache, ElevationService};
use driver_break_core::routing::graph::{
    load_or_build_reweighted_bbox, way_id_from_edge_id, RouteOptions, RoutingProfile,
};
use driver_break_core::routing::toll::TollPolicy;

fn fixture_pbf() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/integration-fixtures/ostlandet-latest.osm.pbf")
}

fn toll_edge_count(
    graph: &driver_break_core::routing::graph::RouteGraph,
    path_edges: &[usize],
) -> usize {
    path_edges
        .iter()
        .filter(|&&i| graph.edges[i].is_toll)
        .count()
}

#[test]
#[ignore = "needs ostlandet fixture under core/target/integration-fixtures"]
fn hamar_to_lillestrom_toll_never_use_and_allow() {
    let pbf = fixture_pbf();
    assert!(pbf.is_file(), "missing {}", pbf.display());

    // Hamar / Lillestrøm place centroids (Nominatim-aligned).
    let start: (f64, f64) = (60.7945, 11.0680);
    let end: (f64, f64) = (59.9155, 11.2170);

    let elev_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/integration-fixtures/elevation-empty");
    let _ = std::fs::create_dir_all(&elev_dir);
    let elev = ElevationService::new(ElevationCache::new(&elev_dir));
    let eco = EcoConfig::default();
    let cache = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/integration-fixtures/graph-cache-hamar-lillestrom");
    // N–S E6 corridor; pad for free parallel detours under NeverUse.
    let pad = 0.50_f64;
    let bbox = [
        start.0.min(end.0) - pad,
        start.1.min(end.1) - pad,
        start.0.max(end.0) + pad,
        start.1.max(end.1) + pad,
    ];
    let (graph, _) =
        load_or_build_reweighted_bbox(&pbf, &cache, &cache, RoutingProfile::Car, &elev, &eco, bbox)
            .expect("graph");

    let (s, _) = graph
        .nearest_routable(start.0, start.1)
        .expect("snap start");
    let (g, _) = graph.nearest_routable(end.0, end.1).expect("snap end");

    let (path_allow, edges_allow, cost_allow) = graph
        .shortest_path_with_options(s, g, false, &RouteOptions::default())
        .expect("route Allow");
    let toll_allow = toll_edge_count(&graph, &edges_allow);
    let dist_allow_km: f64 = edges_allow
        .iter()
        .map(|&i| graph.edges[i].length_m)
        .sum::<f64>()
        / 1000.0;

    eprintln!("=== Hamar → Lillestrøm (toll policy) ===");
    eprintln!(
        "Allow: nodes={} edges={} dist_km={dist_allow_km:.2} cost={cost_allow:.1} toll_edges={toll_allow} uses_tolls={}",
        path_allow.len(),
        edges_allow.len(),
        graph.path_uses_tolls(&edges_allow)
    );
    if toll_allow > 0 {
        let sample: Vec<_> = edges_allow
            .iter()
            .filter(|&&i| graph.edges[i].is_toll)
            .take(8)
            .map(|&i| {
                let e = &graph.edges[i];
                format!(
                    "way={:?} id={} hwy={:?} name={:?} ref={:?}",
                    way_id_from_edge_id(&e.id),
                    e.id,
                    e.highway,
                    e.name,
                    e.road_ref
                )
            })
            .collect();
        eprintln!("Allow toll sample: {sample:?}");
    }

    assert!(
        !path_allow.is_empty() && !edges_allow.is_empty(),
        "Allow must return a non-empty route"
    );
    assert!(
        toll_allow > 0 && graph.path_uses_tolls(&edges_allow),
        "CONTROL FAILED: Allow must use at least one toll edge on Hamar→Lillestrøm \
         (toll_edges={toll_allow}). Do not swap OD unilaterally; report and stop."
    );

    let never_opts = RouteOptions {
        toll_policy: TollPolicy::NeverUse,
        ..Default::default()
    };
    let (path_never, edges_never, cost_never) = graph
        .shortest_path_with_options(s, g, false, &never_opts)
        .expect("route NeverUse");
    let toll_never = toll_edge_count(&graph, &edges_never);
    let dist_never_km: f64 = edges_never
        .iter()
        .map(|&i| graph.edges[i].length_m)
        .sum::<f64>()
        / 1000.0;

    eprintln!(
        "NeverUse: nodes={} edges={} dist_km={dist_never_km:.2} cost={cost_never:.1} toll_edges={toll_never} uses_tolls={}",
        path_never.len(),
        edges_never.len(),
        graph.path_uses_tolls(&edges_never)
    );

    assert!(
        !path_never.is_empty() && !edges_never.is_empty(),
        "NeverUse must still return a route (not empty / not no-route)"
    );
    assert_eq!(
        toll_never, 0,
        "NeverUse must not use any toll edges (is_toll / toll:motor_vehicle)"
    );
    assert!(
        !graph.path_uses_tolls(&edges_never),
        "NeverUse path_uses_tolls must be false"
    );
    // Detour should be meaningfully longer when free parallels exist.
    assert!(
        dist_never_km + 0.5 >= dist_allow_km,
        "NeverUse dist ({dist_never_km:.1} km) should not undercut Allow ({dist_allow_km:.1} km)"
    );
}
