use std::collections::HashMap;
use std::path::Path;

use osm4routing::{
    CarAccessibility, Edge, FootAccessibility, BikeAccessibility, Node, NodeId, Reader,
};
use pathfinding::directed::astar::astar;
use serde::{Deserialize, Serialize};

use crate::config::Profile;
use crate::routing::elevation::ElevationService;

/// Routing profile derived from travel mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RoutingProfile {
    Car,
    Truck,
    Foot,
    Bicycle,
}

impl From<Profile> for RoutingProfile {
    fn from(value: Profile) -> Self {
        match value {
            Profile::Car | Profile::CarElectric | Profile::Motorcycle | Profile::MotorcycleElectric => {
                Self::Car
            }
            Profile::Truck | Profile::TruckElectric => Self::Truck,
            Profile::Hiking => Self::Foot,
            Profile::Cycling => Self::Bicycle,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GraphEdge {
    pub id: String,
    pub source: NodeId,
    pub target: NodeId,
    pub length_m: f64,
    pub base_weight: f64,
    pub eco_weight: Option<f64>,
    pub start_lat: f64,
    pub start_lon: f64,
    pub end_lat: f64,
    pub end_lon: f64,
    pub highway: Option<String>,
    pub maxweight_t: Option<f64>,
    pub maxaxleload_t: Option<f64>,
    pub maxheight_m: Option<f64>,
    pub maxwidth_m: Option<f64>,
}

/// Per-query routing filters (sightseeing avoid-majors, vehicle physical limits).
#[derive(Debug, Clone, Default)]
pub struct RouteOptions {
    pub avoid_major_roads: bool,
    pub vehicle: Option<crate::config::VehicleLimits>,
}

pub struct RouteGraph {
    pub nodes: HashMap<NodeId, Node>,
    pub edges: Vec<GraphEdge>,
    adjacency: HashMap<NodeId, Vec<usize>>,
    profile: RoutingProfile,
}

impl RouteGraph {
    pub fn build_from_pbf(path: impl AsRef<Path>, profile: RoutingProfile) -> anyhow::Result<Self> {
        let (nodes, edges) = Reader::new()
            .read_tag("highway")
            .read_tag("maxweight")
            .read_tag("maxaxleload")
            .read_tag("maxheight")
            .read_tag("maxwidth")
            .read(path.as_ref())
            .map_err(|e| anyhow::anyhow!("osm4routing: {e}"))?;
        let filtered = filter_edges(edges, profile);
        let mut graph = Self {
            nodes: nodes.into_iter().map(|n| (n.id, n)).collect(),
            edges: Vec::new(),
            adjacency: HashMap::new(),
            profile,
        };
        for edge in filtered {
            let start = graph.nodes.get(&edge.source).ok_or_else(|| {
                anyhow::anyhow!("missing source node {}", edge.source.0)
            })?;
            let end = graph.nodes.get(&edge.target).ok_or_else(|| {
                anyhow::anyhow!("missing target node {}", edge.target.0)
            })?;
            let start_lat = start.coord.y;
            let start_lon = start.coord.x;
            let end_lat = end.coord.y;
            let end_lon = end.coord.x;
            let length_m = edge.length();
            let meta = edge_meta(&edge);
            let (forward_ok, backward_ok) = directed_access(&edge, profile);
            if forward_ok {
                push_directed_edge(
                    &mut graph,
                    edge.id.clone(),
                    edge.source,
                    edge.target,
                    start_lat,
                    start_lon,
                    end_lat,
                    end_lon,
                    length_m,
                    &meta,
                );
            }
            if backward_ok {
                push_directed_edge(
                    &mut graph,
                    format!("{}-rev", edge.id),
                    edge.target,
                    edge.source,
                    end_lat,
                    end_lon,
                    start_lat,
                    start_lon,
                    length_m,
                    &meta,
                );
            }
        }
        Ok(graph)
    }

    pub fn profile(&self) -> RoutingProfile {
        self.profile
    }

    pub(crate) fn from_parts(
        nodes: HashMap<NodeId, Node>,
        edges: Vec<GraphEdge>,
        profile: RoutingProfile,
    ) -> Self {
        let mut adjacency: HashMap<NodeId, Vec<usize>> = HashMap::new();
        for (idx, edge) in edges.iter().enumerate() {
            adjacency.entry(edge.source).or_default().push(idx);
        }
        Self {
            nodes,
            edges,
            adjacency,
            profile,
        }
    }

    /// Fast edge lookup using adjacency (O(degree), not O(edges)).
    pub fn edge_index(&self, from: NodeId, to: NodeId) -> Option<usize> {
        self.adjacency.get(&from).and_then(|idxs| {
            idxs.iter()
                .copied()
                .find(|&i| self.edges[i].target == to)
        })
    }

    /// True if this node has at least one outgoing edge for the active profile.
    pub fn has_outgoing(&self, id: NodeId) -> bool {
        self.adjacency
            .get(&id)
            .map(|v| !v.is_empty())
            .unwrap_or(false)
    }

    /// True if the node is incident to any profile edge (source or target).
    pub fn is_linked(&self, id: NodeId) -> bool {
        if self.has_outgoing(id) {
            return true;
        }
        self.edges.iter().any(|e| e.target == id)
    }

    pub fn apply_eco_reweighting(
        &mut self,
        elevation: &ElevationService,
        eco: &crate::config::EcoConfig,
    ) {
        crate::routing::graph::reweight::reweight_graph_for_eco(self, elevation, eco);
    }

    pub fn shortest_path(
        &self,
        start: NodeId,
        goal: NodeId,
        use_eco: bool,
    ) -> Option<(Vec<NodeId>, f64)> {
        self.shortest_path_with_options(start, goal, use_eco, &RouteOptions::default())
    }

    pub fn shortest_path_with_options(
        &self,
        start: NodeId,
        goal: NodeId,
        use_eco: bool,
        options: &RouteOptions,
    ) -> Option<(Vec<NodeId>, f64)> {
        let result = astar(
            &start,
            |node| {
                self.adjacency
                    .get(node)
                    .into_iter()
                    .flatten()
                    .filter_map(|&edge_idx| {
                        let edge = &self.edges[edge_idx];
                        if !edge_allowed_for_options(edge, options) {
                            return None;
                        }
                        let cost = if use_eco {
                            edge.eco_weight.unwrap_or(edge.base_weight)
                        } else {
                            edge.base_weight
                        };
                        Some((edge.target, cost_to_u64(cost)))
                    })
                    .collect::<Vec<_>>()
            },
            |node| {
                cost_to_u64(
                    self.nodes
                        .get(node)
                        .and_then(|n| self.nodes.get(&goal).map(|g| haversine_m(n, g)))
                        .unwrap_or(0.0)
                        * 0.1,
                )
            },
            |node| node == &goal,
        );
        result.map(|(path, cost)| (path, cost as f64 / 1000.0))
    }

    /// Count edges on a path that would be excluded by vehicle/major-road options.
    pub fn restricted_edge_count(&self, edge_indices: &[usize], options: &RouteOptions) -> usize {
        edge_indices
            .iter()
            .filter(|&&i| !edge_allowed_for_options(&self.edges[i], options))
            .count()
    }
}

type EdgeMeta = (Option<String>, Option<f64>, Option<f64>, Option<f64>, Option<f64>);

fn edge_meta(edge: &Edge) -> EdgeMeta {
    let highway = edge.tags.get("highway").cloned();
    let maxweight_t = edge.tags.get("maxweight").and_then(|s| parse_metric(s));
    let maxaxleload_t = edge.tags.get("maxaxleload").and_then(|s| parse_metric(s));
    let maxheight_m = edge.tags.get("maxheight").and_then(|s| parse_metric(s));
    let maxwidth_m = edge.tags.get("maxwidth").and_then(|s| parse_metric(s));
    (highway, maxweight_t, maxaxleload_t, maxheight_m, maxwidth_m)
}

fn push_directed_edge(
    graph: &mut RouteGraph,
    id: String,
    source: NodeId,
    target: NodeId,
    start_lat: f64,
    start_lon: f64,
    end_lat: f64,
    end_lon: f64,
    length_m: f64,
    meta: &EdgeMeta,
) {
    let idx = graph.edges.len();
    graph.edges.push(GraphEdge {
        id,
        source,
        target,
        length_m,
        base_weight: length_m,
        eco_weight: None,
        start_lat,
        start_lon,
        end_lat,
        end_lon,
        highway: meta.0.clone(),
        maxweight_t: meta.1,
        maxaxleload_t: meta.2,
        maxheight_m: meta.3,
        maxwidth_m: meta.4,
    });
    graph.adjacency.entry(source).or_default().push(idx);
}

/// Which OSM-way directions are traversable for `profile`.
///
/// osm4routing stores one undirected topology edge with separate forward/backward
/// accessibility. Car/truck/bicycle must honour those flags; foot is treated as
/// bidirectional when allowed at all.
fn directed_access(edge: &Edge, profile: RoutingProfile) -> (bool, bool) {
    let mut props = edge.properties;
    props.normalize();
    match profile {
        RoutingProfile::Car | RoutingProfile::Truck => (
            props.car_forward != CarAccessibility::Forbidden,
            props.car_backward != CarAccessibility::Forbidden,
        ),
        RoutingProfile::Bicycle => (
            props.bike_forward != BikeAccessibility::Forbidden,
            props.bike_backward != BikeAccessibility::Forbidden,
        ),
        RoutingProfile::Foot => {
            let ok = props.foot != FootAccessibility::Forbidden;
            (ok, ok)
        }
    }
}

fn parse_metric(raw: &str) -> Option<f64> {
    let cleaned = raw.trim().to_lowercase().replace("t", "").replace("m", "");
    cleaned.trim().parse::<f64>().ok()
}

fn is_major_highway(highway: &str) -> bool {
    matches!(
        highway,
        "motorway"
            | "motorway_link"
            | "trunk"
            | "trunk_link"
            | "primary"
            | "primary_link"
    )
}

fn edge_allowed_for_options(edge: &GraphEdge, options: &RouteOptions) -> bool {
    if options.avoid_major_roads {
        if edge
            .highway
            .as_deref()
            .is_some_and(is_major_highway)
        {
            return false;
        }
    }
    if let Some(ref limits) = options.vehicle {
        if let (Some(limit), Some(max)) = (limits.total_weight_kg, edge.maxweight_t) {
            // OSM maxweight is typically tonnes.
            if limit / 1000.0 > max {
                return false;
            }
        }
        if let (Some(axle), Some(max)) = (limits.axle_weight_kg, edge.maxaxleload_t) {
            if axle / 1000.0 > max {
                return false;
            }
        }
        if let (Some(h), Some(max)) = (limits.height_m, edge.maxheight_m) {
            if h > max {
                return false;
            }
        }
        if let (Some(w), Some(max)) = (limits.width_m, edge.maxwidth_m) {
            if w > max {
                return false;
            }
        }
    }
    true
}

fn cost_to_u64(cost: f64) -> u64 {
    (cost.max(0.0) * 1000.0).round() as u64
}

fn filter_edges(edges: Vec<Edge>, profile: RoutingProfile) -> Vec<Edge> {
    edges
        .into_iter()
        .filter(|edge| edge_allowed(edge, profile))
        .collect()
}

fn edge_allowed(edge: &Edge, profile: RoutingProfile) -> bool {
    let mut props = edge.properties;
    props.normalize();
    match profile {
        RoutingProfile::Car | RoutingProfile::Truck => {
            props.car_forward != CarAccessibility::Forbidden
                || props.car_backward != CarAccessibility::Forbidden
        }
        RoutingProfile::Foot => props.foot != FootAccessibility::Forbidden,
        RoutingProfile::Bicycle => {
            props.bike_forward != BikeAccessibility::Forbidden
                || props.bike_backward != BikeAccessibility::Forbidden
        }
    }
}

fn haversine_m(a: &Node, b: &Node) -> f64 {
    let lat1 = a.coord.y.to_radians();
    let lat2 = b.coord.y.to_radians();
    let dlat = (b.coord.y - a.coord.y).to_radians();
    let dlon = (b.coord.x - a.coord.x).to_radians();
    let h = (dlat / 2.0).sin().powi(2)
        + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    2.0 * 6_378_100.0 * h.sqrt().asin()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_mapping() {
        assert_eq!(RoutingProfile::from(Profile::Hiking), RoutingProfile::Foot);
    }

    #[test]
    fn car_directed_access_respects_oneway() {
        let mut edge = Edge::default();
        edge.properties.car_forward = CarAccessibility::Trunk;
        edge.properties.car_backward = CarAccessibility::Forbidden;
        edge.properties.normalize();
        assert_eq!(
            directed_access(&edge, RoutingProfile::Car),
            (true, false)
        );
    }

    #[test]
    fn car_directed_access_two_way() {
        let mut edge = Edge::default();
        edge.properties.car_forward = CarAccessibility::Trunk;
        // Unknown backward copies forward on normalize (two-way road).
        edge.properties.normalize();
        assert_eq!(
            directed_access(&edge, RoutingProfile::Car),
            (true, true)
        );
    }
}
