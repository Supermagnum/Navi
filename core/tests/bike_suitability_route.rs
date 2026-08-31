//! Road-bike profile must avoid a short MTB-scaled leg when a longer paved detour exists.

use std::collections::HashMap;

use driver_break_core::routing::graph::{
    apply_bike_suitability, BikeCapability, GraphEdge, RouteGraph, RoutingProfile,
};
use geo_types::Coord;
use osm4routing::{Node, NodeId};

fn node(id: i64, lat: f64, lon: f64) -> (NodeId, Node) {
    let nid = NodeId(id);
    (
        nid,
        Node {
            id: nid,
            coord: Coord { x: lon, y: lat },
            uses: 0,
        },
    )
}

fn edge(id: &str, source: i64, target: i64, length_m: f64, highway: &str) -> GraphEdge {
    GraphEdge {
        id: id.into(),
        source: NodeId(source),
        target: NodeId(target),
        length_m,
        base_weight: length_m,
        eco_weight: Some(length_m),
        start_lat: 60.0,
        start_lon: 10.0,
        end_lat: 60.0,
        end_lon: 10.01,
        shape: Vec::new(),
        highway: Some(highway.into()),
        maxspeed_kmh: None,
        name: None,
        road_ref: None,
        is_motorroad: false,
        is_expressway: false,
        is_oneway: false,
        lanes: None,
        maxweight_t: None,
        maxaxleload_t: None,
        maxbogieweight_t: None,
        maxheight_m: None,
        maxwidth_m: None,
        maxlength_m: None,
        is_toll: false,
        is_ferry: false,
        is_boardwalk_crossing: false,
        is_roundabout: false,
        motor_vehicle_conditional: None,
        access_conditional: None,
        maxspeed_conditional: None,
        access_forbidden: false,
        surface_quality: driver_break_core::routing::graph::SurfaceQuality::Good,
    }
}

#[test]
fn road_bike_avoids_short_mtb_leg() {
    let mut nodes = HashMap::new();
    for (id, lat, lon) in [
        (1, 60.0, 10.0),
        (2, 60.0, 10.01),
        (3, 60.0, 10.02),
        (4, 60.01, 10.01),
    ] {
        nodes.insert(NodeId(id), node(id, lat, lon).1);
    }
    // A(1) --100m path mtb:2--> B(2) --100m path--> C(3)  total 200m
    // A(1) --350m cycleway paved--> D(4) --350m cycleway--> C(3) total 700m
    let ab = edge("100-0", 1, 2, 100.0, "path");
    let bc = edge("101-0", 2, 3, 100.0, "path");
    let ad = edge("200-0", 1, 4, 350.0, "cycleway");
    let dc = edge("201-0", 4, 3, 350.0, "cycleway");
    let graph = RouteGraph::from_parts(nodes, vec![ab, bc, ad, dc], RoutingProfile::Bicycle);

    let default = graph
        .shortest_path(NodeId(1), NodeId(3), false)
        .expect("path");
    assert!(
        default.0.contains(&NodeId(2)),
        "unfiltered should use short path via B: {:?}",
        default.0
    );

    let mut way_tags = HashMap::new();
    way_tags.insert(100, HashMap::from([("mtb:scale".into(), "3".into())]));
    way_tags.insert(101, HashMap::from([("mtb:scale".into(), "3".into())]));
    way_tags.insert(
        200,
        HashMap::from([
            ("surface".into(), "asphalt".into()),
            ("smoothness".into(), "good".into()),
        ]),
    );
    way_tags.insert(
        201,
        HashMap::from([
            ("surface".into(), "asphalt".into()),
            ("smoothness".into(), "good".into()),
        ]),
    );

    let mut filtered = graph;
    let removed = apply_bike_suitability(&mut filtered, &way_tags, BikeCapability::Road);
    assert_eq!(removed, 2, "road profile drops both MTB-scaled legs");

    let detour = filtered
        .shortest_path(NodeId(1), NodeId(3), false)
        .expect("detour path");
    assert!(
        detour.0.contains(&NodeId(4)) && !detour.0.contains(&NodeId(2)),
        "road bike must take paved detour: {:?}",
        detour.0
    );
}

#[test]
fn mountain_bike_may_use_mtb_leg() {
    let mut nodes = HashMap::new();
    for (id, lat, lon) in [(1, 60.0, 10.0), (2, 60.0, 10.01), (3, 60.0, 10.02)] {
        nodes.insert(NodeId(id), node(id, lat, lon).1);
    }
    let ab = edge("100-0", 1, 2, 100.0, "path");
    let bc = edge("101-0", 2, 3, 100.0, "path");
    let graph = RouteGraph::from_parts(nodes, vec![ab, bc], RoutingProfile::Bicycle);

    let mut way_tags = HashMap::new();
    way_tags.insert(100, HashMap::from([("mtb:scale".into(), "2".into())]));
    way_tags.insert(101, HashMap::from([("mtb:scale".into(), "2".into())]));

    let mut filtered = graph;
    let removed = apply_bike_suitability(&mut filtered, &way_tags, BikeCapability::Mountain);
    assert_eq!(removed, 0);
    assert!(filtered
        .shortest_path(NodeId(1), NodeId(3), false)
        .is_some());
}
