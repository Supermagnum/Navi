//! Build simulation samples and turn maneuvers along a planned graph path.

use osm4routing::NodeId;
use serde::Serialize;

use crate::nav::{prefer_street_label, ManeuverKind};
use crate::routing::eta::{edge_speed_kmh, highway_fallback_kmh};
use crate::routing::graph::RouteGraph;

/// Minimum absolute turn angle (degrees) to emit a maneuver (skip gentle bends).
const MIN_TURN_DEG: f64 = 25.0;
/// Sample spacing along each edge for speed-limit playback (metres).
const SAMPLE_STEP_M: f64 = 20.0;

#[derive(Debug, Clone, Serialize)]
pub struct SimSample {
    pub lat: f64,
    pub lon: f64,
    pub cum_m: f64,
    pub speed_kmh: f64,
    pub highway: Option<String>,
    /// True when OSM `maxspeed` was present on the edge (not highway-class fallback).
    pub maxspeed_posted: bool,
    /// Current-road label: OSM `name`, else `ref`, else null (UI applies highway-class label).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub street: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RouteManeuver {
    pub lat: f64,
    pub lon: f64,
    pub cum_m: f64,
    pub kind: String,
    pub street: Option<String>,
    pub roundabout_exit: Option<u8>,
}

fn bearing_deg(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let p1 = lat1.to_radians();
    let p2 = lat2.to_radians();
    let dl = (lon2 - lon1).to_radians();
    let y = dl.sin() * p2.cos();
    let x = p1.cos() * p2.sin() - p1.sin() * p2.cos() * dl.cos();
    let mut brng = y.atan2(x).to_degrees();
    if brng < 0.0 {
        brng += 360.0;
    }
    brng
}

fn turn_delta_deg(in_brng: f64, out_brng: f64) -> f64 {
    let mut d = out_brng - in_brng;
    while d > 180.0 {
        d -= 360.0;
    }
    while d < -180.0 {
        d += 360.0;
    }
    d
}

fn classify_turn(delta_deg: f64) -> ManeuverKind {
    let a = delta_deg.abs();
    if a < MIN_TURN_DEG {
        return ManeuverKind::Straight;
    }
    let left = delta_deg < 0.0;
    if a < 45.0 {
        if left {
            ManeuverKind::SlightLeft
        } else {
            ManeuverKind::SlightRight
        }
    } else if a < 135.0 {
        if left {
            ManeuverKind::Left
        } else {
            ManeuverKind::Right
        }
    } else if a < 160.0 {
        if left {
            ManeuverKind::SharpLeft
        } else {
            ManeuverKind::SharpRight
        }
    } else {
        ManeuverKind::UTurn
    }
}

fn kind_key(kind: ManeuverKind) -> &'static str {
    match kind {
        ManeuverKind::Left => "left",
        ManeuverKind::Right => "right",
        ManeuverKind::Straight => "straight",
        ManeuverKind::SlightLeft => "slight_left",
        ManeuverKind::SlightRight => "slight_right",
        ManeuverKind::SharpLeft => "sharp_left",
        ManeuverKind::SharpRight => "sharp_right",
        ManeuverKind::Roundabout => "roundabout",
        ManeuverKind::ExitLeft => "exit_left",
        ManeuverKind::ExitRight => "exit_right",
        ManeuverKind::MergeLeft => "merge_left",
        ManeuverKind::MergeRight => "merge_right",
        ManeuverKind::UTurn => "u_turn",
        ManeuverKind::Destination => "destination",
        ManeuverKind::KeepLeft => "keep_left",
        ManeuverKind::KeepRight => "keep_right",
        ManeuverKind::Unknown => "unknown",
    }
}

