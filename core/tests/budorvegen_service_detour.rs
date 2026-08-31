//! Regression: Budorvegen (way 1037045908) must not show a service-road detour
//! onto parallel way 332640378 at the farm junction near 60.884 N, 11.314 E.
//!
//! Fixture: `tests/fixtures/budorvegen-service-detour.osm.pbf` (~cut from Ostlandet).

use driver_break_core::routing::graph::{
    apply_surface_preference, apply_surface_quality_from_pbf, way_id_from_edge_id, RouteGraph,
    RouteOptions, RoutingProfile, SurfaceRoutingMode,
};
use std::path::PathBuf;

const SECONDARY_WAY: &str = "1037045908";
const SERVICE_WAY: &str = "332640378";
const JUNCTION_A: i64 = 3397900348;
const JUNCTION_B: i64 = 3397900317;

fn fixture_pbf() -> PathBuf {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/budorvegen-service-detour.osm.pbf");
    assert!(
        p.is_file(),
        "missing checked-in fixture {} — regenerate with scripts/cut-corridor-extract.py",
        p.display()
    );
    p
}

fn bbox() -> [f64; 4] {
    [60.878, 11.30, 60.890, 11.32]
}

fn build_car_graph(pbf: &std::path::Path) -> RouteGraph {
    let mut graph =
        RouteGraph::build_from_pbf_bbox(pbf, RoutingProfile::Car, bbox()).expect("car graph");
    graph.surface_routing_mode = SurfaceRoutingMode::Car;
    let _ = apply_surface_quality_from_pbf(&mut graph, pbf);
    apply_surface_preference(&mut graph, SurfaceRoutingMode::Car);
    graph
}

#[test]
fn budorvegen_path_geometry_uses_secondary_not_service_parallel() {
    let pbf = fixture_pbf();
    let graph = build_car_graph(&pbf);
    let opts = RouteOptions::default();

    let start = (60.88416, 11.3125);
    let end = (60.88360, 11.3155);
    let (s, _) = graph
        .nearest_routable(start.0, start.1)
        .expect("snap start");
    let (g, _) = graph.nearest_routable(end.0, end.1).expect("snap end");

    let (path, path_edges, cost) = graph
        .shortest_path_with_options(s, g, false, &opts)
        .expect("route must exist");
    assert!(
        (cost - 64.6).abs() < 1.0,
        "A* must use 64.6 m secondary chord, got cost {cost}"
    );
    assert_eq!(
        path_edges.len(),
        path.len().saturating_sub(1),
        "recorded edge count must match node path"
    );

    for &idx in &path_edges {
        let e = &graph.edges[idx];
        let wid = way_id_from_edge_id(&e.id).expect("way id");
        assert_ne!(
            wid, 332640378,
            "recorded edge {} must not be service way at parallel junction",
            e.id
        );
        if e.source.0 == JUNCTION_A && e.target.0 == JUNCTION_B
            || e.source.0 == JUNCTION_B && e.target.0 == JUNCTION_A
        {
            assert!(
                e.id.contains(SECONDARY_WAY),
                "parallel junction must use Budorvegen ({SECONDARY_WAY}), got {}",
                e.id
            );
            assert_eq!(e.highway.as_deref(), Some("secondary"));
        }
    }

    let coords = graph.path_coords_lat_lon_from_edges(&path_edges);
    assert!(
        coords.len() <= 4,
        "secondary chord geometry should be short; service loop has many vertices (got {})",
        coords.len()
    );

    let poly = graph.path_overlay_polyline_from_edges(&path_edges);
    assert!(
        !poly.contains("11.3153482"),
        "service-road shape must not appear in overlay polyline"
    );
}

#[test]
fn budorvegen_longitudinal_route_stays_on_secondary() {
    let pbf = fixture_pbf();
    let graph = build_car_graph(&pbf);
    let opts = RouteOptions::default();

    let start = (60.88559, 11.31469);
    let end = (60.88138, 11.31211);
    let (s, _) = graph
        .nearest_routable(start.0, start.1)
        .expect("snap start");
    let (g, _) = graph.nearest_routable(end.0, end.1).expect("snap end");

    let (_path, path_edges, _) = graph
        .shortest_path_with_options(s, g, false, &opts)
        .expect("longitudinal route");

    for &idx in &path_edges {
        let e = &graph.edges[idx];
        assert!(
            !e.id.contains(SERVICE_WAY),
            "longitudinal Budorvegen route must not use service way {SERVICE_WAY}, took {}",
            e.id
        );
    }
}
