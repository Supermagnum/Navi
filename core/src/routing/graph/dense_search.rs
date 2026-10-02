//! Dense-index A* over flat CSR adjacency (no OSM-id HashMap in the search loop).
//!
//! [`RouteGraph`] still stores nodes keyed by OSM id for snap / overlays; search
//! remaps once at adjacency rebuild into:
//! - `dense_ids` / `id_to_dense`
//! - CSR `adj_off` + `adj_edge`
//! - parallel lat/lon for the heuristic
//!
//! Surface-aware car/truck search keys states as `node_idx * 4 + surface_rank`
//! and stores the incoming edge index in a parallel parent array (not in the
//! open-set key), matching shortest-path semantics while keeping flat arrays.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use osm4routing::NodeId;

use super::builder::{RouteGraph, RouteOptions};
use super::surface_quality::{
    surface_transition_cost_m, SurfaceQuality, SurfaceRoutingMode, SNAP_VIRTUAL_APPROACH_SURFACE,
};

const SURFACES: u32 = 4;
const NO_PARENT: u32 = u32::MAX;
const NO_EDGE: u32 = u32::MAX;

#[derive(Copy, Clone, Eq, PartialEq)]
struct OpenEntry {
    f: u64,
    g: u64,
    state: u32,
}

impl Ord for OpenEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        // Min-heap on f, then g.
        other
            .f
            .cmp(&self.f)
            .then_with(|| other.g.cmp(&self.g))
            .then_with(|| self.state.cmp(&other.state))
    }
}

impl PartialOrd for OpenEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn cost_to_u64(cost: f64) -> u64 {
    (cost.max(0.0) * 1000.0).round() as u64
}

fn state_of(node_idx: u32, surface: SurfaceQuality) -> u32 {
    node_idx * SURFACES + u32::from(surface.rank())
}

fn node_of(state: u32) -> u32 {
    state / SURFACES
}

fn surface_of(state: u32) -> SurfaceQuality {
    match state % SURFACES {
        0 => SurfaceQuality::Good,
        1 => SurfaceQuality::Unknown,
        2 => SurfaceQuality::Marginal,
        _ => SurfaceQuality::Poor,
    }
}

/// Result of dense A*: OSM node path, edge indices, cost (same units as pathfinding).
pub(crate) struct DensePath {
    pub nodes: Vec<NodeId>,
    pub edges: Vec<usize>,
    pub cost: f64,
    pub expansions: u64,
}

/// Search outcome including expansion count on disconnect / cancel.
pub(crate) enum DenseSearchOutcome {
    Found(DensePath),
    Failed { expansions: u64 },
}

impl RouteGraph {
    /// Remap OSM ids → dense indices + CSR adjacency + coord arrays.
    pub(crate) fn rebuild_dense_index(&mut self) {
        // Include every endpoint that appears on an edge (tests sometimes pass
        // edges with an empty `nodes` map; snap/heuristic use coords when present).
        let mut id_set: std::collections::BTreeSet<i64> =
            self.nodes.keys().map(|id| id.0).collect();
        for e in &self.edges {
            id_set.insert(e.source.0);
            id_set.insert(e.target.0);
        }
        let n = id_set.len();
        self.dense_ids.clear();
        self.dense_ids.reserve(n);
        self.id_to_dense.clear();
        self.id_to_dense.reserve(n);
        self.dense_lat.clear();
        self.dense_lon.clear();
        self.dense_lat.reserve(n);
        self.dense_lon.reserve(n);
        self.dense_blocked.clear();
        self.dense_blocked.resize(n, false);

        for (i, osm) in id_set.into_iter().enumerate() {
            let id = NodeId(osm);
            let idx = i as u32;
            self.id_to_dense.insert(id, idx);
            self.dense_ids.push(id);
            if let Some(node) = self.nodes.get(&id) {
                self.dense_lat.push(node.coord.y);
                self.dense_lon.push(node.coord.x);
            } else {
                // Fall back to any incident edge endpoint so the heuristic is finite.
                let (lat, lon) = self
                    .edges
                    .iter()
                    .find_map(|e| {
                        if e.source == id {
                            Some((e.start_lat, e.start_lon))
                        } else if e.target == id {
                            Some((e.end_lat, e.end_lon))
                        } else {
                            None
                        }
                    })
                    .unwrap_or((0.0, 0.0));
                self.dense_lat.push(lat);
                self.dense_lon.push(lon);
            }
        }
        for &id in &self.access_blocked_nodes {
            if let Some(&idx) = self.id_to_dense.get(&id) {
                self.dense_blocked[idx as usize] = true;
            }
        }

        let n = self.dense_ids.len();
        let mut degrees = vec![0u32; n];
        for e in &self.edges {
            if let Some(&s) = self.id_to_dense.get(&e.source) {
                degrees[s as usize] += 1;
            }
        }
        self.adj_off.clear();
        self.adj_off.resize(n + 1, 0);
        let mut sum = 0u32;
        for (i, d) in degrees.iter().enumerate() {
            self.adj_off[i] = sum;
            sum = sum.saturating_add(*d);
        }
        self.adj_off[n] = sum;
        self.adj_edge.clear();
        self.adj_edge.resize(sum as usize, 0);
        let mut cursor = self.adj_off.clone();
        cursor.truncate(n);
        for (ei, e) in self.edges.iter().enumerate() {
            let Some(&s) = self.id_to_dense.get(&e.source) else {
                continue;
            };
            let slot = cursor[s as usize] as usize;
            self.adj_edge[slot] = ei as u32;
            cursor[s as usize] += 1;
        }
    }

