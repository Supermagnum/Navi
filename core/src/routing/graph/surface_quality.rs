//! Surface / tracktype quality for motor routing: soft edge costs, transition
//! penalties, and waypoint snap preference. Internal to pathfinding only — no
//! user-facing warnings.

use std::collections::HashSet;
use std::path::Path;

use osm4routing::NodeId;
use rayon::prelude::*;

use super::bike_suitability::{load_way_terrain_tags, way_id_from_edge_id};
use super::builder::{RouteGraph, RoutingProfile};

/// Soft multiplier applied to poor-surface edges (car profile).
pub const SURFACE_POOR_EDGE_PENALTY: f64 = 4.0;

/// Soft multiplier applied to marginal-surface edges (car profile).
pub const SURFACE_MARGINAL_EDGE_PENALTY: f64 = 1.5;

/// Metre-equivalent penalty when surface class drops by more than
/// [`SURFACE_TRANSITION_MAX_CLASS_DROP`] between consecutive edges.
pub const SURFACE_TRANSITION_PENALTY_M: f64 = 500.0;

/// Transition penalty applies when `to.rank() - from.rank()` exceeds this value.
pub const SURFACE_TRANSITION_MAX_CLASS_DROP: u8 = 1;

/// Virtual surface before the first edge at a snapped waypoint (car profile).
/// Models arriving from the general paved network so connector stubs onto poor
/// tracks incur a transition penalty, not only mid-route edges.
pub const SNAP_VIRTUAL_APPROACH_SURFACE: SurfaceQuality = SurfaceQuality::Good;

/// Motor routing surface strictness (car vs off-road).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum SurfaceRoutingMode {
    /// Prefer good surfaces; penalize poor/unknown tracks and harsh transitions.
    #[default]
    Car,
    /// No surface-based weighting or transition penalties.
    Offroad,
}

impl SurfaceRoutingMode {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "offroad" | "off_road" | "4x4" | "4wd" => Self::Offroad,
            _ => Self::Car,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Car => "car",
            Self::Offroad => "offroad",
        }
    }
}

/// Ranked driveability from OSM `surface` / `tracktype` / `highway=track`.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    serde::Serialize,
    serde::Deserialize,
)]
#[repr(u8)]
pub enum SurfaceQuality {
    Good = 0,
    Marginal = 1,
    #[default]
    Poor = 2,
}

impl SurfaceQuality {
    pub fn rank(self) -> u8 {
        self as u8
    }
}

fn classify_surface_value(raw: &str) -> SurfaceQuality {
    match raw.trim().to_ascii_lowercase().as_str() {
        "paved" | "asphalt" | "concrete" | "concrete:plates" | "concrete:lanes" => {
            SurfaceQuality::Good
        }
        "gravel" | "compacted" | "fine_gravel" => SurfaceQuality::Marginal,
        "dirt" | "earth" | "ground" | "mud" | "sand" | "unpaved" | "grass" | "snow" | "ice" => {
            SurfaceQuality::Poor
        }
        _ => SurfaceQuality::Poor,
    }
}

fn classify_tracktype(raw: &str) -> SurfaceQuality {
    let t = raw.trim().to_ascii_lowercase();
    let Some(rest) = t.strip_prefix("grade") else {
        return SurfaceQuality::Poor;
    };
    match rest.parse::<u8>() {
        Ok(1) => SurfaceQuality::Good,
        Ok(2) => SurfaceQuality::Marginal,
        Ok(3..=5) => SurfaceQuality::Poor,
        _ => SurfaceQuality::Poor,
    }
}

/// Classify one way from OSM tags (conservative: worst explicit tag wins).
pub fn classify_surface_tags(
    highway: Option<&str>,
    surface: Option<&str>,
    tracktype: Option<&str>,
) -> SurfaceQuality {
    let mut from_tags = Vec::new();
    if let Some(s) = surface {
        from_tags.push(classify_surface_value(s));
    }
    if let Some(tt) = tracktype {
        from_tags.push(classify_tracktype(tt));
    }
    if !from_tags.is_empty() {
        return from_tags.into_iter().max().unwrap();
    }
    if highway == Some("track") {
        SurfaceQuality::Poor
    } else {
        SurfaceQuality::Good
    }
}

/// Infer surface class from highway alone when detailed tags are unavailable.
pub fn infer_surface_from_highway(highway: Option<&str>) -> SurfaceQuality {
    if highway == Some("track") {
        SurfaceQuality::Poor
    } else {
        SurfaceQuality::Good
    }
}

/// Soft edge cost multiplier for one surface class under `mode`.
pub fn edge_surface_multiplier(quality: SurfaceQuality, mode: SurfaceRoutingMode) -> f64 {
    if mode == SurfaceRoutingMode::Offroad {
        return 1.0;
    }
    match quality {
        SurfaceQuality::Good => 1.0,
        SurfaceQuality::Marginal => SURFACE_MARGINAL_EDGE_PENALTY,
        SurfaceQuality::Poor => SURFACE_POOR_EDGE_PENALTY,
    }
}

