//! Synthetic toll-policy resilience (no real-world place names).
//!
//! Covers Penalize detour preference, NeverUse hard filter + snap, genuine
//! disconnect, and bbox-clip / widen recovery on hand-built graphs.

use std::collections::HashMap;

use driver_break_core::routing::graph::{
    GraphEdge, RouteGraph, RouteOptions, RoutingProfile, SurfaceQuality,
};
use driver_break_core::routing::plan_bbox::{plan_bbox_pad_schedule, trip_bbox};
use driver_break_core::routing::toll::TollPolicy;
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

fn edge(
    id: &str,
    source: i64,
    target: i64,
    start_lat: f64,
    start_lon: f64,
    end_lat: f64,
    end_lon: f64,
    length_m: f64,
    highway: &str,
) -> GraphEdge {
    GraphEdge {
        id: id.into(),
        source: NodeId(source),
        target: NodeId(target),
        length_m,
        base_weight: length_m,
        eco_weight: Some(length_m),
        start_lat,
        start_lon,
        end_lat,
        end_lon,
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
        surface_quality: SurfaceQuality::Good,
    }
}

fn diamond_with_toll_short() -> RouteGraph {
    let mut nodes = HashMap::new();
    for (id, n) in [
        node(1, 60.0, 10.0),
        node(2, 60.0, 10.01),
        node(3, 60.0, 10.02),
        node(4, 60.05, 10.01),
    ] {
        nodes.insert(id, n);
    }
    let mut ab = edge("ab", 1, 2, 60.0, 10.0, 60.0, 10.01, 100.0, "primary");
    ab.is_toll = true;
    let mut bc = edge("bc", 2, 3, 60.0, 10.01, 60.0, 10.02, 100.0, "primary");
    bc.is_toll = true;
    let ad = edge("ad", 1, 4, 60.0, 10.0, 60.05, 10.01, 800.0, "secondary");
    let dc = edge("dc", 4, 3, 60.05, 10.01, 60.0, 10.02, 800.0, "secondary");
    RouteGraph::from_parts(nodes, vec![ab, bc, ad, dc], RoutingProfile::Car)
}

#[test]
fn penalize_picks_free_detour_over_short_toll() {
    let graph = diamond_with_toll_short();
    let allow = graph
        .shortest_path_with_options(NodeId(1), NodeId(3), false, &RouteOptions::default())
        .expect("allow path");
    assert!(
        allow.0.contains(&NodeId(2)),
        "default should use short toll corridor: {:?}",
        allow.0
    );

    let penalize = graph
        .shortest_path_with_options(
            NodeId(1),
            NodeId(3),
            false,
            &RouteOptions {
                toll_policy: TollPolicy::Penalize,
                ..Default::default()
            },
        )
        .expect("penalize path");
    assert!(
        !penalize.0.contains(&NodeId(2)),
        "penalize must prefer free detour: {:?}",
        penalize.0
    );
    assert!(penalize.0.contains(&NodeId(4)));
}

#[test]
fn never_use_disconnected_returns_none() {
    let mut nodes = HashMap::new();
    for (id, n) in [
        node(1, 60.0, 10.0),
        node(2, 60.0, 10.01),
        node(3, 60.0, 10.02),
    ] {
        nodes.insert(id, n);
    }
    let mut ab = edge("ab", 1, 2, 60.0, 10.0, 60.0, 10.01, 100.0, "primary");
    ab.is_toll = true;
    let mut bc = edge("bc", 2, 3, 60.0, 10.01, 60.0, 10.02, 100.0, "primary");
    bc.is_toll = true;
    let graph = RouteGraph::from_parts(nodes, vec![ab, bc], RoutingProfile::Car);
    let stats = graph.shortest_path_with_options_stats(
        NodeId(1),
        NodeId(3),
        false,
        &RouteOptions {
            toll_policy: TollPolicy::NeverUse,
            ..Default::default()
        },
    );
    assert!(stats.path.is_none());
    assert_eq!(stats.terminate_reason, "disconnected");
}

