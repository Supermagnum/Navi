//! Surface / tracktype quality for motor routing: soft edge costs, transition
//! penalties, and waypoint snap preference. Internal to pathfinding only — no
//! user-facing warnings.

use std::collections::HashSet;
use std::path::Path;

use osm4routing::NodeId;
use rayon::prelude::*;

use crate::config::Profile;

use super::bike_suitability::{load_way_terrain_tags, way_id_from_edge_id};
use super::builder::{GraphEdge, RouteGraph, RoutingProfile};

/// Soft multiplier applied to poor-surface edges (car profile).
pub const SURFACE_POOR_EDGE_PENALTY: f64 = 3.0;

/// Soft multiplier applied to marginal-surface edges (car profile).
pub const SURFACE_MARGINAL_EDGE_PENALTY: f64 = 1.5;

pub const SURFACE_MARGINAL_MOTORCYCLE: f64 = 2.2;
pub const SURFACE_POOR_MOTORCYCLE: f64 = 4.5;
pub const SURFACE_MARGINAL_TRUCK: f64 = 2.0;
pub const SURFACE_POOR_TRUCK: f64 = 4.0;
pub const SURFACE_MARGINAL_MOBILE_HOME: f64 = 2.4;
pub const SURFACE_POOR_MOBILE_HOME: f64 = 5.0;

/// Missing posted/practical/advisory maxspeed — car / motorcycle.
pub const MAXSPEED_MISSING_CAR: f64 = 1.10;
/// Missing maxspeed — truck / mobile home.
pub const MAXSPEED_MISSING_TRUCK: f64 = 1.15;

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

/// Fine-grained motor soft-cost table (Car pack is shared with Motorcycle;
/// Truck pack with MobileHome — multipliers are applied at plan time).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MotorSoftCostProfile {
    Car,
    Motorcycle,
    Truck,
    MobileHome,
}

impl MotorSoftCostProfile {
    /// Map a travel [`Profile`] to motor soft costs, if applicable.
    pub fn from_travel_profile(profile: Profile) -> Option<Self> {
        match profile {
            Profile::Car | Profile::CarElectric => Some(Self::Car),
            Profile::Motorcycle | Profile::MotorcycleElectric => Some(Self::Motorcycle),
            Profile::Truck | Profile::TruckElectric => Some(Self::Truck),
            Profile::MobileHome => Some(Self::MobileHome),
            Profile::Hiking | Profile::Cycling | Profile::CyclingElectric => None,
        }
    }

