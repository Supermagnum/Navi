//! Road ∩ track seed finding and track-walk probes (§1).

use std::collections::HashMap;

use driver_break_core::routing::graph::RouteGraph;
use driver_break_core::tracks::haversine_km;
use osm4routing::NodeId;

/// Default short walk along the track away from the road (metres).
pub const DEFAULT_TRACK_WALK_M: f64 = 120.0;

/// Service∩track seeds need the track to continue at least this far past the
/// junction (metres). Below this, treat as a driveway stub and skip.
pub const SERVICE_TRACK_MIN_CONTINUE_M: f64 = 80.0;

/// How far from the route corridor to search for seeds (metres).
pub const CORRIDOR_SEED_RADIUS_M: f64 = 800.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum JunctionRank {
    /// tertiary or unclassified ∩ track
    Preferred = 0,
    /// service ∩ track (downranked)
    Service = 1,
}

#[derive(Debug, Clone)]
pub struct RoadTrackSeed {
    pub lat: f64,
    pub lon: f64,
    pub node: NodeId,
    pub road_highway: String,
    pub rank: JunctionRank,
    /// Track edge index leaving the junction (away from road).
    pub track_edge_idx: usize,
    pub track_continues_m: f64,
}

#[derive(Debug, Clone)]
pub struct ProbePoint {
    pub lat: f64,
    pub lon: f64,
    pub seed: RoadTrackSeed,
    pub walk_m: f64,
}

fn is_real_road(hw: &str) -> Option<JunctionRank> {
    match hw {
        "tertiary" | "unclassified" => Some(JunctionRank::Preferred),
        "service" => Some(JunctionRank::Service),
        _ => None,
    }
}

fn is_track(hw: &str) -> bool {
    hw == "track"
}

fn dist_m(a: (f64, f64), b: (f64, f64)) -> f64 {
    haversine_km(a.0, a.1, b.0, b.1) * 1000.0
}

fn near_corridor(lat: f64, lon: f64, waypoints: &[[f64; 2]], radius_m: f64) -> bool {
    if waypoints.is_empty() {
        return true;
    }
    waypoints
        .iter()
        .any(|w| dist_m((lat, lon), (w[0], w[1])) <= radius_m)
}

fn corridor_bbox(waypoints: &[[f64; 2]], radius_m: f64) -> (f64, f64, f64, f64) {
    if waypoints.is_empty() {
        return (-90.0, 90.0, -180.0, 180.0);
    }
    let pad = (radius_m / 111_320.0) + 0.002;
    let mut min_lat = f64::MAX;
    let mut max_lat = f64::MIN;
    let mut min_lon = f64::MAX;
    let mut max_lon = f64::MIN;
    for w in waypoints {
        min_lat = min_lat.min(w[0]);
        max_lat = max_lat.max(w[0]);
        min_lon = min_lon.min(w[1]);
        max_lon = max_lon.max(w[1]);
    }
    (min_lat - pad, max_lat + pad, min_lon - pad, max_lon + pad)
}

/// Undirected incidence index so junction walks are O(degree), not O(|edges|).
fn undirected_incident(graph: &RouteGraph) -> HashMap<NodeId, Vec<usize>> {
    let mut incident: HashMap<NodeId, Vec<usize>> = HashMap::new();
    for (ei, e) in graph.edges.iter().enumerate() {
        incident.entry(e.source).or_default().push(ei);
        if e.target != e.source {
            incident.entry(e.target).or_default().push(ei);
        }
    }
    incident
}

fn empty_eis() -> &'static [usize] {
    &[]
}

fn incident_eis(index: &HashMap<NodeId, Vec<usize>>, node: NodeId) -> &[usize] {
    index
        .get(&node)
        .map(|v| v.as_slice())
        .unwrap_or(empty_eis())
}

/// Enumerate road∩track junctions near the corridor. Service seeds are included
/// only when the track continues ≥ [`SERVICE_TRACK_MIN_CONTINUE_M`].
pub fn find_road_track_junctions(
    graph: &RouteGraph,
    corridor_waypoints: &[[f64; 2]],
    corridor_radius_m: f64,
) -> Vec<RoadTrackSeed> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let incident = undirected_incident(graph);
    let (min_lat, max_lat, min_lon, max_lon) = corridor_bbox(corridor_waypoints, corridor_radius_m);

    for (&node_id, node) in &graph.nodes {
        let lat = node.coord.y;
        let lon = node.coord.x;
        if lat < min_lat || lat > max_lat || lon < min_lon || lon > max_lon {
            continue;
        }
        if !near_corridor(lat, lon, corridor_waypoints, corridor_radius_m) {
            continue;
        }
        let mut roads: Vec<(usize, JunctionRank, String)> = Vec::new();
        let mut tracks: Vec<usize> = Vec::new();
        for &ei in incident_eis(&incident, node_id) {
            let e = &graph.edges[ei];
            let Some(ref hw) = e.highway else { continue };
            if is_track(hw) {
                if !tracks.contains(&ei) {
                    tracks.push(ei);
                }
            } else if let Some(rank) = is_real_road(hw) {
                if !roads.iter().any(|(i, _, _)| *i == ei) {
                    roads.push((ei, rank, hw.clone()));
                }
            }
        }

        if roads.is_empty() || tracks.is_empty() {
            continue;
        }

        // Best (lowest) road rank at this node.
        let best_rank = roads.iter().map(|(_, r, _)| *r).min().unwrap();
        let road_hw = roads
            .iter()
            .find(|(_, r, _)| *r == best_rank)
            .map(|(_, _, h)| h.clone())
            .unwrap();

        for &tei in &tracks {
            let continues = track_continue_length_m(graph, &incident, node_id, tei);
            if best_rank == JunctionRank::Service && continues < SERVICE_TRACK_MIN_CONTINUE_M {
                continue;
            }
            let key = (node_id.0, tei);
            if !seen.insert(key) {
                continue;
            }
            out.push(RoadTrackSeed {
                lat: node.coord.y,
                lon: node.coord.x,
                node: node_id,
                road_highway: road_hw.clone(),
                rank: best_rank,
                track_edge_idx: tei,
                track_continues_m: continues,
            });
        }
    }

    out.sort_by(|a, b| {
        a.rank.cmp(&b.rank).then(
            b.track_continues_m
                .partial_cmp(&a.track_continues_m)
                .unwrap(),
        )
    });
    out
}

