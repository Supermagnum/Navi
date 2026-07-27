//! One-shot: plan Fondsbu -> Spiterstulen (foot) and write a MapLibre polyline fixture.
//!
//! ```text
//! cargo test -p driver-break-core --test fondsbu_spiterstulen_polyline -- --nocapture
//! ```

mod helpers;

use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use driver_break_core::routing::graph::{RouteGraph, RoutingProfile};
use helpers::hiking::find_poi_by_name;
use helpers::{nearest_node, CombinedPoiIndex};

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/integration-fixtures")
}

#[test]
#[ignore = "host fixture generator: builds ostlandet foot graph (~3 min)"]
fn write_fondsbu_spiterstulen_polyline() {
    let fixtures = fixture_dir();
    let pbf = fixtures.join("ostlandet-latest.osm.pbf");
    assert!(pbf.is_file(), "missing {}", pbf.display());

    let t0 = Instant::now();
    let poi = CombinedPoiIndex::load(&[
        fixtures.join("oppland-latest.osm.pbf"),
        fixtures.join("hedmark-latest.osm.pbf"),
        pbf.clone(),
    ])
    .expect("poi index");
    eprintln!("poi load {:.1}s", t0.elapsed().as_secs_f64());

    let start_hits = find_poi_by_name(&poi, "Fondsbu", 61.3751863, 8.2973974, 5_000.0);
    let end_hits = find_poi_by_name(&poi, "Spiterstulen", 61.6248261, 8.4045818, 5_000.0);
    let start = start_hits.first().expect("Fondsbu POI").clone();
    let end = end_hits.first().expect("Spiterstulen POI").clone();
    let start_name = start.name.clone().unwrap_or_default();
    let end_name = end.name.clone().unwrap_or_default();
    eprintln!(
        "start={start_name} ({:.5},{:.5}) end={end_name} ({:.5},{:.5})",
        start.lat, start.lon, end.lat, end.lon
    );

    let t1 = Instant::now();
    let graph = RouteGraph::build_from_pbf(&pbf, RoutingProfile::Foot).expect("foot graph");
    eprintln!(
        "graph build {:.1}s nodes={} edges={}",
        t1.elapsed().as_secs_f64(),
        graph.nodes.len(),
        graph.edges.len()
    );

    let (s, _, _) = nearest_node(&graph, start.lat, start.lon);
    let (g, _, _) = nearest_node(&graph, end.lat, end.lon);
    let (path, _cost) = graph
        .shortest_path(s, g, false)
        .expect("no foot route Fondsbu->Spiterstulen");
    assert!(path.len() >= 2);

    let mut distance_m = 0.0;
    let mut polyline = String::new();
    let stride = if path.len() < 200 { 1 } else { 8 };
    for (i, w) in path.windows(2).enumerate() {
        if let Some(idx) = graph.edge_index(w[0], w[1]) {
            distance_m += graph.edges[idx].length_m;
        }
        let n0 = &graph.nodes[&w[0]];
        if i == 0 {
            polyline.push_str(&format!("{},{}", n0.coord.x, n0.coord.y));
        }
        let n1 = &graph.nodes[&w[1]];
        if i % stride == 0 || i + 1 == path.len().saturating_sub(1) {
            polyline.push_str(&format!(";{},{}", n1.coord.x, n1.coord.y));
        }
    }
    let out = fixtures.join("fondsbu_spiterstulen.polyline.txt");
    fs::write(&out, &polyline).expect("write polyline");
    eprintln!(
        "wrote {} ({:.1} km, {} verts, {} chars)",
        out.display(),
        distance_m / 1000.0,
        path.len(),
        polyline.len()
    );
    let staging =
        PathBuf::from("core/target/integration-fixtures/fondsbu_spiterstulen.polyline.txt");
    let _ = fs::copy(&out, &staging);
}
