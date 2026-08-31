//! Path-preference-first hiking: graph A* first, DEM terrain only for gaps.

use osm4routing::NodeId;
use serde_json::json;

use crate::config::EcoConfig;
use crate::routing::elevation::ElevationService;
use crate::routing::graph::RouteGraph;
use crate::routing::terrain::{least_cost_path, TERRAIN_MAX_GAP_M};
use crate::routing::wetland::WetlandIndex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentKind {
    OnTrail,
    OffTrail,
}

#[derive(Debug, Clone)]
pub struct RouteSegment {
    pub kind: SegmentKind,
    /// `(lat, lon)` vertices.
    pub coords: Vec<(f64, f64)>,
    pub length_m: f64,
}

#[derive(Debug, Clone)]
pub struct HybridHikingPath {
    pub segments: Vec<RouteSegment>,
    /// Concatenated node path for on-trail portions (may be empty when pure off-trail).
    pub path_nodes: Vec<NodeId>,
    pub distance_m: f64,
    pub off_trail_m: f64,
}

impl HybridHikingPath {
    pub fn route_mode(&self) -> &'static str {
        let has_on = self.segments.iter().any(|s| s.kind == SegmentKind::OnTrail);
        let has_off = self
            .segments
            .iter()
            .any(|s| s.kind == SegmentKind::OffTrail);
        match (has_on, has_off) {
            (true, true) => "hybrid",
            (true, false) => "on_trail",
            (false, true) => "off_trail",
            (false, false) => "empty",
        }
    }

    pub fn full_coords(&self) -> Vec<(f64, f64)> {
        let mut out = Vec::new();
        for seg in &self.segments {
            for &(lat, lon) in &seg.coords {
                if let Some(&(plat, plon)) = out.last() {
                    if haversine_m(plat, plon, lat, lon) < 0.5 {
                        continue;
                    }
                }
                out.push((lat, lon));
            }
        }
        out
    }

    pub fn polyline_lon_lat(&self) -> String {
        let mut s = String::new();
        for (i, &(lat, lon)) in self.full_coords().iter().enumerate() {
            if i > 0 {
                s.push(';');
            }
            s.push_str(&format!("{lon},{lat}"));
        }
        s
    }

    pub fn segments_json(&self) -> String {
        let arr: Vec<_> = self
            .segments
            .iter()
            .map(|seg| {
                let poly = seg
                    .coords
                    .iter()
                    .map(|(lat, lon)| format!("{lon},{lat}"))
                    .collect::<Vec<_>>()
                    .join(";");
                let kind = match seg.kind {
                    SegmentKind::OnTrail => "on_trail",
                    SegmentKind::OffTrail => "off_trail",
                };
                json!({
                    "kind": kind,
                    "polyline": poly,
                    "length_m": seg.length_m,
                })
            })
            .collect();
        serde_json::to_string(&arr).unwrap_or_else(|_| "[]".into())
    }
}

pub const OFF_TRAIL_ADVISORY: &str = "Off-trail segment: terrain analysis cannot see cliffs, dense vegetation, seasonal snow/ice, or water crossings missing from DEM/OSM. Right-to-roam access does not mean the route is safe or sensible — use your own judgment.";

#[derive(Debug, Clone)]
pub struct HikingWaypoint {
    pub name: String,
    pub lat: f64,
    pub lon: f64,
}

/// Plan a hybrid foot path through waypoints (graph first, terrain for gaps).
pub fn plan_hybrid_hiking_path(
    graph: &RouteGraph,
    elevation: &ElevationService,
    wetlands: &WetlandIndex,
    eco: &EcoConfig,
    wps: &[HikingWaypoint],
) -> Result<HybridHikingPath, String> {
    if wps.len() < 2 {
        return Err("need at least start and end waypoints".into());
    }
    let mut segments: Vec<RouteSegment> = Vec::new();
    let mut path_nodes: Vec<NodeId> = Vec::new();
    let mut distance_m = 0.0;
    let mut off_trail_m = 0.0;

    for pair in wps.windows(2) {
        let a = &pair[0];
        let b = &pair[1];
        let leg = plan_leg(graph, elevation, wetlands, eco, a, b)?;
        for seg in leg.segments {
            if seg.kind == SegmentKind::OffTrail {
                off_trail_m += seg.length_m;
            }
            distance_m += seg.length_m;
            segments.push(seg);
        }
        if path_nodes.is_empty() {
            path_nodes.extend(leg.path_nodes);
        } else {
            path_nodes.extend(leg.path_nodes.into_iter().skip(1));
        }
    }

    Ok(HybridHikingPath {
        segments,
        path_nodes,
        distance_m,
        off_trail_m,
    })
}

struct LegResult {
    segments: Vec<RouteSegment>,
    path_nodes: Vec<NodeId>,
}

