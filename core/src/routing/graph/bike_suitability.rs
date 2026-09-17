//! Hard edge exclusion for bicycle routing by bike capability / terrain tags.
//!
//! Missing OSM tags never exclude a way — only present tags that exceed the
//! selected profile's thresholds remove edges before A* (same discipline as
//! access forbids and wetland hard-avoid).

use std::collections::{HashMap, HashSet};
use std::path::Path;

use osmpbf::Element;
use rayon::prelude::*;

use super::builder::{GraphEdge, RouteGraph, RoutingProfile};
use super::surface_quality::SurfaceQuality;

/// User-selected bike capability (stored in config; applies to Bicycle and
/// Electric cycle — both share the bicycle graph).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BikeCapability {
    /// City / road bike: pavement and good gravel only.
    Road,
    /// Trekking / gravel: moderate unpaved and low MTB difficulty.
    #[default]
    Trekking,
    /// Mountain bike: technical trails; still avoids extreme MTB scale.
    Mountain,
}

impl BikeCapability {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "road" | "city" => Self::Road,
            "mountain" | "mtb" => Self::Mountain,
            _ => Self::Trekking,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Road => "road",
            Self::Trekking => "trekking",
            Self::Mountain => "mountain",
        }
    }
}

fn parse_mtb_scale(raw: &str) -> Option<u8> {
    let head = raw.split(':').next()?.trim();
    head.parse::<u8>().ok().filter(|&v| v <= 6)
}

fn parse_incline_pct(raw: &str) -> Option<f64> {
    let t = raw.trim().trim_end_matches('%');
    t.parse::<f64>().ok()
}

fn smoothness_rank(raw: &str) -> Option<u8> {
    Some(match raw.trim().to_ascii_lowercase().as_str() {
        "excellent" | "good" => 0,
        "intermediate" => 1,
        "bad" => 2,
        "very_bad" => 3,
        "horrible" => 4,
        "very_horrible" => 5,
        "impassable" => 6,
        _ => return None,
    })
}

fn tracktype_grade(raw: &str) -> Option<u8> {
    let t = raw.trim().to_ascii_lowercase();
    if let Some(rest) = t.strip_prefix("grade") {
        return rest.parse::<u8>().ok().filter(|&g| (1..=5).contains(&g));
    }
    None
}

fn is_rough_surface(raw: &str) -> bool {
    matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "ground"
            | "dirt"
            | "earth"
            | "grass"
            | "sand"
            | "mud"
            | "snow"
            | "ice"
            | "compacted"
            | "fine_gravel"
            | "pebblestone"
            | "gravel"
    )
}

fn is_paved_surface(raw: &str) -> bool {
    matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "paved"
            | "asphalt"
            | "concrete"
            | "concrete:plates"
            | "concrete:lanes"
            | "paving_stones"
            | "sett"
            | "cobblestone"
            | "metal"
            | "wood"
    )
}

