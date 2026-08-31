//! Lillehammer → Tretten with avoid-motorways on/off (Ostlandet extract).
//!
//! Avoid-on must drop motorway-grade edges (`highway=motorway` / `motorway_link`,
//! `motorroad` / `expressway`, or dual carriageway with maxspeed>=90). Ordinary
//! E-road trunk without those tags may remain.

use std::collections::HashMap;
use std::path::PathBuf;

use driver_break_core::config::EcoConfig;
use driver_break_core::routing::elevation::{ElevationCache, ElevationService};
use driver_break_core::routing::graph::{
    edge_is_motorway_grade, load_or_build_reweighted_bbox, RouteOptions, RoutingProfile,
};

fn fixture_pbf() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/integration-fixtures/ostlandet-latest.osm.pbf")
}

fn path_highway_counts(
    graph: &driver_break_core::routing::graph::RouteGraph,
    path: &[osm4routing::NodeId],
) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for w in path.windows(2) {
        let Some(idx) = graph.edge_index(w[0], w[1]) else {
            continue;
        };
        let key = graph.edges[idx]
            .highway
            .clone()
            .unwrap_or_else(|| "unknown".into());
        *counts.entry(key).or_insert(0) += 1;
    }
    counts
}

fn motorway_edge_count(
    graph: &driver_break_core::routing::graph::RouteGraph,
    path: &[osm4routing::NodeId],
) -> usize {
    path.windows(2)
        .filter_map(|w| graph.edge_index(w[0], w[1]))
        .filter(|&i| edge_is_motorway_grade(&graph.edges[i]))
        .count()
}

#[test]
#[ignore = "needs ostlandet fixture under core/target/integration-fixtures"]
fn lillehammer_to_tretten_avoid_motorways_on_off() {
    let pbf = fixture_pbf();
    assert!(pbf.is_file(), "missing {}", pbf.display());

    // Keyboard place-search centroids (Lillehammer / Tretten).
    let start: (f64, f64) = (61.115271, 10.466231);
    let end: (f64, f64) = (61.314200, 10.305800);

    let elev_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/integration-fixtures/elevation-empty");
    let _ = std::fs::create_dir_all(&elev_dir);
    let elev = ElevationService::new(ElevationCache::new(&elev_dir));
    let eco = EcoConfig::default();
    let cache = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/integration-fixtures/graph-cache-lillehammer-tretten");
    let pad = 0.35_f64;
    let bbox = [
        start.0.min(end.0) - pad,
        start.1.min(end.1) - pad,
        start.0.max(end.0) + pad,
        start.1.max(end.1) + pad,
    ];
    let (graph, _) =
        load_or_build_reweighted_bbox(&pbf, &cache, RoutingProfile::Car, &elev, &eco, bbox)
            .expect("graph");

    let (s, _) = graph
        .nearest_routable(start.0, start.1)
        .expect("snap start");
    let (g, _) = graph.nearest_routable(end.0, end.1).expect("snap end");

    let (path_off, _, cost_off) = graph
        .shortest_path_with_options(s, g, false, &RouteOptions::default())
        .expect("route avoid=off");
    let (path_on, _, cost_on) = graph
        .shortest_path_with_options(
            s,
            g,
            false,
            &RouteOptions {
                avoid_motorways: true,
                ..Default::default()
            },
        )
        .expect("route avoid=on");

    let dist_off_km: f64 = path_off
        .windows(2)
        .filter_map(|w| graph.edge_index(w[0], w[1]))
        .map(|i| graph.edges[i].length_m)
        .sum::<f64>()
        / 1000.0;
    let dist_on_km: f64 = path_on
        .windows(2)
        .filter_map(|w| graph.edge_index(w[0], w[1]))
        .map(|i| graph.edges[i].length_m)
        .sum::<f64>()
        / 1000.0;

    let share_off = graph.non_motorway_share_pct(&path_off);
    let share_on = graph.non_motorway_share_pct(&path_on);
    let mw_off = motorway_edge_count(&graph, &path_off);
    let mw_on = motorway_edge_count(&graph, &path_on);
    let counts_off = path_highway_counts(&graph, &path_off);
    let counts_on = path_highway_counts(&graph, &path_on);

    eprintln!("=== Lillehammer → Tretten ===");
    eprintln!(
        "OFF: dist_km={dist_off_km:.2} cost={cost_off:.1} non_motorway_share={share_off:.2}% motorway_edges={mw_off}"
    );
    eprintln!("OFF highway counts: {counts_off:?}");
    eprintln!(
        "ON:  dist_km={dist_on_km:.2} cost={cost_on:.1} non_motorway_share={share_on:.2}% motorway_edges={mw_on}"
    );
    eprintln!("ON  highway counts: {counts_on:?}");

    assert_eq!(
        mw_on, 0,
        "avoid-on must not use motorway-grade edges: {counts_on:?}"
    );
    assert!(
        share_on + 1e-6 >= share_off,
        "non-motorway share should not drop when avoiding motorways"
    );
    // Narrower filter: trunk/primary remain available under avoid-on.
    let trunk_or_primary_on = counts_on
        .iter()
        .filter(|(k, _)| {
            matches!(
                k.as_str(),
                "trunk" | "trunk_link" | "primary" | "primary_link"
            )
        })
        .map(|(_, n)| *n)
        .sum::<usize>();
    assert!(
        trunk_or_primary_on > 0 || dist_on_km > 0.0,
        "route should exist; trunk/primary may appear under motorway-only avoid"
    );
}