/// Metre-equivalent transition penalty between consecutive edges.
///
/// Callers seed the path start with [`SNAP_VIRTUAL_APPROACH_SURFACE`] so the
/// first routed edge from a snapped waypoint is not exempt from transition cost.
pub fn surface_transition_cost_m(
    from: Option<SurfaceQuality>,
    to: SurfaceQuality,
    mode: SurfaceRoutingMode,
) -> f64 {
    if mode == SurfaceRoutingMode::Offroad {
        return 0.0;
    }
    let Some(from) = from else {
        return 0.0;
    };
    let drop = to.rank().saturating_sub(from.rank());
    if drop > SURFACE_TRANSITION_MAX_CLASS_DROP {
        SURFACE_TRANSITION_PENALTY_M
    } else {
        0.0
    }
}

/// Worst (highest rank) surface among edges incident to `node`.
pub fn worst_incident_surface(graph: &RouteGraph, node: NodeId) -> SurfaceQuality {
    let mut worst = SurfaceQuality::Good;
    for edge in &graph.edges {
        if (edge.source == node || edge.target == node) && edge.surface_quality > worst {
            worst = edge.surface_quality;
        }
    }
    worst
}

/// Best (lowest rank) surface among edges incident to `node`.
pub fn best_incident_surface(graph: &RouteGraph, node: NodeId) -> SurfaceQuality {
    let mut best = SurfaceQuality::Poor;
    for edge in &graph.edges {
        if (edge.source == node || edge.target == node) && edge.surface_quality < best {
            best = edge.surface_quality;
        }
    }
    best
}

/// Apply surface soft-cost multipliers to motor graph edges.
pub fn apply_surface_preference(graph: &mut RouteGraph, mode: SurfaceRoutingMode) {
    if mode == SurfaceRoutingMode::Offroad {
        return;
    }
    if !matches!(graph.profile(), RoutingProfile::Car | RoutingProfile::Truck) {
        return;
    }
    graph.edges.par_iter_mut().for_each(|edge| {
        let mult = edge_surface_multiplier(edge.surface_quality, mode);
        if mult > 1.0 + 1e-9 {
            edge.base_weight *= mult;
            if let Some(ref mut eco) = edge.eco_weight {
                *eco *= mult;
            }
        }
    });
}

/// Refine [`GraphEdge::surface_quality`] from a PBF pass (indexed packs / bbox builds).
pub fn apply_surface_quality_from_pbf(graph: &mut RouteGraph, pbf: &Path) -> anyhow::Result<usize> {
    if !matches!(graph.profile(), RoutingProfile::Car | RoutingProfile::Truck) {
        return Ok(0);
    }
    let way_ids: HashSet<i64> = graph
        .edges
        .iter()
        .filter_map(|e| way_id_from_edge_id(&e.id))
        .collect();
    let tags = load_way_terrain_tags(pbf, &way_ids)?;
    let mut updated = 0usize;
    for edge in &mut graph.edges {
        let Some(wid) = way_id_from_edge_id(&edge.id) else {
            continue;
        };
        let Some(wtags) = tags.get(&wid) else {
            continue;
        };
        let sq = classify_surface_tags(
            edge.highway
                .as_deref()
                .or_else(|| wtags.get("highway").map(String::as_str)),
            wtags.get("surface").map(String::as_str),
            wtags.get("tracktype").map(String::as_str),
        );
        if edge.surface_quality != sq {
            edge.surface_quality = sq;
            updated += 1;
        }
    }
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_good_paved_and_grade1() {
        assert_eq!(
            classify_surface_tags(None, Some("asphalt"), None),
            SurfaceQuality::Good
        );
        assert_eq!(
            classify_surface_tags(Some("track"), None, Some("grade1")),
            SurfaceQuality::Good
        );
    }

    #[test]
    fn classify_marginal_gravel_and_grade2() {
        assert_eq!(
            classify_surface_tags(None, Some("gravel"), None),
            SurfaceQuality::Marginal
        );
        assert_eq!(
            classify_surface_tags(Some("track"), None, Some("grade2")),
            SurfaceQuality::Marginal
        );
    }

    #[test]
    fn untagged_track_is_poor() {
        assert_eq!(
            classify_surface_tags(Some("track"), None, None),
            SurfaceQuality::Poor
        );
    }

    #[test]
    fn transition_penalty_applies_on_first_edge_from_snap() {
        assert_eq!(
            surface_transition_cost_m(
                Some(SNAP_VIRTUAL_APPROACH_SURFACE),
                SurfaceQuality::Poor,
                SurfaceRoutingMode::Car
            ),
            SURFACE_TRANSITION_PENALTY_M
        );
    }

    #[test]
    fn transition_penalty_only_on_large_drop() {
        assert_eq!(
            surface_transition_cost_m(
                Some(SurfaceQuality::Good),
                SurfaceQuality::Marginal,
                SurfaceRoutingMode::Car
            ),
            0.0
        );
        assert_eq!(
            surface_transition_cost_m(
                Some(SurfaceQuality::Good),
                SurfaceQuality::Poor,
                SurfaceRoutingMode::Car
            ),
            SURFACE_TRANSITION_PENALTY_M
        );
        assert_eq!(
            surface_transition_cost_m(
                Some(SurfaceQuality::Good),
                SurfaceQuality::Poor,
                SurfaceRoutingMode::Offroad
            ),
            0.0
        );
    }

    #[test]
    fn edge_multipliers_car_vs_offroad() {
        assert_eq!(
            edge_surface_multiplier(SurfaceQuality::Poor, SurfaceRoutingMode::Car),
            SURFACE_POOR_EDGE_PENALTY
        );
        assert_eq!(
            edge_surface_multiplier(SurfaceQuality::Poor, SurfaceRoutingMode::Offroad),
            1.0
        );
    }
}