    pub(crate) fn dense_of(&self, id: NodeId) -> Option<u32> {
        self.id_to_dense.get(&id).copied()
    }

    /// Surface-aware A* (car/truck default). Keys by dense node × surface.
    pub(crate) fn dense_astar_surface(
        &self,
        start: NodeId,
        goal: NodeId,
        use_eco: bool,
        options: &RouteOptions,
        heuristic_per_m: f64,
        surface_mode: SurfaceRoutingMode,
    ) -> DenseSearchOutcome {
        let Some(start_i) = self.dense_of(start) else {
            return DenseSearchOutcome::Failed { expansions: 0 };
        };
        let Some(goal_i) = self.dense_of(goal) else {
            return DenseSearchOutcome::Failed { expansions: 0 };
        };
        let n = self.dense_ids.len();
        if n == 0 {
            return DenseSearchOutcome::Failed { expansions: 0 };
        }
        let states = n * SURFACES as usize;
        let mut g_score = vec![u64::MAX; states];
        let mut parent_state = vec![NO_PARENT; states];
        let mut parent_edge = vec![NO_EDGE; states];
        let start_state = state_of(start_i, SNAP_VIRTUAL_APPROACH_SURFACE);
        g_score[start_state as usize] = 0;
        let h0 = cost_to_u64(
            haversine_dense(
                self.dense_lat[start_i as usize],
                self.dense_lon[start_i as usize],
                self.dense_lat[goal_i as usize],
                self.dense_lon[goal_i as usize],
            ) * heuristic_per_m,
        );
        let mut open = BinaryHeap::new();
        open.push(OpenEntry {
            f: h0,
            g: 0,
            state: start_state,
        });
        let plan_id = crate::download::plan_cancel::current_plan_id();
        let mut expansions = 0u64;

        while let Some(OpenEntry { g, state, .. }) = open.pop() {
            if g > g_score[state as usize] {
                continue;
            }
            let node_i = node_of(state);
            if node_i == goal_i {
                return DenseSearchOutcome::Found(reconstruct(
                    self,
                    state,
                    &parent_state,
                    &parent_edge,
                    g,
                    expansions,
                ));
            }
            expansions += 1;
            if plan_id != 0
                && expansions & 2047 == 0
                && crate::download::plan_cancel::is_cancelled_id(plan_id)
            {
                return DenseSearchOutcome::Failed { expansions };
            }
            if self.dense_blocked[node_i as usize] && node_i != start_i {
                continue;
            }
            let prev_surface = surface_of(state);
            let a0 = self.adj_off[node_i as usize] as usize;
            let a1 = self.adj_off[node_i as usize + 1] as usize;
            for &ei in &self.adj_edge[a0..a1] {
                let edge = &self.edges[ei as usize];
                if !super::builder::edge_allowed_for_options(edge, options, self.profile()) {
                    continue;
                }
                let Some(tgt_i) = self.dense_of(edge.target) else {
                    continue;
                };
                let base = super::builder::edge_travel_cost(edge, ei as usize, use_eco, options);
                let transition = surface_transition_cost_m(
                    Some(prev_surface),
                    edge.surface_quality,
                    surface_mode,
                );
                let move_cost = cost_to_u64(base + transition);
                let new_g = g.saturating_add(move_cost);
                let new_state = state_of(tgt_i, edge.surface_quality);
                let slot = new_state as usize;
                if new_g >= g_score[slot] {
                    continue;
                }
                g_score[slot] = new_g;
                parent_state[slot] = state;
                parent_edge[slot] = ei;
                let h = cost_to_u64(
                    haversine_dense(
                        self.dense_lat[tgt_i as usize],
                        self.dense_lon[tgt_i as usize],
                        self.dense_lat[goal_i as usize],
                        self.dense_lon[goal_i as usize],
                    ) * heuristic_per_m,
                );
                open.push(OpenEntry {
                    f: new_g.saturating_add(h),
                    g: new_g,
                    state: new_state,
                });
            }
        }
        DenseSearchOutcome::Failed { expansions }
    }