    /// Fallback from coarse [`RoutingProfile`] (Motorcycle→Car, MobileHome→Truck).
    pub fn from_routing_profile(profile: RoutingProfile) -> Option<Self> {
        match profile {
            RoutingProfile::Car => Some(Self::Car),
            RoutingProfile::Truck => Some(Self::Truck),
            RoutingProfile::Foot | RoutingProfile::Bicycle => None,
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

    pub fn as_u8(self) -> u8 {
        self as u8
    }

    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Good,
            1 => Self::Marginal,
            _ => Self::Poor,
        }
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

/// Soft edge cost multiplier for one surface class under `mode` / cost profile.
pub fn edge_surface_multiplier(
    quality: SurfaceQuality,
    mode: SurfaceRoutingMode,
    cost_profile: MotorSoftCostProfile,
) -> f64 {
    if mode == SurfaceRoutingMode::Offroad {
        return 1.0;
    }
    let (marginal, poor) = match cost_profile {
        MotorSoftCostProfile::Car => (SURFACE_MARGINAL_EDGE_PENALTY, SURFACE_POOR_EDGE_PENALTY),
        MotorSoftCostProfile::Motorcycle => (SURFACE_MARGINAL_MOTORCYCLE, SURFACE_POOR_MOTORCYCLE),
        MotorSoftCostProfile::Truck => (SURFACE_MARGINAL_TRUCK, SURFACE_POOR_TRUCK),
        MotorSoftCostProfile::MobileHome => {
            (SURFACE_MARGINAL_MOBILE_HOME, SURFACE_POOR_MOBILE_HOME)
        }
    };
    match quality {
        SurfaceQuality::Good => 1.0,
        SurfaceQuality::Marginal => marginal,
        SurfaceQuality::Poor => poor,
    }
}

/// True when any of OSM `maxspeed` / `maxspeed:practical` / `maxspeed:advisory` is set.
pub fn edge_has_posted_maxspeed(edge: &GraphEdge) -> bool {
    edge.maxspeed_kmh.is_some()
        || edge.maxspeed_practical_kmh.is_some()
        || edge.maxspeed_advisory_kmh.is_some()
}

fn motor_highway_for_maxspeed_penalty(highway: Option<&str>) -> bool {
    match highway {
        None => false,
        Some("ferry") | Some("path") | Some("footway") | Some("cycleway") | Some("steps")
        | Some("pedestrian") | Some("platform") => false,
        Some(_) => true,
    }
}

/// Soft multiplier when posted/practical/advisory maxspeed are all absent.
pub fn edge_maxspeed_multiplier(
    edge: &GraphEdge,
    mode: SurfaceRoutingMode,
    cost_profile: MotorSoftCostProfile,
) -> f64 {
    if mode == SurfaceRoutingMode::Offroad {
        return 1.0;
    }
    if edge.is_ferry || !motor_highway_for_maxspeed_penalty(edge.highway.as_deref()) {
        return 1.0;
    }
    if edge_has_posted_maxspeed(edge) {
        return 1.0;
    }
    match cost_profile {
        MotorSoftCostProfile::Car | MotorSoftCostProfile::Motorcycle => MAXSPEED_MISSING_CAR,
        MotorSoftCostProfile::Truck | MotorSoftCostProfile::MobileHome => MAXSPEED_MISSING_TRUCK,
    }
}

/// Combined surface × missing-maxspeed soft multiplier (≥ 1.0).
pub fn edge_motor_soft_multiplier(
    edge: &GraphEdge,
    mode: SurfaceRoutingMode,
    cost_profile: MotorSoftCostProfile,
) -> f64 {
    edge_surface_multiplier(edge.surface_quality, mode, cost_profile)
        * edge_maxspeed_multiplier(edge, mode, cost_profile)
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

/// Apply surface + missing-maxspeed soft-cost multipliers to motor graph edges.
///
/// Call once after weights are length- or eco-based (packs store unpenalized
/// `length_m` as `base_weight`). Multipliers are profile-specific so Motorcycle
/// / MobileHome can differ from the Car / Truck packs they share.
pub fn apply_surface_preference(
    graph: &mut RouteGraph,
    mode: SurfaceRoutingMode,
    cost_profile: MotorSoftCostProfile,
) {
    if mode == SurfaceRoutingMode::Offroad {
        return;
    }
    if !matches!(graph.profile(), RoutingProfile::Car | RoutingProfile::Truck) {
        return;
    }
    graph.edges.par_iter_mut().for_each(|edge| {
        let mult = edge_motor_soft_multiplier(edge, mode, cost_profile);
        if mult > 1.0 + 1e-9 {
            edge.base_weight *= mult;
            if let Some(ref mut eco) = edge.eco_weight {
                *eco *= mult;
            }
        }
    });
}

/// Refine [`GraphEdge::surface_quality`] from a PBF pass (bbox / way-id edge ids only).
///
/// Do **not** call on indexed pack-hit graphs: pack edge ids are `node-node-idx`,
/// so [`way_id_from_edge_id`] mis-parses node ids as way ids.
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
    fn edge_multipliers_per_profile_and_offroad() {
        assert_eq!(
            edge_surface_multiplier(
                SurfaceQuality::Poor,
                SurfaceRoutingMode::Car,
                MotorSoftCostProfile::Car
            ),
            SURFACE_POOR_EDGE_PENALTY
        );
        assert_eq!(
            edge_surface_multiplier(
                SurfaceQuality::Marginal,
                SurfaceRoutingMode::Car,
                MotorSoftCostProfile::MobileHome
            ),
            SURFACE_MARGINAL_MOBILE_HOME
        );
        assert_eq!(
            edge_surface_multiplier(
                SurfaceQuality::Poor,
                SurfaceRoutingMode::Offroad,
                MotorSoftCostProfile::Car
            ),
            1.0
        );
        assert!(
            edge_surface_multiplier(
                SurfaceQuality::Marginal,
                SurfaceRoutingMode::Car,
                MotorSoftCostProfile::Motorcycle
            ) > edge_surface_multiplier(
                SurfaceQuality::Marginal,
                SurfaceRoutingMode::Car,
                MotorSoftCostProfile::Car
            )
        );
    }

    #[test]
    fn missing_maxspeed_multipliers() {
        let mut edge = GraphEdge {
            id: "1-0".into(),
            source: NodeId(1),
            target: NodeId(2),
            length_m: 100.0,
            base_weight: 100.0,
            eco_weight: None,
            start_lat: 60.0,
            start_lon: 10.0,
            end_lat: 60.001,
            end_lon: 10.0,
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
            surface_quality: SurfaceQuality::Good,
        };
        assert_eq!(
            edge_maxspeed_multiplier(&edge, SurfaceRoutingMode::Car, MotorSoftCostProfile::Car),
            MAXSPEED_MISSING_CAR
        );
        edge.maxspeed_kmh = Some(80.0);
        assert_eq!(
            edge_maxspeed_multiplier(&edge, SurfaceRoutingMode::Car, MotorSoftCostProfile::Car),
            1.0
        );
    }

    #[test]
    fn travel_profile_maps_to_soft_cost() {
        assert_eq!(
            MotorSoftCostProfile::from_travel_profile(Profile::Motorcycle),
            Some(MotorSoftCostProfile::Motorcycle)
        );
        assert_eq!(
            MotorSoftCostProfile::from_travel_profile(Profile::MobileHome),
            Some(MotorSoftCostProfile::MobileHome)
        );
        assert_eq!(
            MotorSoftCostProfile::from_travel_profile(Profile::Hiking),
            None
        );
    }
}