fn track_continue_length_m(
    graph: &RouteGraph,
    incident: &HashMap<NodeId, Vec<usize>>,
    from: NodeId,
    edge_idx: usize,
) -> f64 {
    let e = &graph.edges[edge_idx];
    let mut total = e.length_m;
    let mut cur = if e.source == from { e.target } else { e.source };
    let mut prev = from;
    for _ in 0..8 {
        let mut next_e: Option<usize> = None;
        for &ei in incident_eis(incident, cur) {
            let ed = &graph.edges[ei];
            let other = if ed.source == cur {
                ed.target
            } else {
                ed.source
            };
            if other == prev {
                continue;
            }
            if ed.highway.as_deref() == Some("track") {
                next_e = Some(ei);
                break;
            }
        }
        let Some(ei) = next_e else { break };
        let ed = &graph.edges[ei];
        total += ed.length_m;
        let other = if ed.source == cur {
            ed.target
        } else {
            ed.source
        };
        prev = cur;
        cur = other;
        if total >= SERVICE_TRACK_MIN_CONTINUE_M * 3.0 {
            break;
        }
    }
    total
}

/// Walk along the track away from the road junction, emitting a probe near
/// `walk_m` (or pack min road distance if larger).
pub fn probe_along_track(
    graph: &RouteGraph,
    seed: &RoadTrackSeed,
    walk_m: f64,
    pack_min_road_m: Option<f64>,
) -> Option<ProbePoint> {
    let target_m = pack_min_road_m.unwrap_or(0.0).max(walk_m);
    if seed.track_continues_m + 1.0 < target_m && pack_min_road_m.is_some() {
        // Track does not reach pack minimum road distance.
        return None;
    }
    let goal = target_m.min(seed.track_continues_m.max(walk_m * 0.5));
    let incident = undirected_incident(graph);
    probe_along_track_indexed(graph, &incident, seed, goal)
}

/// Walk every seed using one undirected index (not rebuilt per seed).
pub fn probe_along_tracks(
    graph: &RouteGraph,
    seeds: &[RoadTrackSeed],
    walk_m: f64,
    pack_min_road_m: Option<f64>,
) -> Vec<ProbePoint> {
    let incident = undirected_incident(graph);
    seeds
        .iter()
        .filter_map(|seed| {
            let target_m = pack_min_road_m.unwrap_or(0.0).max(walk_m);
            if seed.track_continues_m + 1.0 < target_m && pack_min_road_m.is_some() {
                return None;
            }
            let goal = target_m.min(seed.track_continues_m.max(walk_m * 0.5));
            probe_along_track_indexed(graph, &incident, seed, goal)
        })
        .collect()
}

fn probe_along_track_indexed(
    graph: &RouteGraph,
    incident: &HashMap<NodeId, Vec<usize>>,
    seed: &RoadTrackSeed,
    goal: f64,
) -> Option<ProbePoint> {
    let mut travelled = 0.0;
    let mut cur = seed.node;
    let mut edge_idx = seed.track_edge_idx;

    loop {
        let ed = &graph.edges[edge_idx];
        let next = if ed.source == cur {
            ed.target
        } else {
            ed.source
        };
        let next_node = graph.nodes.get(&next)?;
        let seg = ed.length_m;
        if travelled + seg >= goal {
            let frac = ((goal - travelled) / seg.max(1e-3)).clamp(0.0, 1.0);
            let cur_n = graph.nodes.get(&cur)?;
            let lat = cur_n.coord.y + (next_node.coord.y - cur_n.coord.y) * frac;
            let lon = cur_n.coord.x + (next_node.coord.x - cur_n.coord.x) * frac;
            return Some(ProbePoint {
                lat,
                lon,
                seed: seed.clone(),
                walk_m: goal,
            });
        }
        travelled += seg;
        let back = cur;
        cur = next;

        let mut found = None;
        for &ei in incident_eis(incident, cur) {
            let ed2 = &graph.edges[ei];
            let other = if ed2.source == cur {
                ed2.target
            } else {
                ed2.source
            };
            if other == back {
                continue;
            }
            if ed2.highway.as_deref() == Some("track") {
                found = Some(ei);
                break;
            }
        }
        edge_idx = found?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_ranks_below_preferred() {
        assert!(JunctionRank::Preferred < JunctionRank::Service);
    }

    #[test]
    fn service_stub_constant_documented() {
        const {
            assert!(SERVICE_TRACK_MIN_CONTINUE_M > 0.0);
            assert!(DEFAULT_TRACK_WALK_M > 0.0);
        }
    }
}