pub fn parse_maneuver_kind(raw: &str) -> ManeuverKind {
    match raw.trim().to_ascii_lowercase().as_str() {
        "left" => ManeuverKind::Left,
        "right" => ManeuverKind::Right,
        "straight" => ManeuverKind::Straight,
        "slight_left" => ManeuverKind::SlightLeft,
        "slight_right" => ManeuverKind::SlightRight,
        "sharp_left" => ManeuverKind::SharpLeft,
        "sharp_right" => ManeuverKind::SharpRight,
        "roundabout" => ManeuverKind::Roundabout,
        "exit_left" => ManeuverKind::ExitLeft,
        "exit_right" => ManeuverKind::ExitRight,
        "merge_left" => ManeuverKind::MergeLeft,
        "merge_right" => ManeuverKind::MergeRight,
        "u_turn" => ManeuverKind::UTurn,
        "destination" => ManeuverKind::Destination,
        "keep_left" => ManeuverKind::KeepLeft,
        "keep_right" => ManeuverKind::KeepRight,
        _ => ManeuverKind::Unknown,
    }
}

fn interpolate(lat0: f64, lon0: f64, lat1: f64, lon1: f64, t: f64) -> (f64, f64) {
    (lat0 + (lat1 - lat0) * t, lon0 + (lon1 - lon0) * t)
}

/// Densify the A* node path into speed-tagged samples for debug playback.
/// Densify a lat/lon polyline into simulation samples at [SAMPLE_STEP_M] spacing.
///
/// Used for staged overlay polylines (no graph edges) and hiking fixtures that
/// only ship a simplified corridor polyline.
pub fn build_sim_samples_from_lat_lon(
    coords_lat_lon: &[(f64, f64)],
    speed_kmh: f64,
    highway: Option<&str>,
) -> Vec<SimSample> {
    let speed = speed_kmh.max(1.0);
    let hwy = highway.map(|s| s.to_string());
    let mut out = Vec::new();
    if coords_lat_lon.len() < 2 {
        return out;
    }
    let mut seg_lens = Vec::with_capacity(coords_lat_lon.len() - 1);
    let mut total = 0.0;
    for w in coords_lat_lon.windows(2) {
        let d = haversine_m_local(w[0].0, w[0].1, w[1].0, w[1].1);
        seg_lens.push(d);
        total += d;
    }
    if total < 1.0 {
        let (lat, lon) = coords_lat_lon[0];
        out.push(SimSample {
            lat,
            lon,
            cum_m: 0.0,
            speed_kmh: speed,
            highway: hwy.clone(),
            maxspeed_posted: false,
            street: None,
        });
        return out;
    }
    let steps = ((total / SAMPLE_STEP_M).ceil() as usize).max(1);
    for s in 0..=steps {
        let along = (total * (s as f64 / steps as f64)).min(total);
        let (lat, lon) = point_along_verts(coords_lat_lon, &seg_lens, along);
        if out
            .last()
            .map(|p| (p.cum_m - along).abs() < 0.5)
            .unwrap_or(false)
        {
            continue;
        }
        out.push(SimSample {
            lat,
            lon,
            cum_m: along,
            speed_kmh: speed,
            highway: hwy.clone(),
            maxspeed_posted: false,
            street: None,
        });
    }
    out
}

