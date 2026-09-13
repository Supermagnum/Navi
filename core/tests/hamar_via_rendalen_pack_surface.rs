//! Hamar → via (Elverum) → Rendalsportalen: via-point plans must apply soft costs.
//!
//! Uses the same v8 ostlandet car tiles as the Espa→Dombås pack surface check
//! when present under `../target/espa-dombas-e2e`.

use driver_break_core::routing::graph::{
    apply_surface_preference, MotorSoftCostProfile, RouteGraph, RouteOptions, RoutingProfile,
    SurfaceQuality, SurfaceRoutingMode,
};
use driver_break_core::routing::indexed::{load_graph_pack_bbox, merge_tile_graphs};
use std::path::{Path, PathBuf};

const HAMAR: (f64, f64) = (60.7970558, 11.0688819);
const VIA: (f64, f64) = (60.8881202, 11.5078941);
const RENDAL: (f64, f64) = (61.8355734, 10.8871645);

fn data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/espa-dombas-e2e")
}

fn bbox_for(pts: &[(f64, f64)], pad: f64) -> [f64; 4] {
    let mut min_lat = f64::INFINITY;
    let mut min_lon = f64::INFINITY;
    let mut max_lat = f64::NEG_INFINITY;
    let mut max_lon = f64::NEG_INFINITY;
    for &(lat, lon) in pts {
        min_lat = min_lat.min(lat);
        min_lon = min_lon.min(lon);
        max_lat = max_lat.max(lat);
        max_lon = max_lon.max(lon);
    }
    [min_lat - pad, min_lon - pad, max_lat + pad, max_lon + pad]
}

fn load_car_pack(dir: &Path, bbox: [f64; 4]) -> RouteGraph {
    let mut graphs = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .expect("data dir")
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let n = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            n.starts_with("ostlandet-latest.navi-graph-car.") && n.ends_with(".rkyv")
        })
        .collect();
    entries.sort();
    for p in entries {
        if let Ok(g) = load_graph_pack_bbox(&p, RoutingProfile::Car, Some(bbox)) {
            if !g.edges.is_empty() {
                graphs.push(g);
            }
        }
    }
    assert!(!graphs.is_empty(), "no car tiles in {}", dir.display());
    merge_tile_graphs(graphs, RoutingProfile::Car)
}

fn nongood_pct(graph: &RouteGraph, edges: &[usize]) -> f64 {
    let mut total = 0.0;
    let mut nongood = 0.0;
    for &idx in edges {
        let e = &graph.edges[idx];
        total += e.length_m;
        if e.surface_quality != SurfaceQuality::Good {
            nongood += e.length_m;
        }
    }
    100.0 * (nongood / 1000.0) / (total / 1000.0).max(1e-9)
}

fn plan_via(graph: &RouteGraph, pts: &[(f64, f64)]) -> (Vec<usize>, f64) {
    let opts = RouteOptions::default();
    let mut snaps = Vec::new();
    for &(lat, lon) in pts {
        snaps.push(graph.nearest_routable(lat, lon).expect("snap").0);
    }
    let mut edges = Vec::new();
    let mut cost = 0.0;
    for i in 0..snaps.len() - 1 {
        let (path, e, c) = graph
            .shortest_path_with_options(snaps[i], snaps[i + 1], false, &opts)
            .expect("leg");
        assert!(path.len() >= 2);
        cost += c;
        if edges.is_empty() {
            edges = e;
        } else {
            edges.extend(e);
        }
    }
    (edges, cost)
}

#[test]
#[ignore = "needs ostlandet car tiles under target/espa-dombas-e2e"]
fn hamar_via_rendalen_pack_soft_costs_avoid_gravel() {
    let dir = data_dir();
    assert!(dir.is_dir(), "missing {}", dir.display());
    let pts = [HAMAR, VIA, RENDAL];
    let mut graph = load_car_pack(&dir, bbox_for(&pts, 0.35));
    for e in &mut graph.edges {
        e.base_weight = e.length_m;
        if let Some(ref mut eco) = e.eco_weight {
            *eco = e.length_m;
        }
    }
    graph.surface_routing_mode = SurfaceRoutingMode::Car;
    apply_surface_preference(
        &mut graph,
        SurfaceRoutingMode::Car,
        MotorSoftCostProfile::Car,
    );

    let (edges, _cost) = plan_via(&graph, &pts);
    let pct = nongood_pct(&graph, &edges);
    let path_km: f64 = edges.iter().map(|&i| graph.edges[i].length_m).sum::<f64>() / 1000.0;
    let uses_birke = edges.iter().any(|&i| {
        graph.edges[i]
            .name
            .as_deref()
            .is_some_and(|n| n.to_lowercase().contains("birkebeiner"))
    });
    assert!(
        path_km > 148.0 && path_km < 170.0,
        "expected asphalt corridor ~153 km, got {path_km:.1}"
    );
    assert!(
        pct < 5.0,
        "via plan must keep non-good surface under 5% (got {pct:.1}%)"
    );
    assert!(
        !uses_birke,
        "via plan must not use Birkebeinerveien gravel corridor"
    );
}