/// True when present tags make this way unsuitable for `cap` (missing tags → false).
pub fn tags_unsuitable_for(cap: BikeCapability, tags: &HashMap<String, String>) -> bool {
    if tags.is_empty() {
        return false;
    }
    if tag_eq(tags, "route", "mtb") && matches!(cap, BikeCapability::Road) {
        return true;
    }
    if let Some(raw) = tags
        .get("mtb:scale")
        .or_else(|| tags.get("mtb:scale:uphill"))
    {
        if let Some(scale) = parse_mtb_scale(raw) {
            let limit = match cap {
                BikeCapability::Road => 1,
                BikeCapability::Trekking => 2,
                BikeCapability::Mountain => 5,
            };
            if scale >= limit {
                return true;
            }
        }
    }
    if let Some(raw) = tags.get("smoothness") {
        if let Some(rank) = smoothness_rank(raw) {
            let limit = match cap {
                BikeCapability::Road => 2,     // rough or worse
                BikeCapability::Trekking => 4, // horrible or worse
                BikeCapability::Mountain => 6, // impassable only
            };
            if rank >= limit {
                return true;
            }
        }
    }
    if let Some(raw) = tags.get("tracktype") {
        if let Some(grade) = tracktype_grade(raw) {
            let limit = match cap {
                BikeCapability::Road => 3,
                BikeCapability::Trekking => 4,
                BikeCapability::Mountain => 5,
            };
            if grade >= limit {
                return true;
            }
        }
    }
    if let Some(raw) = tags.get("incline") {
        if let Some(pct) = parse_incline_pct(raw) {
            let limit = match cap {
                BikeCapability::Road => 12.0,
                BikeCapability::Trekking => 18.0,
                BikeCapability::Mountain => 30.0,
            };
            if pct.abs() > limit {
                return true;
            }
        }
    }
    if let Some(raw) = tags.get("surface") {
        let s = raw.trim().to_ascii_lowercase();
        match cap {
            BikeCapability::Road => {
                if is_rough_surface(&s) && !is_paved_surface(&s) {
                    return true;
                }
            }
            BikeCapability::Trekking => {
                if matches!(s.as_str(), "sand" | "mud" | "snow" | "ice") {
                    return true;
                }
            }
            BikeCapability::Mountain => {}
        }
    }
    false
}

fn tag_eq(tags: &HashMap<String, String>, key: &str, want: &str) -> bool {
    tags.get(key).is_some_and(|v| v.eq_ignore_ascii_case(want))
}

pub fn way_id_from_edge_id(edge_id: &str) -> Option<i64> {
    edge_id.split('-').next()?.parse().ok()
}

/// Load OSM way tags for terrain suitability (single PBF pass, ways of interest only).
pub fn load_way_terrain_tags(
    pbf: &Path,
    way_ids: &HashSet<i64>,
) -> anyhow::Result<HashMap<i64, HashMap<String, String>>> {
    if way_ids.is_empty() {
        return Ok(HashMap::new());
    }
    const KEYS: &[&str] = &[
        "surface",
        "smoothness",
        "tracktype",
        "mtb:scale",
        "mtb:scale:uphill",
        "incline",
        "route",
        "highway",
    ];
    let mut out = HashMap::new();
    crate::download::pbf_priority::for_each_pbf_elements(pbf, |element| {
        let Element::Way(way) = element else {
            return;
        };
        if !way_ids.contains(&way.id()) {
            return;
        }
        let tags: HashMap<String, String> = way
            .tags()
            .filter(|(k, _)| KEYS.contains(k))
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        if !tags.is_empty() {
            out.insert(way.id(), tags);
        }
    })?;
    Ok(out)
}

/// Hard-remove edges whose OSM way tags exceed the capability thresholds.
pub fn apply_bike_suitability(
    graph: &mut RouteGraph,
    way_tags: &HashMap<i64, HashMap<String, String>>,
    cap: BikeCapability,
) -> usize {
    if graph.profile() != super::builder::RoutingProfile::Bicycle {
        return 0;
    }
    let before = graph.edges.len();
    let mut kept = Vec::with_capacity(before);
    for edge in graph.edges.drain(..) {
        let Some(wid) = way_id_from_edge_id(&edge.id) else {
            kept.push(edge);
            continue;
        };
        let tags = way_tags.get(&wid);
        if tags.is_some_and(|t| tags_unsuitable_for(cap, t)) {
            continue;
        }
        kept.push(edge);
    }
    graph.edges = kept;
    let removed = before.saturating_sub(graph.edges.len());
    if removed > 0 {
        graph.rebuild_after_edge_filter();
    }
    removed
}

/// PBF-backed filter: load tags then hard-remove unsuitable edges.
pub fn apply_bike_suitability_from_pbf(
    graph: &mut RouteGraph,
    pbf: &Path,
    cap: BikeCapability,
) -> anyhow::Result<usize> {
    let mut ids = HashSet::new();
    for e in &graph.edges {
        if let Some(id) = way_id_from_edge_id(&e.id) {
            ids.insert(id);
        }
    }
    let tags = load_way_terrain_tags(pbf, &ids)?;
    Ok(apply_bike_suitability(graph, &tags, cap))
}