fn plan_leg(
    graph: &RouteGraph,
    elevation: &ElevationService,
    wetlands: &WetlandIndex,
    eco: &EcoConfig,
    a: &HikingWaypoint,
    b: &HikingWaypoint,
) -> Result<LegResult, String> {
    let snap_a = graph.nearest_routable(a.lat, a.lon).ok();
    let snap_b = graph.nearest_routable(b.lat, b.lon).ok();

    if crate::download::plan_cancel::is_cancelled() {
        return Err("cancelled".into());
    }
    if let (Some((sa, _)), Some((sb, _))) = (snap_a, snap_b) {
        if let Some((path, _path_edges, _)) = graph.shortest_path(sa, sb, false) {
            if path.len() >= 2 {
                let coords = graph.path_coords_lat_lon(&path);
                let length_m = path_length_m(graph, &path);
                return Ok(LegResult {
                    segments: vec![RouteSegment {
                        kind: SegmentKind::OnTrail,
                        coords,
                        length_m,
                    }],
                    path_nodes: path,
                });
            }
        } else if crate::download::plan_cancel::is_cancelled() {
            return Err("cancelled".into());
        }
    }

    // Gap-fill: prefer graph to a trailhead, then terrain for the remainder.
    gap_fill_leg(graph, elevation, wetlands, eco, a, b, snap_a, snap_b)
}

fn gap_fill_leg(
    graph: &RouteGraph,
    elevation: &ElevationService,
    wetlands: &WetlandIndex,
    eco: &EcoConfig,
    a: &HikingWaypoint,
    b: &HikingWaypoint,
    snap_a: Option<(NodeId, f64)>,
    snap_b: Option<(NodeId, f64)>,
) -> Result<LegResult, String> {
    if crate::download::plan_cancel::is_cancelled() {
        return Err("cancelled".into());
    }
    let crow = haversine_m(a.lat, a.lon, b.lat, b.lon);
    if crow > TERRAIN_MAX_GAP_M {
        return Err(format!(
            "no foot route {} -> {} (gap {:.0} m exceeds terrain limit {:.0} m)",
            a.name, b.name, crow, TERRAIN_MAX_GAP_M
        ));
    }

    // Destination off-trail (or disconnected): on-trail toward trailhead near B, then terrain.
    if let Some((sa, _)) = snap_a {
        if let Some((tb, tdist)) = graph.nearest_linked_unbounded(b.lat, b.lon) {
            if tdist <= TERRAIN_MAX_GAP_M {
                if let Some((path, _path_edges, _)) = graph.shortest_path(sa, tb, false) {
                    if path.len() >= 2 {
                        let mut segments = Vec::new();
                        let on_coords = graph.path_coords_lat_lon(&path);
                        let on_m = path_length_m(graph, &path);
                        segments.push(RouteSegment {
                            kind: SegmentKind::OnTrail,
                            coords: on_coords,
                            length_m: on_m,
                        });
                        let (tlat, tlon) = node_latlon(graph, tb);
                        let terrain =
                            least_cost_path(elevation, wetlands, eco, tlat, tlon, b.lat, b.lon)
                                .map_err(|e| {
                                    format!("terrain gap {} -> {}: {e}", a.name, b.name)
                                })?;
                        segments.push(RouteSegment {
                            kind: SegmentKind::OffTrail,
                            coords: terrain.coords,
                            length_m: terrain.length_m,
                        });
                        return Ok(LegResult {
                            segments,
                            path_nodes: path,
                        });
                    }
                }
            }
        }
    }

    // Start off-trail: terrain to trailhead near A, then on-trail if possible.
    if let Some((sb, _)) = snap_b {
        if let Some((ta, tdist)) = graph.nearest_linked_unbounded(a.lat, a.lon) {
            if tdist <= TERRAIN_MAX_GAP_M {
                if let Some((path, _path_edges, _)) = graph.shortest_path(ta, sb, false) {
                    if path.len() >= 2 {
                        let mut segments = Vec::new();
                        let (tlat, tlon) = node_latlon(graph, ta);
                        let terrain =
                            least_cost_path(elevation, wetlands, eco, a.lat, a.lon, tlat, tlon)
                                .map_err(|e| {
                                    format!("terrain gap {} -> {}: {e}", a.name, b.name)
                                })?;
                        segments.push(RouteSegment {
                            kind: SegmentKind::OffTrail,
                            coords: terrain.coords,
                            length_m: terrain.length_m,
                        });
                        let on_coords = graph.path_coords_lat_lon(&path);
                        let on_m = path_length_m(graph, &path);
                        segments.push(RouteSegment {
                            kind: SegmentKind::OnTrail,
                            coords: on_coords,
                            length_m: on_m,
                        });
                        return Ok(LegResult {
                            segments,
                            path_nodes: path,
                        });
                    }
                }
            }
        }
    }

    // Pure off-trail between actual coordinates.
    let terrain = least_cost_path(elevation, wetlands, eco, a.lat, a.lon, b.lat, b.lon)
        .map_err(|e| format!("no foot route {} -> {} ({e})", a.name, b.name))?;
    Ok(LegResult {
        segments: vec![RouteSegment {
            kind: SegmentKind::OffTrail,
            coords: terrain.coords,
            length_m: terrain.length_m,
        }],
        path_nodes: Vec::new(),
    })
}

fn node_latlon(graph: &RouteGraph, id: NodeId) -> (f64, f64) {
    let n = &graph.nodes[&id];
    (n.coord.y, n.coord.x)
}

fn path_length_m(graph: &RouteGraph, path: &[NodeId]) -> f64 {
    let mut m = 0.0;
    for w in path.windows(2) {
        if let Some(idx) = graph.edge_index(w[0], w[1]) {
            m += graph.edges[idx].length_m;
        }
    }
    m
}

fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6_378_100.0;
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}
