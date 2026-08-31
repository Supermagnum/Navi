//! Host plan: Grimåsfeltet (Raufoss) → Nysethvegen / Tollerud.

use std::path::PathBuf;

use driver_break_core::config::EcoConfig;
use driver_break_core::routing::elevation::{ElevationCache, ElevationService};
use driver_break_core::routing::graph::{load_or_build_reweighted, RoutingProfile};
use osm4routing::NodeId;

fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6_378_100.0;
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

fn nearest(graph: &driver_break_core::RouteGraph, lat: f64, lon: f64) -> NodeId {
    graph
        .nodes
        .values()
        .min_by(|a, b| {
            let da = haversine_m(lat, lon, a.coord.y, a.coord.x);
            let db = haversine_m(lat, lon, b.coord.y, b.coord.x);
            da.partial_cmp(&db).unwrap()
        })
        .map(|n| n.id)
        .expect("empty graph")
}

#[test]
#[ignore = "needs oppland/ostlandet PBF under integration-fixtures"]
fn plan_grimafeltet_to_nysethvegen() {
    // From: Grimåsfeltet suburb, Raufoss (OSM place; "2368" not in FTS as housenumber)
    let start_lat = 60.716_383_4;
    let start_lon = 10.620_291_6;
    // To: Nysethvegen near Tollerud (Nysethvegen 10)
    let end_lat = 60.727_820_7;
    let end_lon = 10.604_953_8;

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/integration-fixtures");
    let pbf = [
        root.join("ostlandet-latest.osm.pbf"),
        root.join("oppland-latest.osm.pbf"),
    ]
    .into_iter()
    .find(|p| p.is_file())
    .expect("ostlandet or oppland PBF");
    let elev = root.join("elevation");
    let cache = root.join("graph-cache-raufoss");
    let _ = std::fs::create_dir_all(&elev);
    let _ = std::fs::create_dir_all(&cache);

    let eco = EcoConfig::default();
    let elevation = ElevationService::new(ElevationCache::new(&elev));
    let (graph, hit) =
        load_or_build_reweighted(&pbf, &cache, RoutingProfile::Car, &elevation, &eco)
            .expect("graph");
    eprintln!(
        "pbf={} cache_hit={hit} nodes={} edges={}",
        pbf.display(),
        graph.nodes.len(),
        graph.edges.len()
    );

    let s = nearest(&graph, start_lat, start_lon);
    let g = nearest(&graph, end_lat, end_lon);
    let (path, _, _cost) = graph
        .shortest_path(s, g, false)
        .expect("route Grimåsfeltet → Nysethvegen");
    assert!(path.len() >= 2, "path too short: {}", path.len());

    let mut distance_m = 0.0;
    let mut polyline = String::new();
    for (i, w) in path.windows(2).enumerate() {
        if let Some(idx) = graph.edge_index(w[0], w[1]) {
            distance_m += graph.edges[idx].length_m;
        }
        let n0 = &graph.nodes[&w[0]];
        if i == 0 {
            polyline.push_str(&format!("{},{}", n0.coord.x, n0.coord.y));
        }
        let n1 = &graph.nodes[&w[1]];
        polyline.push_str(&format!(";{},{}", n1.coord.x, n1.coord.y));
    }
    let dist_km = distance_m / 1000.0;
    eprintln!(
        "distance_km={dist_km:.3} path_nodes={} polyline_chars={}",
        path.len(),
        polyline.len()
    );
    assert!(
        dist_km > 0.3 && dist_km < 8.0,
        "unexpected distance {dist_km}"
    );

    let out = root.join("raufoss_grimafeltet_nysethvegen.polyline.txt");
    std::fs::write(&out, &polyline).expect("write polyline");
    eprintln!("wrote {}", out.display());
}
