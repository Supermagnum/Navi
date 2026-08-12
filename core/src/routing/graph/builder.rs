use std::collections::{HashMap, HashSet};
use std::path::Path;

use osm4routing::{
    BikeAccessibility, CarAccessibility, Edge, FootAccessibility, Node, NodeId, Reader,
};
use pathfinding::directed::astar::astar;
use serde::{Deserialize, Serialize};

use crate::config::{
    Profile, CAR_MAX_WAYPOINT_SNAP_M, CYCLING_MAX_WAYPOINT_SNAP_M, HIKING_MAX_WAYPOINT_SNAP_M,
    TRUCK_MAX_WAYPOINT_SNAP_M,
};
use crate::routing::access::{self, AccessMode};
use crate::routing::elevation::ElevationService;
use crate::routing::wetland::{
    tags_indicate_boardwalk, WetlandClass, WetlandIndex, WETLAND_SOFT_COST_MULT,
};

/// Routing profile derived from travel mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RoutingProfile {
    Car,
    Truck,
    Foot,
    Bicycle,
}

impl RoutingProfile {
    pub fn access_mode(self) -> AccessMode {
        match self {
            Self::Car | Self::Truck => AccessMode::Motor,
            Self::Foot => AccessMode::Foot,
            Self::Bicycle => AccessMode::Bicycle,
        }
    }
}

/// Profile-specific maximum waypoint snap distance (metres).
pub fn max_waypoint_snap_m(profile: RoutingProfile) -> f64 {
    match profile {
        RoutingProfile::Foot => HIKING_MAX_WAYPOINT_SNAP_M,
        RoutingProfile::Bicycle => CYCLING_MAX_WAYPOINT_SNAP_M,
        RoutingProfile::Car => CAR_MAX_WAYPOINT_SNAP_M,
        RoutingProfile::Truck => TRUCK_MAX_WAYPOINT_SNAP_M,
    }
}

/// Nearest linked node exceeded [`max_waypoint_snap_m`] for the graph profile.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnapTooFar {
    pub nearest_m: f64,
    pub max_m: f64,
}

/// Counters from [`RouteGraph::apply_wetland_hazards`].
#[derive(Debug, Clone, Copy, Default)]
pub struct WetlandApplyStats {
    pub soft_penalized: usize,
    pub hard_removed: usize,
    pub boardwalk_kept: usize,
}