    /// Non-surface A* (foot/bike / offroad motor). Keys by dense node only.
    pub(crate) fn dense_astar_simple(
        &self,
        start: NodeId,
        goal: NodeId,
        use_eco: bool,
        options: &RouteOptions,
        heuristic_per_m: f64,
    ) -> DenseSearchOutcome {
        let Some(start_i) = self.dense_of(start) else {
            return DenseSearchOutcome::Failed { expansions: 0 };
        };
        let Some(goal_i) = self.dense_of(goal) else {
            return DenseSearchOutcome::Failed { expansions: 0 };
        };
        let n = self.dense_ids.len();
        if n == 0 {
            return DenseSearchOutcome::Failed { expansions: 0 };
        }
        let mut g_score = vec![u64::MAX; n];
        let mut parent_node = vec![NO_PARENT; n];
        let mut parent_edge = vec![NO_EDGE; n];
        g_score[start_i as usize] = 0;
        let h0 = cost_to_u64(
            haversine_dense(
                self.dense_lat[start_i as usize],
                self.dense_lon[start_i as usize],
                self.dense_lat[goal_i as usize],
                self.dense_lon[goal_i as usize],
            ) * heuristic_per_m,
        );
        let mut open = BinaryHeap::new();
        open.push(OpenEntry {
            f: h0,
            g: 0,
            state: start_i,
        });
        let plan_id = crate::download::plan_cancel::current_plan_id();
        let mut expansions = 0u64;

        while let Some(OpenEntry {
            g, state: node_i, ..
        }) = open.pop()
        {
            if g > g_score[node_i as usize] {
                continue;
            }
            if node_i == goal_i {
                return DenseSearchOutcome::Found(reconstruct_simple(
                    self,
                    node_i,
                    &parent_node,
                    &parent_edge,
                    g,
                    expansions,
                ));
            }
            expansions += 1;
            if plan_id != 0
                && expansions & 2047 == 0
                && crate::download::plan_cancel::is_cancelled_id(plan_id)
            {
                return DenseSearchOutcome::Failed { expansions };
            }
            if self.dense_blocked[node_i as usize] && node_i != start_i {
                continue;
            }
            let a0 = self.adj_off[node_i as usize] as usize;
            let a1 = self.adj_off[node_i as usize + 1] as usize;
            for &ei in &self.adj_edge[a0..a1] {
                let edge = &self.edges[ei as usize];
                if !super::builder::edge_allowed_for_options(edge, options, self.profile()) {
                    continue;
                }
                let Some(tgt_i) = self.dense_of(edge.target) else {
                    continue;
                };
                let move_cost = cost_to_u64(super::builder::edge_travel_cost(
                    edge,
                    ei as usize,
                    use_eco,
                    options,
                ));
                let new_g = g.saturating_add(move_cost);
                let slot = tgt_i as usize;
                if new_g >= g_score[slot] {
                    continue;
                }
                g_score[slot] = new_g;
                parent_node[slot] = node_i;
                parent_edge[slot] = ei;
                let h = cost_to_u64(
                    haversine_dense(
                        self.dense_lat[tgt_i as usize],
                        self.dense_lon[tgt_i as usize],
                        self.dense_lat[goal_i as usize],
                        self.dense_lon[goal_i as usize],
                    ) * heuristic_per_m,
                );
                open.push(OpenEntry {
                    f: new_g.saturating_add(h),
                    g: new_g,
                    state: tgt_i,
                });
            }
        }
        DenseSearchOutcome::Failed { expansions }
    }
}

fn reconstruct(
    g: &RouteGraph,
    mut state: u32,
    parent_state: &[u32],
    parent_edge: &[u32],
    cost: u64,
    expansions: u64,
) -> DensePath {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    loop {
        nodes.push(g.dense_ids[node_of(state) as usize]);
        let pe = parent_edge[state as usize];
        let ps = parent_state[state as usize];
        if ps == NO_PARENT {
            break;
        }
        edges.push(pe as usize);
        state = ps;
    }
    nodes.reverse();
    edges.reverse();
    DensePath {
        nodes,
        edges,
        cost: cost as f64 / 1000.0,
        expansions,
    }
}

fn reconstruct_simple(
    g: &RouteGraph,
    mut node_i: u32,
    parent_node: &[u32],
    parent_edge: &[u32],
    cost: u64,
    expansions: u64,
) -> DensePath {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    loop {
        nodes.push(g.dense_ids[node_i as usize]);
        let pe = parent_edge[node_i as usize];
        let pn = parent_node[node_i as usize];
        if pn == NO_PARENT {
            break;
        }
        edges.push(pe as usize);
        node_i = pn;
    }
    nodes.reverse();
    edges.reverse();
    DensePath {
        nodes,
        edges,
        cost: cost as f64 / 1000.0,
        expansions,
    }
}

fn haversine_dense(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    // Match builder::haversine_latlon_m (WGS84 mean radius used there).
    const R: f64 = 6_378_100.0;
    let (lat1, lon1, lat2, lon2) = (
        lat1.to_radians(),
        lon1.to_radians(),
        lat2.to_radians(),
        lon2.to_radians(),
    );
    let dlat = lat2 - lat1;
    let dlon = lon2 - lon1;
    let a = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    2.0 * R * a.sqrt().asin()
}
