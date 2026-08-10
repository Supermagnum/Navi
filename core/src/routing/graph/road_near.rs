//! Spatial index of routing-graph nodes for “near a road” checks, plus
//! nearest-edge street labels for idle GPS.

use geo::{Distance, Haversine, Point};
use osm4routing::NodeId;
use rstar::RTree;

use crate::nav::current_road_label;

use super::builder::{GraphEdge, RouteGraph};

const MAX_ROAD_LINK_M: f64 = 1_000.0;

/// Nearest-road distance queries against a built [`RouteGraph`].
pub struct RoadNodeIndex {
    /// Points stored as `[lon, lat]`.
    tree: RTree<[f64; 2]>,
}

impl RoadNodeIndex {
    pub fn from_graph(graph: &RouteGraph) -> Self {
        let pts: Vec<[f64; 2]> = graph
            .nodes
            .values()
            .map(|n| [n.coord.x, n.coord.y])
            .collect();
        Self {
            tree: RTree::bulk_load(pts),
        }
    }

    /// Index only nodes on a planned route (road / path / trail geometry).
    /// Includes intermediate edge shape points so mid-edge rest stops still count
    /// as “on the corridor”.
    pub fn from_path_nodes(graph: &RouteGraph, path: &[NodeId]) -> Self {
        let mut pts: Vec<[f64; 2]> = Vec::new();
        for w in path.windows(2) {
            let Some(idx) = graph.edge_index(w[0], w[1]) else {
                continue;
            };
            let e = &graph.edges[idx];
            if pts.is_empty() {
                pts.push([e.start_lon, e.start_lat]);
            }
            for &(lon, lat) in &e.shape {
                pts.push([lon, lat]);
            }
            pts.push([e.end_lon, e.end_lat]);
        }
        if pts.is_empty() {
            pts = path
                .iter()
                .filter_map(|id| graph.nodes.get(id))
                .map(|n| [n.coord.x, n.coord.y])
                .collect();
        }
        Self {
            tree: RTree::bulk_load(pts),
        }
    }

    /// Great-circle distance (m) from `(lat, lon)` to the nearest graph node.
    pub fn distance_m(&self, lat: f64, lon: f64) -> f64 {
        let Some(nn) = self.tree.nearest_neighbor(&[lon, lat]) else {
            return f64::INFINITY;
        };
        Haversine::distance(Point::new(lon, lat), Point::new(nn[0], nn[1]))
    }

    /// True when the point is within [`MAX_ROAD_LINK_M`] of the road network.
    pub fn within_road_link(&self, lat: f64, lon: f64) -> bool {
        self.distance_m(lat, lon) <= MAX_ROAD_LINK_M
    }

    pub const MAX_LINK_M: f64 = MAX_ROAD_LINK_M;
}

/// Distance from a point to an edge segment in metres (local ENU projection).
pub fn dist_point_to_segment_m(
    lat: f64,
    lon: f64,
    a_lat: f64,
    a_lon: f64,
    b_lat: f64,
    b_lon: f64,
) -> f64 {
    let a = Point::new(a_lon, a_lat);
    let b = Point::new(b_lon, b_lat);
    let ab = Haversine::distance(a, b);
    if ab < 1.0 {
        return Haversine::distance(Point::new(lon, lat), a);
    }
    let mid_lat = (a_lat + b_lat) / 2.0;
    let m_per_deg_lat = 111_320.0;
    let m_per_deg_lon = 111_320.0 * mid_lat.to_radians().cos();
    let ax = a_lon * m_per_deg_lon;
    let ay = a_lat * m_per_deg_lat;
    let bx = b_lon * m_per_deg_lon;
    let by = b_lat * m_per_deg_lat;
    let px = lon * m_per_deg_lon;
    let py = lat * m_per_deg_lat;
    let abx = bx - ax;
    let aby = by - ay;
    let t = ((px - ax) * abx + (py - ay) * aby) / (abx * abx + aby * aby);
    let t = t.clamp(0.0, 1.0);
    let qx = ax + t * abx;
    let qy = ay + t * aby;
    let dx = px - qx;
    let dy = py - qy;
    (dx * dx + dy * dy).sqrt()
}

fn edge_label(e: &GraphEdge) -> String {
    current_road_label(
        e.name.as_deref(),
        e.road_ref.as_deref(),
        e.highway.as_deref(),
    )
}

fn edge_has_name_or_ref(e: &GraphEdge) -> bool {
    e.name.as_ref().is_some_and(|s| !s.trim().is_empty())
        || e.road_ref.as_ref().is_some_and(|s| !s.trim().is_empty())
}