pub fn build_sim_samples(graph: &RouteGraph, path: &[NodeId]) -> Vec<SimSample> {
    let mut out = Vec::new();
    if path.len() < 2 {
        return out;
    }
    let mut cum = 0.0;
    for w in path.windows(2) {
        let Some(idx) = graph.edge_index(w[0], w[1]) else {
            continue;
        };
        let e = &graph.edges[idx];
        let speed = edge_speed_kmh(e).max(1.0);
        let posted = e
            .maxspeed_kmh
            .filter(|v| v.is_finite() && *v > 0.0)
            .is_some();
        let street = prefer_street_label(e.name.as_deref(), e.road_ref.as_deref());
        // Follow OSM shape when present so samples stay on the road corridor.
        let mut verts: Vec<(f64, f64)> = Vec::with_capacity(e.shape.len() + 2);
        verts.push((e.start_lat, e.start_lon));
        for &(lon, lat) in &e.shape {
            verts.push((lat, lon));
        }
        verts.push((e.end_lat, e.end_lon));
        let mut seg_lens = Vec::with_capacity(verts.len().saturating_sub(1));
        let mut edge_len = 0.0;
        for vw in verts.windows(2) {
            let d = haversine_m_local(vw[0].0, vw[0].1, vw[1].0, vw[1].1);
            seg_lens.push(d);
            edge_len += d;
        }
        let use_len = if edge_len > 1.0 {
            edge_len
        } else {
            e.length_m.max(1.0)
        };
        let steps = ((use_len / SAMPLE_STEP_M).ceil() as usize).max(1);
        for s in 0..steps {
            let along_edge = use_len * (s as f64 / steps as f64);
            let (lat, lon) = point_along_verts(&verts, &seg_lens, along_edge);
            let along = cum + along_edge;
            if out
                .last()
                .map(|p| (p.cum_m - along).abs() < 0.5)
                .unwrap_or(false)
            {
                continue;
            }
            out.push(SimSample {
                lat,
                lon,
                cum_m: along,
                speed_kmh: speed,
                highway: e.highway.clone(),
                maxspeed_posted: posted,
                street: street.clone(),
            });
        }
        cum += e.length_m;
    }
    if let Some(last) = path.last().and_then(|id| graph.nodes.get(id)) {
        let e_last = path
            .windows(2)
            .rev()
            .find_map(|w| graph.edge_index(w[0], w[1]).map(|i| &graph.edges[i]));
        out.push(SimSample {
            lat: last.coord.y,
            lon: last.coord.x,
            cum_m: cum,
            speed_kmh: e_last
                .map(edge_speed_kmh)
                .unwrap_or_else(|| highway_fallback_kmh(None)),
            highway: e_last.and_then(|e| e.highway.clone()),
            maxspeed_posted: e_last
                .and_then(|e| e.maxspeed_kmh)
                .filter(|v| v.is_finite() && *v > 0.0)
                .is_some(),
            street: e_last
                .and_then(|e| prefer_street_label(e.name.as_deref(), e.road_ref.as_deref())),
        });
    }
    out
}

fn haversine_m_local(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let lat1 = lat1.to_radians();
    let lat2 = lat2.to_radians();
    let dlat = lat2 - lat1;
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    2.0 * 6_378_100.0 * a.sqrt().asin()
}

fn point_along_verts(verts: &[(f64, f64)], seg_lens: &[f64], along_m: f64) -> (f64, f64) {
    if verts.is_empty() {
        return (0.0, 0.0);
    }
    if verts.len() == 1 || seg_lens.is_empty() {
        return verts[0];
    }
    let mut left = along_m.max(0.0);
    for (i, &len) in seg_lens.iter().enumerate() {
        if left <= len || i + 1 == seg_lens.len() {
            let t = if len < 1e-6 {
                0.0
            } else {
                (left / len).clamp(0.0, 1.0)
            };
            let (lat0, lon0) = verts[i];
            let (lat1, lon1) = verts[i + 1];
            return interpolate(lat0, lon0, lat1, lon1, t);
        }
        left -= len;
    }
    *verts.last().unwrap()
}