// --- Soft surface / highway preference (capability-aware) -------------------
//
// Mirrors motor [`apply_surface_preference`]: multiplies `base_weight` (metres)
// so longer preferred corridors beat short "wrong" connectors. Never hard-filters.

/// Soft highway multipliers for Road bikes (prefer asphalt roads over trails).
///
/// Path/footway is steep even when `surface_quality` is Good: pack hits lack
/// `mtb:scale` / smoothness, and untagged paths default to Good in the pack.
pub const BIKE_ROAD_PATH_FOOTWAY: f64 = 5.0;
pub const BIKE_ROAD_TRACK: f64 = 3.5;
pub const BIKE_ROAD_CYCLEWAY: f64 = 1.12;
pub const BIKE_ROAD_SURFACE_MARGINAL: f64 = 2.2;
pub const BIKE_ROAD_SURFACE_POOR: f64 = 4.0;

/// Soft multipliers for Gravel / Trekking (prefer gravel/compacted over asphalt
/// arterials and over technical path/singletrack).
///
/// Path cost must stay clearly above MTB's ~1.0 so the two modes diverge on
/// mixed asphalt/gravel/path graphs. Pack hits cannot hard-filter `mtb:scale`.
pub const BIKE_GRAVEL_ASPHALT_ARTERIAL: f64 = 2.4;
pub const BIKE_GRAVEL_ASPHALT_LOCAL: f64 = 1.55;
pub const BIKE_GRAVEL_SURFACE_POOR: f64 = 2.0;
pub const BIKE_GRAVEL_PATH: f64 = 2.2;
pub const BIKE_GRAVEL_TRACK: f64 = 1.35;

/// Soft multipliers for MTB (prefer path/track/singletrack over paved roads).
pub const BIKE_MTB_ARTERIAL: f64 = 2.6;
pub const BIKE_MTB_LOCAL_PAVED: f64 = 2.8;
pub const BIKE_MTB_CYCLEWAY: f64 = 1.25;
pub const BIKE_MTB_SURFACE_MARGINAL_ON_ROAD: f64 = 1.2;

fn is_path_like(highway: Option<&str>) -> bool {
    matches!(
        highway,
        Some("path") | Some("footway") | Some("steps") | Some("pedestrian") | Some("bridleway")
    )
}

fn is_track(highway: Option<&str>) -> bool {
    highway == Some("track")
}

fn is_cycleway(highway: Option<&str>) -> bool {
    highway == Some("cycleway")
}

fn is_arterial(highway: Option<&str>) -> bool {
    matches!(
        highway,
        Some("motorway")
            | Some("motorway_link")
            | Some("trunk")
            | Some("trunk_link")
            | Some("primary")
            | Some("primary_link")
            | Some("secondary")
            | Some("secondary_link")
    )
}

fn is_local_road(highway: Option<&str>) -> bool {
    matches!(
        highway,
        Some("tertiary")
            | Some("tertiary_link")
            | Some("unclassified")
            | Some("residential")
            | Some("living_street")
            | Some("road")
            | Some("service")
    )
}

