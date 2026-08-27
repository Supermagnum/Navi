//! Pre-departure trip duration estimates (before GPS speed is available).
//!
//! Once the vehicle/hiker/cyclist is moving and live GPS speed can drive an ETA,
//! that live estimate should supersede these starting values.

use osm4routing::NodeId;

use super::graph::{GraphEdge, RouteGraph};

/// Hiking fixed pace: 16 minutes per kilometre (flat; no climb adjustment yet).
///
/// A future climb-adjusted variant could reuse per-edge eco-mode elevation /
/// gradient already available on the graph — not required for this pass.
pub const HIKING_MIN_PER_KM: f64 = 16.0;

/// Cycling fixed pace: ~15 km/h → 4 minutes per kilometre (flat average terrain).
pub const CYCLING_MIN_PER_KM: f64 = 4.0;

/// Default when highway class is unknown / missing.
const DEFAULT_FALLBACK_KMH: f64 = 50.0;

/// Pace model for pre-departure duration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreDeparturePace {
    /// Car / motorcycle / truck: per-edge OSM `maxspeed` with highway-class fallback.
    Motor,
    Hiking,
    Cycling,
}

/// Parse an OSM `maxspeed` tag into km/h.
///
/// Returns `None` for non-numeric / advisory values so the highway-class table is used.
pub fn parse_maxspeed_kmh(raw: &str) -> Option<f64> {
    let s = raw.trim().to_ascii_lowercase();
    if s.is_empty() {
        return None;
    }
    match s.as_str() {
        "none" | "signals" | "variable" | "walk" | "unknown" => return None,
        // Norway implicit limits (OSM national defaults).
        "no:urban" | "no:living_street" => return Some(50.0),
        "no:rural" => return Some(80.0),
        "no:motorway" => return Some(100.0),
        _ => {}
    }

    let (num_part, unit_mph) = if let Some(rest) = s.strip_suffix("mph") {
        (rest.trim(), true)
    } else if let Some(rest) = s.strip_suffix("km/h") {
        (rest.trim(), false)
    } else if let Some(rest) = s.strip_suffix("kmh") {
        (rest.trim(), false)
    } else if let Some(rest) = s.strip_suffix("kph") {
        (rest.trim(), false)
    } else {
        (s.as_str(), false)
    };

    // Take the first numeric token (handles "80;70" by using 80).
    let token = num_part
        .split(|c: char| c == ';' || c == ',' || c.is_whitespace())
        .find(|t| !t.is_empty())?;
    let value: f64 = token.parse().ok()?;
    if !value.is_finite() || value <= 0.0 {
        return None;
    }
    if unit_mph {
        Some(value * 1.609_344)
    } else {
        Some(value)
    }
}

/// Normalize OSM `highway=*` for the shared class tables (strip `_link` suffix).
pub fn highway_class_base(highway: Option<&str>) -> Option<String> {
    let h = highway?.trim().to_ascii_lowercase();
    if h.is_empty() {
        return None;
    }
    Some(h.strip_suffix("_link").unwrap_or(h.as_str()).to_string())
}

/// Highway-class fallback speeds (km/h) when `maxspeed` is absent.
///
/// Tuned as a general starting table; Norway corridors commonly post 80–110 on
/// trunk/motorway and 50–70 on primary/secondary — adjust if reference corridors
/// show systematic bias.
///
/// Class keys must stay aligned with [`highway_class_display_label`].
pub fn highway_fallback_kmh(highway: Option<&str>) -> f64 {
    match highway_class_base(highway).as_deref() {
        Some("motorway") => 100.0,
        Some("trunk") => 80.0,
        Some("primary") => 70.0,
        Some("secondary") => 60.0,
        Some("tertiary") | Some("unclassified") => 50.0,
        Some("residential") | Some("living_street") => 40.0,
        Some("service") | Some("track") | Some("road") => 20.0,
        Some("path") | Some("footway") | Some("cycleway") | Some("bridleway") | Some("steps") => {
            10.0
        }
        _ => DEFAULT_FALLBACK_KMH,
    }
}