/// Label for the nearest graph edge within [max_m] of `(lat, lon)`.
///
/// Among edges within 8 m of the closest hit, prefer one with OSM `name`/`ref`
/// over a class-only label (named through-road vs unnamed driveway stub).
pub fn nearest_road_label(graph: &RouteGraph, lat: f64, lon: f64, max_m: f64) -> Option<String> {
    let max_m = max_m.max(1.0);
    let mut best_d = f64::INFINITY;
    let mut best_named: Option<(f64, String)> = None;
    let mut best_any: Option<(f64, String)> = None;
    for e in &graph.edges {
        let d = dist_point_to_segment_m(lat, lon, e.start_lat, e.start_lon, e.end_lat, e.end_lon);
        if d > max_m {
            continue;
        }
        let label = edge_label(e);
        if d < best_d {
            best_d = d;
            best_any = Some((d, label.clone()));
        }
        if edge_has_name_or_ref(e) {
            match &best_named {
                None => best_named = Some((d, label)),
                Some((bd, _)) if d < *bd => best_named = Some((d, label)),
                _ => {}
            }
        }
    }
    match (best_named, best_any) {
        (Some((nd, nlabel)), Some((ad, _))) if nd <= ad + 8.0 => Some(nlabel),
        (_, Some((_, alabel))) => Some(alabel),
        (Some((_, nlabel)), None) => Some(nlabel),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::graph::{RouteGraph, RoutingProfile};
    use geo_types::Coord;
    use osm4routing::Node;
    use std::collections::HashMap;

    fn edge(
        id: &str,
        s: i64,
        t: i64,
        lat0: f64,
        lon0: f64,
        lat1: f64,
        lon1: f64,
        highway: &str,
        name: Option<&str>,
    ) -> GraphEdge {
        GraphEdge {
            id: id.into(),
            source: NodeId(s),
            target: NodeId(t),
            length_m: 100.0,
            base_weight: 100.0,
            eco_weight: None,
            start_lat: lat0,
            start_lon: lon0,
            end_lat: lat1,
            end_lon: lon1,
            shape: Vec::new(),
            highway: Some(highway.into()),
            maxspeed_kmh: None,
            name: name.map(|s| s.into()),
            road_ref: None,
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
        }
    }

    #[test]
    fn prefers_named_through_road_over_closer_unnamed_stub() {
        let mut nodes = HashMap::new();
        nodes.insert(
            NodeId(1),
            Node {
                id: NodeId(1),
                coord: Coord {
                    x: 9.9270,
                    y: 61.4197,
                },
                uses: 0,
            },
        );
        nodes.insert(
            NodeId(2),
            Node {
                id: NodeId(2),
                coord: Coord {
                    x: 9.9280,
                    y: 61.4197,
                },
                uses: 0,
            },
        );
        nodes.insert(
            NodeId(3),
            Node {
                id: NodeId(3),
                coord: Coord {
                    x: 9.9276,
                    y: 61.4199,
                },
                uses: 0,
            },
        );
        // Through road ~20 m south of fix; short unnamed stub almost under the fix.
        let edges = vec![
            edge(
                "main",
                1,
                2,
                61.4197,
                9.9270,
                61.4197,
                9.9280,
                "secondary",
                Some("Peer Gyntvegen"),
            ),
            edge(
                "stub", 1, 3, 61.4197, 9.9276, 61.4199, 9.9276, "service", None,
            ),
        ];
        let graph = RouteGraph::from_parts(nodes, edges, RoutingProfile::Car);
        let label = nearest_road_label(&graph, 61.419774, 9.927647, 80.0);
        assert_eq!(label.as_deref(), Some("Peer Gyntvegen"));
    }

    #[test]
    fn peer_gyntvegen_from_ostlandet_bbox() {
        let pbf = std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/target/integration-fixtures/ostlandet-latest.osm.pbf"
        ));
        if !pbf.is_file() {
            eprintln!("skip: missing {pbf:?}");
            return;
        }
        let bbox = [61.40, 9.90, 61.44, 9.96];
        let graph =
            RouteGraph::build_from_pbf_bbox(pbf, RoutingProfile::Car, bbox).expect("bbox graph");
        let label = nearest_road_label(&graph, 61.419774, 9.927647, 80.0);
        assert_eq!(
            label.as_deref(),
            Some("Peer Gyntvegen"),
            "got {label:?}; edges={}",
            graph.edges.len()
        );
    }
}