/// Soft cost multiplier (≥ 1.0) for one edge under `cap`.
pub fn edge_bike_soft_multiplier(edge: &GraphEdge, cap: BikeCapability) -> f64 {
    if edge.is_ferry {
        return 1.0;
    }
    let hw = edge.highway.as_deref();
    let sq = edge.surface_quality;
    match cap {
        BikeCapability::Road => {
            let hw_mult = if is_path_like(hw) {
                BIKE_ROAD_PATH_FOOTWAY
            } else if is_track(hw) {
                BIKE_ROAD_TRACK
            } else if is_cycleway(hw) {
                BIKE_ROAD_CYCLEWAY
            } else {
                1.0
            };
            let surf_mult = match sq {
                SurfaceQuality::Good => 1.0,
                SurfaceQuality::Marginal => BIKE_ROAD_SURFACE_MARGINAL,
                SurfaceQuality::Poor => BIKE_ROAD_SURFACE_POOR,
            };
            hw_mult * surf_mult
        }
        BikeCapability::Trekking => {
            // Prefer gravel/compacted (Marginal) roads; soft-penalize asphalt and
            // path/singletrack (touring bikes are not MTB).
            let mut mult = 1.0;
            if is_path_like(hw) {
                mult *= BIKE_GRAVEL_PATH;
            } else if is_track(hw) {
                mult *= BIKE_GRAVEL_TRACK;
            }
            match sq {
                SurfaceQuality::Good if is_arterial(hw) => mult *= BIKE_GRAVEL_ASPHALT_ARTERIAL,
                SurfaceQuality::Good if is_local_road(hw) || is_cycleway(hw) => {
                    mult *= BIKE_GRAVEL_ASPHALT_LOCAL;
                }
                SurfaceQuality::Good => {}
                SurfaceQuality::Marginal => {}
                SurfaceQuality::Poor => mult *= BIKE_GRAVEL_SURFACE_POOR,
            }
            mult
        }
        BikeCapability::Mountain => {
            let mut mult = 1.0;
            if is_arterial(hw) {
                mult *= BIKE_MTB_ARTERIAL;
            } else if is_local_road(hw) && matches!(sq, SurfaceQuality::Good) {
                mult *= BIKE_MTB_LOCAL_PAVED;
            } else if is_cycleway(hw) {
                mult *= BIKE_MTB_CYCLEWAY;
            }
            // Path/track stay near 1.0; gravel roads only lightly nudged.
            if matches!(sq, SurfaceQuality::Marginal) && is_local_road(hw) {
                mult *= BIKE_MTB_SURFACE_MARGINAL_ON_ROAD;
            }
            mult
        }
    }
}