/// Human-readable highway-class label when OSM `name` / `ref` are missing.
///
/// Uses the **same** class keys as [`highway_fallback_kmh`] (plus common
/// foot/cycle classes). Never returns a raw OSM tag; unknown → `"Road"`.
pub fn highway_class_display_label(highway: Option<&str>) -> &'static str {
    match highway_class_base(highway).as_deref() {
        Some("motorway") => "Motorway",
        Some("trunk") => "Trunk road",
        Some("primary") => "Primary road",
        Some("secondary") => "Secondary road",
        Some("tertiary") => "Tertiary road",
        Some("unclassified") => "Unclassified road",
        Some("residential") => "Residential road",
        Some("living_street") => "Living street",
        Some("service") => "Service road",
        Some("track") => "Track",
        Some("road") => "Road",
        Some("path") => "Path",
        Some("footway") => "Footway",
        Some("cycleway") => "Cycleway",
        Some("bridleway") => "Bridleway",
        Some("steps") => "Steps",
        Some("pedestrian") => "Pedestrian street",
        _ => "Road",
    }
}

/// Effective motor speed (km/h): posted OSM `maxspeed` when present, else highway-class fallback.
pub fn edge_speed_kmh(edge: &GraphEdge) -> f64 {
    edge.maxspeed_kmh
        .filter(|v| v.is_finite() && *v > 0.0)
        .unwrap_or_else(|| highway_fallback_kmh(edge.highway.as_deref()))
}

/// Segment travel time in hours from length and speed (km/h).
fn hours_for_segment(length_m: f64, speed_kmh: f64) -> f64 {
    let speed = speed_kmh.max(1.0);
    (length_m / 1000.0) / speed
}

/// Fixed-pace estimate from total distance (hiking / cycling).
pub fn fixed_pace_minutes(distance_km: f64, min_per_km: f64) -> f64 {
    if !distance_km.is_finite() || distance_km <= 0.0 {
        return 0.0;
    }
    distance_km * min_per_km
}

/// Sum motor pre-departure time along a node path using per-edge maxspeed / fallback.
pub fn motor_path_minutes(graph: &RouteGraph, path: &[NodeId]) -> f64 {
    if path.len() < 2 {
        return 0.0;
    }
    let mut hours = 0.0;
    for w in path.windows(2) {
        if let Some(idx) = graph.edge_index(w[0], w[1]) {
            let e = &graph.edges[idx];
            hours += hours_for_segment(e.length_m, edge_speed_kmh(e));
        } else if let (Some(a), Some(b)) = (graph.nodes.get(&w[0]), graph.nodes.get(&w[1])) {
            // Missing directed edge: still estimate from node spacing + default class.
            let length_m = haversine_m(a.coord.y, a.coord.x, b.coord.y, b.coord.x);
            hours += hours_for_segment(length_m, DEFAULT_FALLBACK_KMH);
        }
    }
    hours * 60.0
}

fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6_378_100.0_f64;
    let p1 = lat1.to_radians();
    let p2 = lat2.to_radians();
    let dp = (lat2 - lat1).to_radians();
    let dl = (lon2 - lon1).to_radians();
    let h = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * r * h.sqrt().asin()
}

