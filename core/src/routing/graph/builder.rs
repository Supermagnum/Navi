use std::collections::{HashMap, HashSet};
use std::path::Path;

use osm4routing::{
    BikeAccessibility, CarAccessibility, Edge, FootAccessibility, Node, NodeId, Reader,
};
use pathfinding::directed::astar::astar;
use serde::{Deserialize, Serialize};

use crate::config::{
    Profile, CAR_MAX_WAYPOINT_SNAP_M, CYCLING_MAX_WAYPOINT_SNAP_M, HIKING_MAX_WAYPOINT_SNAP_M,
    SURFACE_VIA_SNAP_SLACK_M, TRUCK_MAX_WAYPOINT_SNAP_M,
};
use crate::routing::access::{self, AccessMode};
use crate::routing::elevation::ElevationService;
use crate::routing::wetland::{
    tags_indicate_boardwalk, WetlandClass, WetlandIndex, WETLAND_SOFT_COST_MULT,
};

use super::surface_quality::{
    classify_surface_tags, surface_transition_cost_m, worst_incident_surface, SurfaceQuality,
    SurfaceRoutingMode, SNAP_VIRTUAL_APPROACH_SURFACE,
};

/// Sentinel `incoming_edge` on the A* start state (no prior graph edge).
const NO_INCOMING_EDGE: usize = usize::MAX;

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

/// Directed-graph usability for waypoint snaps.
///
/// Weak (undirected) connectivity is not enough: a one-way dead-end stub sits in
/// the giant weak component but cannot be used as a destination (unreachable)
/// or origin (cannot depart into the main network). Labels are precomputed in
/// [`RouteGraph::rebuild_adjacency`] from the largest SCC of the giant weak
/// component so plan-time snap stays O(candidates).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SnapRole {
    /// No directed filter (legacy probes / tests).
    #[default]
    Any,
    /// Must reach the main directed network (origin).
    Origin,
    /// Must be reachable from the main directed network (destination).
    Destination,
    /// Must reach main and be reachable from it (via).
    Via,
}

/// Drive-equivalent speed used to turn a ferry crossing into A* metres.
/// Matches navi-server `pack-convert-core` [`ferry_base_weight_m`].
pub const FERRY_DRIVE_EQUIV_KMH: f64 = 80.0;
/// Assumed ferry speed when OSM `duration` is missing.
pub const FERRY_FALLBACK_SPEED_KMH: f64 = 10.0;
/// Extra car/truck boarding cost in minutes, converted at [`FERRY_DRIVE_EQUIV_KMH`].
pub const FERRY_CAR_BOARDING_PENALTY_MIN: f64 = 10.0;

fn ferry_drive_equiv_m_per_s() -> f64 {
    FERRY_DRIVE_EQUIV_KMH * 1000.0 / 3600.0
}

/// Parse OSM `duration` as `H:MM`, `HH:MM`, or `HH:MM:SS`. Minutes must be `< 60`.
pub(crate) fn parse_osm_duration_secs(raw: &str) -> Option<f64> {
    let parts: Vec<&str> = raw.trim().split(':').collect();
    let nums: Vec<f64> = parts
        .iter()
        .map(|p| p.trim().parse::<f64>().ok())
        .collect::<Option<_>>()?;
    if nums.iter().any(|n| !n.is_finite() || *n < 0.0) {
        return None;
    }
    match *nums.as_slice() {
        [h, m] if m < 60.0 => Some((h * 60.0 + m) * 60.0),
        [h, m, s] if m < 60.0 && s < 60.0 => Some(h * 3600.0 + m * 60.0 + s),
        _ => None,
    }
}

fn endpoint_haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6_371_000.0_f64;
    let p1 = lat1.to_radians();
    let p2 = lat2.to_radians();
    let dp = p2 - p1;
    let dl = (lon2 - lon1).to_radians();
    let h = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * r * h.sqrt().asin()
}

/// OSM `highway=construction` / `highway=proposed` (and the same values with
/// a `;` suffix). Must never be routable.
pub fn is_construction_or_proposed_highway(highway: Option<&str>) -> bool {
    let Some(raw) = highway else {
        return false;
    };
    raw.split(';').any(|part| {
        matches!(
            part.trim().to_ascii_lowercase().as_str(),
            "construction" | "proposed"
        )
    })
}

/// True when a pack edge is a non-ferry chord with no interior shape (Fehmarn
/// Belt water duplicate). This is not a water-polygon test: empty `shape`, not
/// ferry/tunnel, not motorway/trunk/primary. There is no length floor.
pub fn is_untagged_water_shortcut(edge: &GraphEdge) -> bool {
    if edge.is_ferry || edge.is_tunnel {
        return false;
    }
    if !edge.shape.is_empty() {
        return false;
    }
    match edge.highway.as_deref() {
        Some(
            "motorway" | "motorway_link" | "trunk" | "trunk_link" | "primary" | "primary_link",
        ) => false,
        _ => true,
    }
}

const OVERLAY_FERRY_MATCH_M: f64 = 2_500.0;

/// Pack edge whose endpoints match an overlay `route=ferry` (either direction).
pub fn pack_edge_matches_overlay_ferry(pack: &GraphEdge, ferry: &GraphEdge) -> bool {
    if !ferry.is_ferry {
        return false;
    }
    let same = endpoint_haversine_m(
        pack.start_lat,
        pack.start_lon,
        ferry.start_lat,
        ferry.start_lon,
    ) < OVERLAY_FERRY_MATCH_M
        && endpoint_haversine_m(pack.end_lat, pack.end_lon, ferry.end_lat, ferry.end_lon)
            < OVERLAY_FERRY_MATCH_M;
    let rev = endpoint_haversine_m(pack.start_lat, pack.start_lon, ferry.end_lat, ferry.end_lon)
        < OVERLAY_FERRY_MATCH_M
        && endpoint_haversine_m(pack.end_lat, pack.end_lon, ferry.start_lat, ferry.start_lon)
            < OVERLAY_FERRY_MATCH_M;
    same || rev
}

/// Stamp pack edges that match overlay ferries: `is_ferry` plus ferry A* cost.
pub fn stamp_overlay_ferry_costs(
    graph: &mut RouteGraph,
    overlay: &RouteGraph,
    profile: RoutingProfile,
) {
    let ferries: Vec<&GraphEdge> = overlay.edges.iter().filter(|e| e.is_ferry).collect();
    if ferries.is_empty() {
        return;
    }
    for e in graph.edges.iter_mut() {
        let hit = ferries
            .iter()
            .copied()
            .find(|f| pack_edge_matches_overlay_ferry(e, f));
        let Some(f) = hit else {
            continue;
        };
        e.is_ferry = true;
        if e.name.is_none() {
            e.name = f.name.clone();
        }
        e.base_weight = ferry_base_weight_m(e.length_m, None, profile);
    }
}

/// A* weight in metres for a ferry edge. `length_m` stays the geometric length.
/// Same formula as navi-server `ferry_base_weight_m`.
pub fn ferry_base_weight_m(
    length_m: f64,
    duration_raw: Option<&str>,
    profile: RoutingProfile,
) -> f64 {
    let travel = match duration_raw.and_then(parse_osm_duration_secs) {
        Some(secs) => secs * ferry_drive_equiv_m_per_s(),
        None => length_m * (FERRY_DRIVE_EQUIV_KMH / FERRY_FALLBACK_SPEED_KMH),
    };
    let boarding = match profile {
        RoutingProfile::Car | RoutingProfile::Truck => {
            FERRY_CAR_BOARDING_PENALTY_MIN * 60.0 * ferry_drive_equiv_m_per_s()
        }
        RoutingProfile::Foot | RoutingProfile::Bicycle => 0.0,
    };
    travel + boarding
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
    /// OSM `maxspeed:practical` in km/h — preferred for ETA when present (achievable speed).
    pub maxspeed_practical_kmh: Option<f64>,
    /// OSM `maxspeed:advisory` in km/h — ETA hint when posted/practical are absent.
    pub maxspeed_advisory_kmh: Option<f64>,
    /// Raw OSM `maxspeed:type` (zone/source metadata; no weight/ETA effect).
    pub maxspeed_type: Option<String>,
    /// OSM `maxspeed:variable` truthy — variable-message / matrix signs (display only).
    pub maxspeed_variable: bool,
    /// OSM `minspeed` in km/h — floor for motor ETA; excludes foot/bicycle when set.
    pub minspeed_kmh: Option<f64>,
    /// OSM `name` (colloquial street name) when present.
    pub name: Option<String>,
    /// OSM `ref` plus `int_ref` when they differ (display / guidance only).
    pub road_ref: Option<String>,
    /// OSM `motorroad=yes` (Norwegian motortrafikkvei).
    pub is_motorroad: bool,
    /// OSM `expressway=yes`.
    pub is_expressway: bool,
    /// OSM `oneway=yes` / `true` / `1` (not `-1`).
    pub is_oneway: bool,
    /// First integer in OSM `lanes`, when parseable.
    pub lanes: Option<u8>,
    pub maxweight_t: Option<f64>,
    pub maxaxleload_t: Option<f64>,
    pub maxbogieweight_t: Option<f64>,
    pub maxheight_m: Option<f64>,
    pub maxwidth_m: Option<f64>,
    pub maxlength_m: Option<f64>,
    pub is_toll: bool,
    pub is_ferry: bool,
    /// OSM `tunnel=*` (any non-empty value other than `no`). Soft-avoid via
    /// [`RouteOptions::avoid_tunnels`], not a hard exclusion.
    pub is_tunnel: bool,
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
    /// Driveability from OSM `surface` / `tracktype` (motor routing preference).
    pub surface_quality: SurfaceQuality,
}

/// Per-query routing filters (avoid motorways, tolls/ferries, vehicle limits).
///
/// Clearance / motorway / ferry / [`TollPolicy::NeverUse`]: violating edges are
/// **excluded** from A*. [`TollPolicy::Penalize`] keeps toll edges but multiplies
/// their cost by [`crate::routing::toll::TOLL_AVOID_PENALTY_MULT`].
/// [`RouteOptions::avoid_tunnels`] likewise keeps tunnel edges but multiplies
/// their cost by [`crate::routing::toll::TUNNEL_AVOID_PENALTY_MULT`].
///
/// Active DATEX constraints ([`crate::datex::planner_impacts`]): [`DatexImpact::Block`]
/// hard-excludes nearby edges; [`DatexImpact::Penalize`] multiplies cost by the
/// constraint's `penalize_mult` (delay-scaled when present, else
/// [`crate::datex::DATEX_PENALIZE_MULT`]). Pass **active-only** situations.
#[derive(Debug, Clone, Default)]
pub struct RouteOptions {
    /// Exclude motorway-grade roads: `highway=motorway` / `motorway_link`,
    /// `motorroad=yes` / `expressway=yes`, or oneway with `lanes>=2` and
    /// `maxspeed>=90`. Not region-gated.
    pub avoid_motorways: bool,
    /// How to treat OSM toll roads. Replaces the former `avoid_tolls: bool`
    /// (`false`→[`TollPolicy::Allow`], `true`→[`TollPolicy::Penalize`]).
    pub toll_policy: crate::routing::toll::TollPolicy,
    /// Exclude ferry connections. Default off.
    pub avoid_ferries: bool,
    /// Soft-prefer tunnel-free roads via
    /// [`crate::routing::toll::TUNNEL_AVOID_PENALTY_MULT`]. Never hard-excludes
    /// tunnels (destinations reachable only via tunnel must still succeed).
    pub avoid_tunnels: bool,
    pub vehicle: Option<crate::config::VehicleLimits>,
    /// Planned departure (local naive). `None` → evaluate seasonal closures at now.
    pub departure_local: Option<chrono::NaiveDateTime>,
    /// Active DATEX planner constraints (empty = no DATEX effect). Prefer
    /// [`crate::datex::planner_impacts`] on the active corridor slice only.
    /// A* / snap must not scan this on the hot path — call
    /// [`RouteGraph::bind_datex_overlay`] after the hop graph is loaded.
    pub datex_impacts: Vec<crate::datex::DatexPlannerConstraint>,
    /// Per-edge DATEX Block (1) / not (0), parallel to [`RouteGraph::edges`].
    pub datex_edge_block: Option<std::sync::Arc<Vec<u8>>>,
    /// Per-edge DATEX extra metres (`0.0` = none), from penalty minutes at 80 km/h.
    pub datex_edge_penalize: Option<std::sync::Arc<Vec<f64>>>,
    /// When `Some`, only traverse edges whose midpoint falls inside one of these
    /// ISO-3166-1 alpha-2 codes (case-insensitive). `None` keeps historical
    /// behaviour (no country filter). Hard constraint — never soft-penalize.
    pub allowed_countries: Option<Vec<String>>,
    /// Per-edge eco joule costs (parallel to `RouteGraph::edges`). When set and
    /// `use_eco`, overrides `GraphEdge::eco_weight` without mutating the graph.
    pub eco_weights: Option<std::sync::Arc<Vec<f64>>>,
    /// Apply motor soft multipliers at cost-eval time (pack-hit Arc graphs).
    pub motor_soft: Option<(
        crate::routing::graph::SurfaceRoutingMode,
        crate::routing::graph::MotorSoftCostProfile,
    )>,
    /// Override [`RouteGraph::surface_routing_mode`] for A* / snap without
    /// mutating a shared corridor Arc.
    pub surface_routing_mode: Option<crate::routing::graph::SurfaceRoutingMode>,
    /// Directed usability filter for waypoint snaps. Ignored by A*.
    pub snap_role: SnapRole,
}

/// Outcome of one A* attempt (path may be absent).
#[derive(Debug, Clone)]
pub struct PathSearchStats {
    pub path: Option<(Vec<NodeId>, Vec<usize>, f64)>,
    pub expansions: u64,
    /// `found`, `disconnected`, `cancelled`, or `outside_countries`.
    pub terminate_reason: &'static str,
}

/// Counts directed `(source, target)` pairs with multiple profile edges.
#[derive(Debug, Clone, Default)]
pub struct ParallelEdgeCensus {
    pub total_directed_edges: usize,
    pub parallel_directed_pairs: usize,
    /// Edges beyond the first per parallel pair (`sum(len-1)`).
    pub extra_parallel_edges: usize,
    /// Pairs where adjacency-first `edge_index` would not pick min `base_weight`.
    pub old_edge_index_would_mismatch: usize,
    /// Parallel pairs whose edges span two or more distinct `highway=*` values.
    pub mixed_highway_class_pairs: usize,
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
    /// Nodes incident to at least one edge (source or target).
    incident: HashSet<NodeId>,
    /// Weakly-connected component root per incident node (undirected).
    component_root: HashMap<NodeId, NodeId>,
    /// Root of the largest weakly-connected component, if any.
    giant_root: Option<NodeId>,
    /// True after [`Self::recompute_directed_main_labels`] populated stub sets.
    directed_labels_ready: bool,
    /// Giant-component nodes that cannot be origins (cannot reach main SCC).
    /// Kept small vs storing the full can-reach set (~250k → hundreds).
    origin_reject: HashSet<NodeId>,
    /// Giant-component nodes that cannot be destinations (not reachable from main).
    dest_reject: HashSet<NodeId>,
    /// Count of giant-component nodes (for diagnostics / label sizes).
    directed_giant_nodes: usize,
    /// Surface strictness for motor snap preference and transition penalties.
    pub surface_routing_mode: SurfaceRoutingMode,
}

impl Clone for RouteGraph {
    fn clone(&self) -> Self {
        Self {
            nodes: self.nodes.clone(),
            edges: self.edges.clone(),
            adjacency: self.adjacency.clone(),
            profile: self.profile,
            access_blocked_nodes: self.access_blocked_nodes.clone(),
            incident: self.incident.clone(),
            component_root: self.component_root.clone(),
            giant_root: self.giant_root,
            directed_labels_ready: self.directed_labels_ready,
            origin_reject: self.origin_reject.clone(),
            dest_reject: self.dest_reject.clone(),
            directed_giant_nodes: self.directed_giant_nodes,
            surface_routing_mode: self.surface_routing_mode,
        }
    }
}

/// Indices into `hard_candidates` that must be restored (as soft cost) so every
/// boardwalk-touching component stays linked to the giant component of `kept`.
fn wetland_boardwalk_bridges(
    kept: &[GraphEdge],
    hard_candidates: &[GraphEdge],
    boardwalk_nodes: &HashSet<NodeId>,
) -> Vec<usize> {
    let mut parent: HashMap<NodeId, NodeId> = HashMap::new();
    let mut size: HashMap<NodeId, usize> = HashMap::new();
    let ensure =
        |id: NodeId, parent: &mut HashMap<NodeId, NodeId>, size: &mut HashMap<NodeId, usize>| {
            parent.entry(id).or_insert(id);
            size.entry(id).or_insert(1);
        };
    for e in kept {
        ensure(e.source, &mut parent, &mut size);
        ensure(e.target, &mut parent, &mut size);
        uf_union(&mut parent, &mut size, e.source, e.target);
    }
    for e in hard_candidates {
        ensure(e.source, &mut parent, &mut size);
        ensure(e.target, &mut parent, &mut size);
    }

    // Roots of components that already contain a boardwalk node via kept edges.
    let mut bw_roots: HashSet<NodeId> = HashSet::new();
    for &bw in boardwalk_nodes {
        if kept.iter().any(|e| e.source == bw || e.target == bw) {
            bw_roots.insert(uf_find(&mut parent, bw));
        }
    }
    if bw_roots.is_empty() {
        return Vec::new();
    }

    // Restore every hard candidate that joins a boardwalk-touched component to a
    // different component (repeat until fixed point so chains grow).
    let mut restore: HashSet<usize> = HashSet::new();
    let mut progressed = true;
    while progressed {
        progressed = false;
        for (i, e) in hard_candidates.iter().enumerate() {
            if restore.contains(&i) {
                continue;
            }
            let ru = uf_find(&mut parent, e.source);
            let rv = uf_find(&mut parent, e.target);
            if ru == rv {
                continue;
            }
            if !(bw_roots.contains(&ru) || bw_roots.contains(&rv)) {
                continue;
            }
            restore.insert(i);
            uf_union(&mut parent, &mut size, e.source, e.target);
            bw_roots.remove(&ru);
            bw_roots.remove(&rv);
            bw_roots.insert(uf_find(&mut parent, e.source));
            progressed = true;
        }
    }
    restore.into_iter().collect()
}