impl From<Profile> for RoutingProfile {
    fn from(value: Profile) -> Self {
        match value {
            Profile::Car
            | Profile::CarElectric
            | Profile::Motorcycle
            | Profile::MotorcycleElectric => Self::Car,
            Profile::Truck | Profile::TruckElectric | Profile::MobileHome => Self::Truck,
            Profile::Hiking => Self::Foot,
            Profile::Cycling => Self::Bicycle,
            Profile::CyclingElectric => Self::Bicycle,
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
    /// Intermediate OSM shape points as `(lon, lat)`, excluding endpoints.
    /// Empty when the edge is a straight junction-to-junction chord only.
    pub shape: Vec<(f64, f64)>,
    pub highway: Option<String>,
    /// OSM `maxspeed` in km/h when parseable; `None` → highway-class fallback for ETA.
    pub maxspeed_kmh: Option<f64>,
    /// OSM `name` (colloquial street name) when present.
    pub name: Option<String>,
    /// OSM `ref` (systematic route number) when present.
    pub road_ref: Option<String>,
    pub maxweight_t: Option<f64>,
    pub maxaxleload_t: Option<f64>,
    pub maxbogieweight_t: Option<f64>,
    pub maxheight_m: Option<f64>,
    pub maxwidth_m: Option<f64>,
    pub maxlength_m: Option<f64>,
    pub is_toll: bool,
    pub is_ferry: bool,
    /// OSM `bridge=boardwalk` or `surface=wood` — carve-out for hard wetlands.
    pub is_boardwalk_crossing: bool,
    /// OSM `junction=roundabout` — ring edges for guidance (not routing weight).
    pub is_roundabout: bool,
    /// Raw OSM `motor_vehicle:conditional` (evaluated at plan time).
    pub motor_vehicle_conditional: Option<String>,
    /// Raw OSM `access:conditional` (evaluated at plan time).
    pub access_conditional: Option<String>,
    /// Raw OSM `maxspeed:conditional` (live speed-camera / ETA use).
    pub maxspeed_conditional: Option<String>,
    /// Static OSM access forbids this graph's profile (`motor_vehicle`/`access`/
    /// `foot`/`bicycle` with tag specificity). Independent of dimension limits.
    pub access_forbidden: bool,
}

/// Per-query routing filters (avoid motorways, tolls/ferries, vehicle limits).
///
/// Clearance: violating edges are **excluded** from A*; the router searches an
/// alternate path rather than failing hard when any restricted edge exists.
#[derive(Debug, Clone, Default)]
pub struct RouteOptions {
    /// Exclude OSM `highway=motorway` / `motorway_link` only (not trunk/primary).
    pub avoid_motorways: bool,
    /// Exclude OSM toll roads (`toll=yes` and related). Default off.
    pub avoid_tolls: bool,
    /// Exclude ferry connections. Default off.
    pub avoid_ferries: bool,
    pub vehicle: Option<crate::config::VehicleLimits>,
    /// Planned departure (local naive). `None` → evaluate seasonal closures at now.
    pub departure_local: Option<chrono::NaiveDateTime>,
}

pub struct RouteGraph {
    pub nodes: HashMap<NodeId, Node>,
    pub edges: Vec<GraphEdge>,
    adjacency: HashMap<NodeId, Vec<usize>>,
    profile: RoutingProfile,
    /// Barrier (and similar) nodes that must not be traversed *through* for this
    /// profile. Arriving at the node as a destination is allowed; leaving it is
    /// not unless the path started there.
    pub access_blocked_nodes: HashSet<NodeId>,
}

impl RouteGraph {
    pub fn build_from_pbf(path: impl AsRef<Path>, profile: RoutingProfile) -> anyhow::Result<Self> {
        let (nodes, edges) = Reader::new()
            .read_tag("highway")
            .read_tag("maxspeed")
            .read_tag("name")
            .read_tag("ref")
            .read_tag("maxweight")
            .read_tag("maxaxleload")
            .read_tag("maxbogieweight")
            .read_tag("maxheight")
            .read_tag("maxwidth")
            .read_tag("maxlength")
            .read_tag("toll")
            .read_tag("route")
            .read_tag("ferry")
            .read_tag("bridge")
            .read_tag("surface")
            .read_tag("junction")
            .read_tag("motor_vehicle")
            .read_tag("access")
            .read_tag("foot")
            .read_tag("bicycle")
            .read_tag("motor_vehicle:conditional")
            .read_tag("access:conditional")
            .read_tag("maxspeed:conditional")
            .read(path.as_ref())
            .map_err(|e| anyhow::anyhow!("osm4routing: {e}"))?;
        let filtered = filter_edges(edges, profile);
        let mut graph = Self {
            nodes: nodes.into_iter().map(|n| (n.id, n)).collect(),
            edges: Vec::new(),
            adjacency: HashMap::new(),
            profile,
            access_blocked_nodes: HashSet::new(),
        };
        for edge in filtered {
            let start = graph
                .nodes
                .get(&edge.source)
                .ok_or_else(|| anyhow::anyhow!("missing source node {}", edge.source.0))?;
            let end = graph
                .nodes
                .get(&edge.target)
                .ok_or_else(|| anyhow::anyhow!("missing target node {}", edge.target.0))?;
            let start_lat = start.coord.y;
            let start_lon = start.coord.x;
            let end_lat = end.coord.y;
            let end_lon = end.coord.x;
            let length_m = edge.length();
            let meta = edge_meta(&edge, profile);
            if meta.17 {
                // Static access forbids this profile — omit from graph.
                continue;
            }
            let (forward_ok, backward_ok) = directed_access(&edge, profile);
            if forward_ok {
                let shape: Vec<(f64, f64)> = edge
                    .geometry
                    .iter()
                    .skip(1)
                    .take(edge.geometry.len().saturating_sub(2))
                    .map(|c| (c.x, c.y))
                    .collect();
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
                    shape,
                    &meta,
                );
            }
            if backward_ok {
                let shape: Vec<(f64, f64)> = edge
                    .geometry
                    .iter()
                    .rev()
                    .skip(1)
                    .take(edge.geometry.len().saturating_sub(2))
                    .map(|c| (c.x, c.y))
                    .collect();
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
                    shape,
                    &meta,
                );
            }
        }
        graph.access_blocked_nodes =
            load_access_blocked_barrier_nodes(path.as_ref(), &graph.nodes, profile)?;
        Ok(graph)
    }

    pub fn profile(&self) -> RoutingProfile {
        self.profile
    }

    /// Build a graph from pre-built nodes/edges (tests and synthetic fixtures).
    pub fn from_parts(
        nodes: HashMap<NodeId, Node>,
        edges: Vec<GraphEdge>,
        profile: RoutingProfile,
    ) -> Self {
        Self::from_parts_with_blocks(nodes, edges, profile, HashSet::new())
    }