/// Geometric turn list + destination at the path end.
///
/// Roundabout ring edges (`GraphEdge::is_roundabout`) produce a single
/// [`ManeuverKind::Roundabout`] at entry with a computed exit number; angle
/// changes along the ring (and the leave turn) are not emitted as separate
/// left/right maneuvers.
pub fn build_maneuvers(graph: &RouteGraph, path: &[NodeId]) -> Vec<RouteManeuver> {
    let mut out = Vec::new();
    if path.len() < 2 {
        return out;
    }
    let spans = find_roundabout_spans(graph, path);
    let mut cum = 0.0;
    for i in 0..path.len().saturating_sub(2) {
        let n0 = path[i];
        let n1 = path[i + 1];
        let n2 = path[i + 2];
        let Some(e_in) = graph.edge_index(n0, n1).map(|idx| &graph.edges[idx]) else {
            continue;
        };
        let Some(e_out) = graph.edge_index(n1, n2).map(|idx| &graph.edges[idx]) else {
            cum += e_in.length_m;
            continue;
        };
        cum += e_in.length_m;
        let turn_node_idx = i + 1;

        if let Some(span) = spans.iter().find(|s| s.entry_idx == turn_node_idx) {
            let leave_edge = graph
                .edge_index(path[span.leave_idx], path[span.leave_idx + 1])
                .map(|idx| &graph.edges[idx]);
            let street = leave_edge
                .and_then(|e| prefer_street_label(e.name.as_deref(), e.road_ref.as_deref()));
            let node = &graph.nodes[&n1];
            out.push(RouteManeuver {
                lat: node.coord.y,
                lon: node.coord.x,
                cum_m: cum,
                kind: kind_key(ManeuverKind::Roundabout).to_string(),
                street,
                roundabout_exit: Some(span.exit_number),
            });
            continue;
        }
        if spans
            .iter()
            .any(|s| turn_node_idx > s.entry_idx && turn_node_idx <= s.leave_idx)
        {
            // Internal ring vertices and the leave turn — covered by entry maneuver.
            continue;
        }

        let in_b = bearing_deg(e_in.start_lat, e_in.start_lon, e_in.end_lat, e_in.end_lon);
        let out_b = bearing_deg(
            e_out.start_lat,
            e_out.start_lon,
            e_out.end_lat,
            e_out.end_lon,
        );
        let delta = turn_delta_deg(in_b, out_b);
        let kind = classify_turn(delta);
        if kind == ManeuverKind::Straight {
            continue;
        }
        let node = &graph.nodes[&n1];
        out.push(RouteManeuver {
            lat: node.coord.y,
            lon: node.coord.x,
            cum_m: cum,
            kind: kind_key(kind).to_string(),
            street: prefer_street_label(e_out.name.as_deref(), e_out.road_ref.as_deref()),
            roundabout_exit: None,
        });
    }
    // Destination at end.
    if let Some(last) = path.last() {
        let node = &graph.nodes[last];
        let total = path
            .windows(2)
            .filter_map(|w| {
                graph
                    .edge_index(w[0], w[1])
                    .map(|i| graph.edges[i].length_m)
            })
            .sum::<f64>();
        out.push(RouteManeuver {
            lat: node.coord.y,
            lon: node.coord.x,
            cum_m: total,
            kind: kind_key(ManeuverKind::Destination).to_string(),
            street: None,
            roundabout_exit: None,
        });
    }
    out
}

#[derive(Debug, Clone, Copy)]
struct RoundaboutSpan {
    /// Path index of the node where the route enters the ring.
    entry_idx: usize,
    /// Path index of the node where the route leaves the ring.
    leave_idx: usize,
    /// 1-based exit number (clamped to 1..=8 for icon range).
    exit_number: u8,
}

fn edge_is_roundabout(graph: &RouteGraph, from: NodeId, to: NodeId) -> bool {
    graph
        .edge_index(from, to)
        .map(|i| graph.edges[i].is_roundabout)
        .unwrap_or(false)
}

fn find_roundabout_spans(graph: &RouteGraph, path: &[NodeId]) -> Vec<RoundaboutSpan> {
    let mut spans = Vec::new();
    let mut i = 0usize;
    while i + 1 < path.len() {
        if !edge_is_roundabout(graph, path[i], path[i + 1]) {
            i += 1;
            continue;
        }
        let entered_from_outside =
            i == 0 || !edge_is_roundabout(graph, path[i.saturating_sub(1)], path[i]);
        if !entered_from_outside {
            i += 1;
            continue;
        }
        let entry_idx = i;
        let mut leave_idx = i + 1;
        while leave_idx + 1 < path.len()
            && edge_is_roundabout(graph, path[leave_idx], path[leave_idx + 1])
        {
            leave_idx += 1;
        }
        // leave_idx is the last path node still arrived-at via a roundabout edge.
        // If path continues, path[leave_idx] -> path[leave_idx+1] leaves the ring.
        if leave_idx + 1 >= path.len() {
            // Route ends on the ring — still emit an entry with best-effort exit.
            let exit_number = count_roundabout_exit(graph, path, entry_idx, leave_idx);
            spans.push(RoundaboutSpan {
                entry_idx,
                leave_idx,
                exit_number,
            });
            break;
        }
        let exit_number = count_roundabout_exit(graph, path, entry_idx, leave_idx);
        spans.push(RoundaboutSpan {
            entry_idx,
            leave_idx,
            exit_number,
        });
        i = leave_idx;
        if i == entry_idx {
            i += 1;
        }
    }
    spans
}

