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

fn interpolate(
    lat0: f64,
    lon0: f64,
    lat1: f64,
    lon1: f64,
    t: f64,
) -> (f64, f64) {
    (lat0 + (lat1 - lat0) * t, lon0 + (lon1 - lon0) * t)
}

/// Densify the A* node path into speed-tagged samples for debug playback.
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
        let steps = ((e.length_m / SAMPLE_STEP_M).ceil() as usize).max(1);
        for s in 0..steps {
            let t0 = s as f64 / steps as f64;
            let (lat, lon) = interpolate(e.start_lat, e.start_lon, e.end_lat, e.end_lon, t0);
            let along = cum + e.length_m * t0;
            if out.last().map(|p| (p.cum_m - along).abs() < 0.5).unwrap_or(false) {
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
            speed_kmh: e_last.map(edge_speed_kmh).unwrap_or_else(|| highway_fallback_kmh(None)),
            highway: e_last.and_then(|e| e.highway.clone()),
            maxspeed_posted: e_last
                .and_then(|e| e.maxspeed_kmh)
                .filter(|v| v.is_finite() && *v > 0.0)
                .is_some(),
            street: e_last.and_then(|e| prefer_street_label(e.name.as_deref(), e.road_ref.as_deref())),
        });
    }
    out
}

/// Geometric turn list + destination at the path end.
pub fn build_maneuvers(graph: &RouteGraph, path: &[NodeId]) -> Vec<RouteManeuver> {
    let mut out = Vec::new();
    if path.len() < 2 {
        return out;
    }
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
        let in_b = bearing_deg(e_in.start_lat, e_in.start_lon, e_in.end_lat, e_in.end_lon);
        let out_b = bearing_deg(e_out.start_lat, e_out.start_lon, e_out.end_lat, e_out.end_lon);
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
            .filter_map(|w| graph.edge_index(w[0], w[1]).map(|i| graph.edges[i].length_m))
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
                coord: Coord { x: 11.002, y: 60.002 },
                uses: 0,
            },
        );
        let edges = vec![
            edge("a", 1, 2, 60.0, 11.0, 60.0, 11.002, "residential", None, Some("Storgata")),
            edge("b", 2, 3, 60.0, 11.002, 60.002, 11.002, "tertiary", Some(60.0), Some("Rv3")),
        ];
        let graph = RouteGraph::from_parts(nodes, edges, crate::routing::graph::RoutingProfile::Car);
        let path = vec![NodeId(1), NodeId(2), NodeId(3)];
        let samples = build_sim_samples(&graph, &path);
        assert!(samples.len() > 2);
        let res = samples.iter().find(|s| s.highway.as_deref() == Some("residential")).unwrap();
        assert!((res.speed_kmh - 40.0).abs() < 0.01);
        assert!(!res.maxspeed_posted);
        assert_eq!(res.street.as_deref(), Some("Storgata"));
        let tert = samples.iter().find(|s| s.highway.as_deref() == Some("tertiary")).unwrap();
        assert!((tert.speed_kmh - 60.0).abs() < 0.01);
        assert!(tert.maxspeed_posted);
        assert_eq!(tert.street.as_deref(), Some("Rv3"));

        let man = build_maneuvers(&graph, &path);
        assert!(man.iter().any(|m| m.kind == "destination"));
        // Right-ish turn at node 2 (east then north).
        assert!(man.iter().any(|m| m.kind.contains("left") || m.kind.contains("right")));
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
        )];
        let graph = RouteGraph::from_parts(nodes, edges, crate::routing::graph::RoutingProfile::Car);
        let samples = build_sim_samples(&graph, &[NodeId(1), NodeId(2)]);
        let json = samples_to_json(&samples);
        assert!(json.contains("Mjøsvegen"), "json lost ø: {json}");
        assert_eq!(samples[0].street.as_deref(), Some("Mjøsvegen"));
    }
}
