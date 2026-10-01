//! Espa (Bolleland) → Atnbrua: avoid-tunnels must steer around major E6 tunnels
//! when a land alternative exists.
//!
//! Named tunnels on the default E6 corridor:
//! Skarpsnotunnelen, Moelvtunnelen, Mosoddentunnelen, Øyertunnelen.
//!
//! Run: `cargo test -p driver-break-core --test espa_atnbrua_avoid_tunnels -- --nocapture --ignored`

use std::path::PathBuf;

use driver_break_core::routing::graph::{RouteGraph, RouteOptions, RoutingProfile};

const ESPA: (f64, f64) = (60.562_191_4, 11.256_123_9);
const ATNBRUA: (f64, f64) = (61.851_250_0, 10.233_842_0);

/// Midpoints of OSM-named tunnel ways in the Espa–Atnbrufossen corridor extract.
const NAMED_TUNNELS: &[(&str, f64, f64)] = &[
    ("Skarpsnotunnelen", 60.912_751, 10.718_960),
    ("Moelvtunnelen", 60.925_047, 10.688_553),
    ("Mosoddentunnelen", 61.122_161, 10.443_571),
    ("Øyertunnelen", 61.284_624, 10.321_107),
];

fn corridor_pbf() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/integration-fixtures/espa-atnbrufossen-corridor.osm.pbf")
}

fn bbox() -> [f64; 4] {
    let pad = 0.40;
    [
        ESPA.0.min(ATNBRUA.0) - pad,
        ESPA.1.min(ATNBRUA.1) - pad,
        ESPA.0.max(ATNBRUA.0) + pad,
        ESPA.1.max(ATNBRUA.1) + pad,
    ]
}

fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6_371_000.0;
    let p1 = lat1.to_radians();
    let p2 = lat2.to_radians();
    let dp = (lat2 - lat1).to_radians();
    let dl = (lon2 - lon1).to_radians();
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

fn path_km(graph: &RouteGraph, edges: &[usize]) -> f64 {
    edges.iter().map(|&i| graph.edges[i].length_m).sum::<f64>() / 1000.0
}

fn tunnel_edge_km(graph: &RouteGraph, edges: &[usize]) -> f64 {
    edges
        .iter()
        .filter(|&&i| graph.edges[i].is_tunnel)
        .map(|&i| graph.edges[i].length_m)
        .sum::<f64>()
        / 1000.0
}

fn named_tunnel_hits(
    graph: &RouteGraph,
    path: &[osm4routing::NodeId],
    edges: &[usize],
    within_m: f64,
) -> Vec<String> {
    let mut hits = Vec::new();
    for &(name, tlat, tlon) in NAMED_TUNNELS {
        let by_name = edges.iter().any(|&i| {
            graph.edges[i]
                .name
                .as_deref()
                .is_some_and(|n| n.eq_ignore_ascii_case(name))
        });
        let near = path.iter().any(|id| {
            graph
                .nodes
                .get(id)
                .is_some_and(|n| haversine_m(tlat, tlon, n.coord.y, n.coord.x) <= within_m)
        });
        if by_name || near {
            hits.push(name.to_string());
        }
    }
    hits
}

fn plan(graph: &RouteGraph, avoid_tunnels: bool) -> (Vec<osm4routing::NodeId>, Vec<usize>, f64) {
    let (s, _) = graph.nearest_routable(ESPA.0, ESPA.1).expect("snap Espa");
    let (g, _) = graph
        .nearest_routable(ATNBRUA.0, ATNBRUA.1)
        .expect("snap Atnbrua");
    graph
        .shortest_path_with_options(
            s,
            g,
            false,
            &RouteOptions {
                avoid_tunnels,
                ..Default::default()
            },
        )
        .expect("route")
}

#[test]
#[ignore = "needs espa-atnbrufossen-corridor.osm.pbf under target/integration-fixtures"]
fn espa_atnbrua_avoid_tunnels_skips_named_e6_tunnels() {
    let pbf = corridor_pbf();
    assert!(pbf.is_file(), "missing {}", pbf.display());

    let graph = RouteGraph::build_from_pbf_bbox(&pbf, RoutingProfile::Car, bbox()).expect("bbox");
    let tunnel_edges = graph.edges.iter().filter(|e| e.is_tunnel).count();
    assert!(
        tunnel_edges > 50,
        "corridor graph must carry tunnel flags; got {tunnel_edges}"
    );

    let (path_off, edges_off, _) = plan(&graph, false);
    let (path_on, edges_on, _) = plan(&graph, true);

    let km_off = path_km(&graph, &edges_off);
    let km_on = path_km(&graph, &edges_on);
    let tun_km_off = tunnel_edge_km(&graph, &edges_off);
    let tun_km_on = tunnel_edge_km(&graph, &edges_on);
    let hits_off = named_tunnel_hits(&graph, &path_off, &edges_off, 250.0);
    let hits_on = named_tunnel_hits(&graph, &path_on, &edges_on, 250.0);

    eprintln!("=== Espa → Atnbrua avoid tunnels ===");
    eprintln!(
        "OFF: km={km_off:.1} tunnel_km={tun_km_off:.2} named={hits_off:?} edges={}",
        edges_off.len()
    );
    eprintln!(
        "ON:  km={km_on:.1} tunnel_km={tun_km_on:.2} named={hits_on:?} edges={}",
        edges_on.len()
    );
    eprintln!(
        "TUNNEL_AVOID_PENALTY_MULT={}",
        driver_break_core::routing::toll::TUNNEL_AVOID_PENALTY_MULT
    );

    assert!(
        !hits_off.is_empty() || tun_km_off > 0.5,
        "baseline should use E6 tunnels so the avoid case is meaningful; named={hits_off:?} tunnel_km={tun_km_off}"
    );
    assert!(
        hits_on.is_empty(),
        "avoid tunnels ON must not pass named E6 tunnels; still used {hits_on:?} (tunnel_km={tun_km_on:.2})"
    );
    assert!(
        tun_km_on < 0.5,
        "avoid tunnels ON should drop tunnel-edge km below 0.5; got {tun_km_on:.2}"
    );
}