/// Count genuine leave-roads from after entry through the taken exit (inclusive).
fn count_roundabout_exit(
    graph: &RouteGraph,
    path: &[NodeId],
    entry_idx: usize,
    leave_idx: usize,
) -> u8 {
    let mut n = 0u8;
    for i in (entry_idx + 1)..=leave_idx {
        let node = path[i];
        let prev = path[i - 1];
        let Some(prev_n) = graph.nodes.get(&prev) else {
            continue;
        };
        let Some(node_n) = graph.nodes.get(&node) else {
            continue;
        };
        let in_brng = bearing_deg(
            prev_n.coord.y,
            prev_n.coord.x,
            node_n.coord.y,
            node_n.coord.x,
        );

        let mut exits: Vec<(f64, NodeId)> = Vec::new();
        for &ei in graph.outgoing_edge_indices(node) {
            let e = &graph.edges[ei];
            if e.is_roundabout {
                continue;
            }
            if e.target == prev {
                continue;
            }
            let Some(tn) = graph.nodes.get(&e.target) else {
                continue;
            };
            let out_b = bearing_deg(node_n.coord.y, node_n.coord.x, tn.coord.y, tn.coord.x);
            let delta = turn_delta_deg(in_brng, out_b);
            exits.push((delta, e.target));
        }
        // Right-hand traffic: count exits from rightmost (largest +delta) toward left.
        exits.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        for &(_, target) in &exits {
            n = n.saturating_add(1);
            if i == leave_idx && i + 1 < path.len() && target == path[i + 1] {
                return n.clamp(1, 8);
            }
        }
    }
    n.clamp(1, 8)
}

pub fn samples_to_json(samples: &[SimSample]) -> String {
    serde_json::to_string(samples).unwrap_or_else(|_| "[]".into())
}

pub fn maneuvers_to_json(maneuvers: &[RouteManeuver]) -> String {
    serde_json::to_string(maneuvers).unwrap_or_else(|_| "[]".into())
}