/// Pre-departure ETA in minutes for a planned path / distance.
pub fn predeparture_eta_minutes(
    pace: PreDeparturePace,
    graph: &RouteGraph,
    path: &[NodeId],
    distance_km: f64,
) -> f64 {
    match pace {
        PreDeparturePace::Motor => motor_path_minutes(graph, path),
        PreDeparturePace::Hiking => fixed_pace_minutes(distance_km, HIKING_MIN_PER_KM),
        PreDeparturePace::Cycling => fixed_pace_minutes(distance_km, CYCLING_MIN_PER_KM),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::graph::{GraphEdge, RouteGraph, RoutingProfile};
    use geo_types::Coord;
    use osm4routing::Node;
    use std::collections::HashMap;

    #[test]
    fn parse_maxspeed_numeric_and_units() {
        assert_eq!(parse_maxspeed_kmh("80"), Some(80.0));
        assert_eq!(parse_maxspeed_kmh("50 km/h"), Some(50.0));
        assert!((parse_maxspeed_kmh("30 mph").unwrap() - 48.28).abs() < 0.1);
        assert_eq!(parse_maxspeed_kmh("signals"), None);
        assert_eq!(parse_maxspeed_kmh("NO:rural"), Some(80.0));
    }

    #[test]
    fn highway_fallback_table() {
        assert_eq!(highway_fallback_kmh(Some("motorway")), 100.0);
        assert_eq!(highway_fallback_kmh(Some("motorway_link")), 100.0);
        assert_eq!(highway_fallback_kmh(Some("residential")), 40.0);
        assert_eq!(highway_fallback_kmh(Some("service")), 20.0);
        assert_eq!(highway_fallback_kmh(None), 50.0);
    }

    #[test]
    fn highway_display_labels_align_with_fallback_classes() {
        assert_eq!(
            highway_class_display_label(Some("motorway_link")),
            "Motorway"
        );
        assert_eq!(highway_class_display_label(Some("trunk")), "Trunk road");
        assert_eq!(highway_class_display_label(Some("primary")), "Primary road");
        assert_eq!(
            highway_class_display_label(Some("secondary")),
            "Secondary road"
        );
        assert_eq!(
            highway_class_display_label(Some("tertiary")),
            "Tertiary road"
        );
        assert_eq!(
            highway_class_display_label(Some("unclassified")),
            "Unclassified road"
        );
        assert_eq!(
            highway_class_display_label(Some("residential")),
            "Residential road"
        );
        assert_eq!(highway_class_display_label(Some("service")), "Service road");
        assert_eq!(highway_class_display_label(Some("track")), "Track");
        assert_eq!(highway_class_display_label(Some("path")), "Path");
        assert_eq!(highway_class_display_label(None), "Road");
        // Never echo raw OSM tags.
        assert_ne!(highway_class_display_label(Some("foo_bar")), "foo_bar");
    }

    #[test]
    fn missing_maxspeed_uses_highway_fallback_not_zero() {
        let mut nodes = HashMap::new();
        nodes.insert(
            NodeId(1),
            Node {
                id: NodeId(1),
                coord: Coord { x: 10.0, y: 60.0 },
                uses: 0,
            },
        );
        nodes.insert(
            NodeId(2),
            Node {
                id: NodeId(2),
                coord: Coord { x: 10.1, y: 60.0 },
                uses: 0,
            },
        );
        // ~5.56 km at primary fallback 70 km/h → ~4.76 min
        let length_m = 5_560.0;
        let edges = vec![GraphEdge {
            id: "e1".into(),
            source: NodeId(1),
            target: NodeId(2),
            length_m,
            base_weight: length_m,
            eco_weight: None,
            start_lat: 60.0,
            start_lon: 10.0,
            end_lat: 60.0,
            end_lon: 10.1,
            shape: Vec::new(),
            highway: Some("primary".into()),
            maxspeed_kmh: None,
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
            is_boardwalk_crossing: false,
            is_roundabout: false,
            motor_vehicle_conditional: None,
            access_conditional: None,
            maxspeed_conditional: None,
            access_forbidden: false,
        }];
        let graph = RouteGraph::from_parts(nodes, edges, RoutingProfile::Car);
        let mins = motor_path_minutes(&graph, &[NodeId(1), NodeId(2)]);
        assert!(mins > 4.0 && mins < 6.0, "got {mins}");
        assert!(mins > 0.0);
    }

    #[test]
    fn tagged_maxspeed_overrides_fallback() {
        let mut nodes = HashMap::new();
        nodes.insert(
            NodeId(1),
            Node {
                id: NodeId(1),
                coord: Coord { x: 10.0, y: 60.0 },
                uses: 0,
            },
        );
        nodes.insert(
            NodeId(2),
            Node {
                id: NodeId(2),
                coord: Coord { x: 10.1, y: 60.0 },
                uses: 0,
            },
        );
        let length_m = 10_000.0; // 10 km at 100 km/h → 6 min
        let edges = vec![GraphEdge {
            id: "e1".into(),
            source: NodeId(1),
            target: NodeId(2),
            length_m,
            base_weight: length_m,
            eco_weight: None,
            start_lat: 60.0,
            start_lon: 10.0,
            end_lat: 60.0,
            end_lon: 10.1,
            shape: Vec::new(),
            highway: Some("residential".into()), // fallback would be 40
            maxspeed_kmh: Some(100.0),
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
            is_boardwalk_crossing: false,
            is_roundabout: false,
            motor_vehicle_conditional: None,
            access_conditional: None,
            maxspeed_conditional: None,
            access_forbidden: false,
        }];
        let graph = RouteGraph::from_parts(nodes, edges, RoutingProfile::Car);
        let mins = motor_path_minutes(&graph, &[NodeId(1), NodeId(2)]);
        assert!((mins - 6.0).abs() < 0.01, "got {mins}");
    }

    #[test]
    fn hiking_and_cycling_fixed_pace() {
        assert!((fixed_pace_minutes(10.0, HIKING_MIN_PER_KM) - 160.0).abs() < 1e-9);
        assert!((fixed_pace_minutes(15.0, CYCLING_MIN_PER_KM) - 60.0).abs() < 1e-9);
    }
}