impl RouteGraph {
    pub fn build_from_pbf(path: impl AsRef<Path>, profile: RoutingProfile) -> anyhow::Result<Self> {
        crate::routing::pbf_extract::ensure_pbf_usable_for_graph_build(path.as_ref())?;
        let (nodes, edges) = Reader::new()
            .read_tag("highway")
            .read_tag("maxspeed")
            .read_tag("maxspeed:practical")
            .read_tag("maxspeed:advisory")
            .read_tag("maxspeed:type")
            .read_tag("maxspeed:variable")
            .read_tag("minspeed")
            .read_tag("name")
            .read_tag("ref")
            .read_tag("int_ref")
            .read_tag("motorroad")
            .read_tag("expressway")
            .read_tag("oneway")
            .read_tag("lanes")
            .read_tag("maxweight")
            .read_tag("maxaxleload")
            .read_tag("maxbogieweight")
            .read_tag("maxheight")
            .read_tag("maxwidth")
            .read_tag("maxlength")
            .read_tag("toll")
            .read_tag("toll:motor_vehicle")
            .read_tag("toll:motorcar")
            .read_tag("toll:motorcycle")
            .read_tag("toll:hgv")
            .read_tag("toll:bicycle")
            .read_tag("toll:foot")
            .read_tag("route")
            .read_tag("ferry")
            .read_tag("bridge")
            .read_tag("surface")
            .read_tag("tracktype")
            .read_tag("junction")
            .read_tag("motor_vehicle")
            .read_tag("access")
            .read_tag("foot")
            .read_tag("bicycle")
            .read_tag("motor_vehicle:conditional")
            .read_tag("access:conditional")
            .read_tag("maxspeed:conditional")
            .read_tag("duration")
            .read(path.as_ref())
            .map_err(|e| anyhow::anyhow!("osm4routing: {e}"))?;
        let filtered = filter_edges(edges, profile);
        let mut graph = Self {
            nodes: nodes.into_iter().map(|n| (n.id, n)).collect(),
            edges: Vec::new(),
            adjacency: HashMap::new(),
            profile,
            access_blocked_nodes: HashSet::new(),
            incident: HashSet::new(),
            component_root: HashMap::new(),
            giant_root: None,
            directed_labels_ready: false,
            origin_reject: HashSet::new(),
            dest_reject: HashSet::new(),
            directed_giant_nodes: 0,
            surface_routing_mode: SurfaceRoutingMode::default(),
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
            if meta.access_forbidden {
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
        graph.rebuild_adjacency();
        Ok(graph)
    }

    pub fn profile(&self) -> RoutingProfile {
        self.profile
    }

    /// One pass over hop edges: DATEX Block/Penalize become per-edge markers.
    /// Snap and A* only test those markers afterwards.
    ///
    /// Impacts are put on a lat/lon grid so each edge is tested only against
    /// nearby records, not the full list (O(E × D) was 25–32 s on hop 13).
    pub fn bind_datex_overlay(&self, options: &mut RouteOptions) {
        if options.datex_impacts.is_empty() {
            options.datex_edge_block = None;
            options.datex_edge_penalize = None;
            return;
        }
        let n = self.edges.len();
        let mut block = vec![0u8; n];
        let mut penalize = vec![0.0f64; n];
        let mut max_radius_m = 250.0_f64;
        for c in &options.datex_impacts {
            if matches!(
                c.impact,
                crate::datex::DatexImpact::Ignore | crate::datex::DatexImpact::Warn
            ) {
                continue;
            }
            max_radius_m = max_radius_m.max(c.radius_m);
        }
        // ~1 km cells; pad queries by the largest impact radius.
        const CELL_DEG: f64 = 0.01;
        let mut grid: std::collections::HashMap<(i32, i32), Vec<usize>> =
            std::collections::HashMap::new();
        for (j, c) in options.datex_impacts.iter().enumerate() {
            if matches!(
                c.impact,
                crate::datex::DatexImpact::Ignore | crate::datex::DatexImpact::Warn
            ) {
                continue;
            }
            grid.entry(datex_cell(c.lat, c.lon, CELL_DEG))
                .or_default()
                .push(j);
        }
        let mut seen = vec![0u32; options.datex_impacts.len()];
        let mut stamp = 1u32;
        for (i, e) in self.edges.iter().enumerate() {
            let (min_lat, max_lat, min_lon, max_lon) = edge_latlon_bbox(e);
            let mid_lat = ((min_lat + max_lat) * 0.5).clamp(-89.0, 89.0);
            let lat_pad = max_radius_m / 111_320.0;
            let lon_pad = max_radius_m / (111_320.0 * mid_lat.to_radians().cos().max(0.2));
            let gi0 = ((min_lat - lat_pad) / CELL_DEG).floor() as i32;
            let gi1 = ((max_lat + lat_pad) / CELL_DEG).floor() as i32;
            let gj0 = ((min_lon - lon_pad) / CELL_DEG).floor() as i32;
            let gj1 = ((max_lon + lon_pad) / CELL_DEG).floor() as i32;
            if stamp == u32::MAX {
                seen.fill(0);
                stamp = 1;
            }
            for gi in gi0..=gi1 {
                for gj in gj0..=gj1 {
                    let Some(js) = grid.get(&(gi, gj)) else {
                        continue;
                    };
                    for &j in js {
                        if seen[j] == stamp {
                            continue;
                        }
                        seen[j] = stamp;
                        let c = &options.datex_impacts[j];
                        if !edge_hit_by_datex(e, c) {
                            continue;
                        }
                        match c.impact {
                            crate::datex::DatexImpact::Block => block[i] = 1,
                            crate::datex::DatexImpact::Penalize => {
                                let extra = (c.penalty_minutes.max(0.0) * 80_000.0 / 60.0).max(0.0);
                                penalize[i] = penalize[i].max(extra);
                            }
                            crate::datex::DatexImpact::Ignore | crate::datex::DatexImpact::Warn => {
                            }
                        }
                    }
                }
            }
            stamp = stamp.saturating_add(1);
        }
        options.datex_edge_block = Some(std::sync::Arc::new(block));
        options.datex_edge_penalize = Some(std::sync::Arc::new(penalize));
    }

    /// Build a graph from pre-built nodes/edges (tests and synthetic fixtures).
    pub fn from_parts(
        nodes: HashMap<NodeId, Node>,
        edges: Vec<GraphEdge>,
        profile: RoutingProfile,
    ) -> Self {
        // Defer Kosaraju until a plan proves dig-style Any snaps are not
        // directed-reachable (Stavanger stub). Dig-matching ODs never pay the
        // label working set (~tens of MiB peak that dig never held).
        Self::from_parts_with_blocks(nodes, edges, profile, HashSet::new(), false)
    }

    /// Like [`from_parts`], with explicit barrier / access-blocked junctions.
    ///
    /// When `with_directed_labels` is false (tile hydrate), skip Kosaraju; call
    /// [`Self::ensure_directed_snap_labels`] on the final merged corridor only.
    pub fn from_parts_with_blocks(
        nodes: HashMap<NodeId, Node>,
        edges: Vec<GraphEdge>,
        profile: RoutingProfile,
        access_blocked_nodes: HashSet<NodeId>,
        with_directed_labels: bool,
    ) -> Self {
        let mut graph = Self {
            nodes,
            edges,
            adjacency: HashMap::new(),
            profile,
            access_blocked_nodes,
            incident: HashSet::new(),
            component_root: HashMap::new(),
            giant_root: None,
            directed_labels_ready: false,
            origin_reject: HashSet::new(),
            dest_reject: HashSet::new(),
            directed_giant_nodes: 0,
            surface_routing_mode: SurfaceRoutingMode::default(),
        };
        graph.rebuild_adjacency();
        if with_directed_labels {
            graph.ensure_directed_snap_labels();
        }
        graph
    }

    /// Cheapest parallel edge between `from` and `to` under the same costing model as
    /// [`Self::shortest_path_with_options`] (base/eco weight plus surface transition).
    pub fn best_edge_index_between(
        &self,
        from: NodeId,
        to: NodeId,
        prev_surface: Option<SurfaceQuality>,
        use_eco: bool,
        options: &RouteOptions,
    ) -> Option<usize> {
        let surface_mode = options
            .surface_routing_mode
            .unwrap_or(self.surface_routing_mode);
        let use_surface_transitions = surface_mode == SurfaceRoutingMode::Car
            && matches!(self.profile, RoutingProfile::Car | RoutingProfile::Truck);
        let mut best: Option<(usize, u64)> = None;
        for &idx in self.outgoing_edge_indices(from) {
            let edge = &self.edges[idx];
            if edge.target != to || !edge_allowed_at(edge, idx, options, self.profile) {
                continue;
            }
            let base = edge_travel_cost(edge, idx, use_eco, options);
            let transition = if use_surface_transitions {
                surface_transition_cost_m(prev_surface, edge.surface_quality, surface_mode)
            } else {
                0.0
            };
            let cost = cost_to_u64(base + transition);
            if best.map(|(_, c)| cost < c).unwrap_or(true) {
                best = Some((idx, cost));
            }
        }
        best.map(|(idx, _)| idx)
    }

    /// Edge indices along a node path, matching the edges A* would have taken.
    pub fn path_edge_indices_with_options(
        &self,
        path: &[NodeId],
        use_eco: bool,
        options: &RouteOptions,
    ) -> Vec<usize> {
        let use_surface = self.surface_routing_mode == SurfaceRoutingMode::Car
            && matches!(self.profile, RoutingProfile::Car | RoutingProfile::Truck);
        let mut prev_surface = if use_surface {
            Some(SNAP_VIRTUAL_APPROACH_SURFACE)
        } else {
            None
        };
        let mut out = Vec::with_capacity(path.len().saturating_sub(1));
        for w in path.windows(2) {
            let Some(idx) =
                self.best_edge_index_between(w[0], w[1], prev_surface, use_eco, options)
            else {
                continue;
            };
            if use_surface {
                prev_surface = Some(self.edges[idx].surface_quality);
            }
            out.push(idx);
        }
        out
    }

    /// Fast edge lookup using adjacency (O(degree), not O(edges)).
    ///
    /// When several parallel edges share the same endpoints, returns the cheapest
    /// under default route options (no eco, no surface approach seed).
    pub fn edge_index(&self, from: NodeId, to: NodeId) -> Option<usize> {
        self.best_edge_index_between(from, to, None, false, &RouteOptions::default())
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
        self.incident.contains(&id)
    }

    /// Nearest linked graph node within the profile snap budget.
    ///
    /// Chooses the nearest allowed edge (polyline distance), then its closer
    /// eligible endpoint. Prefers the largest weakly-connected component when a
    /// candidate exists inside the snap budget so farms on leftover private-track
    /// islands snap to the public road network instead of a disconnected
    /// courtyard. Returns [`SnapTooFar`] when the closest linked node exceeds
    /// [`max_waypoint_snap_m`] — callers must treat that as unreachable, not
    /// silently substitute a distant network node.
    ///
    /// Does not apply surface preference (start/destination behaviour). Use
    /// [`Self::nearest_routable_with_options`] with `prefer_better_surface`
    /// for via points.
    pub fn nearest_routable(&self, lat: f64, lon: f64) -> Result<(NodeId, f64), SnapTooFar> {
        self.nearest_routable_with_options(lat, lon, &RouteOptions::default(), false)
    }

    /// Nearest routable node that remains usable under `options` (hard filters).
    ///
    /// Prefer the largest weakly-connected component formed only by edges that
    /// pass [`edge_allowed_for_options`], so avoid-toll / avoid-motorway / ferry
    /// / clearance settings cannot snap onto an island that A* cannot leave.
    ///
    /// Snaps to the **nearest allowed edge** (polyline distance via
    /// [`edge_distance_m`](super::edge_distance_m)), then the closer eligible
    /// endpoint of that edge. Mid-way OSM nodes stay in edge shape (not graph
    /// topology); edge distance is what the snap budget compares against so
    /// addresses beside long ways still resolve within
    /// [`max_waypoint_snap_m`].
    ///
    /// When `prefer_better_surface` is true (intermediate vias only) and the
    /// graph is in car surface mode, among giant-component *nodes* within
    /// [`SURFACE_VIA_SNAP_SLACK_M`] of the literal nearest edge snap, prefer
    /// better `worst_incident_surface` (distance is the tiebreaker). Start and
    /// destination snaps must pass `false` so rural addresses keep the last
    /// metres of gravel driveway instead of jumping to a paved road hundreds
    /// of metres away.
    pub fn nearest_routable_with_options(
        &self,
        lat: f64,
        lon: f64,
        options: &RouteOptions,
        prefer_better_surface: bool,
    ) -> Result<(NodeId, f64), SnapTooFar> {
        self.nearest_routable_with_options_max(
            lat,
            lon,
            options,
            prefer_better_surface,
            max_waypoint_snap_m(self.profile),
        )
    }

    /// Like [`Self::nearest_routable_with_options`], with an explicit snap budget.
    /// Used for long-trip densify hop endpoints (region centroids) that may sit
    /// farther from the network than a user-entered waypoint.
    pub fn nearest_routable_with_options_max(
        &self,
        lat: f64,
        lon: f64,
        options: &RouteOptions,
        prefer_better_surface: bool,
        max_m: f64,
    ) -> Result<(NodeId, f64), SnapTooFar> {
        let role = options.snap_role;
        // Rebuilding Union-Find over a multi-tile Automotive graph (~200k+ nodes)
        // is multi-second work; skip it when RouteOptions do not remove edges.
        let filtered = options_need_filtered_components(options)
            .then(|| self.option_filtered_components(options));
        // Degree pad for a cheap reject before edge distance (Automotive
        // multi-tile graphs are 100k–200k nodes / ~450k edges).
        let pad_deg = (max_m / 100_000.0).max(0.02);
        let endpoint_in_pad = |id: NodeId| -> bool {
            self.nodes.get(&id).is_some_and(|n| {
                (n.coord.y - lat).abs() <= pad_deg && (n.coord.x - lon).abs() <= pad_deg
            })
        };
        let edge_in_pad =
            |e: &GraphEdge| -> bool { endpoint_in_pad(e.source) || endpoint_in_pad(e.target) };
        let in_filtered_giant = |id: NodeId| -> bool {
            match &filtered {
                Some((filtered_root, filtered_giant)) => {
                    match (*filtered_giant, filtered_root.get(&id)) {
                        (Some(giant), Some(root)) => *root == giant,
                        _ => self.in_giant_component(id),
                    }
                }
                None => self.in_giant_component(id),
            }
        };
        // When RouteOptions remove edges (vehicle limits, avoid-*, …), the
        // filtered Union-Find map keys *are* the allowed-incident set — O(1).
        let has_allowed_incident = |id: NodeId| -> bool {
            match &filtered {
                Some((filtered_root, _)) => filtered_root.contains_key(&id),
                None => self.node_has_allowed_incident_unfiltered(id, options),
            }
        };
        let directed_ok = |id: NodeId| -> bool { self.directed_snap_ok(id, role) };
        // Same edge-distance choice as dig (`SnapRole::Any`): nearest polyline,
        // then closer endpoint. Directed filter only rejects an endpoint that
        // fails the role gate — dig's node is kept whenever it is directed-usable.
        let closer_endpoint =
            |e: &GraphEdge, require_giant: bool, require_directed: bool| -> Option<(NodeId, f64)> {
                let mut best: Option<(NodeId, f64)> = None;
                for id in [e.source, e.target] {
                    if !has_allowed_incident(id) {
                        continue;
                    }
                    if require_giant && !in_filtered_giant(id) {
                        continue;
                    }
                    if require_directed && !directed_ok(id) {
                        continue;
                    }
                    let Some(n) = self.nodes.get(&id) else {
                        continue;
                    };
                    let dist = haversine_point_m(lat, lon, n);
                    if best.is_none_or(|(_, d)| dist < d) {
                        best = Some((id, dist));
                    }
                }
                best
            };
        // Collect edges by polyline distance (dig order). Prefer the nearest
        // edge whose closer directed-ok giant endpoint exists; only then walk
        // farther edges. Filter never reorders ahead of a dig-usable node.
        let mut candidates: Vec<(usize, f64)> = Vec::new();
        let mut nearest_any_edge: Option<(usize, f64)> = None;
        for pass in 0..2 {
            if pass == 1 && nearest_any_edge.is_some() {
                break;
            }
            for (idx, e) in self.edges.iter().enumerate() {
                if pass == 0 && !edge_in_pad(e) {
                    continue;
                }
                if !edge_allowed_at(e, idx, options, self.profile) {
                    continue;
                }
                let edge_d = super::edge_distance_m(e, lat, lon);
                if closer_endpoint(e, false, false).is_some()
                    && nearest_any_edge.is_none_or(|(_, d)| edge_d < d)
                {
                    nearest_any_edge = Some((idx, edge_d));
                }
                if edge_d <= max_m && closer_endpoint(e, true, false).is_some() {
                    candidates.push((idx, edge_d));
                }
            }
        }
        let Some((best_any_idx, nearest_edge_m)) = nearest_any_edge else {
            return Err(SnapTooFar {
                nearest_m: f64::INFINITY,
                max_m,
            });
        };
        candidates.sort_by(|a, b| a.1.total_cmp(&b.1));

        let use_surface_snap = prefer_better_surface
            && self.surface_routing_mode == SurfaceRoutingMode::Car
            && matches!(self.profile, RoutingProfile::Car | RoutingProfile::Truck);
        if use_surface_snap {
            // Literal nearest is edge-based; surface preference still compares
            // *nodes* within slack of that snap so a long Good connector edge
            // cannot leap to a paved end outside the slack budget.
            if let Some(&(idx, _)) = candidates.first() {
                if let Some((_, nearest_giant_m)) = closer_endpoint(&self.edges[idx], true, true) {
                    let surface_limit_m = (nearest_giant_m + SURFACE_VIA_SNAP_SLACK_M).min(max_m);
                    let surface_pad = (surface_limit_m / 100_000.0).max(0.02);
                    let mut best_surface_giant: Option<(NodeId, f64)> = None;
                    for (n_id, n) in &self.nodes {
                        if (n.coord.y - lat).abs() > surface_pad
                            || (n.coord.x - lon).abs() > surface_pad
                        {
                            continue;
                        }
                        if !has_allowed_incident(*n_id) || !directed_ok(*n_id) {
                            continue;
                        }
                        let dist = haversine_point_m(lat, lon, n);
                        if dist > surface_limit_m || !in_filtered_giant(*n_id) {
                            continue;
                        }
                        let sq = worst_incident_surface(self, *n_id);
                        let replace = match best_surface_giant {
                            None => true,
                            Some((prev_id, prev_d)) => {
                                let prev_sq = worst_incident_surface(self, prev_id);
                                sq < prev_sq || (sq == prev_sq && dist < prev_d)
                            }
                        };
                        if replace {
                            best_surface_giant = Some((*n_id, dist));
                        }
                    }
                    if let Some((id, dist)) = best_surface_giant {
                        return Ok((id, dist));
                    }
                }
            }
        }

        for (idx, _) in candidates {
            if let Some((id, dist)) = closer_endpoint(&self.edges[idx], true, true) {
                return Ok((id, dist));
            }
        }
        // Nearest edge exists but no directed-usable giant endpoint within budget.
        let Some((id, nearest_m)) = closer_endpoint(&self.edges[best_any_idx], false, false) else {
            return Err(SnapTooFar {
                nearest_m: nearest_edge_m,
                max_m,
            });
        };
        if nearest_m > max_m || !directed_ok(id) {
            return Err(SnapTooFar { nearest_m, max_m });
        }
        Ok((id, nearest_m))
    }

    /// Allowed-incident check when `options` do **not** remove edges vs the
    /// base graph (no vehicle / avoid-* rebuild). O(degree) outgoing, then
    /// O(1) `incident` for one-way sinks — never O(E).
    fn node_has_allowed_incident_unfiltered(&self, id: NodeId, options: &RouteOptions) -> bool {
        if self
            .adjacency
            .get(&id)
            .into_iter()
            .flatten()
            .any(|&idx| edge_allowed_at(&self.edges[idx], idx, options, self.profile))
        {
            return true;
        }
        // Incoming-only sink under unrestricted options: still linked.
        self.incident.contains(&id)
    }

    /// Weak components using only edges allowed under `options`.
    fn option_filtered_components(
        &self,
        options: &RouteOptions,
    ) -> (HashMap<NodeId, NodeId>, Option<NodeId>) {
        let mut parent: HashMap<NodeId, NodeId> = HashMap::new();
        let mut size: HashMap<NodeId, usize> = HashMap::new();
        let mut incident: HashSet<NodeId> = HashSet::new();
        for (idx, edge) in self.edges.iter().enumerate() {
            if !edge_allowed_at(edge, idx, options, self.profile) {
                continue;
            }
            incident.insert(edge.source);
            incident.insert(edge.target);
        }
        for &id in &incident {
            parent.insert(id, id);
            size.insert(id, 1);
        }
        for (idx, edge) in self.edges.iter().enumerate() {
            if !edge_allowed_at(edge, idx, options, self.profile) {
                continue;
            }
            uf_union(&mut parent, &mut size, edge.source, edge.target);
        }
        let mut roots = HashMap::new();
        let mut giant: Option<(NodeId, usize)> = None;
        for &id in &incident {
            let root = uf_find(&mut parent, id);
            roots.insert(id, root);
            let n = size.get(&root).copied().unwrap_or(1);
            if giant.is_none_or(|(_, s)| n > s) {
                giant = Some((root, n));
            }
        }
        (roots, giant.map(|(r, _)| r))
    }

    fn in_giant_component(&self, id: NodeId) -> bool {
        match (self.giant_root, self.component_root.get(&id)) {
            (Some(giant), Some(root)) => *root == giant,
            _ => true,
        }
    }

    /// True when `a` and `b` share a weakly-connected component (undirected edges).
    ///
    /// O(1) after [`Self::rebuild_adjacency`]. Used to detect a disconnected
    /// corridor load (origin/destination on different stems) before A* explores
    /// an entire component for minutes.
    pub fn same_weak_component(&self, a: NodeId, b: NodeId) -> bool {
        match (self.component_root.get(&a), self.component_root.get(&b)) {
            (Some(ra), Some(rb)) => ra == rb,
            _ => false,
        }
    }

    /// Union-find root for diagnostics (`-1` when the node is not in the graph).
    pub fn weak_component_id(&self, id: NodeId) -> i64 {
        self.component_root.get(&id).map(|r| r.0).unwrap_or(-1)
    }

    pub fn node_lat_lon(&self, id: NodeId) -> Option<(f64, f64)> {
        self.nodes.get(&id).map(|n| (n.coord.y, n.coord.x))
    }

    /// True when `goal` is reachable from `start` on directed edges allowed by
    /// `options` (ferries/one-ways/access). Prefer this over
    /// [`Self::same_weak_component`] when deciding whether a ferry overlay is
    /// still needed — undirected UF can report connected while A* cannot travel.
    pub fn directed_reachable_with_options(
        &self,
        start: NodeId,
        goal: NodeId,
        options: &RouteOptions,
    ) -> bool {
        if start == goal {
            return true;
        }
        let mut seen = HashSet::new();
        let mut q = std::collections::VecDeque::new();
        q.push_back(start);
        seen.insert(start);
        while let Some(u) = q.pop_front() {
            for &idx in self.outgoing_edge_indices(u) {
                let e = &self.edges[idx];
                if !edge_allowed_at(e, idx, options, self.profile) {
                    continue;
                }
                let v = e.target;
                if v == goal {
                    return true;
                }
                if seen.insert(v) {
                    q.push_back(v);
                }
            }
        }
        false
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
        let boardwalk_nodes: HashSet<NodeId> = self
            .edges
            .iter()
            .filter(|e| e.is_boardwalk_crossing)
            .flat_map(|e| [e.source, e.target])
            .collect();
        let mut kept = Vec::with_capacity(self.edges.len());
        let mut hard_candidates: Vec<GraphEdge> = Vec::new();
        for mut edge in self.edges.drain(..) {
            let mid_lat = (edge.start_lat + edge.end_lat) * 0.5;
            let mid_lon = (edge.start_lon + edge.end_lon) * 0.5;
            match wetlands.class_at(mid_lat, mid_lon) {
                Some(WetlandClass::HardAvoid) => {
                    if edge.is_boardwalk_crossing {
                        boardwalk_kept += 1;
                        kept.push(edge);
                    } else {
                        hard_candidates.push(edge);
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
        // Soft-penalized bridges keep boardwalk-touched components attached to
        // the giant hiking network when hard-avoid wetlands would otherwise
        // isolate approach edges.
        if !boardwalk_nodes.is_empty() && !hard_candidates.is_empty() {
            let bridges = wetland_boardwalk_bridges(&kept, &hard_candidates, &boardwalk_nodes);
            let mut use_bridge = vec![false; hard_candidates.len()];
            for i in bridges {
                use_bridge[i] = true;
            }
            for (i, mut edge) in hard_candidates.into_iter().enumerate() {
                if use_bridge[i] {
                    soft += 1;
                    edge.base_weight *= WETLAND_SOFT_COST_MULT;
                    if let Some(w) = edge.eco_weight.as_mut() {
                        *w *= WETLAND_SOFT_COST_MULT;
                    }
                    kept.push(edge);
                } else {
                    hard_removed += 1;
                }
            }
        } else {
            hard_removed += hard_candidates.len();
        }
        self.edges = kept;
        self.rebuild_adjacency();
        WetlandApplyStats {
            soft_penalized: soft,
            hard_removed,
            boardwalk_kept,
        }
    }

    /// Rebuild adjacency after hard edge removal (bike suitability, etc.).
    pub fn rebuild_after_edge_filter(&mut self) {
        self.rebuild_adjacency();
    }

    fn rebuild_adjacency(&mut self) {
        self.adjacency.clear();
        self.incident.clear();
        self.component_root.clear();
        self.giant_root = None;
        self.directed_labels_ready = false;
        self.origin_reject.clear();
        self.dest_reject.clear();
        self.directed_giant_nodes = 0;
        for (idx, edge) in self.edges.iter().enumerate() {
            self.adjacency.entry(edge.source).or_default().push(idx);
            self.incident.insert(edge.source);
            self.incident.insert(edge.target);
        }
        self.recompute_weak_components();
        // Directed stubs: [`Self::ensure_directed_snap_labels`] on final corridor.
    }

    /// Compute compact directed snap stubs if missing (idempotent).
    pub fn ensure_directed_snap_labels(&mut self) {
        if self.directed_labels_ready {
            return;
        }
        self.recompute_directed_main_labels();
    }

    /// True after [`Self::ensure_directed_snap_labels`] populated stub sets.
    pub fn directed_labels_ready(&self) -> bool {
        self.directed_labels_ready
    }

    fn recompute_weak_components(&mut self) {
        let mut parent: HashMap<NodeId, NodeId> = HashMap::new();
        let mut size: HashMap<NodeId, usize> = HashMap::new();
        for &id in &self.incident {
            parent.insert(id, id);
            size.insert(id, 1);
        }
        for edge in &self.edges {
            uf_union(&mut parent, &mut size, edge.source, edge.target);
        }
        let mut giant: Option<(NodeId, usize)> = None;
        for &id in &self.incident {
            let root = uf_find(&mut parent, id);
            self.component_root.insert(id, root);
            let n = size.get(&root).copied().unwrap_or(1);
            if giant.is_none_or(|(_, s)| n > s) {
                giant = Some((root, n));
            }
        }
        self.giant_root = giant.map(|(root, _)| root);
    }

    /// Largest SCC inside the giant weak component, then compact directed stubs.
    ///
    /// Stores only *rejected* giant nodes (one-way source/sink stubs), not the
    /// full can-reach / reachable-from sets (~250k NodeIds / tens of MiB).
    /// Temporary working sets use dense `Vec<bool>` indexed by giant order so
    /// Kosaraju does not allocate two ~250k-entry `HashSet<NodeId>` (dig held
    /// none of this; those sets were the ~10 MiB VmHWM bump vs dig).
    fn recompute_directed_main_labels(&mut self) {
        let t0 = std::time::Instant::now();
        self.directed_labels_ready = false;
        self.origin_reject.clear();
        self.dest_reject.clear();
        self.directed_giant_nodes = 0;
        let Some(giant) = self.giant_root else {
            crate::routing::plan_perf::note_u64("directed_label_ms", 0);
            return;
        };
        let giant_nodes: Vec<NodeId> = self
            .incident
            .iter()
            .copied()
            .filter(|id| self.component_root.get(id) == Some(&giant))
            .collect();
        if giant_nodes.is_empty() {
            crate::routing::plan_perf::note_u64("directed_label_ms", 0);
            return;
        }
        let n = giant_nodes.len();
        self.directed_giant_nodes = n;
        // Dense index via sorted pairs + binary search (avoids HashMap of ~N
        // NodeIds that dig never allocated — that map alone was several MiB).
        let mut id_rank: Vec<(NodeId, u32)> = Vec::with_capacity(n);
        for (i, &id) in giant_nodes.iter().enumerate() {
            id_rank.push((id, i as u32));
        }
        id_rank.sort_unstable_by_key(|(id, _)| id.0);
        let idx_of = |id: NodeId| -> Option<u32> {
            id_rank
                .binary_search_by_key(&id.0, |(nid, _)| nid.0)
                .ok()
                .map(|p| id_rank[p].1)
        };
        // Flat CSR reverse adjacency (no per-node Vec header tax).
        let mut rev_deg = vec![0u32; n];
        let mut giant_edge_count = 0usize;
        for e in &self.edges {
            let Some(si) = idx_of(e.source) else {
                continue;
            };
            let Some(ti) = idx_of(e.target) else {
                continue;
            };
            let _ = si;
            rev_deg[ti as usize] = rev_deg[ti as usize].saturating_add(1);
            giant_edge_count = giant_edge_count.saturating_add(1);
        }
        let mut rev_off = vec![0u32; n + 1];
        for i in 0..n {
            rev_off[i + 1] = rev_off[i].saturating_add(rev_deg[i]);
        }
        let mut rev_flat = vec![0u32; giant_edge_count];
        let mut cursor = rev_off.clone();
        for e in &self.edges {
            let Some(si) = idx_of(e.source) else {
                continue;
            };
            let Some(ti) = idx_of(e.target) else {
                continue;
            };
            let slot = cursor[ti as usize] as usize;
            rev_flat[slot] = si;
            cursor[ti as usize] = cursor[ti as usize].saturating_add(1);
        }
        drop(rev_deg);
        drop(cursor);
        let mut visited = vec![false; n];
        let mut order: Vec<u32> = Vec::with_capacity(n);
        let mut stack: Vec<(u32, usize)> = Vec::new();
        for start in 0..n as u32 {
            if visited[start as usize] {
                continue;
            }
            stack.push((start, 0));
            visited[start as usize] = true;
            while let Some((u, ei)) = stack.pop() {
                let uid = giant_nodes[u as usize];
                let outs = self
                    .adjacency
                    .get(&uid)
                    .map(|v| v.as_slice())
                    .unwrap_or(&[]);
                if ei < outs.len() {
                    stack.push((u, ei + 1));
                    let v = self.edges[outs[ei]].target;
                    if let Some(vi) = idx_of(v) {
                        if !visited[vi as usize] {
                            visited[vi as usize] = true;
                            stack.push((vi, 0));
                        }
                    }
                } else {
                    order.push(u);
                }
            }
        }
        visited.fill(false);
        let mut best_scc: Vec<u32> = Vec::new();
        let mut rev_stack: Vec<u32> = Vec::new();
        for &start in order.iter().rev() {
            if visited[start as usize] {
                continue;
            }
            let mut comp: Vec<u32> = Vec::new();
            rev_stack.clear();
            rev_stack.push(start);
            visited[start as usize] = true;
            while let Some(u) = rev_stack.pop() {
                comp.push(u);
                let a = rev_off[u as usize] as usize;
                let b = rev_off[u as usize + 1] as usize;
                for &p in &rev_flat[a..b] {
                    if !visited[p as usize] {
                        visited[p as usize] = true;
                        rev_stack.push(p);
                    }
                }
            }
            if comp.len() > best_scc.len() {
                best_scc = comp;
            }
        }
        if best_scc.is_empty() {
            best_scc = (0..n as u32).collect();
        }
        let mut reach_fwd = vec![false; n];
        let mut reach_rev = vec![false; n];
        let mut q: Vec<u32> = best_scc.clone();
        for &i in &best_scc {
            reach_fwd[i as usize] = true;
        }
        let mut qi = 0usize;
        while qi < q.len() {
            let u = q[qi];
            qi += 1;
            let uid = giant_nodes[u as usize];
            if let Some(outs) = self.adjacency.get(&uid) {
                for &idx in outs {
                    let v = self.edges[idx].target;
                    if let Some(vi) = idx_of(v) {
                        if !reach_fwd[vi as usize] {
                            reach_fwd[vi as usize] = true;
                            q.push(vi);
                        }
                    }
                }
            }
        }
        q.clear();
        q.extend(best_scc.iter().copied());
        for &i in &best_scc {
            reach_rev[i as usize] = true;
        }
        qi = 0;
        while qi < q.len() {
            let u = q[qi];
            qi += 1;
            let a = rev_off[u as usize] as usize;
            let b = rev_off[u as usize + 1] as usize;
            for &p in &rev_flat[a..b] {
                if !reach_rev[p as usize] {
                    reach_rev[p as usize] = true;
                    q.push(p);
                }
            }
        }
        for (i, &id) in giant_nodes.iter().enumerate() {
            if !reach_rev[i] {
                self.origin_reject.insert(id);
            }
            if !reach_fwd[i] {
                self.dest_reject.insert(id);
            }
        }
        drop(reach_fwd);
        drop(reach_rev);
        drop(rev_flat);
        drop(rev_off);
        drop(visited);
        drop(id_rank);
        drop(order);
        drop(best_scc);
        drop(q);
        self.directed_labels_ready = true;
        let ms = t0.elapsed().as_millis() as u64;
        crate::routing::plan_perf::note_u64("directed_label_ms", ms);
        crate::routing::plan_perf::note_u64(
            "directed_origin_reject",
            self.origin_reject.len() as u64,
        );
        crate::routing::plan_perf::note_u64("directed_dest_reject", self.dest_reject.len() as u64);
        // Compact stubs only (~8 B/NodeId); dig held none of this.
        let reject_bytes = (self.origin_reject.len() + self.dest_reject.len())
            .saturating_mul(std::mem::size_of::<NodeId>());
        crate::routing::plan_perf::note_u64("directed_label_bytes", reject_bytes as u64);
    }

    /// True when `id` is usable for `role` in the directed graph.
    pub fn directed_snap_ok(&self, id: NodeId, role: SnapRole) -> bool {
        if !self.directed_labels_ready {
            return true;
        }
        match role {
            SnapRole::Any => true,
            SnapRole::Origin => self.in_giant_component(id) && !self.origin_reject.contains(&id),
            SnapRole::Destination => self.in_giant_component(id) && !self.dest_reject.contains(&id),
            SnapRole::Via => {
                self.in_giant_component(id)
                    && !self.origin_reject.contains(&id)
                    && !self.dest_reject.contains(&id)
            }
        }
    }

    /// Size of the directed component label set used for `role` (diagnostics).
    pub fn directed_label_size(&self, role: SnapRole) -> usize {
        if !self.directed_labels_ready {
            return match role {
                SnapRole::Any => self.incident.len(),
                _ => 0,
            };
        }
        match role {
            SnapRole::Any => self.incident.len(),
            SnapRole::Origin => self
                .directed_giant_nodes
                .saturating_sub(self.origin_reject.len()),
            SnapRole::Destination => self
                .directed_giant_nodes
                .saturating_sub(self.dest_reject.len()),
            SnapRole::Via => self
                .directed_giant_nodes
                .saturating_sub(self.origin_reject.len().max(self.dest_reject.len())),
        }
    }

    /// Unweighted directed reachability (BFS). Used to gate ferry overlay after
    /// directed snaps without paying full A* cost on every warm cache hit.
    pub fn directed_path_exists(&self, start: NodeId, goal: NodeId) -> bool {
        if start == goal {
            return true;
        }
        let mut seen = HashSet::new();
        let mut q = vec![start];
        seen.insert(start);
        let mut qi = 0usize;
        while qi < q.len() {
            let u = q[qi];
            qi += 1;
            if let Some(outs) = self.adjacency.get(&u) {
                for &idx in outs {
                    let v = self.edges[idx].target;
                    if v == goal {
                        return true;
                    }
                    if seen.insert(v) {
                        q.push(v);
                    }
                }
            }
        }
        false
    }

    /// Map overlay polyline (`lon,lat;…`) following each edge’s OSM shape when present.
    pub fn path_overlay_polyline(&self, path: &[NodeId]) -> String {
        self.path_overlay_polyline_with_options(path, false, &RouteOptions::default())
    }

    /// Like [`Self::path_overlay_polyline`] using A*-recorded edge indices (preferred).
    pub fn path_overlay_polyline_from_edges(&self, edge_indices: &[usize]) -> String {
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
        for &idx in edge_indices {
            let e = &self.edges[idx];
            push(e.start_lon, e.start_lat);
            for &(lon, lat) in &e.shape {
                push(lon, lat);
            }
            push(e.end_lon, e.end_lat);
        }
        out
    }

    /// Like [`Self::path_overlay_polyline`] with explicit costing (eco / avoidance).
    /// Prefer [`Self::path_overlay_polyline_from_edges`] when edge indices came from A*.
    pub fn path_overlay_polyline_with_options(
        &self,
        path: &[NodeId],
        use_eco: bool,
        options: &RouteOptions,
    ) -> String {
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
        for idx in self.path_edge_indices_with_options(path, use_eco, options) {
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
        self.path_coords_lat_lon_with_options(path, false, &RouteOptions::default())
    }

    /// Like [`Self::path_coords_lat_lon`] using A*-recorded edge indices (preferred).
    pub fn path_coords_lat_lon_from_edges(&self, edge_indices: &[usize]) -> Vec<(f64, f64)> {
        let mut out = Vec::new();
        for &idx in edge_indices {
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

    /// Like [`Self::path_coords_lat_lon`] with explicit costing (eco / avoidance).
    /// Prefer [`Self::path_coords_lat_lon_from_edges`] when edge indices came from A*.
    pub fn path_coords_lat_lon_with_options(
        &self,
        path: &[NodeId],
        use_eco: bool,
        options: &RouteOptions,
    ) -> Vec<(f64, f64)> {
        let mut out = Vec::new();
        for idx in self.path_edge_indices_with_options(path, use_eco, options) {
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
    ) -> Option<(Vec<NodeId>, Vec<usize>, f64)> {
        self.shortest_path_with_options(start, goal, use_eco, &RouteOptions::default())
    }

    pub fn shortest_path_with_options(
        &self,
        start: NodeId,
        goal: NodeId,
        use_eco: bool,
        options: &RouteOptions,
    ) -> Option<(Vec<NodeId>, Vec<usize>, f64)> {
        self.shortest_path_with_options_stats(start, goal, use_eco, options)
            .path
    }

    /// Like [`Self::shortest_path_with_options`] with expansion count and terminate reason.
    pub fn shortest_path_with_options_stats(
        &self,
        start: NodeId,
        goal: NodeId,
        use_eco: bool,
        options: &RouteOptions,
    ) -> PathSearchStats {
        let stats = self.shortest_path_with_options_stats_raw(start, goal, use_eco, options);
        self.reclassify_outside_countries(start, goal, use_eco, options, stats)
    }

    /// When a country filter disconnects A* but an unrestricted search finds a
    /// path, surface `outside_countries` so callers can emit
    /// [`crate::pack_server::RegionPlanError::NoRouteInsideCountries`].
    fn reclassify_outside_countries(
        &self,
        start: NodeId,
        goal: NodeId,
        use_eco: bool,
        options: &RouteOptions,
        stats: PathSearchStats,
    ) -> PathSearchStats {
        if stats.path.is_some() || stats.terminate_reason != "disconnected" {
            return stats;
        }
        if options.allowed_countries.is_none() {
            return stats;
        }
        let mut open = options.clone();
        open.allowed_countries = None;
        let alt = self.shortest_path_with_options_stats_raw(start, goal, use_eco, &open);
        if alt.path.is_some() {
            PathSearchStats {
                path: None,
                expansions: stats.expansions,
                terminate_reason: "outside_countries",
            }
        } else {
            stats
        }
    }

    fn shortest_path_with_options_stats_raw(
        &self,
        start: NodeId,
        goal: NodeId,
        use_eco: bool,
        options: &RouteOptions,
    ) -> PathSearchStats {
        let plan_id = crate::download::plan_cancel::current_plan_id();
        let expansions = std::sync::atomic::AtomicU64::new(0);
        let surface_mode = options
            .surface_routing_mode
            .unwrap_or(self.surface_routing_mode);
        let use_surface_transitions = surface_mode == SurfaceRoutingMode::Car
            && matches!(self.profile, RoutingProfile::Car | RoutingProfile::Truck);
        // Soft multipliers are ≥ 1. Heuristic uses min(cost/endpoint_chord) so it
        // stays admissible on real OSM (≈1.0×haversine) and on synthetic fixtures.
        let heuristic_per_m = self.astar_heuristic_cost_per_metre(use_eco, options);

        if use_surface_transitions {
            let result = astar(
                &(start, Some(SNAP_VIRTUAL_APPROACH_SURFACE), NO_INCOMING_EDGE),
                |state| {
                    let (node, prev_surface, _) = *state;
                    let n = expansions.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if self.nodes.len() < 15_000 && n > 500_000 {
                        return Vec::new();
                    }
                    if plan_id != 0
                        && n & 2047 == 0
                        && crate::download::plan_cancel::is_cancelled_id(plan_id)
                    {
                        return Vec::new();
                    }
                    if self.access_blocked_nodes.contains(&node) && node != start {
                        return Vec::new();
                    }
                    self.adjacency
                        .get(&node)
                        .into_iter()
                        .flatten()
                        .filter_map(|&edge_idx| {
                            let edge = &self.edges[edge_idx];
                            if !edge_allowed_at(edge, edge_idx, options, self.profile) {
                                return None;
                            }
                            let base = edge_travel_cost(edge, edge_idx, use_eco, options);
                            let transition = surface_transition_cost_m(
                                prev_surface,
                                edge.surface_quality,
                                surface_mode,
                            );
                            let cost = cost_to_u64(base + transition);
                            Some(((edge.target, Some(edge.surface_quality), edge_idx), cost))
                        })
                        .collect::<Vec<_>>()
                },
                |state| {
                    cost_to_u64(
                        self.nodes
                            .get(&state.0)
                            .and_then(|n| self.nodes.get(&goal).map(|g| haversine_m(n, g)))
                            .unwrap_or(0.0)
                            * heuristic_per_m,
                    )
                },
                |state| state.0 == goal,
            );
            let expansions = expansions.load(std::sync::atomic::Ordering::Relaxed);
            if crate::download::plan_cancel::is_cancelled_id(plan_id) {
                return PathSearchStats {
                    path: None,
                    expansions,
                    terminate_reason: "cancelled",
                };
            }
            let path = result.map(|(path, cost)| decode_recorded_path(path, cost));
            return PathSearchStats {
                terminate_reason: if path.is_some() {
                    "found"
                } else {
                    "disconnected"
                },
                path,
                expansions,
            };
        }

        let result = astar(
            &(start, NO_INCOMING_EDGE),
            |state| {
                let (node, _) = *state;
                let n = expansions.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if self.nodes.len() < 15_000 && n > 500_000 {
                    return Vec::new();
                }
                if plan_id != 0
                    && n & 2047 == 0
                    && crate::download::plan_cancel::is_cancelled_id(plan_id)
                {
                    return Vec::new();
                }
                if self.access_blocked_nodes.contains(&node) && node != start {
                    return Vec::new();
                }
                self.adjacency
                    .get(&node)
                    .into_iter()
                    .flatten()
                    .filter_map(|&edge_idx| {
                        let edge = &self.edges[edge_idx];
                        if !edge_allowed_at(edge, edge_idx, options, self.profile) {
                            return None;
                        }
                        let cost = edge_travel_cost(edge, edge_idx, use_eco, options);
                        Some(((edge.target, edge_idx), cost_to_u64(cost)))
                    })
                    .collect::<Vec<_>>()
            },
            |state| {
                cost_to_u64(
                    self.nodes
                        .get(&state.0)
                        .and_then(|n| self.nodes.get(&goal).map(|g| haversine_m(n, g)))
                        .unwrap_or(0.0)
                        * heuristic_per_m,
                )
            },
            |(node, _)| *node == goal,
        );
        let expansions = expansions.load(std::sync::atomic::Ordering::Relaxed);
        if crate::download::plan_cancel::is_cancelled_id(plan_id) {
            return PathSearchStats {
                path: None,
                expansions,
                terminate_reason: "cancelled",
            };
        }
        let path = result.map(|(path, cost)| decode_recorded_path_simple(path, cost));
        PathSearchStats {
            terminate_reason: if path.is_some() {
                "found"
            } else {
                "disconnected"
            },
            path,
            expansions,
        }
    }

    /// Admissible A* scale: cost units per metre of great-circle remainder.
    ///
    /// Uses `min(edge_cost / endpoint_chord_m)` so the heuristic stays admissible
    /// when `length_m` is synthetic or otherwise shorter than the geographic chord
    /// (fixtures), while remaining ~1.0×haversine on real OSM length weights and
    /// correctly scaled for eco joule costs.
    fn astar_heuristic_cost_per_metre(&self, use_eco: bool, options: &RouteOptions) -> f64 {
        let mut min_ratio = f64::INFINITY;
        for (idx, edge) in self.edges.iter().enumerate() {
            let chord =
                haversine_latlon_m(edge.start_lat, edge.start_lon, edge.end_lat, edge.end_lon);
            if chord < 1.0 {
                continue;
            }
            // Ignore toll/tunnel/datex penalties here (same as before); include
            // eco overlay + motor soft so the heuristic stays admissible.
            let cost = edge_travel_cost(
                edge,
                idx,
                use_eco,
                &RouteOptions {
                    toll_policy: crate::routing::toll::TollPolicy::Allow,
                    avoid_tunnels: false,
                    datex_impacts: Vec::new(),
                    eco_weights: options.eco_weights.clone(),
                    motor_soft: options.motor_soft,
                    ..RouteOptions::default()
                },
            );
            if cost.is_finite() && cost >= 0.0 {
                min_ratio = min_ratio.min(cost / chord);
            }
        }
        if min_ratio.is_finite() && min_ratio > 0.0 {
            min_ratio
        } else {
            1.0
        }
    }

    /// Count edges on a path that would be excluded by vehicle/avoidance options.
    pub fn restricted_edge_count(&self, edge_indices: &[usize], options: &RouteOptions) -> usize {
        edge_indices
            .iter()
            .filter(|&&i| !edge_allowed_at(&self.edges[i], i, options, self.profile))
            .count()
    }

    /// True when any edge on the path is flagged as toll.
    pub fn path_uses_tolls(&self, edge_indices: &[usize]) -> bool {
        edge_indices.iter().any(|&i| self.edges[i].is_toll)
    }

    /// True when any edge on the path is a ferry.
    pub fn path_uses_ferries(&self, edge_indices: &[usize]) -> bool {
        edge_indices.iter().any(|&i| self.edges[i].is_ferry)
    }

    /// Contiguous ferry-edge runs on the path: `(label, length_m)`.
    /// Label prefers OSM `name`, else `ref`, else `unnamed`.
    pub fn path_ferry_legs(&self, edge_indices: &[usize]) -> Vec<(String, f64)> {
        let mut legs: Vec<(String, f64)> = Vec::new();
        let mut cur_label: Option<String> = None;
        let mut cur_m = 0.0_f64;
        let flush = |legs: &mut Vec<(String, f64)>, label: &mut Option<String>, m: &mut f64| {
            if let Some(l) = label.take() {
                if *m > 0.0 {
                    legs.push((l, *m));
                }
            }
            *m = 0.0;
        };
        for &i in edge_indices {
            let e = &self.edges[i];
            if !e.is_ferry {
                flush(&mut legs, &mut cur_label, &mut cur_m);
                continue;
            }
            let label = e
                .name
                .as_deref()
                .or(e.road_ref.as_deref())
                .unwrap_or("unnamed")
                .to_string();
            match cur_label.as_ref() {
                Some(l) if l == &label => cur_m += e.length_m,
                Some(_) => {
                    flush(&mut legs, &mut cur_label, &mut cur_m);
                    cur_label = Some(label);
                    cur_m = e.length_m;
                }
                None => {
                    cur_label = Some(label);
                    cur_m = e.length_m;
                }
            }
        }
        flush(&mut legs, &mut cur_label, &mut cur_m);
        legs
    }

    /// Contiguous tunnel-edge runs on the path: `(label, length_m)`.
    /// Label prefers OSM `name`, else `ref`, else `unnamed`.
    pub fn path_tunnel_legs(&self, edge_indices: &[usize]) -> Vec<(String, f64)> {
        let mut legs: Vec<(String, f64)> = Vec::new();
        let mut cur_label: Option<String> = None;
        let mut cur_m = 0.0_f64;
        let flush = |legs: &mut Vec<(String, f64)>, label: &mut Option<String>, m: &mut f64| {
            if let Some(l) = label.take() {
                if *m > 0.0 {
                    legs.push((l, *m));
                }
            }
            *m = 0.0;
        };
        for &i in edge_indices {
            let e = &self.edges[i];
            if !e.is_tunnel {
                flush(&mut legs, &mut cur_label, &mut cur_m);
                continue;
            }
            let label = e
                .name
                .as_deref()
                .or(e.road_ref.as_deref())
                .unwrap_or("unnamed")
                .to_string();
            match cur_label.as_ref() {
                Some(l) if l == &label => cur_m += e.length_m,
                Some(_) => {
                    flush(&mut legs, &mut cur_label, &mut cur_m);
                    cur_label = Some(label);
                    cur_m = e.length_m;
                }
                None => {
                    cur_label = Some(label);
                    cur_m = e.length_m;
                }
            }
        }
        flush(&mut legs, &mut cur_label, &mut cur_m);
        legs
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
        self.motorway_share_pct_with_options(path, false, &RouteOptions::default())
    }

    /// Like [`Self::motorway_share_pct`] with explicit costing (eco / avoidance).
    pub fn motorway_share_pct_with_options(
        &self,
        path: &[NodeId],
        use_eco: bool,
        options: &RouteOptions,
    ) -> f64 {
        let mut total_m = 0.0;
        let mut motorway_m = 0.0;
        for idx in self.path_edge_indices_with_options(path, use_eco, options) {
            let e = &self.edges[idx];
            let len = e.length_m.max(0.0);
            total_m += len;
            if edge_is_motorway_grade(e) {
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

    /// Statistics on directed node pairs with more than one graph edge (parallel edges).
    pub fn parallel_edge_census(&self) -> ParallelEdgeCensus {
        let mut by_pair: HashMap<(NodeId, NodeId), Vec<usize>> = HashMap::new();
        for (idx, edge) in self.edges.iter().enumerate() {
            by_pair
                .entry((edge.source, edge.target))
                .or_default()
                .push(idx);
        }
        let mut census = ParallelEdgeCensus {
            total_directed_edges: self.edges.len(),
            ..Default::default()
        };
        for ((from, to), indices) in &by_pair {
            if indices.len() < 2 {
                continue;
            }
            census.parallel_directed_pairs += 1;
            census.extra_parallel_edges += indices.len() - 1;
            let first_adj = self
                .adjacency
                .get(from)
                .and_then(|adj| adj.iter().copied().find(|&i| self.edges[i].target == *to));
            let cheapest = indices.iter().copied().min_by(|&a, &b| {
                self.edges[a]
                    .base_weight
                    .partial_cmp(&self.edges[b].base_weight)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            if first_adj != cheapest {
                census.old_edge_index_would_mismatch += 1;
            }
            let hw_set: HashSet<_> = indices
                .iter()
                .filter_map(|&i| self.edges[i].highway.as_deref())
                .collect();
            if hw_set.len() >= 2 {
                census.mixed_highway_class_pairs += 1;
            }
        }
        census
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
        match options.toll_policy {
            crate::routing::toll::TollPolicy::Allow => "OFF",
            crate::routing::toll::TollPolicy::Penalize => "ON (penalize)",
            crate::routing::toll::TollPolicy::NeverUse => "ON (never use)",
        }
    ));
    lines.push(format!(
        "Avoid ferries: {}",
        if options.avoid_ferries { "ON" } else { "OFF" }
    ));
    lines.push(format!(
        "Avoid tunnels: {}",
        if options.avoid_tunnels {
            "ON (penalize)"
        } else {
            "OFF"
        }
    ));
    let datex_blocks = options
        .datex_impacts
        .iter()
        .filter(|c| c.impact == crate::datex::DatexImpact::Block)
        .count();
    let datex_penalize = options
        .datex_impacts
        .iter()
        .filter(|c| c.impact == crate::datex::DatexImpact::Penalize)
        .count();
    lines.push(format!(
        "DATEX impacts: {datex_blocks} block, {datex_penalize} penalize"
    ));
    if options.vehicle.is_some() {
        lines.push(format!(
            "Route avoids {avoided_on_reference} weight/height/width/length-restricted segments (vs unrestricted reference)"
        ));
    }
    lines.push(format!(
        "Non-motorway road share on last plan: {priority_path_share_pct_hint:.1}% (100% minus motorway-grade length)"
    ));
    lines.join("\n")
}

/// Append seasonal-closure counters to an avoidance report (plan-time).
pub fn append_seasonal_closure_report(report: &str, excluded_edges: usize) -> String {
    format!("{report}\nseasonal_closure_excluded_edges={excluded_edges}")
}

/// Tags extracted once per OSM way and copied onto each directed [`GraphEdge`].
#[derive(Debug, Clone)]
struct EdgeMeta {
    highway: Option<String>,
    maxspeed_kmh: Option<f64>,
    maxspeed_practical_kmh: Option<f64>,
    maxspeed_advisory_kmh: Option<f64>,
    maxspeed_type: Option<String>,
    maxspeed_variable: bool,
    minspeed_kmh: Option<f64>,
    name: Option<String>,
    road_ref: Option<String>,
    maxweight_t: Option<f64>,
    maxaxleload_t: Option<f64>,
    maxbogieweight_t: Option<f64>,
    maxheight_m: Option<f64>,
    maxwidth_m: Option<f64>,
    maxlength_m: Option<f64>,
    is_toll: bool,
    is_ferry: bool,
    /// OSM `duration` on ferry ways (H:MM / HH:MM:SS), when present.
    ferry_duration: Option<String>,
    is_tunnel: bool,
    is_boardwalk_crossing: bool,
    is_roundabout: bool,
    motor_vehicle_conditional: Option<String>,
    access_conditional: Option<String>,
    maxspeed_conditional: Option<String>,
    access_forbidden: bool,
    is_motorroad: bool,
    is_expressway: bool,
    is_oneway: bool,
    lanes: Option<u8>,
    surface_quality: SurfaceQuality,
}

fn edge_meta(edge: &Edge, profile: RoutingProfile) -> EdgeMeta {
    let highway = edge.tags.get("highway").cloned();
    let maxspeed_kmh = edge
        .tags
        .get("maxspeed")
        .and_then(|s| crate::routing::eta::parse_maxspeed_kmh(s));
    let maxspeed_practical_kmh = edge
        .tags
        .get("maxspeed:practical")
        .and_then(|s| crate::routing::eta::parse_maxspeed_kmh(s));
    let maxspeed_advisory_kmh = edge
        .tags
        .get("maxspeed:advisory")
        .and_then(|s| crate::routing::eta::parse_maxspeed_kmh(s));
    let maxspeed_type = edge.tags.get("maxspeed:type").cloned();
    let maxspeed_variable = edge
        .tags
        .get("maxspeed:variable")
        .is_some_and(|s| is_truthy_tag(s));
    let minspeed_kmh = edge
        .tags
        .get("minspeed")
        .and_then(|s| crate::routing::eta::parse_maxspeed_kmh(s));
    let name = edge.tags.get("name").cloned();
    let road_ref = combine_osm_road_refs(
        edge.tags.get("ref").cloned(),
        edge.tags.get("int_ref").cloned(),
    );
    let is_motorroad = edge.tags.get("motorroad").is_some_and(|s| is_truthy_tag(s));
    let is_expressway = edge
        .tags
        .get("expressway")
        .is_some_and(|s| is_truthy_tag(s));
    let is_oneway = edge
        .tags
        .get("oneway")
        .is_some_and(|s| is_oneway_yes_tag(s));
    let lanes = edge.tags.get("lanes").and_then(|s| parse_lanes_tag(s));
    let maxweight_t = edge.tags.get("maxweight").and_then(|s| parse_metric(s));
    let maxaxleload_t = edge.tags.get("maxaxleload").and_then(|s| parse_metric(s));
    let maxbogieweight_t = edge
        .tags
        .get("maxbogieweight")
        .and_then(|s| parse_metric(s));
    let maxheight_m = edge.tags.get("maxheight").and_then(|s| parse_metric(s));
    let maxwidth_m = edge.tags.get("maxwidth").and_then(|s| parse_metric(s));
    let maxlength_m = edge.tags.get("maxlength").and_then(|s| parse_metric(s));
    let is_toll = crate::routing::toll::toll_applies_for_profile(profile, |k| {
        edge.tags.get(k).map(String::as_str)
    });
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
    let ferry_duration = if is_ferry {
        edge.tags.get("duration").cloned()
    } else {
        None
    };
    let is_tunnel = edge.tags.get("tunnel").is_some_and(|s| is_tunnel_tag(s));
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
    let surface_quality = classify_surface_tags(
        highway.as_deref(),
        edge.tags.get("surface").map(String::as_str),
        edge.tags.get("tracktype").map(String::as_str),
    );
    EdgeMeta {
        highway,
        maxspeed_kmh,
        maxspeed_practical_kmh,
        maxspeed_advisory_kmh,
        maxspeed_type,
        maxspeed_variable,
        minspeed_kmh,
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
        ferry_duration,
        is_tunnel,
        is_boardwalk_crossing,
        is_roundabout,
        motor_vehicle_conditional,
        access_conditional,
        maxspeed_conditional,
        access_forbidden,
        is_motorroad,
        is_expressway,
        is_oneway,
        lanes,
        surface_quality,
    }
}

pub(crate) fn parse_lanes_tag(raw: &str) -> Option<u8> {
    raw.split(|c: char| !c.is_ascii_digit())
        .find(|s| !s.is_empty())
        .and_then(|s| s.parse::<u8>().ok())
}

pub(crate) fn is_oneway_yes_tag(raw: &str) -> bool {
    matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "yes" | "true" | "1"
    )
}

pub(crate) fn is_truthy_tag(raw: &str) -> bool {
    matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "yes" | "true" | "1" | "toll"
    )
}

/// OSM `tunnel=*` — any non-empty value other than `no` (covers `yes`,
/// `building_passage`, `culvert`, …).
pub(crate) fn is_tunnel_tag(raw: &str) -> bool {
    let t = raw.trim();
    !t.is_empty() && !t.eq_ignore_ascii_case("no")
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
    let base_weight = if meta.is_ferry {
        ferry_base_weight_m(length_m, meta.ferry_duration.as_deref(), graph.profile)
    } else {
        length_m
    };
    graph.edges.push(GraphEdge {
        id,
        source,
        target,
        length_m,
        base_weight,
        eco_weight: None,
        start_lat,
        start_lon,
        end_lat,
        end_lon,
        shape,
        highway: meta.highway.clone(),
        maxspeed_kmh: meta.maxspeed_kmh,
        maxspeed_practical_kmh: meta.maxspeed_practical_kmh,
        maxspeed_advisory_kmh: meta.maxspeed_advisory_kmh,
        maxspeed_type: meta.maxspeed_type.clone(),
        maxspeed_variable: meta.maxspeed_variable,
        minspeed_kmh: meta.minspeed_kmh,
        name: meta.name.clone(),
        road_ref: meta.road_ref.clone(),
        is_motorroad: meta.is_motorroad,
        is_expressway: meta.is_expressway,
        is_oneway: meta.is_oneway,
        lanes: meta.lanes,
        maxweight_t: meta.maxweight_t,
        maxaxleload_t: meta.maxaxleload_t,
        maxbogieweight_t: meta.maxbogieweight_t,
        maxheight_m: meta.maxheight_m,
        maxwidth_m: meta.maxwidth_m,
        maxlength_m: meta.maxlength_m,
        is_toll: meta.is_toll,
        is_ferry: meta.is_ferry,
        is_tunnel: meta.is_tunnel,
        is_boardwalk_crossing: meta.is_boardwalk_crossing,
        is_roundabout: meta.is_roundabout,
        motor_vehicle_conditional: meta.motor_vehicle_conditional.clone(),
        access_conditional: meta.access_conditional.clone(),
        maxspeed_conditional: meta.maxspeed_conditional.clone(),
        access_forbidden: meta.access_forbidden,
        surface_quality: meta.surface_quality,
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

/// Join OSM `ref` and `int_ref` for display / guidance (not avoidance).
pub(crate) fn combine_osm_road_refs(
    osm_ref: Option<String>,
    int_ref: Option<String>,
) -> Option<String> {
    let trim_nonempty = |s: String| {
        let t = s.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    };
    match (
        osm_ref.and_then(trim_nonempty),
        int_ref.and_then(trim_nonempty),
    ) {
        (Some(a), Some(b)) if !a.eq_ignore_ascii_case(&b) => Some(format!("{a};{b}")),
        (a, b) => a.or(b),
    }
}

/// Motorway-grade: OSM motorway class, motortrafikkvei/expressway, or dual+fast.
pub fn edge_is_motorway_grade(edge: &GraphEdge) -> bool {
    motorway_grade_from_parts(
        edge.highway.as_deref(),
        edge.is_motorroad,
        edge.is_expressway,
        edge.is_oneway,
        edge.lanes,
        edge.maxspeed_kmh,
    )
}

pub(crate) fn motorway_grade_from_parts(
    highway: Option<&str>,
    is_motorroad: bool,
    is_expressway: bool,
    is_oneway: bool,
    lanes: Option<u8>,
    maxspeed_kmh: Option<f64>,
) -> bool {
    if highway_is_motorway(highway) {
        return true;
    }
    if is_motorroad || is_expressway {
        return true;
    }
    is_oneway && lanes.unwrap_or(0) >= 2 && maxspeed_kmh.is_some_and(|v| v >= 90.0)
}

/// Foot and bicycle graphs must never use motorway-grade edges (illegal / unsuitable).
pub fn profile_locks_avoid_motorways(profile: RoutingProfile) -> bool {
    matches!(profile, RoutingProfile::Foot | RoutingProfile::Bicycle)
}

fn edge_avoided_as_motorway(
    edge: &GraphEdge,
    options: &RouteOptions,
    profile: RoutingProfile,
) -> bool {
    let avoid = options.avoid_motorways || profile_locks_avoid_motorways(profile);
    avoid && edge_is_motorway_grade(edge)
}

fn edge_hit_by_datex(edge: &GraphEdge, c: &crate::datex::DatexPlannerConstraint) -> bool {
    crate::routing::graph::edge_distance_m(edge, c.lat, c.lon) <= c.radius_m
}

fn datex_cell(lat: f64, lon: f64, cell_deg: f64) -> (i32, i32) {
    (
        (lat / cell_deg).floor() as i32,
        (lon / cell_deg).floor() as i32,
    )
}

fn edge_latlon_bbox(e: &GraphEdge) -> (f64, f64, f64, f64) {
    let mut min_lat = e.start_lat.min(e.end_lat);
    let mut max_lat = e.start_lat.max(e.end_lat);
    let mut min_lon = e.start_lon.min(e.end_lon);
    let mut max_lon = e.start_lon.max(e.end_lon);
    for &(lon, lat) in &e.shape {
        min_lat = min_lat.min(lat);
        max_lat = max_lat.max(lat);
        min_lon = min_lon.min(lon);
        max_lon = max_lon.max(lon);
    }
    (min_lat, max_lat, min_lon, max_lon)
}

fn edge_blocked_by_datex(
    edge: &GraphEdge,
    options: &RouteOptions,
    edge_idx: Option<usize>,
) -> bool {
    if let (Some(bits), Some(i)) = (options.datex_edge_block.as_ref(), edge_idx) {
        return bits.get(i).copied().unwrap_or(0) != 0;
    }
    options
        .datex_impacts
        .iter()
        .any(|c| c.impact == crate::datex::DatexImpact::Block && edge_hit_by_datex(edge, c))
}

/// Strongest Penalize multiplier among DATEX constraints that hit this edge.
fn datex_penalize_extra_m(
    edge: &GraphEdge,
    options: &RouteOptions,
    edge_idx: usize,
) -> Option<f64> {
    if let Some(pen) = options.datex_edge_penalize.as_ref() {
        let m = *pen.get(edge_idx)?;
        return if m > 1e-9 { Some(m) } else { None };
    }
    options
        .datex_impacts
        .iter()
        .filter(|c| c.impact == crate::datex::DatexImpact::Penalize && edge_hit_by_datex(edge, c))
        .map(|c| (c.penalty_minutes.max(0.0) * 80_000.0 / 60.0).max(0.0))
        .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .filter(|m| *m > 1e-9)
}

/// True when `options` can remove edges vs an unrestricted car graph. Snap then
/// rebuilds Union-Find; skip that O(E) pass when nothing is filtered.
fn options_need_filtered_components(options: &RouteOptions) -> bool {
    options.avoid_motorways
        || options.avoid_ferries
        || options.avoid_tunnels
        || options.toll_policy != crate::routing::toll::TollPolicy::Allow
        || options.vehicle.is_some()
        || options
            .datex_edge_block
            .as_ref()
            .is_some_and(|b| b.iter().any(|&v| v != 0))
        || (options.datex_edge_block.is_none() && !options.datex_impacts.is_empty())
        || options.allowed_countries.is_some()
        || options.departure_local.is_some()
}

fn edge_allowed_at(
    edge: &GraphEdge,
    edge_idx: usize,
    options: &RouteOptions,
    profile: RoutingProfile,
) -> bool {
    edge_allowed_core(edge, options, profile, Some(edge_idx))
}

fn edge_allowed_for_options(
    edge: &GraphEdge,
    options: &RouteOptions,
    profile: RoutingProfile,
) -> bool {
    edge_allowed_core(edge, options, profile, None)
}

fn edge_allowed_core(
    edge: &GraphEdge,
    options: &RouteOptions,
    profile: RoutingProfile,
    edge_idx: Option<usize>,
) -> bool {
    if edge.access_forbidden {
        return false;
    }
    if edge_avoided_as_motorway(edge, options, profile) {
        return false;
    }
    if options.toll_policy == crate::routing::toll::TollPolicy::NeverUse && edge.is_toll {
        return false;
    }
    if edge_blocked_by_datex(edge, options, edge_idx) {
        return false;
    }
    if options.avoid_ferries && edge.is_ferry {
        return false;
    }
    if let Some(ref allowed) = options.allowed_countries {
        if !edge_in_allowed_countries(edge, allowed) {
            return false;
        }
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

/// Hard country filter: start, midpoint and end must all resolve to an allowed
/// ISO code (semantic change from midpoint-only).
///
/// Attribution uses [`crate::routing::elevation::country_iso_at`] (Natural Earth
/// Admin-0 polygons), not pack stem / catalog path — Norwegian extract bboxes
/// and Geofabrik clips both spill past the border (see
/// `ostlandet_catalog_bbox_spills_into_sweden` and the Langflon spill probe).
/// Unresolved points (`None` after coastal snap) are excluded when the filter
/// is active, as unknown codes were under the old midpoint rule.
fn edge_in_allowed_countries(edge: &GraphEdge, allowed: &[String]) -> bool {
    if allowed.is_empty() {
        return false;
    }
    let mid_lat = (edge.start_lat + edge.end_lat) * 0.5;
    let mid_lon = (edge.start_lon + edge.end_lon) * 0.5;
    // Midpoint first: matches the old hot path and rejects cross-border edges early.
    let points = [
        (mid_lat, mid_lon),
        (edge.start_lat, edge.start_lon),
        (edge.end_lat, edge.end_lon),
    ];
    for (lat, lon) in points {
        let Some(iso) = crate::routing::elevation::country_iso_at(lat, lon) else {
            return false;
        };
        if !allowed.iter().any(|c| c.trim().eq_ignore_ascii_case(iso)) {
            return false;
        }
    }
    true
}

fn edge_travel_cost(
    edge: &GraphEdge,
    edge_idx: usize,
    use_eco: bool,
    options: &RouteOptions,
) -> f64 {
    let mut cost = if use_eco {
        options
            .eco_weights
            .as_ref()
            .and_then(|w| w.get(edge_idx).copied())
            .or(edge.eco_weight)
            .unwrap_or(edge.base_weight)
    } else {
        edge.base_weight
    };
    if let Some((mode, profile)) = options.motor_soft {
        let mult = crate::routing::graph::edge_motor_soft_multiplier(edge, mode, profile);
        if mult > 1.0 + 1e-9 {
            cost *= mult;
        }
    }
    if options.toll_policy == crate::routing::toll::TollPolicy::Penalize && edge.is_toll {
        cost *= crate::routing::toll::TOLL_AVOID_PENALTY_MULT;
    }
    if options.avoid_tunnels && edge.is_tunnel {
        cost *= crate::routing::toll::TUNNEL_AVOID_PENALTY_MULT;
    }
    if let Some(extra) = datex_penalize_extra_m(edge, options, edge_idx) {
        cost += extra;
    }
    cost
}

fn decode_recorded_path(
    path: Vec<(NodeId, Option<SurfaceQuality>, usize)>,
    cost: u64,
) -> (Vec<NodeId>, Vec<usize>, f64) {
    let nodes: Vec<NodeId> = path.iter().map(|(n, _, _)| *n).collect();
    let edges: Vec<usize> = path.iter().skip(1).map(|(_, _, e)| *e).collect();
    (nodes, edges, cost as f64 / 1000.0)
}

fn decode_recorded_path_simple(
    path: Vec<(NodeId, usize)>,
    cost: u64,
) -> (Vec<NodeId>, Vec<usize>, f64) {
    let nodes: Vec<NodeId> = path.iter().map(|(n, _)| *n).collect();
    let edges: Vec<usize> = path.iter().skip(1).map(|(_, e)| *e).collect();
    (nodes, edges, cost as f64 / 1000.0)
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
    // Mandated minimum speed: foot/bicycle cannot legally use the way.
    if matches!(profile, RoutingProfile::Foot | RoutingProfile::Bicycle)
        && edge
            .tags
            .get("minspeed")
            .and_then(|s| crate::routing::eta::parse_maxspeed_kmh(s))
            .is_some()
    {
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

fn uf_find(parent: &mut HashMap<NodeId, NodeId>, x: NodeId) -> NodeId {
    let mut root = x;
    loop {
        let p = *parent.get(&root).unwrap_or(&root);
        if p == root {
            break;
        }
        root = p;
    }
    let mut cur = x;
    while cur != root {
        let next = *parent.get(&cur).unwrap_or(&cur);
        parent.insert(cur, root);
        cur = next;
    }
    parent.entry(x).or_insert(root);
    root
}

fn uf_union(
    parent: &mut HashMap<NodeId, NodeId>,
    size: &mut HashMap<NodeId, usize>,
    a: NodeId,
    b: NodeId,
) {
    let ra = uf_find(parent, a);
    let rb = uf_find(parent, b);
    if ra == rb {
        return;
    }
    let sa = size.get(&ra).copied().unwrap_or(1);
    let sb = size.get(&rb).copied().unwrap_or(1);
    let (keep, drop, new_size) = if sa >= sb {
        (ra, rb, sa + sb)
    } else {
        (rb, ra, sa + sb)
    };
    parent.insert(drop, keep);
    size.insert(keep, new_size);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::HIKING_MAX_WAYPOINT_SNAP_M;
    use crate::routing::graph::{apply_surface_preference, MotorSoftCostProfile};
    use geo_types::Coord;

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
            surface_quality: SurfaceQuality::Good,
        }];
        RouteGraph::from_parts(nodes, edges, RoutingProfile::Foot)
    }

    #[test]
    fn profile_mapping() {
        assert_eq!(RoutingProfile::from(Profile::Hiking), RoutingProfile::Foot);
    }

    #[test]
    fn datex_grid_bind_hits_nearby_edge_and_skips_far_impacts() {
        let g = two_node_foot_graph();
        let mut far = Vec::new();
        for i in 0..200 {
            far.push(crate::datex::DatexPlannerConstraint {
                lat: 70.0 + (i as f64) * 0.01,
                lon: 20.0,
                impact: crate::datex::DatexImpact::Block,
                radius_m: 250.0,
                situation_id: format!("far-{i}"),
                penalize_mult: 50.0,
                penalty_minutes: 0.0,
                road: String::new(),
                place: String::new(),
                validity_text: String::new(),
                xsi_type: String::new(),
            });
        }
        far.push(crate::datex::DatexPlannerConstraint {
            lat: 60.0,
            lon: 10.005,
            impact: crate::datex::DatexImpact::Block,
            radius_m: 250.0,
            situation_id: "near".into(),
            penalize_mult: 50.0,
            penalty_minutes: 0.0,
            road: String::new(),
            place: String::new(),
            validity_text: String::new(),
            xsi_type: String::new(),
        });
        let mut opts = RouteOptions {
            datex_impacts: far,
            ..RouteOptions::default()
        };
        g.bind_datex_overlay(&mut opts);
        let bits = opts.datex_edge_block.as_ref().expect("bits");
        assert_eq!(bits.len(), 1);
        assert_eq!(bits[0], 1, "midpoint DATEX must block the only edge");
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

    fn test_node(id: i64, lat: f64, lon: f64) -> (NodeId, Node) {
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

    fn test_edge(
        source: i64,
        target: i64,
        slat: f64,
        slon: f64,
        elat: f64,
        elon: f64,
    ) -> GraphEdge {
        GraphEdge {
            id: format!("{source}-{target}"),
            source: NodeId(source),
            target: NodeId(target),
            length_m: 100.0,
            base_weight: 100.0,
            eco_weight: None,
            start_lat: slat,
            start_lon: slon,
            end_lat: elat,
            end_lon: elon,
            shape: Vec::new(),
            highway: Some("residential".into()),
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
            surface_quality: SurfaceQuality::Good,
        }
    }

    /// Island courtyard next to a longer public road, matching farm snap-off-network.
    fn island_and_public_car_graph(public_lat: f64) -> RouteGraph {
        let mut nodes = HashMap::new();
        for (id, n) in [
            test_node(1, 60.0, 10.0),
            test_node(2, 60.0, 10.001),
            test_node(10, public_lat, 10.0),
            test_node(11, public_lat, 10.002),
            test_node(12, public_lat, 10.004),
            test_node(13, public_lat, 10.006),
        ] {
            nodes.insert(id, n);
        }
        let mut edges = Vec::new();
        let pairs = [
            (1, 2, 60.0, 10.0, 60.0, 10.001),
            (10, 11, public_lat, 10.0, public_lat, 10.002),
            (11, 12, public_lat, 10.002, public_lat, 10.004),
            (12, 13, public_lat, 10.004, public_lat, 10.006),
        ];
        for (s, t, slat, slon, elat, elon) in pairs {
            edges.push(test_edge(s, t, slat, slon, elat, elon));
            edges.push(test_edge(t, s, elat, elon, slat, slon));
        }
        RouteGraph::from_parts(nodes, edges, RoutingProfile::Car)
    }

    fn test_edge_with_surface(
        source: i64,
        target: i64,
        slat: f64,
        slon: f64,
        elat: f64,
        elon: f64,
        highway: &str,
        surface_quality: SurfaceQuality,
    ) -> GraphEdge {
        let mut edge = test_edge(source, target, slat, slon, elat, elon);
        edge.highway = Some(highway.into());
        edge.surface_quality = surface_quality;
        edge
    }

    #[test]
    fn nearest_routable_prefers_better_surface_within_snap_budget() {
        // POI at (60, 10). Nearby 2-node track island ~30 m north; 3-node paved component ~400 m north.
        // Giant-component preference (not surface) must still prefer the public network.
        let poi_lat = 60.0;
        let track_lat = 60.0 + (30.0 / 111_320.0);
        let paved_lat = 60.0 + (400.0 / 111_320.0);
        let mut nodes = HashMap::new();
        for (id, n) in [
            test_node(1, track_lat, 10.0),
            test_node(2, track_lat, 10.001),
            test_node(10, paved_lat, 10.0),
            test_node(11, paved_lat, 10.002),
            test_node(12, paved_lat, 10.004),
        ] {
            nodes.insert(id, n);
        }
        let edges = vec![
            test_edge_with_surface(
                1,
                2,
                track_lat,
                10.0,
                track_lat,
                10.001,
                "track",
                SurfaceQuality::Poor,
            ),
            test_edge_with_surface(
                2,
                1,
                track_lat,
                10.001,
                track_lat,
                10.0,
                "track",
                SurfaceQuality::Poor,
            ),
            test_edge_with_surface(
                10,
                11,
                paved_lat,
                10.0,
                paved_lat,
                10.002,
                "primary",
                SurfaceQuality::Good,
            ),
            test_edge_with_surface(
                11,
                10,
                paved_lat,
                10.002,
                paved_lat,
                10.0,
                "primary",
                SurfaceQuality::Good,
            ),
            test_edge_with_surface(
                11,
                12,
                paved_lat,
                10.002,
                paved_lat,
                10.004,
                "primary",
                SurfaceQuality::Good,
            ),
            test_edge_with_surface(
                12,
                11,
                paved_lat,
                10.004,
                paved_lat,
                10.002,
                "primary",
                SurfaceQuality::Good,
            ),
        ];
        let mut graph = RouteGraph::from_parts(nodes, edges, RoutingProfile::Car);
        graph.surface_routing_mode = SurfaceRoutingMode::Car;
        let (id, dist) = graph
            .nearest_routable(poi_lat, 10.0)
            .expect("paved network within car snap budget");
        assert_eq!(
            id,
            NodeId(10),
            "must prefer paved giant over nearby track island"
        );
        assert!(dist > 350.0 && dist < 450.0, "dist_m={dist}");
    }

    /// Rural address: gravel driveway ~100 m away, paved through-road ~400 m away,
    /// same connected component. Endpoint snap must keep the driveway.
    fn gravel_driveway_vs_distant_paved_graph() -> RouteGraph {
        let gravel_lat = 60.0 + (100.0 / 111_320.0);
        let paved_lat = 60.0 + (400.0 / 111_320.0);
        let mut nodes = HashMap::new();
        for (id, n) in [
            test_node(1, gravel_lat, 10.0),
            test_node(2, gravel_lat, 10.001),
            test_node(3, paved_lat, 10.0),
        ] {
            nodes.insert(id, n);
        }
        let edges = vec![
            test_edge_with_surface(
                1,
                2,
                gravel_lat,
                10.0,
                gravel_lat,
                10.001,
                "track",
                SurfaceQuality::Poor,
            ),
            test_edge_with_surface(
                2,
                1,
                gravel_lat,
                10.001,
                gravel_lat,
                10.0,
                "track",
                SurfaceQuality::Poor,
            ),
            test_edge_with_surface(
                2,
                3,
                gravel_lat,
                10.001,
                paved_lat,
                10.0,
                "primary",
                SurfaceQuality::Good,
            ),
            test_edge_with_surface(
                3,
                2,
                paved_lat,
                10.0,
                gravel_lat,
                10.001,
                "primary",
                SurfaceQuality::Good,
            ),
        ];
        let mut graph = RouteGraph::from_parts(nodes, edges, RoutingProfile::Car);
        graph.surface_routing_mode = SurfaceRoutingMode::Car;
        graph
    }

    #[test]
    fn nearest_routable_endpoint_keeps_near_gravel_over_distant_paved() {
        let graph = gravel_driveway_vs_distant_paved_graph();
        let (id, dist) = graph
            .nearest_routable(60.0, 10.0)
            .expect("gravel driveway within snap budget");
        assert_eq!(
            id,
            NodeId(1),
            "endpoint must snap to literal nearest gravel node, not paved ~400 m away"
        );
        assert!(dist > 80.0 && dist < 130.0, "dist_m={dist}");
    }

    #[test]
    fn nearest_routable_via_surface_does_not_jump_across_full_budget() {
        // Via surface preference is capped near the literal nearest node (~100 m + 150 m
        // slack); a paved node at ~400 m must not win.
        let graph = gravel_driveway_vs_distant_paved_graph();
        let (id, dist) = graph
            .nearest_routable_with_options(60.0, 10.0, &RouteOptions::default(), true)
            .expect("gravel driveway within snap budget");
        assert_eq!(
            id,
            NodeId(1),
            "via surface preference must not leap to paved outside slack"
        );
        assert!(dist > 80.0 && dist < 130.0, "dist_m={dist}");
    }

    #[test]
    fn nearest_routable_via_prefers_better_surface_within_slack() {
        // Gravel ~50 m, paved ~120 m on same component: via may prefer paved (within slack).
        let gravel_lat = 60.0 + (50.0 / 111_320.0);
        let paved_lat = 60.0 + (120.0 / 111_320.0);
        let mut nodes = HashMap::new();
        for (id, n) in [
            test_node(1, gravel_lat, 10.0),
            test_node(2, gravel_lat, 10.001),
            test_node(3, paved_lat, 10.0),
        ] {
            nodes.insert(id, n);
        }
        let edges = vec![
            test_edge_with_surface(
                1,
                2,
                gravel_lat,
                10.0,
                gravel_lat,
                10.001,
                "track",
                SurfaceQuality::Poor,
            ),
            test_edge_with_surface(
                2,
                1,
                gravel_lat,
                10.001,
                gravel_lat,
                10.0,
                "track",
                SurfaceQuality::Poor,
            ),
            test_edge_with_surface(
                2,
                3,
                gravel_lat,
                10.001,
                paved_lat,
                10.0,
                "primary",
                SurfaceQuality::Good,
            ),
            test_edge_with_surface(
                3,
                2,
                paved_lat,
                10.0,
                gravel_lat,
                10.001,
                "primary",
                SurfaceQuality::Good,
            ),
        ];
        let mut graph = RouteGraph::from_parts(nodes, edges, RoutingProfile::Car);
        graph.surface_routing_mode = SurfaceRoutingMode::Car;

        let (end_id, end_dist) = graph
            .nearest_routable_with_options(60.0, 10.0, &RouteOptions::default(), false)
            .expect("endpoint snap");
        assert_eq!(end_id, NodeId(1), "endpoint keeps nearest gravel");
        assert!(end_dist > 40.0 && end_dist < 70.0, "end_dist_m={end_dist}");

        let (via_id, via_dist) = graph
            .nearest_routable_with_options(60.0, 10.0, &RouteOptions::default(), true)
            .expect("via snap");
        assert_eq!(
            via_id,
            NodeId(3),
            "via may prefer paved within surface slack of nearest"
        );
        assert!(
            via_dist > 100.0 && via_dist < 140.0,
            "via_dist_m={via_dist}"
        );
    }

    #[test]
    fn nearest_routable_prefers_good_surface_on_same_component() {
        // Legacy name: endpoint behaviour now prefers nearer track; via still
        // prefers good surface only inside SURFACE_VIA_SNAP_SLACK_M of nearest.
        let poi_lat = 60.0;
        let track_lat = 60.0 + (30.0 / 111_320.0);
        let paved_lat = 60.0 + (400.0 / 111_320.0);
        let mut nodes = HashMap::new();
        for (id, n) in [
            test_node(1, track_lat, 10.0),
            test_node(2, track_lat, 10.001),
            test_node(3, paved_lat, 10.0),
        ] {
            nodes.insert(id, n);
        }
        let edges = vec![
            test_edge_with_surface(
                1,
                2,
                track_lat,
                10.0,
                track_lat,
                10.001,
                "track",
                SurfaceQuality::Poor,
            ),
            test_edge_with_surface(
                2,
                1,
                track_lat,
                10.001,
                track_lat,
                10.0,
                "track",
                SurfaceQuality::Poor,
            ),
            test_edge_with_surface(
                2,
                3,
                track_lat,
                10.001,
                paved_lat,
                10.0,
                "primary",
                SurfaceQuality::Good,
            ),
            test_edge_with_surface(
                3,
                2,
                paved_lat,
                10.0,
                track_lat,
                10.001,
                "primary",
                SurfaceQuality::Good,
            ),
        ];
        let mut graph = RouteGraph::from_parts(nodes, edges, RoutingProfile::Car);
        graph.surface_routing_mode = SurfaceRoutingMode::Car;
        let (id, dist) = graph
            .nearest_routable(poi_lat, 10.0)
            .expect("track junction within car snap budget");
        assert_eq!(
            id,
            NodeId(1),
            "endpoint must prefer nearer track over paved 400 m away"
        );
        assert!(dist > 20.0 && dist < 50.0, "dist_m={dist}");
    }

    #[test]
    fn shortest_path_avoids_paved_to_poor_transition_when_alternate_exists() {
        // Diamond: A --good--> B --poor--> D vs A --good--> C --good--> D
        let mut nodes = HashMap::new();
        for (id, lat, lon) in [
            (1, 60.0, 10.0),
            (2, 60.001, 10.0),
            (3, 60.001, 10.002),
            (4, 60.002, 10.001),
        ] {
            let (nid, n) = test_node(id, lat, lon);
            nodes.insert(nid, n);
        }
        let edges = vec![
            test_edge_with_surface(
                1,
                2,
                60.0,
                10.0,
                60.001,
                10.0,
                "primary",
                SurfaceQuality::Good,
            ),
            test_edge_with_surface(
                2,
                1,
                60.001,
                10.0,
                60.0,
                10.0,
                "primary",
                SurfaceQuality::Good,
            ),
            test_edge_with_surface(
                1,
                3,
                60.0,
                10.0,
                60.001,
                10.002,
                "primary",
                SurfaceQuality::Good,
            ),
            test_edge_with_surface(
                3,
                1,
                60.001,
                10.002,
                60.0,
                10.0,
                "primary",
                SurfaceQuality::Good,
            ),
            test_edge_with_surface(
                2,
                4,
                60.001,
                10.0,
                60.002,
                10.001,
                "track",
                SurfaceQuality::Poor,
            ),
            test_edge_with_surface(
                4,
                2,
                60.002,
                10.001,
                60.001,
                10.0,
                "track",
                SurfaceQuality::Poor,
            ),
            test_edge_with_surface(
                3,
                4,
                60.001,
                10.002,
                60.002,
                10.001,
                "primary",
                SurfaceQuality::Good,
            ),
            test_edge_with_surface(
                4,
                3,
                60.002,
                10.001,
                60.001,
                10.002,
                "primary",
                SurfaceQuality::Good,
            ),
        ];
        let mut graph = RouteGraph::from_parts(nodes, edges, RoutingProfile::Car);
        graph.surface_routing_mode = SurfaceRoutingMode::Car;
        apply_surface_preference(
            &mut graph,
            SurfaceRoutingMode::Car,
            MotorSoftCostProfile::Car,
        );
        let path = graph
            .shortest_path(NodeId(1), NodeId(4), false)
            .expect("route exists")
            .0;
        assert!(
            path.contains(&NodeId(3)),
            "expected good-surface detour via node 3, got {path:?}"
        );
        assert!(
            !path.contains(&NodeId(2)),
            "should avoid poor track stub via node 2, got {path:?}"
        );
    }

    #[test]
    #[ignore = "ostlandet PBF census probe — run with --ignored --nocapture"]
    fn census_parallel_edges_ostlandet_car() {
        let pbf = std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/target/integration-fixtures/ostlandet-latest.osm.pbf"
        ));
        if !pbf.is_file() {
            eprintln!("skip: missing {pbf:?}");
            return;
        }
        let graph = RouteGraph::build_from_pbf(pbf, RoutingProfile::Car).expect("car graph");
        let c = graph.parallel_edge_census();
        eprintln!("parallel_edge_census_ostlandet_car: {c:?}");
        eprintln!(
            "parallel_pair_rate={:.4}% extra_edge_rate={:.4}% mismatch_rate={:.1}%",
            100.0 * c.parallel_directed_pairs as f64 / c.total_directed_edges as f64,
            100.0 * c.extra_parallel_edges as f64 / c.total_directed_edges as f64,
            if c.parallel_directed_pairs > 0 {
                100.0 * c.old_edge_index_would_mismatch as f64 / c.parallel_directed_pairs as f64
            } else {
                0.0
            }
        );
    }

    #[test]
    fn parallel_edges_resolve_to_cheapest_for_path_geometry() {
        // Same endpoints: secondary chord vs longer service loop.
        let mut nodes = HashMap::new();
        for (id, lat, lon) in [
            (3397900348_i64, 60.8841608, 11.3138178),
            (3397900317, 60.8836048, 11.3134738),
        ] {
            let (nid, n) = test_node(id, lat, lon);
            nodes.insert(nid, n);
        }
        let edges = vec![
            test_edge_with_surface(
                3397900348,
                3397900317,
                60.8841608,
                11.3138178,
                60.8836048,
                11.3134738,
                "service",
                SurfaceQuality::Good,
            ),
            {
                let mut e = test_edge_with_surface(
                    3397900348,
                    3397900317,
                    60.8841608,
                    11.3138178,
                    60.8836048,
                    11.3134738,
                    "secondary",
                    SurfaceQuality::Good,
                );
                e.length_m = 64.6;
                e.base_weight = 64.6;
                e.id = "1037045908-1".into();
                e
            },
        ];
        let mut graph = RouteGraph::from_parts(nodes, edges, RoutingProfile::Car);
        graph.surface_routing_mode = SurfaceRoutingMode::Car;
        let (_path, path_edges, _) = graph
            .shortest_path(NodeId(3397900348), NodeId(3397900317), false)
            .expect("parallel chord route");
        assert_eq!(path_edges.len(), 1);
        assert!(
            graph.edges[path_edges[0]].id.contains("1037045908"),
            "A* must record cheapest secondary edge, not parallel service"
        );
        let coords = graph.path_coords_lat_lon_from_edges(&path_edges);
        assert!(
            coords.len() <= 3,
            "secondary chord should be direct, service loop has many shape points: {}",
            coords.len()
        );
    }

    #[test]
    fn nearest_routable_prefers_public_network_over_nearby_island() {
        // ~500 m north: inside the 750 m car snap budget.
        let public_lat = 60.0 + (500.0 / 111_320.0);
        let graph = island_and_public_car_graph(public_lat);
        let (id, dist) = graph
            .nearest_routable(60.0, 10.0)
            .expect("public road within car snap budget");
        assert_eq!(id, NodeId(10), "must skip the 2-node island at the farm");
        assert!(dist > 400.0 && dist < 600.0, "dist_m={dist}");
    }

    #[test]
    fn nearest_routable_keeps_island_when_public_road_is_beyond_budget() {
        // ~900 m north: outside the 750 m car snap budget.
        let public_lat = 60.0 + (900.0 / 111_320.0);
        let graph = island_and_public_car_graph(public_lat);
        let (id, dist) = graph
            .nearest_routable(60.0, 10.0)
            .expect("island still within budget");
        assert_eq!(id, NodeId(1));
        assert!(dist < 50.0, "dist_m={dist}");
    }

    /// Regression: vehicle-filtered snap must not scan all edges per candidate.
    ///
    /// Mirrors the MobileHome long-trip hang (Bad Bevensen first densify hop):
    /// ~10k nodes / ~20k edges inside a 25 km pad, most edges fail maxwidth, so
    /// the old adjacency-miss → `edges.iter()` path was O(N·E). With filtered
    /// component roots this stays O(E) once + O(N) lookups.
    #[test]
    fn vehicle_filtered_snap_uses_o1_incident_not_full_edge_scan() {
        const N: i64 = 10_000;
        let mut nodes = HashMap::new();
        let mut edges = Vec::with_capacity((N as usize) * 2 + 4);
        // Dense cluster well inside CHUNK_INTERMEDIATE_SNAP_M (25 km / pad≈0.25°).
        for i in 0..N {
            let row = (i / 100) as f64;
            let col = (i % 100) as f64;
            let lat = 60.0 + row * 0.001;
            let lon = 10.0 + col * 0.001;
            let (nid, n) = test_node(i, lat, lon);
            nodes.insert(nid, n);
            if i + 1 < N {
                let mut e = test_edge(i, i + 1, lat, lon, lat, lon + 0.001);
                e.maxwidth_m = Some(2.0); // fails MobileHome width 2.297
                edges.push(e);
                let mut e_back = test_edge(i + 1, i, lat, lon + 0.001, lat, lon);
                e_back.maxwidth_m = Some(2.0);
                edges.push(e_back);
            }
        }
        // Wide spine next to the query — only legal network under vehicle limits.
        for (a, b, lat, lon0, lon1) in [
            (N, N + 1, 60.0, 10.0, 10.002),
            (N + 1, N + 2, 60.0, 10.002, 10.004),
        ] {
            let (na, na_n) = test_node(a, lat, lon0);
            let (nb, nb_n) = test_node(b, lat, lon1);
            nodes.insert(na, na_n);
            nodes.insert(nb, nb_n);
            let mut fwd = test_edge(a, b, lat, lon0, lat, lon1);
            fwd.maxwidth_m = Some(3.0);
            fwd.highway = Some("primary".into());
            edges.push(fwd);
            let mut back = test_edge(b, a, lat, lon1, lat, lon0);
            back.maxwidth_m = Some(3.0);
            back.highway = Some("primary".into());
            edges.push(back);
        }
        let graph = RouteGraph::from_parts(nodes, edges, RoutingProfile::Truck);
        let opts = RouteOptions {
            vehicle: Some(crate::config::VehicleLimits {
                width_m: Some(2.297),
                length_m: Some(5.304),
                ..Default::default()
            }),
            ..Default::default()
        };
        let t0 = std::time::Instant::now();
        let (id, dist) = graph
            .nearest_routable_with_options_max(60.0, 10.0, &opts, false, 25_000.0)
            .expect("wide spine within 25 km snap");
        let ms = t0.elapsed().as_millis();
        assert!(
            ms < 1_500,
            "vehicle-filtered snap took {ms} ms — likely reintroduced O(E) per-node scan"
        );
        assert!(
            id.0 >= N,
            "must snap onto wide spine (id>={N}), got {id:?} dist_m={dist}"
        );
        assert!(dist < 500.0, "dist_m={dist}");
    }

    /// Unfiltered (no vehicle) snap still succeeds on the same graph quickly.
    #[test]
    fn unfiltered_snap_unaffected_on_large_synthetic_graph() {
        const N: i64 = 2_000;
        let mut nodes = HashMap::new();
        let mut edges = Vec::new();
        for i in 0..N {
            let lat = 60.0 + (i as f64) * 0.0001;
            let (nid, n) = test_node(i, lat, 10.0);
            nodes.insert(nid, n);
            if i + 1 < N {
                edges.push(test_edge(i, i + 1, lat, 10.0, lat + 0.0001, 10.0));
                edges.push(test_edge(i + 1, i, lat + 0.0001, 10.0, lat, 10.0));
            }
        }
        let graph = RouteGraph::from_parts(nodes, edges, RoutingProfile::Car);
        let t0 = std::time::Instant::now();
        let (id, dist) = graph.nearest_routable(60.0, 10.0).expect("unfiltered snap");
        let ms = t0.elapsed().as_millis();
        assert!(ms < 500, "unfiltered snap took {ms} ms");
        assert_eq!(id, NodeId(0));
        assert!(dist < 50.0, "dist_m={dist}");
    }

    #[test]
    fn shortest_path_reaches_public_snap_from_other_end() {
        let public_lat = 60.0 + (500.0 / 111_320.0);
        let graph = island_and_public_car_graph(public_lat);
        let start = graph.nearest_routable(public_lat, 10.006).unwrap().0;
        let goal = graph.nearest_routable(60.0, 10.0).unwrap().0;
        assert_eq!(start, NodeId(13));
        assert_eq!(goal, NodeId(10));
        assert!(graph.shortest_path(start, goal, false).is_some());
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
            maxheight_m: max_h,
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
            surface_quality: SurfaceQuality::Good,
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
            is_toll: true,
            is_ferry: false,
            is_tunnel: false,
            is_boardwalk_crossing: false,
            is_roundabout: false,
            motor_vehicle_conditional: None,
            access_conditional: None,
            maxspeed_conditional: None,
            access_forbidden: false,
            surface_quality: SurfaceQuality::Good,
        };
        assert!(!edge_allowed_for_options(
            &edge,
            &RouteOptions {
                toll_policy: crate::routing::toll::TollPolicy::NeverUse,
                ..Default::default()
            },
            RoutingProfile::Car,
        ));
        // Penalize keeps the edge searchable (cost multiplier only).
        assert!(edge_allowed_for_options(
            &edge,
            &RouteOptions {
                toll_policy: crate::routing::toll::TollPolicy::Penalize,
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
    fn avoid_tunnels_soft_cost_keeps_edge_allowed() {
        let mut edge = GraphEdge {
            id: "tun".into(),
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
            is_tunnel: true,
            is_boardwalk_crossing: false,
            is_roundabout: false,
            motor_vehicle_conditional: None,
            access_conditional: None,
            maxspeed_conditional: None,
            access_forbidden: false,
            surface_quality: SurfaceQuality::Good,
        };
        let avoid = RouteOptions {
            avoid_tunnels: true,
            ..Default::default()
        };
        assert!(
            edge_allowed_for_options(&edge, &avoid, RoutingProfile::Car),
            "tunnels must stay searchable under soft avoid"
        );
        let base = edge_travel_cost(&edge, 0, false, &RouteOptions::default());
        let penalized = edge_travel_cost(&edge, 0, false, &avoid);
        assert!(
            (penalized - base * crate::routing::toll::TUNNEL_AVOID_PENALTY_MULT).abs() < 1e-9,
            "base={base} penalized={penalized}"
        );
        assert!(is_tunnel_tag("yes"));
        assert!(is_tunnel_tag("building_passage"));
        assert!(!is_tunnel_tag("no"));
        assert!(!is_tunnel_tag(""));
        edge.is_tunnel = false;
        assert_eq!(
            edge_travel_cost(&edge, 0, false, &avoid),
            edge_travel_cost(&edge, 0, false, &RouteOptions::default())
        );
    }

    #[test]
    fn foot_and_bicycle_always_avoid_motorway_grade() {
        let edge = GraphEdge {
            id: "mw".into(),
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
            highway: Some("motorway".into()),
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
            surface_quality: SurfaceQuality::Good,
        };
        let opts = RouteOptions {
            avoid_motorways: false,
            ..Default::default()
        };
        assert!(
            edge_allowed_for_options(&edge, &opts, RoutingProfile::Car),
            "car may use motorway when avoid flag is off"
        );
        assert!(!edge_allowed_for_options(
            &edge,
            &opts,
            RoutingProfile::Bicycle
        ));
        assert!(!edge_allowed_for_options(
            &edge,
            &opts,
            RoutingProfile::Foot
        ));
        assert!(profile_locks_avoid_motorways(RoutingProfile::Bicycle));
        assert!(profile_locks_avoid_motorways(RoutingProfile::Foot));
        assert!(!profile_locks_avoid_motorways(RoutingProfile::Car));
    }

    #[test]
    fn seasonal_conditional_blocks_car_not_foot() {
        use chrono::NaiveDate;
        let mut edge = GraphEdge {
            id: "cond-seasonal-0".into(),
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
            maxspeed_practical_kmh: None,
            maxspeed_advisory_kmh: None,
            maxspeed_type: None,
            maxspeed_variable: false,
            minspeed_kmh: None,
            name: Some("SeasonalRoad".into()),
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
            motor_vehicle_conditional: Some("no @ Nov-Jun".into()),
            access_conditional: None,
            maxspeed_conditional: None,
            access_forbidden: false,
            surface_quality: SurfaceQuality::Good,
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

    #[test]
    fn avoid_motorways_blocks_motorway_class_motorroad_and_dual_fast_not_e_ref() {
        let opts = RouteOptions {
            avoid_motorways: true,
            ..Default::default()
        };
        let mut edge = GraphEdge {
            id: "mw".into(),
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
            highway: Some("motorway".into()),
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
            surface_quality: SurfaceQuality::Good,
        };
        assert!(!edge_allowed_for_options(&edge, &opts, RoutingProfile::Car));
        edge.highway = Some("motorway_link".into());
        assert!(!edge_allowed_for_options(&edge, &opts, RoutingProfile::Car));
        edge.highway = Some("trunk".into());
        assert!(
            edge_allowed_for_options(&edge, &opts, RoutingProfile::Car),
            "plain trunk must remain usable"
        );
        edge.road_ref = Some("E6".into());
        assert!(
            edge_allowed_for_options(&edge, &opts, RoutingProfile::Car),
            "E-ref without motorroad/dual+90 must remain usable"
        );
        edge.is_motorroad = true;
        assert!(!edge_allowed_for_options(&edge, &opts, RoutingProfile::Car));
        edge.is_motorroad = false;
        edge.is_expressway = true;
        assert!(!edge_allowed_for_options(&edge, &opts, RoutingProfile::Car));
        edge.is_expressway = false;
        edge.is_oneway = true;
        edge.lanes = Some(2);
        edge.maxspeed_kmh = Some(90.0);
        assert!(!edge_allowed_for_options(&edge, &opts, RoutingProfile::Car));
        edge.maxspeed_kmh = Some(70.0);
        assert!(
            edge_allowed_for_options(&edge, &opts, RoutingProfile::Car),
            "urban dual at 70 km/h without motorroad is not motorway-grade"
        );
    }

    #[test]
    fn motorway_grade_from_parts_and_lane_parse() {
        assert!(motorway_grade_from_parts(
            Some("motorway"),
            false,
            false,
            false,
            None,
            None
        ));
        assert!(motorway_grade_from_parts(
            Some("trunk"),
            true,
            false,
            false,
            None,
            None
        ));
        assert!(!motorway_grade_from_parts(
            Some("trunk"),
            false,
            false,
            false,
            None,
            None
        ));
        assert!(!motorway_grade_from_parts(
            Some("trunk"),
            false,
            false,
            true,
            Some(2),
            Some(70.0)
        ));
        assert!(motorway_grade_from_parts(
            Some("trunk"),
            false,
            false,
            true,
            Some(2),
            Some(90.0)
        ));
        assert_eq!(parse_lanes_tag("2"), Some(2));
        assert_eq!(parse_lanes_tag("2;2"), Some(2));
        assert_eq!(
            combine_osm_road_refs(Some("Rv15".into()), Some("E16".into())).as_deref(),
            Some("Rv15;E16")
        );
    }

    #[test]
    fn format_route_avoidance_report_reflects_all_toll_policies() {
        let base = RouteOptions::default();
        let allow = format_route_avoidance_report(&base, 0, 50.0);
        assert!(allow.contains("Avoid toll roads: OFF"), "{allow}");

        let penalize = format_route_avoidance_report(
            &RouteOptions {
                toll_policy: crate::routing::toll::TollPolicy::Penalize,
                ..Default::default()
            },
            0,
            50.0,
        );
        assert!(
            penalize.contains("Avoid toll roads: ON (penalize)"),
            "{penalize}"
        );

        let never = format_route_avoidance_report(
            &RouteOptions {
                toll_policy: crate::routing::toll::TollPolicy::NeverUse,
                ..Default::default()
            },
            0,
            50.0,
        );
        assert!(
            never.contains("Avoid toll roads: ON (never use)"),
            "{never}"
        );
    }

    /// Synthetic diamond: short leg crosses into Sweden; long leg stays in Norway.
    fn norway_sweden_border_diamond() -> RouteGraph {
        // Oslo (NO) --short--> Langflon area (SE) --short--> goal near border (NO-ish east)
        // Oslo (NO) --long--> inland NO detour --> goal
        let mut nodes = HashMap::new();
        for (id, n) in [
            test_node(1, 59.91, 10.75), // start Oslo NO
            test_node(2, 61.90, 12.27), // SE shortcut (Langflon)
            test_node(3, 60.50, 10.80), // NO detour
            test_node(4, 59.95, 11.20), // goal inside NO
        ] {
            nodes.insert(id, n);
        }
        let mut e_short_a = test_edge(1, 2, 59.91, 10.75, 61.90, 12.27);
        e_short_a.length_m = 100.0;
        e_short_a.base_weight = 100.0;
        let mut e_short_b = test_edge(2, 4, 61.90, 12.27, 59.95, 11.20);
        e_short_b.length_m = 100.0;
        e_short_b.base_weight = 100.0;
        let mut e_long_a = test_edge(1, 3, 59.91, 10.75, 60.50, 10.80);
        e_long_a.length_m = 500.0;
        e_long_a.base_weight = 500.0;
        let mut e_long_b = test_edge(3, 4, 60.50, 10.80, 59.95, 11.20);
        e_long_b.length_m = 500.0;
        e_long_b.base_weight = 500.0;
        // Bidirectional copies so A* can traverse either way if needed.
        let mut edges = vec![e_short_a, e_short_b, e_long_a, e_long_b];
        let rev: Vec<_> = edges
            .iter()
            .map(|e| {
                let mut r = e.clone();
                r.id = format!("{}-rev", e.id);
                std::mem::swap(&mut r.source, &mut r.target);
                std::mem::swap(&mut r.start_lat, &mut r.end_lat);
                std::mem::swap(&mut r.start_lon, &mut r.end_lon);
                r
            })
            .collect();
        edges.extend(rev);
        RouteGraph::from_parts(nodes, edges, RoutingProfile::Car)
    }

    #[test]
    fn allowed_countries_none_keeps_shortest_cross_border_path() {
        let graph = norway_sweden_border_diamond();
        let path = graph
            .shortest_path_with_options(NodeId(1), NodeId(4), false, &RouteOptions::default())
            .expect("unrestricted path");
        assert!(
            path.0.contains(&NodeId(2)),
            "default None must take SE shortcut: {:?}",
            path.0
        );
        assert!(!path.0.contains(&NodeId(3)));
    }

    #[test]
    fn allowed_countries_norway_only_takes_longer_domestic_path() {
        let graph = norway_sweden_border_diamond();
        let opts = RouteOptions {
            allowed_countries: Some(vec!["no".into()]),
            ..Default::default()
        };
        let path = graph
            .shortest_path_with_options(NodeId(1), NodeId(4), false, &opts)
            .expect("Norway-only path");
        assert!(
            path.0.contains(&NodeId(3)),
            "must stay in Norway via detour: {:?}",
            path.0
        );
        assert!(
            !path.0.contains(&NodeId(2)),
            "must not use SE shortcut: {:?}",
            path.0
        );
    }

    #[test]
    fn allowed_countries_impossible_yields_outside_countries() {
        // Graph where the only path uses a Sweden midpoint — Norway filter must
        // fail with outside_countries (not a plain disconnect).
        let mut nodes = HashMap::new();
        for (id, n) in [
            test_node(1, 59.91, 10.75),
            test_node(2, 61.90, 12.27),
            test_node(4, 59.95, 11.20),
        ] {
            nodes.insert(id, n);
        }
        let mut a = test_edge(1, 2, 59.91, 10.75, 61.90, 12.27);
        a.length_m = 100.0;
        a.base_weight = 100.0;
        let mut b = test_edge(2, 4, 61.90, 12.27, 59.95, 11.20);
        b.length_m = 100.0;
        b.base_weight = 100.0;
        let mut a_rev = a.clone();
        a_rev.id = "a-rev".into();
        std::mem::swap(&mut a_rev.source, &mut a_rev.target);
        std::mem::swap(&mut a_rev.start_lat, &mut a_rev.end_lat);
        std::mem::swap(&mut a_rev.start_lon, &mut a_rev.end_lon);
        let mut b_rev = b.clone();
        b_rev.id = "b-rev".into();
        std::mem::swap(&mut b_rev.source, &mut b_rev.target);
        std::mem::swap(&mut b_rev.start_lat, &mut b_rev.end_lat);
        std::mem::swap(&mut b_rev.start_lon, &mut b_rev.end_lon);
        let graph = RouteGraph::from_parts(nodes, vec![a, b, a_rev, b_rev], RoutingProfile::Car);
        let opts = RouteOptions {
            allowed_countries: Some(vec!["no".into()]),
            ..Default::default()
        };
        let stats = graph.shortest_path_with_options_stats(NodeId(1), NodeId(4), false, &opts);
        assert!(stats.path.is_none());
        assert_eq!(stats.terminate_reason, "outside_countries");
    }

    #[test]
    fn via_points_are_visited_in_order() {
        // A -> V -> B must visit V; A -> B alone would be shorter without V.
        let mut nodes = HashMap::new();
        for (id, n) in [
            test_node(1, 59.91, 10.70),
            test_node(2, 59.91, 10.80), // via
            test_node(3, 59.91, 10.90),
        ] {
            nodes.insert(id, n);
        }
        let edges = vec![
            {
                let mut e = test_edge(1, 2, 59.91, 10.70, 59.91, 10.80);
                e.length_m = 100.0;
                e.base_weight = 100.0;
                e
            },
            {
                let mut e = test_edge(2, 3, 59.91, 10.80, 59.91, 10.90);
                e.length_m = 100.0;
                e.base_weight = 100.0;
                e
            },
            {
                let mut e = test_edge(1, 3, 59.91, 10.70, 59.91, 10.90);
                e.length_m = 50.0;
                e.base_weight = 50.0;
                e
            },
        ];
        // reverse
        let mut all = edges.clone();
        for e in &edges {
            let mut r = e.clone();
            r.id = format!("{}-rev", e.id);
            std::mem::swap(&mut r.source, &mut r.target);
            std::mem::swap(&mut r.start_lat, &mut r.end_lat);
            std::mem::swap(&mut r.start_lon, &mut r.end_lon);
            all.push(r);
        }
        let graph = RouteGraph::from_parts(nodes, all, RoutingProfile::Car);
        let direct = graph
            .shortest_path_with_options(NodeId(1), NodeId(3), false, &RouteOptions::default())
            .expect("direct");
        assert!(
            !direct.0.contains(&NodeId(2)),
            "direct must skip via: {:?}",
            direct.0
        );
        let leg1 = graph
            .shortest_path_with_options(NodeId(1), NodeId(2), false, &RouteOptions::default())
            .expect("to via");
        let leg2 = graph
            .shortest_path_with_options(NodeId(2), NodeId(3), false, &RouteOptions::default())
            .expect("from via");
        assert_eq!(leg1.0.last().copied(), Some(NodeId(2)));
        assert_eq!(leg2.0.first().copied(), Some(NodeId(2)));
        assert_eq!(leg2.0.last().copied(), Some(NodeId(3)));
    }

    #[test]
    fn edge_filter_roros_halden_style_allowed_for_norway() {
        // West of old SE box edge (lon 11): Roros latitudes down to inland Ostlandet.
        let west = test_edge(1, 2, 62.57, 10.90, 59.91, 10.75);
        assert!(edge_in_allowed_countries(&west, &["no".into()]));
        // East of lon 11 but still in Norway (Roros → Halden).
        let east = test_edge(1, 2, 62.5747, 11.3842, 59.1248, 11.3875);
        assert!(edge_in_allowed_countries(&east, &["no".into()]));
    }

    #[test]
    fn edge_filter_kautokeino_alta_vs_karesuando() {
        let ok = test_edge(1, 2, 69.0125, 23.0415, 69.9689, 23.2717);
        assert!(edge_in_allowed_countries(&ok, &["no".into()]));
        let cross = test_edge(1, 2, 69.0125, 23.0415, 68.4417, 22.4800);
        assert!(!edge_in_allowed_countries(&cross, &["no".into()]));
    }

    #[test]
    fn edge_filter_svinesund_bridge_excluded_for_no_and_se() {
        let bridge = test_edge(1, 2, 59.1200, 11.3000, 59.0800, 11.2600);
        assert!(!edge_in_allowed_countries(&bridge, &["no".into()]));
        assert!(!edge_in_allowed_countries(&bridge, &["se".into()]));
    }

    #[test]
    fn edge_filter_flensburg_padborg_excluded_for_germany() {
        let e = test_edge(1, 2, 54.7930, 9.4330, 54.8250, 9.3600);
        assert!(!edge_in_allowed_countries(&e, &["de".into()]));
    }

    #[test]
    fn edge_filter_us_border_crossings_excluded_for_us() {
        let detroit_windsor = test_edge(1, 2, 42.3314, -83.0458, 42.3149, -83.0364);
        assert!(!edge_in_allowed_countries(&detroit_windsor, &["us".into()]));
        let blaine_white_rock = test_edge(1, 2, 48.9937, -122.7470, 49.0250, -122.8030);
        assert!(!edge_in_allowed_countries(
            &blaine_white_rock,
            &["us".into()]
        ));
        let san_ysidro_tijuana = test_edge(1, 2, 32.5550, -117.0450, 32.5149, -117.0382);
        assert!(!edge_in_allowed_countries(
            &san_ysidro_tijuana,
            &["us".into()]
        ));
        let el_paso_juarez = test_edge(1, 2, 31.7619, -106.4850, 31.6904, -106.4245);
        assert!(!edge_in_allowed_countries(&el_paso_juarez, &["us".into()]));
    }

    #[test]
    fn destination_skips_one_way_dead_end_stub() {
        // Main bidirectional corridor 1 <-> 2 <-> 3.
        // One-way spur 2 -> 4 (dead-end sink): reachable, cannot depart.
        // One-way spur 5 -> 2 (dead-end source): can depart, not reachable.
        let mut nodes = HashMap::new();
        for (id, lat, lon) in [
            (1, 60.0, 10.0),
            (2, 60.0, 10.01),
            (3, 60.0, 10.02),
            (4, 60.001, 10.01), // sink next to 2
            (5, 59.999, 10.01), // source next to 2
        ] {
            nodes.insert(
                NodeId(id),
                osm4routing::Node {
                    id: NodeId(id),
                    coord: geo_types::Coord { x: lon, y: lat },
                    uses: 0,
                },
            );
        }
        let mut edges = vec![GraphEdge {
            id: "12".into(),
            source: NodeId(1),
            target: NodeId(2),
            length_m: 100.0,
            base_weight: 100.0,
            eco_weight: None,
            start_lat: 60.0,
            start_lon: 10.0,
            end_lat: 60.0,
            end_lon: 10.01,
            shape: vec![],
            highway: Some("residential".into()),
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
            surface_quality: SurfaceQuality::Good,
        }];
        // Clone helper for remaining edges
        fn e(
            id: &str,
            s: i64,
            t: i64,
            slat: f64,
            slon: f64,
            elat: f64,
            elon: f64,
            oneway: bool,
        ) -> GraphEdge {
            GraphEdge {
                id: id.into(),
                source: NodeId(s),
                target: NodeId(t),
                length_m: 100.0,
                base_weight: 100.0,
                eco_weight: None,
                start_lat: slat,
                start_lon: slon,
                end_lat: elat,
                end_lon: elon,
                shape: vec![],
                highway: Some("residential".into()),
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
                is_oneway: oneway,
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
                surface_quality: SurfaceQuality::Good,
            }
        }
        edges = vec![
            e("12", 1, 2, 60.0, 10.0, 60.0, 10.01, false),
            e("21", 2, 1, 60.0, 10.01, 60.0, 10.0, false),
            e("23", 2, 3, 60.0, 10.01, 60.0, 10.02, false),
            e("32", 3, 2, 60.0, 10.02, 60.0, 10.01, false),
            e("24", 2, 4, 60.0, 10.01, 60.001, 10.01, true),
            e("52", 5, 2, 59.999, 10.01, 60.0, 10.01, true),
        ];
        let mut g = RouteGraph::from_parts(nodes, edges, RoutingProfile::Car);
        g.ensure_directed_snap_labels();
        // Destination next to sink 4 must prefer a reachable main-network node.
        let dest = RouteOptions {
            snap_role: SnapRole::Destination,
            ..Default::default()
        };
        // Node 5 is a one-way *source* into the network: directed dead-end for
        // arrivals (nothing reaches it). Destination snap must skip it.
        assert!(!g.directed_snap_ok(NodeId(5), SnapRole::Destination));
        assert!(g.directed_snap_ok(NodeId(5), SnapRole::Origin));
        let (d, _) = g
            .nearest_routable_with_options_max(59.999, 10.01, &dest, false, 750.0)
            .expect("dest snap");
        assert_ne!(
            d,
            NodeId(5),
            "must not snap destination to unreachable one-way source"
        );
        assert!(g.directed_snap_ok(d, SnapRole::Destination));

        // Sink 4 is reachable from main — valid destination (but bad origin).
        assert!(g.directed_snap_ok(NodeId(4), SnapRole::Destination));
        assert!(!g.directed_snap_ok(NodeId(4), SnapRole::Origin));
        let origin = RouteOptions {
            snap_role: SnapRole::Origin,
            ..Default::default()
        };
        let (o, _) = g
            .nearest_routable_with_options_max(60.001, 10.01, &origin, false, 750.0)
            .expect("origin near sink");
        assert_ne!(o, NodeId(4), "must not snap origin to one-way sink");
        assert!(g.directed_snap_ok(o, SnapRole::Origin));
    }

    #[test]
    fn origin_rejects_tiny_isolated_sink() {
        // Main 0 <-> 1 <-> 2 (clearly giant). Isolated one-way 3 -> 4 (sink at 4 near query).
        let mut nodes = HashMap::new();
        for (id, lat, lon) in [
            (0, 60.0, 9.99),
            (1, 60.0, 10.0),
            (2, 60.0, 10.01),
            (3, 60.0005, 10.005),
            (4, 60.0006, 10.005), // sink, nearest to query
        ] {
            nodes.insert(
                NodeId(id),
                osm4routing::Node {
                    id: NodeId(id),
                    coord: geo_types::Coord { x: lon, y: lat },
                    uses: 0,
                },
            );
        }
        fn e(id: &str, s: i64, t: i64, slat: f64, slon: f64, elat: f64, elon: f64) -> GraphEdge {
            GraphEdge {
                id: id.into(),
                source: NodeId(s),
                target: NodeId(t),
                length_m: 50.0,
                base_weight: 50.0,
                eco_weight: None,
                start_lat: slat,
                start_lon: slon,
                end_lat: elat,
                end_lon: elon,
                shape: vec![],
                highway: Some("service".into()),
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
                is_oneway: true,
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
                surface_quality: SurfaceQuality::Good,
            }
        }
        let mut edges = vec![
            e("01", 0, 1, 60.0, 9.99, 60.0, 10.0),
            e("10", 1, 0, 60.0, 10.0, 60.0, 9.99),
            e("12", 1, 2, 60.0, 10.0, 60.0, 10.01),
            e("21", 2, 1, 60.0, 10.01, 60.0, 10.0),
            e("34", 3, 4, 60.0005, 10.005, 60.0006, 10.005),
        ];
        for e in &mut edges[..4] {
            e.is_oneway = false;
        }
        let mut g = RouteGraph::from_parts(nodes, edges, RoutingProfile::Car);
        g.ensure_directed_snap_labels();
        let origin = RouteOptions {
            snap_role: SnapRole::Origin,
            ..Default::default()
        };
        let (o, _) = g
            .nearest_routable_with_options_max(60.0006, 10.005, &origin, false, 750.0)
            .expect("origin snap");
        assert!(
            o == NodeId(0) || o == NodeId(1) || o == NodeId(2),
            "origin must snap to main network, got {o:?}"
        );
        assert!(g.directed_snap_ok(o, SnapRole::Origin));
        assert!(!g.directed_snap_ok(NodeId(4), SnapRole::Origin));
        assert!(!g.directed_snap_ok(NodeId(3), SnapRole::Origin));
    }

    #[test]
    fn ferry_base_weight_matches_server_formula() {
        let mps = FERRY_DRIVE_EQUIV_KMH * 1000.0 / 3600.0;
        let boarding = FERRY_CAR_BOARDING_PENALTY_MIN * 60.0 * mps;
        let tagged = ferry_base_weight_m(1_000.0, Some("0:10"), RoutingProfile::Car);
        assert!((tagged - (600.0 * mps + boarding)).abs() < 1e-6);
        let fallback = ferry_base_weight_m(1_000.0, None, RoutingProfile::Car);
        let expected = 1_000.0 * (FERRY_DRIVE_EQUIV_KMH / FERRY_FALLBACK_SPEED_KMH) + boarding;
        assert!((fallback - expected).abs() < 1e-6);
        let foot = ferry_base_weight_m(1_000.0, Some("0:10"), RoutingProfile::Foot);
        assert!((foot - 600.0 * mps).abs() < 1e-6);
    }

    /// Length-only ferry weights make a long unnamed coastal chain look competitive
    /// with a short tagged crossing (Bergen→Stavanger `unnamed@195`). Server costing
    /// prices the chain ~8× higher so A* keeps the short ferry.
    #[test]
    fn ferry_costing_rejects_long_unnamed_chain_vs_short_tagged() {
        let short_len = 20_000.0;
        let chain_len = 195_000.0;
        let short_server = ferry_base_weight_m(short_len, Some("0:40"), RoutingProfile::Car);
        let chain_server = ferry_base_weight_m(chain_len, None, RoutingProfile::Car);
        let short_length_only = short_len;
        let chain_length_only = chain_len;
        // BEFORE (plain length): chain ≈ short.
        assert!(
            (chain_length_only - short_length_only).abs() / short_length_only < 10.0,
            "length-only: 195 km chain is in the same ballpark as a 20 km hop"
        );
        assert!(
            chain_length_only < short_length_only * 12.0,
            "length-only admits coast chaining"
        );
        // AFTER (server formula): chain is far more expensive than a 40 min ferry.
        assert!(
            chain_server > short_server * 5.0,
            "server costing: chain={chain_server} short={short_server}"
        );
        // With length-only weights A* would prefer the chain if land legs were equal;
        // with server weights the short tagged ferry wins by a wide margin.
        assert!(short_length_only < chain_length_only);
        assert!(short_server < chain_server);
    }

    #[test]
    fn untagged_water_shortcut_matches_fehmarn_belt_duplicate() {
        let mut water = test_edge(1, 2, 54.50709, 11.23183, 54.62456, 11.30643);
        water.length_m = 13_919.0;
        water.highway = Some("unclassified".into());
        water.is_ferry = false;
        assert!(is_untagged_water_shortcut(&water));
        let mut ferry = water.clone();
        ferry.is_ferry = true;
        ferry.length_m = 18_900.0;
        assert!(!is_untagged_water_shortcut(&ferry));
        let mut motorway = water.clone();
        motorway.highway = Some("motorway".into());
        motorway.length_m = 18_000.0;
        assert!(!is_untagged_water_shortcut(&motorway));
        water.length_m = 3_000.0;
        assert!(is_untagged_water_shortcut(&water));
        water.shape.push((11.25, 54.56));
        assert!(!is_untagged_water_shortcut(&water));
        let mut proposed = test_edge(1, 2, 54.5, 11.2, 54.6, 11.3);
        proposed.highway = Some("proposed".into());
        assert!(is_construction_or_proposed_highway(
            proposed.highway.as_deref()
        ));
        proposed.highway = Some("construction".into());
        assert!(is_construction_or_proposed_highway(
            proposed.highway.as_deref()
        ));
        proposed.highway = Some("motorway".into());
        assert!(!is_construction_or_proposed_highway(
            proposed.highway.as_deref()
        ));
    }

    fn named_ferry(
        s: i64,
        t: i64,
        slat: f64,
        slon: f64,
        elat: f64,
        elon: f64,
        name: &str,
    ) -> GraphEdge {
        let mut e = test_edge(s, t, slat, slon, elat, elon);
        e.is_ferry = true;
        e.name = Some(name.into());
        e.highway = None;
        e
    }

    #[test]
    fn overlay_ferry_stamp_puttgarden_kinsarvik_oresund_gedser() {
        let crossings = [
            (54.50709, 11.23183, 54.62456, 11.30643, "Puttgarden-Rodby"),
            (60.391, 6.721, 60.425, 6.621, "Kinsarvik-Utne"),
            (56.034, 12.617, 56.043, 12.691, "Helsingor-Helsingborg"),
            (54.574, 11.926, 54.079, 12.135, "Rostock-Gedser"),
        ];
        for (i, (slat, slon, elat, elon, name)) in crossings.iter().enumerate() {
            let id = (i as i64 + 1) * 10;
            let mut pack = test_edge(id, id + 1, *slat, *slon, *elat, *elon);
            pack.highway = Some("unclassified".into());
            pack.length_m = 13_000.0;
            let ferry = named_ferry(id + 2, id + 3, *slat, *slon, *elat, *elon, name);
            assert!(pack_edge_matches_overlay_ferry(&pack, &ferry), "{name}");
            let mut nodes = HashMap::new();
            for (nid, n) in [test_node(id, *slat, *slon), test_node(id + 1, *elat, *elon)] {
                nodes.insert(nid, n);
            }
            let mut g = RouteGraph::from_parts(nodes, vec![pack], RoutingProfile::Car);
            let mut onodes = HashMap::new();
            for (nid, n) in [
                test_node(id + 2, *slat, *slon),
                test_node(id + 3, *elat, *elon),
            ] {
                onodes.insert(nid, n);
            }
            let overlay = RouteGraph::from_parts(onodes, vec![ferry], RoutingProfile::Car);
            stamp_overlay_ferry_costs(&mut g, &overlay, RoutingProfile::Car);
            assert!(g.edges[0].is_ferry, "{name}");
            assert_eq!(g.edges[0].name.as_deref(), Some(*name));
            let mut avoid = RouteOptions::default();
            avoid.avoid_ferries = true;
            assert!(
                !edge_allowed_for_options(&g.edges[0], &avoid, RoutingProfile::Car),
                "avoid_ferries must drop {name}"
            );
        }
    }

    #[test]
    fn path_ferry_and_tunnel_legs_count_contiguous_runs() {
        let mut nodes = HashMap::new();
        for (id, n) in [
            test_node(1, 60.0, 10.00),
            test_node(2, 60.0, 10.01),
            test_node(3, 60.0, 10.02),
            test_node(4, 60.0, 10.03),
            test_node(5, 60.0, 10.04),
            test_node(6, 60.0, 10.05),
        ] {
            nodes.insert(id, n);
        }
        let mut land_a = test_edge(1, 2, 60.0, 10.00, 60.0, 10.01);
        land_a.length_m = 100.0;
        let mut ferry_a = test_edge(2, 3, 60.0, 10.01, 60.0, 10.02);
        ferry_a.is_ferry = true;
        ferry_a.name = Some("Horten-Moss".into());
        ferry_a.length_m = 200.0;
        let mut ferry_b = test_edge(3, 4, 60.0, 10.02, 60.0, 10.03);
        ferry_b.is_ferry = true;
        ferry_b.name = Some("Horten-Moss".into());
        ferry_b.length_m = 150.0;
        let mut tunnel_a = test_edge(4, 5, 60.0, 10.03, 60.0, 10.04);
        tunnel_a.is_tunnel = true;
        tunnel_a.name = Some("Lærdalstunnelen".into());
        tunnel_a.length_m = 300.0;
        let mut tunnel_b = test_edge(5, 6, 60.0, 10.04, 60.0, 10.05);
        tunnel_b.is_tunnel = true;
        tunnel_b.name = Some("Lærdalstunnelen".into());
        tunnel_b.length_m = 50.0;
        let graph = RouteGraph::from_parts(
            nodes,
            vec![land_a, ferry_a, ferry_b, tunnel_a, tunnel_b],
            RoutingProfile::Car,
        );
        let idxs: Vec<usize> = (0..graph.edges.len()).collect();
        let ferries = graph.path_ferry_legs(&idxs);
        assert_eq!(ferries.len(), 1);
        assert_eq!(ferries[0].0, "Horten-Moss");
        assert!((ferries[0].1 - 350.0).abs() < 1e-9);
        let tunnels = graph.path_tunnel_legs(&idxs);
        assert_eq!(tunnels.len(), 1);
        assert_eq!(tunnels[0].0, "Lærdalstunnelen");
        assert!((tunnels[0].1 - 350.0).abs() < 1e-9);
        assert!(graph.path_ferry_legs(&[]).is_empty());
        assert!(graph.path_tunnel_legs(&[]).is_empty());
    }
}
