//! Spatial index of routing-graph nodes for “near a road” checks.

use geo::{Distance, Haversine, Point};
use osm4routing::NodeId;
use rstar::RTree;

use super::builder::RouteGraph;

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
    pub fn from_path_nodes(graph: &RouteGraph, path: &[NodeId]) -> Self {
        let pts: Vec<[f64; 2]> = path
            .iter()
            .filter_map(|id| graph.nodes.get(id))
            .map(|n| [n.coord.x, n.coord.y])
            .collect();
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