/// Fallback speed for a highway class (tests / Android spot-checks).
pub fn expected_fallback_kmh(highway: Option<&str>) -> f64 {
    highway_fallback_kmh(highway)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::graph::{GraphEdge, RouteGraph};
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
        maxspeed: Option<f64>,
        name: Option<&str>,
        is_roundabout: bool,
    ) -> GraphEdge {
        GraphEdge {
            id: id.into(),
            source: NodeId(s),
            target: NodeId(t),
            length_m: 200.0,
            base_weight: 200.0,
            eco_weight: None,
            start_lat: lat0,
            start_lon: lon0,
            end_lat: lat1,
            end_lon: lon1,
            shape: Vec::new(),
            highway: Some(highway.into()),
            maxspeed_kmh: maxspeed,
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
            is_roundabout,
        }
    }

    #[test]
    fn samples_use_posted_and_residential_fallback() {
        let mut nodes = HashMap::new();
        nodes.insert(
            NodeId(1),
            Node {
                id: NodeId(1),
                coord: Coord { x: 11.0, y: 60.0 },
                uses: 0,
            },
        );
        nodes.insert(
            NodeId(2),
            Node {
                id: NodeId(2),
                coord: Coord { x: 11.002, y: 60.0 },
                uses: 0,
            },
        );
        nodes.insert(
            NodeId(3),
            Node {
                id: NodeId(3),
                coord: Coord {
                    x: 11.002,
                    y: 60.002,
                },
                uses: 0,
            },
        );
        let edges = vec![
            edge(
                "a",
                1,
                2,
                60.0,
                11.0,
                60.0,
                11.002,
                "residential",
                None,
                Some("Storgata"),
                false,
            ),
            edge(
                "b",
                2,
                3,
                60.0,
                11.002,
                60.002,
                11.002,
                "tertiary",
                Some(60.0),
                Some("Rv3"),
                false,
            ),
        ];
        let graph =
            RouteGraph::from_parts(nodes, edges, crate::routing::graph::RoutingProfile::Car);
        let path = vec![NodeId(1), NodeId(2), NodeId(3)];
        let samples = build_sim_samples(&graph, &path);
        assert!(samples.len() > 2);
        let res = samples
            .iter()
            .find(|s| s.highway.as_deref() == Some("residential"))
            .unwrap();
        assert!((res.speed_kmh - 40.0).abs() < 0.01);
        assert!(!res.maxspeed_posted);
        assert_eq!(res.street.as_deref(), Some("Storgata"));
        let tert = samples
            .iter()
            .find(|s| s.highway.as_deref() == Some("tertiary"))
            .unwrap();
        assert!((tert.speed_kmh - 60.0).abs() < 0.01);
        assert!(tert.maxspeed_posted);
        assert_eq!(tert.street.as_deref(), Some("Rv3"));

        let man = build_maneuvers(&graph, &path);
        assert!(man.iter().any(|m| m.kind == "destination"));
        // Right-ish turn at node 2 (east then north).
        assert!(man
            .iter()
            .any(|m| m.kind.contains("left") || m.kind.contains("right")));
        let turn = man.iter().find(|m| m.kind != "destination").unwrap();
        assert_eq!(turn.street.as_deref(), Some("Rv3"));
    }

    #[test]
    fn samples_preserve_norwegian_street_utf8() {
        let mut nodes = HashMap::new();
        nodes.insert(
            NodeId(1),
            Node {
                id: NodeId(1),
                coord: Coord { x: 11.0, y: 60.0 },
                uses: 0,
            },
        );
        nodes.insert(
            NodeId(2),
            Node {
                id: NodeId(2),
                coord: Coord { x: 11.002, y: 60.0 },
                uses: 0,
            },
        );
        let edges = vec![edge(
            "a",
            1,
            2,
            60.0,
            11.0,
            60.0,
            11.002,
            "tertiary",
            Some(50.0),
            Some("Mjøsvegen"),
            false,
        )];
        let graph =
            RouteGraph::from_parts(nodes, edges, crate::routing::graph::RoutingProfile::Car);
        let samples = build_sim_samples(&graph, &[NodeId(1), NodeId(2)]);
        let json = samples_to_json(&samples);
        assert!(json.contains("Mjøsvegen"), "json lost ø: {json}");
        assert_eq!(samples[0].street.as_deref(), Some("Mjøsvegen"));
    }

    #[test]
    fn roundabout_emits_kind_with_exit_number_not_ring_turns() {
        // Approach A → ring R0→R1→R2 → leave to X2.
        // Side exit X1 at R1 is passed but not taken → exit number 2 at X2.
        //
        // Coordinates (lon/lat as x/y): A south of R0; ring CCW; X1 east of R1; X2 north of R2.
        let mut nodes = HashMap::new();
        let pts = [
            (1, 10.000, 60.000), // A approach
            (2, 10.000, 60.001), // R0 entry
            (3, 10.001, 60.001), // R1
            (4, 10.001, 60.002), // R2 leave
            (5, 10.002, 60.001), // X1 side exit (not taken)
            (6, 10.001, 60.003), // X2 taken exit
        ];
        for &(id, lon, lat) in &pts {
            nodes.insert(
                NodeId(id),
                Node {
                    id: NodeId(id),
                    coord: Coord { x: lon, y: lat },
                    uses: 0,
                },
            );
        }
        let edges = vec![
            edge(
                "appr",
                1,
                2,
                60.000,
                10.000,
                60.001,
                10.000,
                "secondary",
                None,
                None,
                false,
            ),
            edge(
                "r01",
                2,
                3,
                60.001,
                10.000,
                60.001,
                10.001,
                "secondary",
                None,
                Some("ring"),
                true,
            ),
            edge(
                "r12",
                3,
                4,
                60.001,
                10.001,
                60.002,
                10.001,
                "secondary",
                None,
                Some("ring"),
                true,
            ),
            edge(
                "x1",
                3,
                5,
                60.001,
                10.001,
                60.001,
                10.002,
                "residential",
                None,
                Some("Sidevegen"),
                false,
            ),
            edge(
                "x2",
                4,
                6,
                60.002,
                10.001,
                60.003,
                10.001,
                "residential",
                None,
                Some("Utgangen"),
                false,
            ),
        ];
        let graph =
            RouteGraph::from_parts(nodes, edges, crate::routing::graph::RoutingProfile::Car);
        let path = vec![NodeId(1), NodeId(2), NodeId(3), NodeId(4), NodeId(6)];
        let mans = build_maneuvers(&graph, &path);
        let ra: Vec<_> = mans.iter().filter(|m| m.kind == "roundabout").collect();
        assert_eq!(
            ra.len(),
            1,
            "expected one roundabout maneuver, got {mans:?}"
        );
        assert_eq!(ra[0].roundabout_exit, Some(2));
        assert_eq!(ra[0].street.as_deref(), Some("Utgangen"));
        // No geometric left/right for ring vertices or leave turn.
        assert!(
            mans.iter()
                .all(|m| m.kind == "roundabout" || m.kind == "destination"),
            "unexpected geometric turns inside roundabout: {mans:?}"
        );
        assert_eq!(
            crate::nav::ManeuverKind::roundabout_icon_key(ra[0].roundabout_exit),
            "nav_roundabout_r2"
        );
    }

    #[test]
    fn roundabout_first_exit_is_one() {
        // A → R0 → R1 → X1 (leave at first opportunity).
        let mut nodes = HashMap::new();
        let pts = [
            (1, 10.000, 60.000),
            (2, 10.000, 60.001),
            (3, 10.001, 60.001),
            (4, 10.002, 60.001),
        ];
        for &(id, lon, lat) in &pts {
            nodes.insert(
                NodeId(id),
                Node {
                    id: NodeId(id),
                    coord: Coord { x: lon, y: lat },
                    uses: 0,
                },
            );
        }
        let edges = vec![
            edge(
                "appr",
                1,
                2,
                60.000,
                10.000,
                60.001,
                10.000,
                "secondary",
                None,
                None,
                false,
            ),
            edge(
                "r01",
                2,
                3,
                60.001,
                10.000,
                60.001,
                10.001,
                "secondary",
                None,
                None,
                true,
            ),
            edge(
                "x1",
                3,
                4,
                60.001,
                10.001,
                60.001,
                10.002,
                "residential",
                None,
                Some("Første"),
                false,
            ),
        ];
        let graph =
            RouteGraph::from_parts(nodes, edges, crate::routing::graph::RoutingProfile::Car);
        let path = vec![NodeId(1), NodeId(2), NodeId(3), NodeId(4)];
        let mans = build_maneuvers(&graph, &path);
        let ra = mans.iter().find(|m| m.kind == "roundabout").expect("ra");
        assert_eq!(ra.roundabout_exit, Some(1));
        assert_eq!(
            crate::nav::ManeuverKind::roundabout_icon_key(ra.roundabout_exit),
            "nav_roundabout_r1"
        );
    }

    #[test]
    fn densify_lat_lon_polyline_has_spacing_and_cum() {
        let coords = vec![(60.0, 11.0), (60.0, 11.01), (60.01, 11.01)];
        let samples = build_sim_samples_from_lat_lon(&coords, 3.75, Some("path"));
        assert!(
            samples.len() > 10,
            "expected densified samples, got {}",
            samples.len()
        );
        assert!((samples[0].cum_m - 0.0).abs() < 1e-6);
        assert!(samples.last().unwrap().cum_m > 1_000.0);
        assert!((samples[0].speed_kmh - 3.75).abs() < 1e-9);
        assert_eq!(samples[0].highway.as_deref(), Some("path"));
    }
}