#[test]
fn snap_under_never_use_skips_toll_only_island() {
    // Toll island is the larger component under Allow (3 nodes) so snap prefers
    // it over the free 2-node stub. Under NeverUse the toll edges vanish and
    // the free stub becomes the only filtered giant.
    let mut nodes = HashMap::new();
    for (id, n) in [
        node(1, 60.0, 10.0),
        node(2, 60.0, 10.02),
        node(9, 60.0001, 10.0001),
        node(8, 60.0002, 10.0002),
        node(7, 60.0003, 10.0003),
    ] {
        nodes.insert(id, n);
    }
    let free = edge("12", 1, 2, 60.0, 10.0, 60.0, 10.02, 200.0, "primary");
    let free_r = edge("21", 2, 1, 60.0, 10.02, 60.0, 10.0, 200.0, "primary");
    let mut t98 = edge(
        "98", 9, 8, 60.0001, 10.0001, 60.0002, 10.0002, 50.0, "primary",
    );
    t98.is_toll = true;
    let mut t89 = edge(
        "89", 8, 9, 60.0002, 10.0002, 60.0001, 10.0001, 50.0, "primary",
    );
    t89.is_toll = true;
    let mut t87 = edge(
        "87", 8, 7, 60.0002, 10.0002, 60.0003, 10.0003, 50.0, "primary",
    );
    t87.is_toll = true;
    let mut t78 = edge(
        "78", 7, 8, 60.0003, 10.0003, 60.0002, 10.0002, 50.0, "primary",
    );
    t78.is_toll = true;
    let graph = RouteGraph::from_parts(
        nodes,
        vec![free, free_r, t98, t89, t87, t78],
        RoutingProfile::Car,
    );

    let query_lat = 60.00015;
    let query_lon = 10.00015;
    let (snap_allow, _) = graph
        .nearest_routable_with_options(query_lat, query_lon, &RouteOptions::default())
        .expect("allow snap");
    assert!(
        matches!(snap_allow, NodeId(7) | NodeId(8) | NodeId(9)),
        "Allow must prefer the larger toll island giant: got {snap_allow:?}"
    );

    let never = RouteOptions {
        toll_policy: TollPolicy::NeverUse,
        ..Default::default()
    };
    let (snap_never, _) = graph
        .nearest_routable_with_options(query_lat, query_lon, &never)
        .expect("never-use snap");
    assert!(
        matches!(snap_never, NodeId(1) | NodeId(2)),
        "NeverUse must snap onto the free stub, not the toll island: got {snap_never:?}"
    );
}

#[test]
fn bbox_clip_then_widen_recovers_free_detour() {
    let mut nodes = HashMap::new();
    for (id, n) in [
        node(1, 60.0, 10.0),
        node(2, 60.0, 10.01),
        node(3, 60.0, 10.02),
        node(4, 60.8, 10.01),
    ] {
        nodes.insert(id, n);
    }
    let mut ab = edge("ab", 1, 2, 60.0, 10.0, 60.0, 10.01, 100.0, "primary");
    ab.is_toll = true;
    let mut bc = edge("bc", 2, 3, 60.0, 10.01, 60.0, 10.02, 100.0, "primary");
    bc.is_toll = true;
    let ad = edge("ad", 1, 4, 60.0, 10.0, 60.8, 10.01, 90_000.0, "secondary");
    let dc = edge("dc", 4, 3, 60.8, 10.01, 60.0, 10.02, 90_000.0, "secondary");
    let full = RouteGraph::from_parts(nodes, vec![ab, bc, ad, dc], RoutingProfile::Car);

    let opts = RouteOptions {
        toll_policy: TollPolicy::NeverUse,
        ..Default::default()
    };
    let pads = plan_bbox_pad_schedule(60.0, 10.0, 60.0, 10.02);
    assert!(
        pads[0] < 0.8,
        "initial pad must clip the far detour node: {:?}",
        pads
    );

    let mut recovered = false;
    let mut pad_history = Vec::new();
    for pad in pads {
        pad_history.push(pad);
        let bbox = trip_bbox(60.0, 10.0, 60.0, 10.02, pad);
        let clipped_nodes: HashMap<_, _> = full
            .nodes
            .iter()
            .filter(|(_, n)| {
                n.coord.y >= bbox[0]
                    && n.coord.y <= bbox[2]
                    && n.coord.x >= bbox[1]
                    && n.coord.x <= bbox[3]
            })
            .map(|(k, v)| (*k, *v))
            .collect();
        let clipped_edges: Vec<_> = full
            .edges
            .iter()
            .filter(|e| {
                clipped_nodes.contains_key(&e.source) && clipped_nodes.contains_key(&e.target)
            })
            .cloned()
            .collect();
        if clipped_nodes.len() < 2 {
            continue;
        }
        let g = RouteGraph::from_parts(clipped_nodes, clipped_edges, RoutingProfile::Car);
        if let Some(path) = g.shortest_path_with_options(NodeId(1), NodeId(3), false, &opts) {
            assert!(path.0.contains(&NodeId(4)));
            recovered = true;
            break;
        }
    }
    assert!(
        recovered,
        "widen schedule must eventually include free detour; pads={pad_history:?}"
    );
}

#[test]
fn avoid_tolls_bool_serialization_back_compat() {
    assert_eq!(
        TollPolicy::from_avoid_tolls_bool(true),
        TollPolicy::Penalize
    );
    assert_eq!(TollPolicy::from_avoid_tolls_bool(false), TollPolicy::Allow);
    let legacy: HashMap<String, String> = HashMap::from([("avoid_tolls".into(), "true".into())]);
    assert_eq!(
        TollPolicy::from_summary_json(|k| legacy.get(k).cloned()),
        TollPolicy::Penalize
    );
    let modern: HashMap<String, String> =
        HashMap::from([("toll_policy".into(), "never_use".into())]);
    assert_eq!(
        TollPolicy::from_summary_json(|k| modern.get(k).cloned()),
        TollPolicy::NeverUse
    );
}
