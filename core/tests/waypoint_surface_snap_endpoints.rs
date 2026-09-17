//! Endpoint surface-snap regressions for rural addresses.
//!
//! Before the prefer_better_surface flag + SURFACE_VIA_SNAP_SLACK_M change,
//! Car-mode destination snaps preferred the best surface anywhere in the full
//! snap budget (750 m), so gravel driveways lost to paved through-roads
//! ~125–260 m away. These fixtures reproduce Espedalsvegen 656 and
//! Søndre Grøtting (Rendalen) geometry with a connected gravel stub near the
//! query and a paved road several hundred metres away.

use driver_break_core::routing::graph::{
    RouteGraph, RouteOptions, RoutingProfile, SurfaceQuality, SurfaceRoutingMode,
};
use geo_types::Coord;
use osm4routing::{Node, NodeId};
use std::collections::HashMap;

/// Espedalsvegen 656 (real reported destination).
const ESPEDALSVEGEN: (f64, f64) = (61.3636391, 9.6735332);
/// Old incorrect paved snap distance observed in the field.
const ESPEDALSVEGEN_OLD_PAVED_SNAP_M: f64 = 125.0;

/// Søndre Grøtting, Rendalen (real reported destination).
const SONDRE_GROTTING: (f64, f64) = (61.865580, 10.898674);
/// Old incorrect paved snap distance observed in the field.
const SONDRE_GROTTING_OLD_PAVED_SNAP_M: f64 = 260.0;

const ENDPOINT_SNAP_TOLERANCE_M: f64 = 50.0;

fn meters_north(lat: f64, meters: f64) -> f64 {
    lat + (meters / 111_320.0)
}

fn meters_east(lat: f64, lon: f64, meters: f64) -> f64 {
    let m_per_deg = 111_320.0 * lat.to_radians().cos();
    lon + (meters / m_per_deg)
}

fn node(id: i64, lat: f64, lon: f64) -> (NodeId, Node) {
    let nid = NodeId(id);
    (
        nid,
        Node {
            id: nid,
            coord: Coord { x: lon, y: lat },
            uses: 2,
        },
    )
}

fn edge(
    source: i64,
    target: i64,
    slat: f64,
    slon: f64,
    elat: f64,
    elon: f64,
    highway: &str,
    surface_quality: SurfaceQuality,
    length_m: f64,
) -> driver_break_core::routing::graph::GraphEdge {
    driver_break_core::routing::graph::GraphEdge {
        id: format!("{source}-{target}"),
        source: NodeId(source),
        target: NodeId(target),
        length_m,
        base_weight: length_m,
        eco_weight: None,
        start_lat: slat,
        start_lon: slon,
        end_lat: elat,
        end_lon: elon,
        shape: Vec::new(),
        highway: Some(highway.into()),
        maxspeed_kmh: None,
        maxspeed_practical_kmh: None,
        maxspeed_advisory_kmh: None,
        maxspeed_type: None,
        maxspeed_variable: false,
        minspeed_kmh: None,
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
        is_tunnel: false,
        is_boardwalk_crossing: false,
        is_roundabout: false,
        motor_vehicle_conditional: None,
        access_conditional: None,
        maxspeed_conditional: None,
        access_forbidden: false,
        surface_quality,
    }
}

/// Connected component: gravel driveway near the query + paved through-road
/// at `paved_offset_m` (matching the old wrong snap distance).
fn rural_driveway_graph(query: (f64, f64), gravel_m: f64, paved_m: f64) -> RouteGraph {
    let (qlat, qlon) = query;
    let gravel_lat = meters_north(qlat, gravel_m);
    let gravel_lon = qlon;
    let gravel_b_lon = meters_east(gravel_lat, gravel_lon, 40.0);
    let paved_lat = meters_north(qlat, paved_m);
    let paved_lon = meters_east(qlat, qlon, 30.0);
    let paved_b_lon = meters_east(paved_lat, paved_lon, 80.0);

    let mut nodes = HashMap::new();
    for (id, n) in [
        node(1, gravel_lat, gravel_lon),
        node(2, gravel_lat, gravel_b_lon),
        node(10, paved_lat, paved_lon),
        node(11, paved_lat, paved_b_lon),
        node(12, meters_north(paved_lat, 40.0), paved_b_lon),
    ] {
        nodes.insert(id, n);
    }

    let edges = vec![
        edge(
            1,
            2,
            gravel_lat,
            gravel_lon,
            gravel_lat,
            gravel_b_lon,
            "track",
            SurfaceQuality::Poor,
            40.0,
        ),
        edge(
            2,
            1,
            gravel_lat,
            gravel_b_lon,
            gravel_lat,
            gravel_lon,
            "track",
            SurfaceQuality::Poor,
            40.0,
        ),
        // Connect driveway to paved network (same giant component).
        edge(
            2,
            10,
            gravel_lat,
            gravel_b_lon,
            paved_lat,
            paved_lon,
            "unclassified",
            SurfaceQuality::Poor,
            (paved_m - gravel_m).abs().max(50.0),
        ),
        edge(
            10,
            2,
            paved_lat,
            paved_lon,
            gravel_lat,
            gravel_b_lon,
            "unclassified",
            SurfaceQuality::Poor,
            (paved_m - gravel_m).abs().max(50.0),
        ),
        edge(
            10,
            11,
            paved_lat,
            paved_lon,
            paved_lat,
            paved_b_lon,
            "secondary",
            SurfaceQuality::Good,
            80.0,
        ),
        edge(
            11,
            10,
            paved_lat,
            paved_b_lon,
            paved_lat,
            paved_lon,
            "secondary",
            SurfaceQuality::Good,
            80.0,
        ),
        edge(
            11,
            12,
            paved_lat,
            paved_b_lon,
            meters_north(paved_lat, 40.0),
            paved_b_lon,
            "secondary",
            SurfaceQuality::Good,
            40.0,
        ),
        edge(
            12,
            11,
            meters_north(paved_lat, 40.0),
            paved_b_lon,
            paved_lat,
            paved_b_lon,
            "secondary",
            SurfaceQuality::Good,
            40.0,
        ),
    ];

    let mut graph = RouteGraph::from_parts(nodes, edges, RoutingProfile::Car);
    graph.surface_routing_mode = SurfaceRoutingMode::Car;
    graph
}

