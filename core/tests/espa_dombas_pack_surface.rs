//! Espa → Dombås: indexed-pack surface preference must match PBF classification.
//!
//! Builds a car bbox graph, round-trips through FlatGraphPack (device pack path),
//! applies plan-time soft costs without PBF refine, and checks non-good km.

use driver_break_core::routing::graph::{
    apply_surface_preference, MotorSoftCostProfile, RouteGraph, RouteOptions, RoutingProfile,
    SurfaceQuality, SurfaceRoutingMode,
};
use driver_break_core::routing::indexed::FlatGraphPack;
use std::path::PathBuf;
use std::time::Instant;

const ESPA: (f64, f64) = (60.5621914, 11.2561239);
const DOMBAS: (f64, f64) = (62.0756, 9.1278);

fn pbf() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/integration-fixtures/norway-latest.osm.pbf")
}

fn bbox() -> [f64; 4] {
    let pad = 0.35;
    [
        ESPA.0.min(DOMBAS.0) - pad,
        ESPA.1.min(DOMBAS.1) - pad,
        ESPA.0.max(DOMBAS.0) + pad,
        ESPA.1.max(DOMBAS.1) + pad,
    ]
}

fn nongood_km(graph: &RouteGraph, edges: &[usize]) -> (f64, f64) {
    let mut total = 0.0;
    let mut nongood = 0.0;
    for &idx in edges {
        let e = &graph.edges[idx];
        total += e.length_m;
        if e.surface_quality != SurfaceQuality::Good {
            nongood += e.length_m;
        }
    }
    (total / 1000.0, nongood / 1000.0)
}

#[test]
#[ignore = "needs norway-latest.osm.pbf under target/integration-fixtures"]
fn espa_dombas_pack_roundtrip_avoids_gravel() {
    let pbf = pbf();
    assert!(pbf.is_file(), "missing {}", pbf.display());

    let built =
        RouteGraph::build_from_pbf_bbox(&pbf, RoutingProfile::Car, bbox()).expect("bbox build");

    // Indexed-pack path: serialize classified surfaces, restore without PBF refine.
    let pack = FlatGraphPack::from_route_graph(&built, None);
    assert_eq!(
        pack.edge_surface_quality.len(),
        pack.edge_src.len(),
        "v8 packs must store surface_quality per edge"
    );
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&pack).expect("serialize");
    let archived = rkyv::access::<
        driver_break_core::routing::indexed::ArchivedFlatGraphPack,
        rkyv::rancor::Error,
    >(&bytes[..])
    .expect("access");
    let restored: FlatGraphPack =
        rkyv::deserialize::<FlatGraphPack, rkyv::rancor::Error>(archived).expect("deserialize");

    let mut graph = restored.to_route_graph(RoutingProfile::Car);
    // Confirm gravel survived pack (not collapsed to Good via highway-only infer).
    let marginal = graph
        .edges
        .iter()
        .filter(|e| e.surface_quality == SurfaceQuality::Marginal)
        .count();
    assert!(
        marginal > 1000,
        "pack must retain marginal/gravel classes, got {marginal}"
    );

    graph.surface_routing_mode = SurfaceRoutingMode::Car;
    apply_surface_preference(
        &mut graph,
        SurfaceRoutingMode::Car,
        MotorSoftCostProfile::Car,
    );

    let (s, _) = graph.nearest_routable(ESPA.0, ESPA.1).expect("snap Espa");
    let (g, _) = graph
        .nearest_routable(DOMBAS.0, DOMBAS.1)
        .expect("snap Dombås");

    let t0 = Instant::now();
    let stats = graph.shortest_path_with_options_stats(s, g, false, &RouteOptions::default());
    let astar_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let (_path, edges, cost) = stats.path.expect("route");
    let (path_km, nongood_km) = nongood_km(&graph, &edges);
    let nongood_pct = 100.0 * nongood_km / path_km.max(1e-9);

    eprintln!(
        "espa_dombas_pack: path_km={path_km:.1} nongood_km={nongood_km:.2} ({nongood_pct:.2}%) \
         expansions={} astar_ms={astar_ms:.0} cost={cost:.0}",
        stats.expansions
    );

    assert!(
        nongood_pct < 1.0,
        "pack path non-good surface must be ~0% (was 13.7% before fix); got {nongood_pct:.2}%"
    );
    // Before: ~610k expansions with 0.1×haversine. Tight heuristic roughly halves that.
    assert!(
        stats.expansions < 400_000,
        "tight heuristic should cut expansions vs ~610k baseline; got {}",
        stats.expansions
    );
}

#[test]
#[ignore = "needs norway-latest.osm.pbf under target/integration-fixtures"]
fn espa_dombas_mobile_home_penalizes_gravel_harder_than_car() {
    let pbf = pbf();
    assert!(pbf.is_file(), "missing {}", pbf.display());

    let built =
        RouteGraph::build_from_pbf_bbox(&pbf, RoutingProfile::Truck, bbox()).expect("bbox build");
    let pack = FlatGraphPack::from_route_graph(&built, None);

    let mut car_g = pack.to_route_graph(RoutingProfile::Truck);
    let mut mh_g = pack.to_route_graph(RoutingProfile::Truck);
    car_g.surface_routing_mode = SurfaceRoutingMode::Car;
    mh_g.surface_routing_mode = SurfaceRoutingMode::Car;
    apply_surface_preference(
        &mut car_g,
        SurfaceRoutingMode::Car,
        MotorSoftCostProfile::Car,
    );
    apply_surface_preference(
        &mut mh_g,
        SurfaceRoutingMode::Car,
        MotorSoftCostProfile::MobileHome,
    );

    // Spot-check: same gravel edge gets a higher weight under mobile home.
    let mut found = false;
    for (c, m) in car_g.edges.iter().zip(mh_g.edges.iter()) {
        if c.surface_quality == SurfaceQuality::Marginal && c.length_m > 50.0 {
            assert!(
                m.base_weight > c.base_weight + 1.0,
                "mobile home marginal weight ({}) must exceed car ({})",
                m.base_weight,
                c.base_weight
            );
            found = true;
            break;
        }
    }
    assert!(found, "expected at least one marginal edge in the corridor");
}
