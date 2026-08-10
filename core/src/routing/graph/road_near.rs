//! Spatial index of routing-graph nodes for “near a road” checks, plus
//! nearest-edge street labels for idle GPS.

use geo::{Distance, Haversine, Point};
use osm4routing::NodeId;
use rstar::RTree;

use crate::nav::current_road_label;

use super::builder::{GraphEdge, RouteGraph};

const MAX_ROAD_LINK_M: f64 = 1_000.0;

/// Prefer a locked road until another is clearly closer by this margin (m).
/// Sized for parallel corridors like Furnesvegen / E6 (~25 m) so GPS noise at
/// the geometric midpoint does not flip the HUD label.
const ROAD_LABEL_SWITCH_MARGIN_M: f64 = 10.0;

/// Consecutive polls where an alternate wins by [`ROAD_LABEL_SWITCH_MARGIN_M`]
/// before the sticky label switches — same “sustained, not single-sample”
/// idea as off-route confirm (MainActivity polls idle labels ~every 3 s).
const ROAD_LABEL_SWITCH_CONFIRM_HITS: u32 = 2;

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

/// Distance (m) from a fix to the edge’s real polyline (shape), not the chord
/// between graph endpoints alone.
pub fn edge_distance_m(e: &GraphEdge, lat: f64, lon: f64) -> f64 {
    let mut best =
        dist_point_to_segment_m(lat, lon, e.start_lat, e.start_lon, e.end_lat, e.end_lon);
    if e.shape.is_empty() {
        return best;
    }
    let mut prev_lat = e.start_lat;
    let mut prev_lon = e.start_lon;
    for &(slon, slat) in &e.shape {
        best = best.min(dist_point_to_segment_m(
            lat, lon, prev_lat, prev_lon, slat, slon,
        ));
        prev_lat = slat;
        prev_lon = slon;
    }
    best.min(dist_point_to_segment_m(
        lat, lon, prev_lat, prev_lon, e.end_lat, e.end_lon,
    ))
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

/// Instantaneous nearest-edge label within [max_m] of `(lat, lon)`.
///
/// Among edges within 8 m of the closest hit, prefer one with OSM `name`/`ref`
/// over a class-only label (named through-road vs unnamed driveway stub).
///
/// Distance uses full edge shape when present. Prefer [`RoadLabelSticky`] for
/// live GPS so parallel roads do not flip-flop on noise.
pub fn nearest_road_label(graph: &RouteGraph, lat: f64, lon: f64, max_m: f64) -> Option<String> {
    nearest_road_candidate(graph, lat, lon, max_m).map(|(_, label)| label)
}

fn nearest_road_candidate(
    graph: &RouteGraph,
    lat: f64,
    lon: f64,
    max_m: f64,
) -> Option<(f64, String)> {
    let max_m = max_m.max(1.0);
    let mut best_d = f64::INFINITY;
    let mut best_named: Option<(f64, String)> = None;
    let mut best_any: Option<(f64, String)> = None;
    for e in &graph.edges {
        let d = edge_distance_m(e, lat, lon);
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
        (Some((nd, nlabel)), Some((ad, _))) if nd <= ad + 8.0 => Some((nd, nlabel)),
        (_, Some((ad, alabel))) => Some((ad, alabel)),
        (Some((nd, nlabel)), None) => Some((nd, nlabel)),
        (None, None) => None,
    }
}

fn distance_to_label(
    graph: &RouteGraph,
    lat: f64,
    lon: f64,
    max_m: f64,
    label: &str,
) -> Option<f64> {
    let max_m = max_m.max(1.0);
    let mut best = f64::INFINITY;
    for e in &graph.edges {
        if edge_label(e) != label {
            continue;
        }
        let d = edge_distance_m(e, lat, lon);
        if d <= max_m && d < best {
            best = d;
        }
    }
    if best.is_finite() {
        Some(best)
    } else {
        None
    }
}

/// Sticky idle-GPS road label: hysteresis + sustained confirm before switching.
///
/// Mirrors off-route debounce: a single noisy sample that crosses the geometric
/// midpoint between two real roads must not flip the HUD. Clear a larger margin
/// for [`ROAD_LABEL_SWITCH_CONFIRM_HITS`] consecutive updates before changing.
#[derive(Debug, Default, Clone)]
pub struct RoadLabelSticky {
    last_label: Option<String>,
    pending_label: Option<String>,
    pending_hits: u32,
}

impl RoadLabelSticky {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn last_label(&self) -> Option<&str> {
        self.last_label.as_deref()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Resolve label for this fix; updates internal lock / pending state.
    pub fn update(&mut self, graph: &RouteGraph, lat: f64, lon: f64, max_m: f64) -> Option<String> {
        let Some((best_d, best_label)) = nearest_road_candidate(graph, lat, lon, max_m) else {
            self.reset();
            return None;
        };

        let Some(locked) = self.last_label.clone() else {
            self.last_label = Some(best_label.clone());
            self.pending_label = None;
            self.pending_hits = 0;
            return Some(best_label);
        };

        if locked == best_label {
            self.pending_label = None;
            self.pending_hits = 0;
            return Some(locked);
        }

        let locked_d = distance_to_label(graph, lat, lon, max_m, &locked);
        match locked_d {
            None => {
                // Locked road left the snap radius — take instantaneous best.
                self.last_label = Some(best_label.clone());
                self.pending_label = None;
                self.pending_hits = 0;
                Some(best_label)
            }
            Some(ld) => {
                let advantage = ld - best_d;
                if advantage < ROAD_LABEL_SWITCH_MARGIN_M {
                    // Still within hysteresis band — keep lock (GPS noise).
                    self.pending_label = None;
                    self.pending_hits = 0;
                    return Some(locked);
                }
                if self.pending_label.as_deref() == Some(best_label.as_str()) {
                    self.pending_hits = self.pending_hits.saturating_add(1);
                } else {
                    self.pending_label = Some(best_label.clone());
                    self.pending_hits = 1;
                }
                if self.pending_hits >= ROAD_LABEL_SWITCH_CONFIRM_HITS {
                    self.last_label = Some(best_label.clone());
                    self.pending_label = None;
                    self.pending_hits = 0;
                    Some(best_label)
                } else {
                    Some(locked)
                }
            }
        }
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
        road_ref: Option<&str>,
        shape: Vec<(f64, f64)>,
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
            shape,
            highway: Some(highway.into()),
            maxspeed_kmh: None,
            name: name.map(|s| s.into()),
            road_ref: road_ref.map(|s| s.into()),
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
                None,
                Vec::new(),
            ),
            edge(
                "stub",
                1,
                3,
                61.4197,
                9.9276,
                61.4199,
                9.9276,
                "service",
                None,
                None,
                Vec::new(),
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

    /// Parallel roads ~26 m apart (Furnesvegen / E6 class of problem).
    fn parallel_furnes_e6_graph() -> RouteGraph {
        let mut nodes = HashMap::new();
        // ~26 m north-south separation at lat 60.85
        let lat_f = 60.85250;
        let lat_e = 60.85227; // ~25.6 m south
        let lon0 = 11.0060;
        let lon1 = 11.0100;
        for (id, lat, lon) in [
            (1, lat_f, lon0),
            (2, lat_f, lon1),
            (3, lat_e, lon0),
            (4, lat_e, lon1),
        ] {
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
                "furnes",
                1,
                2,
                lat_f,
                lon0,
                lat_f,
                lon1,
                "primary",
                Some("Furnesvegen"),
                Some("184"),
                Vec::new(),
            ),
            edge(
                "e6",
                3,
                4,
                lat_e,
                lon0,
                lat_e,
                lon1,
                "motorway",
                None,
                Some("E 6"),
                Vec::new(),
            ),
        ];
        RouteGraph::from_parts(nodes, edges, RoutingProfile::Car)
    }

    #[test]
    fn sticky_holds_furnes_under_midpoint_gps_noise() {
        let graph = parallel_furnes_e6_graph();
        let mid_lat = 60.85250;
        let mid_lon = 11.0080;
        let mut sticky = RoadLabelSticky::new();
        let first = sticky
            .update(&graph, mid_lat, mid_lon, 80.0)
            .expect("label");
        assert_eq!(first, "Furnesvegen");

        // Drift ~15 m toward E6 — instantaneous nearest flips; sticky must hold.
        let noisy_lat = 60.85236;
        for _ in 0..3 {
            let label = sticky
                .update(&graph, noisy_lat, mid_lon, 80.0)
                .expect("label");
            assert_eq!(
                label, "Furnesvegen",
                "must not flip on single-side noise samples"
            );
        }
    }

    #[test]
    fn sticky_switches_after_sustained_clear_move_to_e6() {
        let graph = parallel_furnes_e6_graph();
        let mid_lon = 11.0080;
        let mut sticky = RoadLabelSticky::new();
        assert_eq!(
            sticky.update(&graph, 60.85250, mid_lon, 80.0).as_deref(),
            Some("Furnesvegen")
        );

        // Sit on E6 centerline — large margin; needs CONFIRM_HITS sustained.
        let on_e6 = 60.85227;
        let a = sticky.update(&graph, on_e6, mid_lon, 80.0);
        assert_eq!(
            a.as_deref(),
            Some("Furnesvegen"),
            "first clear sample holds"
        );
        let b = sticky.update(&graph, on_e6, mid_lon, 80.0);
        assert_eq!(
            b.as_deref(),
            Some("E 6"),
            "second sustained sample switches"
        );
    }

    #[test]
    fn shape_distance_beats_endpoint_chord() {
        // Endpoints far; mid shape point under the fix.
        let mut nodes = HashMap::new();
        nodes.insert(
            NodeId(1),
            Node {
                id: NodeId(1),
                coord: Coord { x: 11.0, y: 60.85 },
                uses: 0,
            },
        );
        nodes.insert(
            NodeId(2),
            Node {
                id: NodeId(2),
                coord: Coord { x: 11.02, y: 60.85 },
                uses: 0,
            },
        );
        let mid_lon = 11.01;
        let mid_lat = 60.851; // ~111 m north of chord
        let edges = vec![edge(
            "bent",
            1,
            2,
            60.85,
            11.0,
            60.85,
            11.02,
            "primary",
            Some("Bent Road"),
            None,
            vec![(mid_lon, mid_lat)],
        )];
        let graph = RouteGraph::from_parts(nodes, edges, RoutingProfile::Car);
        let chord = dist_point_to_segment_m(mid_lat, mid_lon, 60.85, 11.0, 60.85, 11.02);
        let shaped = edge_distance_m(&graph.edges[0], mid_lat, mid_lon);
        assert!(shaped < 5.0 && chord > 50.0, "shape={shaped} chord={chord}");
        assert_eq!(
            nearest_road_label(&graph, mid_lat, mid_lon, 80.0).as_deref(),
            Some("Bent Road")
        );
    }

    #[test]
    fn furnes_e6_from_ostlandet_bbox_sticky() {
        let pbf = std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/target/integration-fixtures/ostlandet-latest.osm.pbf"
        ));
        if !pbf.is_file() {
            eprintln!("skip: missing {pbf:?}");
            return;
        }
        // Around OSM way 626364973 midpoint.
        let bbox = [60.848, 11.000, 60.856, 11.025];
        let graph =
            RouteGraph::build_from_pbf_bbox(pbf, RoutingProfile::Car, bbox).expect("bbox graph");
        let lat = 60.852498;
        let lon = 11.007872;
        let mut sticky = RoadLabelSticky::new();
        let on_furnes = sticky.update(&graph, lat, lon, 80.0);
        assert!(
            on_furnes
                .as_deref()
                .is_some_and(|s| s.contains("Furnes") || s == "184" || s.contains("184")),
            "expected Furnesvegen/184 on way, got {on_furnes:?}"
        );

        // ~15 m toward E6 (south) — must not flip under sticky.
        let noisy = lat - 15.0 / 111_320.0;
        for _ in 0..3 {
            let label = sticky.update(&graph, noisy, lon, 80.0);
            assert_eq!(
                label, on_furnes,
                "sticky must hold Furnes under ~15 m GPS noise toward E6"
            );
        }
    }
}