    /// Like [`from_parts`], with explicit barrier / access-blocked junctions.
    pub fn from_parts_with_blocks(
        nodes: HashMap<NodeId, Node>,
        edges: Vec<GraphEdge>,
        profile: RoutingProfile,
        access_blocked_nodes: HashSet<NodeId>,
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
            access_blocked_nodes,
        }
    }

    /// Fast edge lookup using adjacency (O(degree), not O(edges)).
    pub fn edge_index(&self, from: NodeId, to: NodeId) -> Option<usize> {
        self.adjacency
            .get(&from)
            .and_then(|idxs| idxs.iter().copied().find(|&i| self.edges[i].target == to))
    }

    /// True if this node has at least one outgoing edge for the active profile.
    pub fn has_outgoing(&self, id: NodeId) -> bool {
        self.adjacency
            .get(&id)
            .map(|v| !v.is_empty())
            .unwrap_or(false)
    }

    /// Outgoing edges from `from` (profile-directed adjacency).
    pub fn outgoing_edge_indices(&self, from: NodeId) -> &[usize] {
        self.adjacency
            .get(&from)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// True if the node is incident to any profile edge (source or target).
    pub fn is_linked(&self, id: NodeId) -> bool {
        if self.has_outgoing(id) {
            return true;
        }
        self.edges.iter().any(|e| e.target == id)
    }

    /// Nearest linked graph node within the profile snap budget.
    ///
    /// Prefer linked nodes (same as historical FFI snap). Returns
    /// [`SnapTooFar`] when the closest candidate exceeds
    /// [`max_waypoint_snap_m`] — callers must treat that as unreachable, not
    /// silently substitute a distant network node.
    pub fn nearest_routable(&self, lat: f64, lon: f64) -> Result<(NodeId, f64), SnapTooFar> {
        let max_m = max_waypoint_snap_m(self.profile);
        let linked = self.nodes.values().filter(|n| self.is_linked(n.id));
        let pool: Vec<&Node> = {
            let v: Vec<_> = linked.collect();
            if v.is_empty() {
                self.nodes.values().collect()
            } else {
                v
            }
        };
        let Some(best) = pool.into_iter().min_by(|a, b| {
            let da = haversine_point_m(lat, lon, a);
            let db = haversine_point_m(lat, lon, b);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        }) else {
            return Err(SnapTooFar {
                nearest_m: f64::INFINITY,
                max_m,
            });
        };
        let dist = haversine_point_m(lat, lon, best);
        if dist > max_m {
            return Err(SnapTooFar {
                nearest_m: dist,
                max_m,
            });
        }
        Ok((best.id, dist))
    }

    /// Nearest linked node with **no** snap-distance budget (trailhead for gap-fill).
    pub fn nearest_linked_unbounded(&self, lat: f64, lon: f64) -> Option<(NodeId, f64)> {
        let linked = self.nodes.values().filter(|n| self.is_linked(n.id));
        let pool: Vec<&Node> = {
            let v: Vec<_> = linked.collect();
            if v.is_empty() {
                self.nodes.values().collect()
            } else {
                v
            }
        };
        let best = pool.into_iter().min_by(|a, b| {
            let da = haversine_point_m(lat, lon, a);
            let db = haversine_point_m(lat, lon, b);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })?;
        Some((best.id, haversine_point_m(lat, lon, best)))
    }

    /// Soft-penalize / hard-exclude edges from wetland polygons.
    ///
    /// Hard wetlands exclude the edge unless [`GraphEdge::is_boardwalk_crossing`].
    /// Soft wetlands multiply weights by [`WETLAND_SOFT_COST_MULT`].
    pub fn apply_wetland_hazards(&mut self, wetlands: &WetlandIndex) -> WetlandApplyStats {
        if wetlands.is_empty() {
            return WetlandApplyStats::default();
        }
        let mut soft = 0usize;
        let mut hard_removed = 0usize;
        let mut boardwalk_kept = 0usize;
        let mut kept = Vec::with_capacity(self.edges.len());
        for mut edge in self.edges.drain(..) {
            let mid_lat = (edge.start_lat + edge.end_lat) * 0.5;
            let mid_lon = (edge.start_lon + edge.end_lon) * 0.5;
            match wetlands.class_at(mid_lat, mid_lon) {
                Some(WetlandClass::HardAvoid) => {
                    if edge.is_boardwalk_crossing {
                        boardwalk_kept += 1;
                        kept.push(edge);
                    } else {
                        hard_removed += 1;
                    }
                }
                Some(WetlandClass::SoftAvoid) => {
                    soft += 1;
                    edge.base_weight *= WETLAND_SOFT_COST_MULT;
                    if let Some(w) = edge.eco_weight.as_mut() {
                        *w *= WETLAND_SOFT_COST_MULT;
                    }
                    kept.push(edge);
                }
                None => kept.push(edge),
            }
        }
        self.edges = kept;
        self.rebuild_adjacency();
        WetlandApplyStats {
            soft_penalized: soft,
            hard_removed,
            boardwalk_kept,
        }
    }

    fn rebuild_adjacency(&mut self) {
        self.adjacency.clear();
        for (idx, edge) in self.edges.iter().enumerate() {
            self.adjacency.entry(edge.source).or_default().push(idx);
        }
    }

    /// Map overlay polyline (`lon,lat;…`) following each edge’s OSM shape when present.
    pub fn path_overlay_polyline(&self, path: &[NodeId]) -> String {
        let mut out = String::new();
        let mut last: Option<(f64, f64)> = None;
        let mut push = |lon: f64, lat: f64| {
            if last == Some((lon, lat)) {
                return;
            }
            if out.is_empty() {
                out.push_str(&format!("{lon},{lat}"));
            } else {
                out.push_str(&format!(";{lon},{lat}"));
            }
            last = Some((lon, lat));
        };
        for w in path.windows(2) {
            let Some(idx) = self.edge_index(w[0], w[1]) else {
                continue;
            };
            let e = &self.edges[idx];
            push(e.start_lon, e.start_lat);
            for &(lon, lat) in &e.shape {
                push(lon, lat);
            }
            push(e.end_lon, e.end_lat);
        }
        out
    }

    /// `(lat, lon)` vertices along the path including edge shape (for overnight / samples).
    pub fn path_coords_lat_lon(&self, path: &[NodeId]) -> Vec<(f64, f64)> {
        let mut out = Vec::new();
        for w in path.windows(2) {
            let Some(idx) = self.edge_index(w[0], w[1]) else {
                continue;
            };
            let e = &self.edges[idx];
            if out.is_empty() {
                out.push((e.start_lat, e.start_lon));
            }
            for &(lon, lat) in &e.shape {
                out.push((lat, lon));
            }
            out.push((e.end_lat, e.end_lon));
        }
        out
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
                // Node-scoped barrier block: may arrive as destination, but must
                // not continue through unless this node was the path start.
                if self.access_blocked_nodes.contains(node) && node != &start {
                    return Vec::new();
                }
                self.adjacency
                    .get(node)
                    .into_iter()
                    .flatten()
                    .filter_map(|&edge_idx| {
                        let edge = &self.edges[edge_idx];
                        if !edge_allowed_for_options(edge, options, self.profile) {
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

    /// Count edges on a path that would be excluded by vehicle/avoidance options.
    pub fn restricted_edge_count(&self, edge_indices: &[usize], options: &RouteOptions) -> usize {
        edge_indices
            .iter()
            .filter(|&&i| !edge_allowed_for_options(&self.edges[i], options, self.profile))
            .count()
    }

    /// Count edges excluded specifically by seasonal access conditionals at departure.
    pub fn seasonal_closure_excluded_count(
        &self,
        edge_indices: &[usize],
        options: &RouteOptions,
    ) -> usize {
        let apply_motor = matches!(self.profile, RoutingProfile::Car | RoutingProfile::Truck);
        edge_indices
            .iter()
            .filter(|&&i| {
                let e = &self.edges[i];
                crate::routing::conditional::edge_seasonally_closed(
                    e.motor_vehicle_conditional.as_deref(),
                    e.access_conditional.as_deref(),
                    apply_motor,
                    options.departure_local,
                )
            })
            .count()
    }

    /// Count edges in this planning graph that are hard-filtered by seasonal
    /// conditionals at `options.departure_local` (or local now when unset).
    pub fn seasonal_closure_excluded_in_graph(&self, options: &RouteOptions) -> usize {
        let apply_motor = matches!(self.profile, RoutingProfile::Car | RoutingProfile::Truck);
        self.edges
            .iter()
            .filter(|e| {
                crate::routing::conditional::edge_seasonally_closed(
                    e.motor_vehicle_conditional.as_deref(),
                    e.access_conditional.as_deref(),
                    apply_motor,
                    options.departure_local,
                )
            })
            .count()
    }

    /// Distance-weighted share (%) of path length on motorway / motorway_link.
    pub fn motorway_share_pct(&self, path: &[NodeId]) -> f64 {
        let mut total_m = 0.0;
        let mut motorway_m = 0.0;
        for w in path.windows(2) {
            let Some(idx) = self.edge_index(w[0], w[1]) else {
                continue;
            };
            let e = &self.edges[idx];
            let len = e.length_m.max(0.0);
            total_m += len;
            if highway_is_motorway(e.highway.as_deref()) {
                motorway_m += len;
            }
        }
        if total_m <= 0.0 {
            return 0.0;
        }
        100.0 * motorway_m / total_m
    }

    /// Share (%) of path length **not** on motorways (avoid-motorways “priority-path”
    /// metric for motor profiles — higher when motorways are avoided).
    pub fn non_motorway_share_pct(&self, path: &[NodeId]) -> f64 {
        (100.0 - self.motorway_share_pct(path)).clamp(0.0, 100.0)
    }

    /// Human-readable summary of what a route avoided / how many restricted segments.
    pub fn format_avoidance_report(
        &self,
        edge_indices: &[usize],
        options: &RouteOptions,
        priority_path_share_pct: f64,
    ) -> String {
        let avoided = self.restricted_edge_count(edge_indices, options);
        format_route_avoidance_report(options, avoided, priority_path_share_pct)
    }
}

/// Shared text for UI validation (motorway / toll / ferry / clearance).
pub fn format_route_avoidance_report(
    options: &RouteOptions,
    avoided_on_reference: usize,
    priority_path_share_pct_hint: f64,
) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "Avoid motorways: {}",
        if options.avoid_motorways { "ON" } else { "OFF" }
    ));
    lines.push(format!(
        "Avoid toll roads: {}",
        if options.avoid_tolls { "ON" } else { "OFF" }
    ));
    lines.push(format!(
        "Avoid ferries: {}",
        if options.avoid_ferries { "ON" } else { "OFF" }
    ));
    if options.vehicle.is_some() {
        lines.push(format!(
            "Route avoids {avoided_on_reference} weight/height/width/length-restricted segments (vs unrestricted reference)"
        ));
    }
    lines.push(format!(
        "Non-motorway road share on last plan: {priority_path_share_pct_hint:.1}% (100% minus motorway length)"
    ));
    lines.join("\n")
}

/// Append seasonal-closure counters to an avoidance report (plan-time).
pub fn append_seasonal_closure_report(report: &str, excluded_edges: usize) -> String {
    format!("{report}\nseasonal_closure_excluded_edges={excluded_edges}")
}

type EdgeMeta = (
    Option<String>,
    Option<f64>,
    Option<String>,
    Option<String>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    bool,
    bool,
    bool,
    bool,
    Option<String>,
    Option<String>,
    Option<String>,
    bool,
);

fn edge_meta(edge: &Edge, profile: RoutingProfile) -> EdgeMeta {
    let highway = edge.tags.get("highway").cloned();
    let maxspeed_kmh = edge
        .tags
        .get("maxspeed")
        .and_then(|s| crate::routing::eta::parse_maxspeed_kmh(s));
    let name = edge.tags.get("name").cloned();
    let road_ref = edge.tags.get("ref").cloned();
    let maxweight_t = edge.tags.get("maxweight").and_then(|s| parse_metric(s));
    let maxaxleload_t = edge.tags.get("maxaxleload").and_then(|s| parse_metric(s));
    let maxbogieweight_t = edge
        .tags
        .get("maxbogieweight")
        .and_then(|s| parse_metric(s));
    let maxheight_m = edge.tags.get("maxheight").and_then(|s| parse_metric(s));
    let maxwidth_m = edge.tags.get("maxwidth").and_then(|s| parse_metric(s));
    let maxlength_m = edge.tags.get("maxlength").and_then(|s| parse_metric(s));
    let is_toll = edge
        .tags
        .get("toll")
        .map(|s| is_truthy_tag(s))
        .unwrap_or(false);
    let is_ferry = edge
        .tags
        .get("route")
        .map(|s| s.eq_ignore_ascii_case("ferry"))
        .unwrap_or(false)
        || edge
            .tags
            .get("ferry")
            .map(|s| is_truthy_tag(s))
            .unwrap_or(false)
        || highway.as_deref() == Some("ferry");
    let is_boardwalk_crossing = tags_indicate_boardwalk(
        edge.tags.get("bridge").map(String::as_str),
        edge.tags.get("surface").map(String::as_str),
    );
    let is_roundabout = edge
        .tags
        .get("junction")
        .map(|s| s.eq_ignore_ascii_case("roundabout"))
        .unwrap_or(false);
    let motor_vehicle_conditional = edge.tags.get("motor_vehicle:conditional").cloned();
    let access_conditional = edge.tags.get("access:conditional").cloned();
    let maxspeed_conditional = edge.tags.get("maxspeed:conditional").cloned();
    let access_forbidden = access::mode_access_forbidden(
        profile.access_mode(),
        edge.tags.get("motor_vehicle").map(String::as_str),
        edge.tags.get("access").map(String::as_str),
        edge.tags.get("foot").map(String::as_str),
        edge.tags.get("bicycle").map(String::as_str),
    );
    (
        highway,
        maxspeed_kmh,
        name,
        road_ref,
        maxweight_t,
        maxaxleload_t,
        maxbogieweight_t,
        maxheight_m,
        maxwidth_m,
        maxlength_m,
        is_toll,
        is_ferry,
        is_boardwalk_crossing,
        is_roundabout,
        motor_vehicle_conditional,
        access_conditional,
        maxspeed_conditional,
        access_forbidden,
    )
}

fn is_truthy_tag(raw: &str) -> bool {
    matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "yes" | "true" | "1" | "toll"
    )
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
    shape: Vec<(f64, f64)>,
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
        shape,
        highway: meta.0.clone(),
        maxspeed_kmh: meta.1,
        name: meta.2.clone(),
        road_ref: meta.3.clone(),
        maxweight_t: meta.4,
        maxaxleload_t: meta.5,
        maxbogieweight_t: meta.6,
        maxheight_m: meta.7,
        maxwidth_m: meta.8,
        maxlength_m: meta.9,
        is_toll: meta.10,
        is_ferry: meta.11,
        is_boardwalk_crossing: meta.12,
        is_roundabout: meta.13,
        motor_vehicle_conditional: meta.14.clone(),
        access_conditional: meta.15.clone(),
        maxspeed_conditional: meta.16.clone(),
        access_forbidden: meta.17,
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

fn is_motorway_highway(highway: &str) -> bool {
    matches!(highway, "motorway" | "motorway_link")
}

/// True when OSM `highway` is motorway or motorway_link.
pub fn highway_is_motorway(highway: Option<&str>) -> bool {
    highway.is_some_and(is_motorway_highway)
}

fn edge_allowed_for_options(
    edge: &GraphEdge,
    options: &RouteOptions,
    profile: RoutingProfile,
) -> bool {
    if edge.access_forbidden {
        return false;
    }
    if options.avoid_motorways && edge.highway.as_deref().is_some_and(is_motorway_highway) {
        return false;
    }
    if options.avoid_tolls && edge.is_toll {
        return false;
    }
    if options.avoid_ferries && edge.is_ferry {
        return false;
    }
    let apply_motor = matches!(profile, RoutingProfile::Car | RoutingProfile::Truck);
    if crate::routing::conditional::edge_seasonally_closed(
        edge.motor_vehicle_conditional.as_deref(),
        edge.access_conditional.as_deref(),
        apply_motor,
        options.departure_local,
    ) {
        return false;
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
        if let (Some(bogie), Some(max)) = (limits.bogie_weight_kg, edge.maxbogieweight_t) {
            if bogie / 1000.0 > max {
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
        if let (Some(len), Some(max)) = (limits.length_m, edge.maxlength_m) {
            if len > max {
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
    if access::mode_access_forbidden(
        profile.access_mode(),
        edge.tags.get("motor_vehicle").map(String::as_str),
        edge.tags.get("access").map(String::as_str),
        edge.tags.get("foot").map(String::as_str),
        edge.tags.get("bicycle").map(String::as_str),
    ) {
        return false;
    }
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

fn load_access_blocked_barrier_nodes(
    path: &Path,
    graph_nodes: &HashMap<NodeId, Node>,
    profile: RoutingProfile,
) -> anyhow::Result<HashSet<NodeId>> {
    use osmpbf::{Element, ElementReader};

    let mode = profile.access_mode();
    let mut blocked = HashSet::new();
    let file = std::fs::File::open(path)?;
    let reader = ElementReader::new(file);
    reader.for_each(|element| {
        let (id, tags): (i64, HashMap<String, String>) = match element {
            Element::Node(n) => (
                n.id(),
                n.tags()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            ),
            Element::DenseNode(n) => (
                n.id(),
                n.tags()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            ),
            _ => return,
        };
        if !graph_nodes.contains_key(&NodeId(id)) {
            return;
        }
        if access::barrier_node_forbids_mode(&tags, mode) {
            blocked.insert(NodeId(id));
        }
    })?;
    Ok(blocked)
}

fn haversine_m(a: &Node, b: &Node) -> f64 {
    haversine_latlon_m(a.coord.y, a.coord.x, b.coord.y, b.coord.x)
}

fn haversine_point_m(lat: f64, lon: f64, n: &Node) -> f64 {
    haversine_latlon_m(lat, lon, n.coord.y, n.coord.x)
}

fn haversine_latlon_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let rlat1 = lat1.to_radians();
    let rlat2 = lat2.to_radians();
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let h = (dlat / 2.0).sin().powi(2) + rlat1.cos() * rlat2.cos() * (dlon / 2.0).sin().powi(2);
    2.0 * 6_378_100.0 * h.sqrt().asin()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::HIKING_MAX_WAYPOINT_SNAP_M;

    #[test]
    fn profile_mapping() {
        assert_eq!(RoutingProfile::from(Profile::Hiking), RoutingProfile::Foot);
    }

    fn two_node_foot_graph() -> RouteGraph {
        use geo_types::Coord;
        use std::collections::HashMap;

        let n_a = Node {
            id: NodeId(1),
            coord: Coord { x: 10.0, y: 60.0 },
            uses: 0,
        };
        let n_b = Node {
            id: NodeId(2),
            coord: Coord { x: 10.01, y: 60.0 },
            uses: 0,
        };
        let mut nodes = HashMap::new();
        nodes.insert(n_a.id, n_a);
        nodes.insert(n_b.id, n_b);
        let edges = vec![GraphEdge {
            id: "ab".into(),
            source: NodeId(1),
            target: NodeId(2),
            length_m: 500.0,
            base_weight: 500.0,
            eco_weight: None,
            start_lat: 60.0,
            start_lon: 10.0,
            end_lat: 60.0,
            end_lon: 10.01,
            shape: Vec::new(),
            highway: Some("path".into()),
            maxspeed_kmh: None,
            name: None,
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
            access_forbidden: false,
        }];
        RouteGraph::from_parts(nodes, edges, RoutingProfile::Foot)
    }

    #[test]
    fn nearest_routable_accepts_inside_hiking_snap_budget() {
        let graph = two_node_foot_graph();
        // ~400 m north of node A (1° lat ≈ 111_320 m).
        let lat = 60.0 + (400.0 / 111_320.0);
        let (id, dist) = graph
            .nearest_routable(lat, 10.0)
            .expect("within 500 m budget");
        assert_eq!(id, NodeId(1));
        assert!(dist < HIKING_MAX_WAYPOINT_SNAP_M);
        assert!(dist > 350.0);
    }

    #[test]
    fn nearest_routable_rejects_outside_hiking_snap_budget() {
        let graph = two_node_foot_graph();
        let lat = 60.0 + (600.0 / 111_320.0);
        let err = graph
            .nearest_routable(lat, 10.0)
            .expect_err("600 m exceeds 500 m hiking budget");
        assert!(err.nearest_m > HIKING_MAX_WAYPOINT_SNAP_M);
        assert_eq!(err.max_m, HIKING_MAX_WAYPOINT_SNAP_M);
    }

    #[test]
    fn car_directed_access_respects_oneway() {
        let mut edge = Edge::default();
        edge.properties.car_forward = CarAccessibility::Trunk;
        edge.properties.car_backward = CarAccessibility::Forbidden;
        edge.properties.normalize();
        assert_eq!(directed_access(&edge, RoutingProfile::Car), (true, false));
    }

    #[test]
    fn car_directed_access_two_way() {
        let mut edge = Edge::default();
        edge.properties.car_forward = CarAccessibility::Trunk;
        // Unknown backward copies forward on normalize (two-way road).
        edge.properties.normalize();
        assert_eq!(directed_access(&edge, RoutingProfile::Car), (true, true));
    }

    #[test]
    fn mobile_home_maps_to_truck_graph() {
        assert_eq!(
            RoutingProfile::from(Profile::MobileHome),
            RoutingProfile::Truck
        );
    }

    #[test]
    fn clearance_excludes_low_bridge_and_finds_alternate() {
        use geo_types::Coord;
        use std::collections::HashMap;

        let n_a = Node {
            id: NodeId(1),
            coord: Coord { x: 10.0, y: 60.0 },
            uses: 0,
        };
        let n_b = Node {
            id: NodeId(2),
            coord: Coord { x: 10.01, y: 60.0 },
            uses: 0,
        };
        let n_c = Node {
            id: NodeId(3),
            coord: Coord { x: 10.02, y: 60.0 },
            uses: 0,
        };
        let n_d = Node {
            id: NodeId(4),
            coord: Coord { x: 10.01, y: 60.01 },
            uses: 0,
        };
        let mut nodes = HashMap::new();
        nodes.insert(n_a.id, n_a);
        nodes.insert(n_b.id, n_b);
        nodes.insert(n_c.id, n_c);
        nodes.insert(n_d.id, n_d);
        let edge = |id: &str,
                    s: i64,
                    t: i64,
                    lat0: f64,
                    lon0: f64,
                    lat1: f64,
                    lon1: f64,
                    max_h: Option<f64>| GraphEdge {
            id: id.into(),
            source: NodeId(s),
            target: NodeId(t),
            length_m: 1000.0,
            base_weight: 1000.0,
            eco_weight: None,
            start_lat: lat0,
            start_lon: lon0,
            end_lat: lat1,
            end_lon: lon1,
            shape: Vec::new(),
            highway: Some("secondary".into()),
            maxspeed_kmh: None,
            name: None,
            road_ref: None,
            maxweight_t: None,
            maxaxleload_t: None,
            maxbogieweight_t: None,
            maxheight_m: max_h,
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
        };
        let edges = vec![
            edge("low", 1, 2, 60.0, 10.0, 60.0, 10.01, Some(3.0)),
            edge("bc", 2, 3, 60.0, 10.01, 60.0, 10.02, None),
            edge("ad", 1, 4, 60.0, 10.0, 60.01, 10.01, None),
            edge("dc", 4, 3, 60.01, 10.01, 60.0, 10.02, None),
        ];
        let graph = RouteGraph::from_parts(nodes, edges, RoutingProfile::Truck);
        let limits = crate::config::VehicleLimits {
            height_m: Some(4.0),
            ..Default::default()
        };
        let opts = RouteOptions {
            vehicle: Some(limits),
            ..Default::default()
        };
        let path = graph
            .shortest_path_with_options(NodeId(1), NodeId(3), false, &opts)
            .expect("alternate around low bridge");
        assert!(
            !path.0.contains(&NodeId(2)),
            "must not use low bridge node B: {:?}",
            path.0
        );
        assert!(path.0.contains(&NodeId(4)));
    }

    #[test]
    fn avoid_toll_and_ferry_flags() {
        let mut edge = GraphEdge {
            id: "t".into(),
            source: NodeId(1),
            target: NodeId(2),
            length_m: 100.0,
            base_weight: 100.0,
            eco_weight: None,
            start_lat: 0.0,
            start_lon: 0.0,
            end_lat: 0.0,
            end_lon: 0.0,
            shape: Vec::new(),
            highway: Some("primary".into()),
            maxspeed_kmh: None,
            name: None,
            road_ref: None,
            maxweight_t: None,
            maxaxleload_t: None,
            maxbogieweight_t: None,
            maxheight_m: None,
            maxwidth_m: None,
            maxlength_m: None,
            is_toll: true,
            is_ferry: false,
            is_boardwalk_crossing: false,
            is_roundabout: false,
            motor_vehicle_conditional: None,
            access_conditional: None,
            maxspeed_conditional: None,
            access_forbidden: false,
        };
        assert!(!edge_allowed_for_options(
            &edge,
            &RouteOptions {
                avoid_tolls: true,
                ..Default::default()
            },
            RoutingProfile::Car,
        ));
        edge.is_toll = false;
        edge.is_ferry = true;
        assert!(!edge_allowed_for_options(
            &edge,
            &RouteOptions {
                avoid_ferries: true,
                ..Default::default()
            },
            RoutingProfile::Car,
        ));
        assert!(edge_allowed_for_options(
            &edge,
            &RouteOptions::default(),
            RoutingProfile::Car,
        ));
    }

    #[test]
    fn friisvegen_style_conditional_blocks_car_not_foot() {
        use chrono::NaiveDate;
        let mut edge = GraphEdge {
            id: "361797686-0".into(),
            source: NodeId(1),
            target: NodeId(2),
            length_m: 100.0,
            base_weight: 100.0,
            eco_weight: None,
            start_lat: 61.9,
            start_lon: 10.0,
            end_lat: 61.91,
            end_lon: 10.01,
            shape: Vec::new(),
            highway: Some("unclassified".into()),
            maxspeed_kmh: Some(80.0),
            name: Some("Friisvegen".into()),
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
            motor_vehicle_conditional: Some("no @ Nov-Jun".into()),
            access_conditional: None,
            maxspeed_conditional: None,
            access_forbidden: false,
        };
        let jan = NaiveDate::from_ymd_opt(2026, 1, 15)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap();
        let jul = NaiveDate::from_ymd_opt(2026, 7, 15)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap();
        let winter = RouteOptions {
            departure_local: Some(jan),
            ..Default::default()
        };
        let summer = RouteOptions {
            departure_local: Some(jul),
            ..Default::default()
        };
        assert!(!edge_allowed_for_options(
            &edge,
            &winter,
            RoutingProfile::Car
        ));
        assert!(!edge_allowed_for_options(
            &edge,
            &winter,
            RoutingProfile::Truck
        ));
        assert!(edge_allowed_for_options(
            &edge,
            &summer,
            RoutingProfile::Car
        ));
        // Hiking/Bicycle must ignore motor_vehicle:conditional.
        assert!(edge_allowed_for_options(
            &edge,
            &winter,
            RoutingProfile::Foot
        ));
        assert!(edge_allowed_for_options(
            &edge,
            &winter,
            RoutingProfile::Bicycle
        ));
        edge.motor_vehicle_conditional = None;
        edge.access_conditional = Some("no @ Nov-Jun".into());
        assert!(!edge_allowed_for_options(
            &edge,
            &winter,
            RoutingProfile::Foot
        ));
    }
}