/// Apply capability-aware soft costs to bicycle graph edges.
///
/// Uses packed/built [`GraphEdge::highway`] + [`GraphEdge::surface_quality`] so
/// both on-device PBF builds and v8 pack hits work (no OSM way-id lookup).
pub fn apply_bike_surface_preference(graph: &mut RouteGraph, cap: BikeCapability) {
    if graph.profile() != RoutingProfile::Bicycle {
        return;
    }
    graph.edges.par_iter_mut().for_each(|edge| {
        let mult = edge_bike_soft_multiplier(edge, cap);
        if mult > 1.0 + 1e-9 {
            edge.base_weight *= mult;
            if let Some(ref mut eco) = edge.eco_weight {
                *eco *= mult;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_tags_never_unsuitable() {
        assert!(!tags_unsuitable_for(BikeCapability::Road, &HashMap::new()));
    }

    #[test]
    fn road_excludes_mtb_scale_and_mtb_route() {
        let mut t = HashMap::new();
        t.insert("mtb:scale".into(), "2".into());
        assert!(tags_unsuitable_for(BikeCapability::Road, &t));
        assert!(!tags_unsuitable_for(BikeCapability::Mountain, &t));
        t.clear();
        t.insert("route".into(), "mtb".into());
        assert!(tags_unsuitable_for(BikeCapability::Road, &t));
        assert!(!tags_unsuitable_for(BikeCapability::Mountain, &t));
    }

    #[test]
    fn paved_road_unaffected() {
        let mut t = HashMap::new();
        t.insert("surface".into(), "asphalt".into());
        t.insert("smoothness".into(), "good".into());
        for cap in [
            BikeCapability::Road,
            BikeCapability::Trekking,
            BikeCapability::Mountain,
        ] {
            assert!(!tags_unsuitable_for(cap, &t), "{cap:?}");
        }
    }

    fn bike_edge(
        id: &str,
        src: i64,
        tgt: i64,
        length_m: f64,
        highway: &str,
        surface: SurfaceQuality,
    ) -> GraphEdge {
        GraphEdge {
            id: id.into(),
            source: osm4routing::NodeId(src),
            target: osm4routing::NodeId(tgt),
            length_m,
            base_weight: length_m,
            eco_weight: None,
            start_lat: 60.0,
            start_lon: 10.0,
            end_lat: 60.01,
            end_lon: 10.01,
            shape: Vec::new(),
            highway: Some(highway.into()),
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
            surface_quality: surface,
        }
    }

    fn bike_graph(edges: Vec<GraphEdge>) -> RouteGraph {
        use geo_types::Coord;
        use std::collections::HashMap as StdHashMap;
        let mut nodes = StdHashMap::new();
        for (id, lat, lon) in [
            (1, 60.0, 10.0),
            (2, 60.01, 10.01),
            (3, 60.005, 10.02),
            (4, 60.005, 9.98),
        ] {
            nodes.insert(
                osm4routing::NodeId(id),
                osm4routing::Node {
                    id: osm4routing::NodeId(id),
                    coord: Coord { x: lon, y: lat },
                    uses: 0,
                },
            );
        }
        RouteGraph::from_parts(nodes, edges, RoutingProfile::Bicycle)
    }

    /// Road: short path loses to longer asphalt residential.
    #[test]
    fn road_prefers_asphalt_over_shorter_path() {
        // 1 --path 800m--> 2
        // 1 --residential asphalt 1000m--> 3 --residential asphalt 1000m--> 2
        let edges = vec![
            bike_edge("p12", 1, 2, 800.0, "path", SurfaceQuality::Good),
            bike_edge("p21", 2, 1, 800.0, "path", SurfaceQuality::Good),
            bike_edge("r13", 1, 3, 1000.0, "residential", SurfaceQuality::Good),
            bike_edge("r31", 3, 1, 1000.0, "residential", SurfaceQuality::Good),
            bike_edge("r32", 3, 2, 1000.0, "residential", SurfaceQuality::Good),
            bike_edge("r23", 2, 3, 1000.0, "residential", SurfaceQuality::Good),
        ];
        let mut graph = bike_graph(edges);
        apply_bike_surface_preference(&mut graph, BikeCapability::Road);
        let (path, _, _) = graph
            .shortest_path(osm4routing::NodeId(1), osm4routing::NodeId(2), false)
            .expect("path");
        assert_eq!(
            path,
            vec![
                osm4routing::NodeId(1),
                osm4routing::NodeId(3),
                osm4routing::NodeId(2)
            ],
            "Road should prefer asphalt via 3, got {path:?}"
        );
    }

    /// Gravel/Trekking: shorter asphalt arterial loses to longer gravel tertiary.
    #[test]
    fn gravel_prefers_gravel_over_shorter_asphalt_arterial() {
        // 1 --primary asphalt 1200m--> 2
        // 1 --tertiary gravel 1000m--> 3 --tertiary gravel 1000m--> 2
        let edges = vec![
            bike_edge("a12", 1, 2, 1200.0, "primary", SurfaceQuality::Good),
            bike_edge("a21", 2, 1, 1200.0, "primary", SurfaceQuality::Good),
            bike_edge("g13", 1, 3, 1000.0, "tertiary", SurfaceQuality::Marginal),
            bike_edge("g31", 3, 1, 1000.0, "tertiary", SurfaceQuality::Marginal),
            bike_edge("g32", 3, 2, 1000.0, "tertiary", SurfaceQuality::Marginal),
            bike_edge("g23", 2, 3, 1000.0, "tertiary", SurfaceQuality::Marginal),
        ];
        let mut graph = bike_graph(edges);
        apply_bike_surface_preference(&mut graph, BikeCapability::Trekking);
        let (path, _, _) = graph
            .shortest_path(osm4routing::NodeId(1), osm4routing::NodeId(2), false)
            .expect("path");
        assert_eq!(
            path,
            vec![
                osm4routing::NodeId(1),
                osm4routing::NodeId(3),
                osm4routing::NodeId(2)
            ],
            "Gravel should prefer gravel via 3, got {path:?}"
        );
    }

    /// MTB: shorter residential asphalt loses to longer path.
    #[test]
    fn mtb_prefers_path_over_shorter_paved_local() {
        // 1 --residential asphalt 900m--> 2
        // 1 --path 1200m--> 3 --path 1200m--> 2
        let edges = vec![
            bike_edge("r12", 1, 2, 900.0, "residential", SurfaceQuality::Good),
            bike_edge("r21", 2, 1, 900.0, "residential", SurfaceQuality::Good),
            bike_edge("p13", 1, 3, 1200.0, "path", SurfaceQuality::Poor),
            bike_edge("p31", 3, 1, 1200.0, "path", SurfaceQuality::Poor),
            bike_edge("p32", 3, 2, 1200.0, "path", SurfaceQuality::Poor),
            bike_edge("p23", 2, 3, 1200.0, "path", SurfaceQuality::Poor),
        ];
        let mut graph = bike_graph(edges);
        apply_bike_surface_preference(&mut graph, BikeCapability::Mountain);
        let (path, _, _) = graph
            .shortest_path(osm4routing::NodeId(1), osm4routing::NodeId(2), false)
            .expect("path");
        assert_eq!(
            path,
            vec![
                osm4routing::NodeId(1),
                osm4routing::NodeId(3),
                osm4routing::NodeId(2)
            ],
            "MTB should prefer path via 3, got {path:?}"
        );
    }

    #[test]
    fn road_soft_cost_does_not_block_short_path_connector_when_only_option() {
        // Only a path exists — soft cost must not remove connectivity.
        let edges = vec![
            bike_edge("p12", 1, 2, 500.0, "path", SurfaceQuality::Poor),
            bike_edge("p21", 2, 1, 500.0, "path", SurfaceQuality::Poor),
        ];
        let mut graph = bike_graph(edges);
        apply_bike_surface_preference(&mut graph, BikeCapability::Road);
        assert!(graph
            .shortest_path(osm4routing::NodeId(1), osm4routing::NodeId(2), false)
            .is_some());
    }

    /// Pack-hit stand-in: soft costs only (no hard suitability). Short poor path
    /// that hard-filter would drop for Road must still lose to asphalt.
    #[test]
    fn pack_hit_soft_only_road_avoids_short_poor_path() {
        // 1 --path dirt 600m--> 2   (would be hard-unsuitable: rough surface)
        // 1 --residential 1100m--> 3 --residential 1100m--> 2
        let edges = vec![
            bike_edge("p12", 1, 2, 600.0, "path", SurfaceQuality::Poor),
            bike_edge("p21", 2, 1, 600.0, "path", SurfaceQuality::Poor),
            bike_edge("r13", 1, 3, 1100.0, "residential", SurfaceQuality::Good),
            bike_edge("r31", 3, 1, 1100.0, "residential", SurfaceQuality::Good),
            bike_edge("r32", 3, 2, 1100.0, "residential", SurfaceQuality::Good),
            bike_edge("r23", 2, 3, 1100.0, "residential", SurfaceQuality::Good),
        ];
        let mut graph = bike_graph(edges);
        apply_bike_surface_preference(&mut graph, BikeCapability::Road);
        let (path, _, _) = graph
            .shortest_path(osm4routing::NodeId(1), osm4routing::NodeId(2), false)
            .expect("path");
        assert_eq!(
            path,
            vec![
                osm4routing::NodeId(1),
                osm4routing::NodeId(3),
                osm4routing::NodeId(2)
            ],
            "Road soft-only must avoid short poor path, got {path:?}"
        );
    }

    /// Pack-hit stand-in: Trekking soft costs must prefer gravel over short poor path.
    #[test]
    fn pack_hit_soft_only_trekking_avoids_short_poor_path() {
        // 1 --path dirt 700m--> 2
        // 1 --tertiary gravel 1200m--> 3 --tertiary gravel 1200m--> 2
        let edges = vec![
            bike_edge("p12", 1, 2, 700.0, "path", SurfaceQuality::Poor),
            bike_edge("p21", 2, 1, 700.0, "path", SurfaceQuality::Poor),
            bike_edge("g13", 1, 3, 1200.0, "tertiary", SurfaceQuality::Marginal),
            bike_edge("g31", 3, 1, 1200.0, "tertiary", SurfaceQuality::Marginal),
            bike_edge("g32", 3, 2, 1200.0, "tertiary", SurfaceQuality::Marginal),
            bike_edge("g23", 2, 3, 1200.0, "tertiary", SurfaceQuality::Marginal),
        ];
        let mut graph = bike_graph(edges);
        apply_bike_surface_preference(&mut graph, BikeCapability::Trekking);
        let (path, _, _) = graph
            .shortest_path(osm4routing::NodeId(1), osm4routing::NodeId(2), false)
            .expect("path");
        assert_eq!(
            path,
            vec![
                osm4routing::NodeId(1),
                osm4routing::NodeId(3),
                osm4routing::NodeId(2)
            ],
            "Trekking soft-only must avoid short poor path, got {path:?}"
        );
    }

    /// Same mixed graph for all three modes: asphalt / gravel / path must diverge.
    #[test]
    fn mtb_and_gravel_diverge_on_mixed_asphalt_gravel_path() {
        // 1 --residential asphalt 1000m--> 2
        // 1 --tertiary gravel 700m--> 3 --tertiary gravel 700m--> 2  (1400)
        // 1 --path poor 550m--> 4 --path poor 550m--> 2             (1100)
        let edges = vec![
            bike_edge("a12", 1, 2, 1000.0, "residential", SurfaceQuality::Good),
            bike_edge("a21", 2, 1, 1000.0, "residential", SurfaceQuality::Good),
            bike_edge("g13", 1, 3, 700.0, "tertiary", SurfaceQuality::Marginal),
            bike_edge("g31", 3, 1, 700.0, "tertiary", SurfaceQuality::Marginal),
            bike_edge("g32", 3, 2, 700.0, "tertiary", SurfaceQuality::Marginal),
            bike_edge("g23", 2, 3, 700.0, "tertiary", SurfaceQuality::Marginal),
            bike_edge("p14", 1, 4, 550.0, "path", SurfaceQuality::Poor),
            bike_edge("p41", 4, 1, 550.0, "path", SurfaceQuality::Poor),
            bike_edge("p42", 4, 2, 550.0, "path", SurfaceQuality::Poor),
            bike_edge("p24", 2, 4, 550.0, "path", SurfaceQuality::Poor),
        ];

        let via3 = vec![
            osm4routing::NodeId(1),
            osm4routing::NodeId(3),
            osm4routing::NodeId(2),
        ];
        let via4 = vec![
            osm4routing::NodeId(1),
            osm4routing::NodeId(4),
            osm4routing::NodeId(2),
        ];
        let direct = vec![osm4routing::NodeId(1), osm4routing::NodeId(2)];

        let mut road_g = bike_graph(edges.clone());
        apply_bike_surface_preference(&mut road_g, BikeCapability::Road);
        let (road_path, _, _) = road_g
            .shortest_path(osm4routing::NodeId(1), osm4routing::NodeId(2), false)
            .expect("road");
        assert_eq!(
            road_path, direct,
            "Road should take asphalt direct, got {road_path:?}"
        );

        let mut gravel_g = bike_graph(edges.clone());
        apply_bike_surface_preference(&mut gravel_g, BikeCapability::Trekking);
        let (gravel_path, _, _) = gravel_g
            .shortest_path(osm4routing::NodeId(1), osm4routing::NodeId(2), false)
            .expect("gravel");
        assert_eq!(
            gravel_path, via3,
            "Gravel should take gravel via 3, got {gravel_path:?}"
        );

        let mut mtb_g = bike_graph(edges);
        apply_bike_surface_preference(&mut mtb_g, BikeCapability::Mountain);
        let (mtb_path, _, _) = mtb_g
            .shortest_path(osm4routing::NodeId(1), osm4routing::NodeId(2), false)
            .expect("mtb");
        assert_eq!(
            mtb_path, via4,
            "MTB should take path via 4, got {mtb_path:?}"
        );

        assert_ne!(
            gravel_path, mtb_path,
            "Gravel and MTB must diverge on this mixed graph"
        );
    }
}