fn assert_destination_keeps_driveway(query: (f64, f64), gravel_m: f64, paved_m: f64, label: &str) {
    let graph = rural_driveway_graph(query, gravel_m, paved_m);
    let opts = RouteOptions::default();

    // Destination / start: must not apply surface preference.
    let (id, dist) = graph
        .nearest_routable_with_options(query.0, query.1, &opts, false)
        .unwrap_or_else(|e| panic!("{label}: endpoint snap failed: {e:?}"));
    assert_eq!(
        id,
        NodeId(1),
        "{label}: destination must snap to gravel driveway node, not paved (got {id:?})"
    );
    assert!(
        dist <= ENDPOINT_SNAP_TOLERANCE_M,
        "{label}: destination snap {dist:.1} m exceeds {ENDPOINT_SNAP_TOLERANCE_M} m tolerance \
         (old paved snap was ~{paved_m} m)"
    );
    assert!(
        dist < paved_m * 0.5,
        "{label}: snap {dist:.1} m is not clearly nearer than old paved offset {paved_m} m"
    );

    // Convenience wrapper must match endpoint semantics.
    let (wrap_id, wrap_dist) = graph
        .nearest_routable(query.0, query.1)
        .expect("nearest_routable wrapper");
    assert_eq!(wrap_id, id);
    assert!((wrap_dist - dist).abs() < 1e-6);

    // Counterfactual: full-budget surface preference (old bug) would pick paved.
    // Emulate that by scanning giant candidates within car budget and scoring surface.
    let (_paved_id, paved_dist) = graph
        .nearest_routable_with_options(query.0, query.1, &opts, true)
        .expect("via-style snap");
    // For Espedalsvegen (~125 m), paved is inside the 150 m via slack of a ~20 m
    // nearest node, so via may still prefer paved — that is intentional.
    // For Søndre Grøtting (~260 m), paved is outside slack, so via stays on gravel.
    if paved_m > 150.0 + gravel_m + 5.0 {
        assert_eq!(
            _paved_id,
            NodeId(1),
            "{label}: via surface slack must not reach paved at ~{paved_m} m"
        );
    } else {
        assert!(
            paved_dist > dist,
            "{label}: when via prefers paved inside slack, that snap ({paved_dist:.1} m) \
             must be farther than the endpoint driveway snap ({dist:.1} m)"
        );
    }
}

#[test]
fn espedalsvegen_656_destination_snaps_to_driveway_not_paved() {
    // Gravel access ~20 m from the house pin; paved road ~125 m away (old bug).
    assert_destination_keeps_driveway(
        ESPEDALSVEGEN,
        20.0,
        ESPEDALSVEGEN_OLD_PAVED_SNAP_M,
        "Espedalsvegen 656",
    );
}

#[test]
fn sondre_grotting_rendalen_destination_snaps_to_driveway_not_paved() {
    // Gravel/farm track ~25 m from the pin; paved road ~260 m away (old bug).
    assert_destination_keeps_driveway(
        SONDRE_GROTTING,
        25.0,
        SONDRE_GROTTING_OLD_PAVED_SNAP_M,
        "Søndre Grøtting",
    );
}

#[test]
fn multi_stop_plan_disables_surface_on_start_and_destination_only() {
    // Mimic navi-ffi stop loop: start → via → end.
    let start = ESPEDALSVEGEN;
    let via = (
        meters_north(start.0, 80.0),
        meters_east(start.0, start.1, 40.0),
    );
    let end = SONDRE_GROTTING;

    let g_start = rural_driveway_graph(start, 20.0, 125.0);
    let g_end = rural_driveway_graph(end, 25.0, 260.0);
    let opts = RouteOptions::default();

    let points = [start, via, end];
    let graphs = [&g_start, &g_start, &g_end];
    for (i, (&(lat, lon), graph)) in points.iter().zip(graphs.iter()).enumerate() {
        let prefer_better_surface = i > 0 && i + 1 < points.len();
        let (id, dist) = graph
            .nearest_routable_with_options(lat, lon, &opts, prefer_better_surface)
            .expect("snap");
        if i == 0 || i + 1 == points.len() {
            assert!(
                !prefer_better_surface,
                "start/end must not request surface preference"
            );
            assert_eq!(id, NodeId(1), "stop {i} must keep driveway");
            assert!(dist <= ENDPOINT_SNAP_TOLERANCE_M, "stop {i} dist={dist}");
        } else {
            assert!(prefer_better_surface, "via must request surface preference");
        }
    }
}
