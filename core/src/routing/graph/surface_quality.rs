//! Surface / tracktype quality for motor routing: soft edge costs, transition
//! penalties, and waypoint snap preference. Internal to pathfinding only — no
//! user-facing warnings.

use std::collections::HashSet;
use std::path::Path;

use osm4routing::NodeId;

use crate::config::Profile;

use super::bike_suitability::{load_way_terrain_tags, way_id_from_edge_id};
use super::builder::{GraphEdge, RouteGraph, RoutingProfile};

/// Soft multiplier applied to poor-surface edges (car profile).
pub const SURFACE_POOR_EDGE_PENALTY: f64 = 4.0;

/// Soft multiplier applied to marginal-surface edges (car profile).
///
/// Strong enough that Hamar→Rendalen prefers Elverum/Rv3 asphalt over a shorter
/// gravel tertiary corridor (~1.5× was not enough once surface tags are missing
/// from a pack and only length remains).
pub const SURFACE_MARGINAL_EDGE_PENALTY: f64 = 2.2;

/// Soft multiplier for untagged / unknown surface (car profile).
///
/// Softer than Marginal: never equals Good, but does not treat every untagged
/// residential/service as confirmed gravel.
pub const SURFACE_UNKNOWN_EDGE_PENALTY: f64 = 1.4;

pub const SURFACE_UNKNOWN_MOTORCYCLE: f64 = 1.55;
pub const SURFACE_MARGINAL_MOTORCYCLE: f64 = 3.0;
pub const SURFACE_POOR_MOTORCYCLE: f64 = 5.5;
pub const SURFACE_UNKNOWN_TRUCK: f64 = 1.5;
pub const SURFACE_MARGINAL_TRUCK: f64 = 2.8;
pub const SURFACE_POOR_TRUCK: f64 = 5.0;
pub const SURFACE_UNKNOWN_MOBILE_HOME: f64 = 1.6;
pub const SURFACE_MARGINAL_MOBILE_HOME: f64 = 3.2;
pub const SURFACE_POOR_MOBILE_HOME: f64 = 6.0;

/// Missing posted/practical/advisory maxspeed — car / motorcycle.
pub const MAXSPEED_MISSING_CAR: f64 = 1.20;
/// Missing maxspeed — truck / mobile home.
pub const MAXSPEED_MISSING_TRUCK: f64 = 1.30;

/// Reference posted speed for Good asphalt with maxspeed > 50 (typical primary).
/// Used only for Marginal/Poor edges that *do* carry a posted maxspeed: folds
/// length/speed into soft cost so a shorter gravel@60 loses to asphalt@80.
pub const MOTOR_ROUGH_SURFACE_SPEED_REF_KMH: f64 = 80.0;

/// Cap on [`edge_rough_surface_speed_factor`] so very low posted maxspeed on
/// rough surfaces cannot make a barely-related asphalt detour win when gravel
/// is the only sensible corridor (e.g. maxspeed=20 → uncapped 4×).
pub const MOTOR_ROUGH_SURFACE_SPEED_FACTOR_MAX: f64 = 2.0;

/// Soft highway-class multipliers (motor). Prefer trunk/primary even when pack
/// `surface_quality` is missing/all-Good (pre-v8 or incomplete tags).
pub const HIGHWAY_CLASS_TRUNK_PRIMARY: f64 = 1.0;
pub const HIGHWAY_CLASS_SECONDARY: f64 = 1.12;
pub const HIGHWAY_CLASS_TERTIARY: f64 = 1.60;
pub const HIGHWAY_CLASS_LOCAL: f64 = 1.80;
pub const HIGHWAY_CLASS_SERVICE_TRACK: f64 = 2.20;

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

/// Ranked driveability from OSM `surface` / `tracktype` / highway-class fallback.
///
/// Discriminant order is the transition rank: Good < Unknown < Marginal < Poor.
/// Pack wire bytes (`as_u8` / `from_u8`) keep the v8 layout for the original
/// three classes and append Unknown as `3` so existing packs never reinterpret
/// Marginal/Poor ordinals.
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
    Unknown = 1,
    Marginal = 2,
    #[default]
    Poor = 3,
}

impl SurfaceQuality {
    /// Transition / Ord rank (Good=0 … Poor=3).
    pub fn rank(self) -> u8 {
        self as u8
    }

    /// Pack wire encoding. Stable vs v8: Good=0, Marginal=1, Poor=2; Unknown=3.
    pub fn as_u8(self) -> u8 {
        match self {
            Self::Good => 0,
            Self::Marginal => 1,
            Self::Poor => 2,
            Self::Unknown => 3,
        }
    }

    /// Decode pack wire byte. Unknown values collapse to Poor (conservative).
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Good,
            1 => Self::Marginal,
            2 => Self::Poor,
            3 => Self::Unknown,
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
///
/// When `surface` and `tracktype` are both absent, falls back to a per-highway
/// default via [`infer_surface_from_highway`] (Option C: paved-typical classes
/// stay Good; tertiary→Marginal; local/service→Unknown; track→Poor).
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
    infer_surface_from_highway(highway)
}

/// Infer surface class from highway alone when detailed tags are unavailable.
///
/// Untagged (no `surface` / `tracktype`) defaults:
/// - motorway…secondary (+ `_link`): Good
/// - tertiary (+ `_link`): Marginal
/// - unclassified / residential / living_street / road / service: Unknown
/// - track: Poor
/// - path / footway / cycleway / other: Good (same as pre-fix paved-typical
///   default; motor soft costs already heavily weight path-like classes)
pub fn infer_surface_from_highway(highway: Option<&str>) -> SurfaceQuality {
    match highway {
        Some("track") => SurfaceQuality::Poor,
        Some("tertiary") | Some("tertiary_link") => SurfaceQuality::Marginal,
        Some("unclassified")
        | Some("residential")
        | Some("living_street")
        | Some("road")
        | Some("service") => SurfaceQuality::Unknown,
        // motorway…secondary (+ links), path/footway/cycleway, missing, etc.
        _ => SurfaceQuality::Good,
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
    let (unknown, marginal, poor) = match cost_profile {
        MotorSoftCostProfile::Car => (
            SURFACE_UNKNOWN_EDGE_PENALTY,
            SURFACE_MARGINAL_EDGE_PENALTY,
            SURFACE_POOR_EDGE_PENALTY,
        ),
        MotorSoftCostProfile::Motorcycle => (
            SURFACE_UNKNOWN_MOTORCYCLE,
            SURFACE_MARGINAL_MOTORCYCLE,
            SURFACE_POOR_MOTORCYCLE,
        ),
        MotorSoftCostProfile::Truck => (
            SURFACE_UNKNOWN_TRUCK,
            SURFACE_MARGINAL_TRUCK,
            SURFACE_POOR_TRUCK,
        ),
        MotorSoftCostProfile::MobileHome => (
            SURFACE_UNKNOWN_MOBILE_HOME,
            SURFACE_MARGINAL_MOBILE_HOME,
            SURFACE_POOR_MOBILE_HOME,
        ),
    };
    match quality {
        SurfaceQuality::Good => 1.0,
        SurfaceQuality::Unknown => unknown,
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

/// Prefer higher-class motor roads (trunk/primary) over tertiary/local shortcuts.
///
/// Length-only A* otherwise prefers short gravel/tertiary corridors when pack
/// surface classes are missing (all Good). Offroad mode disables this.
pub fn edge_highway_class_multiplier(edge: &GraphEdge, mode: SurfaceRoutingMode) -> f64 {
    if mode == SurfaceRoutingMode::Offroad || edge.is_ferry {
        return 1.0;
    }
    match edge.highway.as_deref() {
        Some("motorway")
        | Some("motorway_link")
        | Some("trunk")
        | Some("trunk_link")
        | Some("primary")
        | Some("primary_link") => HIGHWAY_CLASS_TRUNK_PRIMARY,
        Some("secondary") | Some("secondary_link") => HIGHWAY_CLASS_SECONDARY,
        Some("tertiary") | Some("tertiary_link") => HIGHWAY_CLASS_TERTIARY,
        Some("unclassified") | Some("residential") | Some("living_street") | Some("road") => {
            HIGHWAY_CLASS_LOCAL
        }
        Some("service") | Some("track") | Some("path") | Some("footway") | Some("cycleway") => {
            HIGHWAY_CLASS_SERVICE_TRACK
        }
        _ => 1.0,
    }
}

/// Extra soft cost for Marginal/Poor edges with a posted maxspeed, so slower
/// rough roads are not preferred over a longer Good asphalt detour that posts
/// above 50 km/h. Good surfaces are unchanged (factor 1.0).
pub fn edge_rough_surface_speed_factor(edge: &GraphEdge, mode: SurfaceRoutingMode) -> f64 {
    if mode == SurfaceRoutingMode::Offroad {
        return 1.0;
    }
    match edge.surface_quality {
        SurfaceQuality::Good => 1.0,
        SurfaceQuality::Unknown | SurfaceQuality::Marginal | SurfaceQuality::Poor => {
            let Some(ms) = edge
                .maxspeed_kmh
                .or(edge.maxspeed_practical_kmh)
                .or(edge.maxspeed_advisory_kmh)
                .filter(|v| *v > 0.0)
            else {
                return 1.0;
            };
            (MOTOR_ROUGH_SURFACE_SPEED_REF_KMH / ms)
                .clamp(1.0, MOTOR_ROUGH_SURFACE_SPEED_FACTOR_MAX)
        }
    }
}

/// Combined surface × highway-class × missing-maxspeed × rough-speed soft
/// multiplier (≥ 1.0).
pub fn edge_motor_soft_multiplier(
    edge: &GraphEdge,
    mode: SurfaceRoutingMode,
    cost_profile: MotorSoftCostProfile,
) -> f64 {
    edge_surface_multiplier(edge.surface_quality, mode, cost_profile)
        * edge_highway_class_multiplier(edge, mode)
        * edge_maxspeed_multiplier(edge, mode, cost_profile)
        * edge_rough_surface_speed_factor(edge, mode)
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

/// Apply surface + highway-class + missing-maxspeed soft-cost multipliers to
/// motor graph edges.
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
    // Sequential: nested `par_iter` under Android's shared Rayon pool (place-index
    // / convert) can park the plan thread indefinitely waiting for worker slots.
    graph.edges.iter_mut().for_each(|edge| {
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
    fn untagged_per_highway_class_table() {
        // motorway…secondary stay Good
        for hw in [
            "motorway",
            "motorway_link",
            "trunk",
            "trunk_link",
            "primary",
            "primary_link",
            "secondary",
            "secondary_link",
        ] {
            assert_eq!(
                classify_surface_tags(Some(hw), None, None),
                SurfaceQuality::Good,
                "{hw}"
            );
            assert_eq!(
                infer_surface_from_highway(Some(hw)),
                SurfaceQuality::Good,
                "{hw}"
            );
        }
        for hw in ["tertiary", "tertiary_link"] {
            assert_eq!(
                classify_surface_tags(Some(hw), None, None),
                SurfaceQuality::Marginal,
                "{hw}"
            );
        }
        for hw in [
            "unclassified",
            "residential",
            "living_street",
            "road",
            "service",
        ] {
            assert_eq!(
                classify_surface_tags(Some(hw), None, None),
                SurfaceQuality::Unknown,
                "{hw}"
            );
        }
        assert_eq!(
            classify_surface_tags(Some("track"), None, None),
            SurfaceQuality::Poor
        );
        // Explicit tags still win over highway defaults.
        assert_eq!(
            classify_surface_tags(Some("tertiary"), Some("asphalt"), None),
            SurfaceQuality::Good
        );
        assert_eq!(
            classify_surface_tags(Some("residential"), Some("gravel"), None),
            SurfaceQuality::Marginal
        );
    }

    #[test]
    fn wire_encoding_preserves_v8_ordinals_and_appends_unknown() {
        assert_eq!(SurfaceQuality::Good.as_u8(), 0);
        assert_eq!(SurfaceQuality::Marginal.as_u8(), 1);
        assert_eq!(SurfaceQuality::Poor.as_u8(), 2);
        assert_eq!(SurfaceQuality::Unknown.as_u8(), 3);
        assert_eq!(SurfaceQuality::from_u8(0), SurfaceQuality::Good);
        assert_eq!(SurfaceQuality::from_u8(1), SurfaceQuality::Marginal);
        assert_eq!(SurfaceQuality::from_u8(2), SurfaceQuality::Poor);
        assert_eq!(SurfaceQuality::from_u8(3), SurfaceQuality::Unknown);
        // Rank order for transitions (discriminant), independent of wire bytes.
        assert!(SurfaceQuality::Good < SurfaceQuality::Unknown);
        assert!(SurfaceQuality::Unknown < SurfaceQuality::Marginal);
        assert!(SurfaceQuality::Marginal < SurfaceQuality::Poor);
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
        // Adjacent step (Good→Unknown): no penalty.
        assert_eq!(
            surface_transition_cost_m(
                Some(SurfaceQuality::Good),
                SurfaceQuality::Unknown,
                SurfaceRoutingMode::Car
            ),
            0.0
        );
        // Two-step drop (Good→Marginal) with Unknown in the ladder: penalty.
        assert_eq!(
            surface_transition_cost_m(
                Some(SurfaceQuality::Good),
                SurfaceQuality::Marginal,
                SurfaceRoutingMode::Car
            ),
            SURFACE_TRANSITION_PENALTY_M
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
            is_tunnel: false,
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

    fn motor_edge(
        id: &str,
        source: i64,
        target: i64,
        length_m: f64,
        maxspeed_kmh: Option<f64>,
        surface: SurfaceQuality,
    ) -> GraphEdge {
        motor_edge_hw(
            id,
            source,
            target,
            length_m,
            maxspeed_kmh,
            surface,
            "tertiary",
        )
    }

    fn motor_edge_hw(
        id: &str,
        source: i64,
        target: i64,
        length_m: f64,
        maxspeed_kmh: Option<f64>,
        surface: SurfaceQuality,
        highway: &str,
    ) -> GraphEdge {
        GraphEdge {
            id: id.into(),
            source: NodeId(source),
            target: NodeId(target),
            length_m,
            base_weight: length_m,
            eco_weight: Some(length_m),
            start_lat: 60.0,
            start_lon: 10.0,
            end_lat: 60.01,
            end_lon: 10.01,
            shape: Vec::new(),
            highway: Some(highway.into()),
            maxspeed_kmh,
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

    /// Short gravel@60 vs longer asphalt@80: asphalt must win for all motor soft profiles.
    #[test]
    fn prefers_asphalt_maxspeed_over_50_to_shorter_gravel_60() {
        use geo_types::Coord;
        use std::collections::HashMap;

        // Topology: 1 --gravel 1000m@60--> 2
        //           1 --asphalt 900m@80--> 3 --asphalt 900m@80--> 2
        let mut nodes = HashMap::new();
        for (id, lat, lon) in [(1, 60.0, 10.0), (2, 60.01, 10.01), (3, 60.005, 10.02)] {
            nodes.insert(
                NodeId(id),
                osm4routing::Node {
                    id: NodeId(id),
                    coord: Coord { x: lon, y: lat },
                    uses: 0,
                },
            );
        }
        let edges = vec![
            motor_edge("g12", 1, 2, 1000.0, Some(60.0), SurfaceQuality::Marginal),
            motor_edge("g21", 2, 1, 1000.0, Some(60.0), SurfaceQuality::Marginal),
            motor_edge("a13", 1, 3, 900.0, Some(80.0), SurfaceQuality::Good),
            motor_edge("a31", 3, 1, 900.0, Some(80.0), SurfaceQuality::Good),
            motor_edge("a32", 3, 2, 900.0, Some(80.0), SurfaceQuality::Good),
            motor_edge("a23", 2, 3, 900.0, Some(80.0), SurfaceQuality::Good),
        ];

        for cost_profile in [
            MotorSoftCostProfile::Car,
            MotorSoftCostProfile::Motorcycle,
            MotorSoftCostProfile::Truck,
            MotorSoftCostProfile::MobileHome,
        ] {
            let mut graph =
                RouteGraph::from_parts(nodes.clone(), edges.clone(), RoutingProfile::Car);
            apply_surface_preference(&mut graph, SurfaceRoutingMode::Car, cost_profile);
            let (path, _, _) = graph
                .shortest_path(NodeId(1), NodeId(2), false)
                .unwrap_or_else(|| panic!("{cost_profile:?}: no path"));
            assert!(
                path.contains(&NodeId(3)),
                "{cost_profile:?}: expected asphalt via node 3, got {path:?}"
            );
            assert_eq!(
                path,
                vec![NodeId(1), NodeId(3), NodeId(2)],
                "{cost_profile:?}: path={path:?}"
            );
        }
    }

    #[test]
    fn rough_speed_factor_only_boosts_non_good_with_posted_maxspeed() {
        let mut gravel = motor_edge("g", 1, 2, 1000.0, Some(60.0), SurfaceQuality::Marginal);
        assert!(
            (edge_rough_surface_speed_factor(&gravel, SurfaceRoutingMode::Car) - 80.0 / 60.0).abs()
                < 1e-9
        );
        gravel.maxspeed_kmh = None;
        assert_eq!(
            edge_rough_surface_speed_factor(&gravel, SurfaceRoutingMode::Car),
            1.0
        );
        let asphalt = motor_edge("a", 1, 2, 1000.0, Some(80.0), SurfaceQuality::Good);
        assert_eq!(
            edge_rough_surface_speed_factor(&asphalt, SurfaceRoutingMode::Car),
            1.0
        );
        // Very low posted maxspeed is capped (not 80/20 = 4).
        let slow = motor_edge("s", 1, 2, 1000.0, Some(20.0), SurfaceQuality::Marginal);
        assert_eq!(
            edge_rough_surface_speed_factor(&slow, SurfaceRoutingMode::Car),
            MOTOR_ROUGH_SURFACE_SPEED_FACTOR_MAX
        );
    }

    /// Sole gravel corridor @40: penalty must not invent a non-existent asphalt path.
    #[test]
    fn low_maxspeed_gravel_still_used_when_no_asphalt_alternative() {
        use geo_types::Coord;
        use std::collections::HashMap;

        let mut nodes = HashMap::new();
        for (id, lat, lon) in [(1, 60.0, 10.0), (2, 60.01, 10.01)] {
            nodes.insert(
                NodeId(id),
                osm4routing::Node {
                    id: NodeId(id),
                    coord: Coord { x: lon, y: lat },
                    uses: 0,
                },
            );
        }
        let edges = vec![
            motor_edge("g12", 1, 2, 1000.0, Some(40.0), SurfaceQuality::Marginal),
            motor_edge("g21", 2, 1, 1000.0, Some(40.0), SurfaceQuality::Marginal),
        ];
        for cost_profile in [
            MotorSoftCostProfile::Car,
            MotorSoftCostProfile::Motorcycle,
            MotorSoftCostProfile::Truck,
            MotorSoftCostProfile::MobileHome,
        ] {
            let mut graph =
                RouteGraph::from_parts(nodes.clone(), edges.clone(), RoutingProfile::Car);
            apply_surface_preference(&mut graph, SurfaceRoutingMode::Car, cost_profile);
            let (path, _, _) = graph
                .shortest_path(NodeId(1), NodeId(2), false)
                .unwrap_or_else(|| panic!("{cost_profile:?}: gravel-only must remain routable"));
            assert_eq!(path, vec![NodeId(1), NodeId(2)], "{cost_profile:?}");
        }
    }

    /// Absurd asphalt detour must not beat short gravel@40 even after the speed factor.
    #[test]
    fn gravel_40_beats_absurdly_long_asphalt_detour() {
        use geo_types::Coord;
        use std::collections::HashMap;

        // 1 --gravel 1km@40--> 2
        // 1 --asphalt 50km@80--> 3 --asphalt 50km@80--> 2
        let mut nodes = HashMap::new();
        for (id, lat, lon) in [(1, 60.0, 10.0), (2, 60.01, 10.01), (3, 60.5, 11.0)] {
            nodes.insert(
                NodeId(id),
                osm4routing::Node {
                    id: NodeId(id),
                    coord: Coord { x: lon, y: lat },
                    uses: 0,
                },
            );
        }
        let edges = vec![
            motor_edge("g12", 1, 2, 1_000.0, Some(40.0), SurfaceQuality::Marginal),
            motor_edge("g21", 2, 1, 1_000.0, Some(40.0), SurfaceQuality::Marginal),
            motor_edge("a13", 1, 3, 50_000.0, Some(80.0), SurfaceQuality::Good),
            motor_edge("a31", 3, 1, 50_000.0, Some(80.0), SurfaceQuality::Good),
            motor_edge("a32", 3, 2, 50_000.0, Some(80.0), SurfaceQuality::Good),
            motor_edge("a23", 2, 3, 50_000.0, Some(80.0), SurfaceQuality::Good),
        ];
        for cost_profile in [
            MotorSoftCostProfile::Car,
            MotorSoftCostProfile::Motorcycle,
            MotorSoftCostProfile::Truck,
            MotorSoftCostProfile::MobileHome,
        ] {
            let mut graph =
                RouteGraph::from_parts(nodes.clone(), edges.clone(), RoutingProfile::Car);
            apply_surface_preference(&mut graph, SurfaceRoutingMode::Car, cost_profile);
            let (path, _, _) = graph
                .shortest_path(NodeId(1), NodeId(2), false)
                .unwrap_or_else(|| panic!("{cost_profile:?}: no path"));
            assert_eq!(
                path,
                vec![NodeId(1), NodeId(2)],
                "{cost_profile:?}: must keep short gravel, not 100km asphalt; got {path:?}"
            );
        }
    }

    /// All-Good pack regression: shorter tertiary must lose to longer trunk.
    #[test]
    fn all_good_prefers_trunk_over_shorter_tertiary() {
        use geo_types::Coord;
        use std::collections::HashMap;

        // 1 --tertiary Good 1000m--> 2
        // 1 --trunk Good 530m--> 3 --trunk Good 530m--> 2 (~6% longer, like Elverum vs shortcut)
        let mut nodes = HashMap::new();
        for (id, lat, lon) in [(1, 60.0, 10.0), (2, 60.01, 10.01), (3, 60.005, 10.02)] {
            nodes.insert(
                NodeId(id),
                osm4routing::Node {
                    id: NodeId(id),
                    coord: Coord { x: lon, y: lat },
                    uses: 0,
                },
            );
        }
        let edges = vec![
            motor_edge_hw(
                "t12",
                1,
                2,
                1000.0,
                Some(60.0),
                SurfaceQuality::Good,
                "tertiary",
            ),
            motor_edge_hw(
                "t21",
                2,
                1,
                1000.0,
                Some(60.0),
                SurfaceQuality::Good,
                "tertiary",
            ),
            motor_edge_hw(
                "a13",
                1,
                3,
                530.0,
                Some(80.0),
                SurfaceQuality::Good,
                "trunk",
            ),
            motor_edge_hw(
                "a31",
                3,
                1,
                530.0,
                Some(80.0),
                SurfaceQuality::Good,
                "trunk",
            ),
            motor_edge_hw(
                "a32",
                3,
                2,
                530.0,
                Some(80.0),
                SurfaceQuality::Good,
                "trunk",
            ),
            motor_edge_hw(
                "a23",
                2,
                3,
                530.0,
                Some(80.0),
                SurfaceQuality::Good,
                "trunk",
            ),
        ];
        let mut graph = RouteGraph::from_parts(nodes, edges, RoutingProfile::Car);
        apply_surface_preference(
            &mut graph,
            SurfaceRoutingMode::Car,
            MotorSoftCostProfile::Car,
        );
        let (path, _, _) = graph
            .shortest_path(NodeId(1), NodeId(2), false)
            .expect("path");
        assert_eq!(
            path,
            vec![NodeId(1), NodeId(3), NodeId(2)],
            "highway-class soft cost must prefer trunk detour; got {path:?}"
        );
    }

    /// Untagged tertiary (Marginal) vs tagged asphalt tertiary of similar length:
    /// asphalt wins when lengths are within ~20% (here equal length).
    #[test]
    fn untagged_tertiary_loses_to_asphalt_tertiary_when_lengths_close() {
        use geo_types::Coord;
        use std::collections::HashMap;

        // 1 --untagged tertiary 1000m--> 2
        // 1 --asphalt tertiary 1000m--> 3 --asphalt tertiary 50m--> 2  (~5% longer total)
        // Equal primary leg + short connector: asphalt path ≈ 1050m vs 1000m untagged.
        let mut nodes = HashMap::new();
        for (id, lat, lon) in [(1, 60.0, 10.0), (2, 60.01, 10.01), (3, 60.005, 10.005)] {
            nodes.insert(
                NodeId(id),
                osm4routing::Node {
                    id: NodeId(id),
                    coord: Coord { x: lon, y: lat },
                    uses: 0,
                },
            );
        }
        let edges = vec![
            motor_edge_hw(
                "u12",
                1,
                2,
                1000.0,
                Some(60.0),
                SurfaceQuality::Marginal, // untagged tertiary default
                "tertiary",
            ),
            motor_edge_hw(
                "u21",
                2,
                1,
                1000.0,
                Some(60.0),
                SurfaceQuality::Marginal,
                "tertiary",
            ),
            motor_edge_hw(
                "a13",
                1,
                3,
                1000.0,
                Some(60.0),
                SurfaceQuality::Good,
                "tertiary",
            ),
            motor_edge_hw(
                "a31",
                3,
                1,
                1000.0,
                Some(60.0),
                SurfaceQuality::Good,
                "tertiary",
            ),
            motor_edge_hw(
                "a32",
                3,
                2,
                50.0,
                Some(60.0),
                SurfaceQuality::Good,
                "tertiary",
            ),
            motor_edge_hw(
                "a23",
                2,
                3,
                50.0,
                Some(60.0),
                SurfaceQuality::Good,
                "tertiary",
            ),
        ];
        let mut graph = RouteGraph::from_parts(nodes, edges, RoutingProfile::Car);
        apply_surface_preference(
            &mut graph,
            SurfaceRoutingMode::Car,
            MotorSoftCostProfile::Car,
        );
        let (path, _, _) = graph
            .shortest_path(NodeId(1), NodeId(2), false)
            .expect("path");
        assert_eq!(
            path,
            vec![NodeId(1), NodeId(3), NodeId(2)],
            "asphalt tertiary (~1050m × 1.0) must beat untagged tertiary (1000m × 2.2); got {path:?}"
        );
    }

    /// Sole untagged residential corridor remains routable; cost reflects Unknown × local highway.
    #[test]
    fn untagged_residential_still_routable_when_no_alternative() {
        use geo_types::Coord;
        use std::collections::HashMap;

        let mut nodes = HashMap::new();
        for (id, lat, lon) in [(1, 60.0, 10.0), (2, 60.01, 10.01)] {
            nodes.insert(
                NodeId(id),
                osm4routing::Node {
                    id: NodeId(id),
                    coord: Coord { x: lon, y: lat },
                    uses: 0,
                },
            );
        }
        let length_m = 1000.0;
        let edges = vec![
            motor_edge_hw(
                "r12",
                1,
                2,
                length_m,
                Some(80.0), // posted @ ref speed → rough-speed factor 1.0
                SurfaceQuality::Unknown,
                "residential",
            ),
            motor_edge_hw(
                "r21",
                2,
                1,
                length_m,
                Some(80.0),
                SurfaceQuality::Unknown,
                "residential",
            ),
        ];
        let mut graph = RouteGraph::from_parts(nodes, edges, RoutingProfile::Car);
        apply_surface_preference(
            &mut graph,
            SurfaceRoutingMode::Car,
            MotorSoftCostProfile::Car,
        );
        let (path, _, cost) = graph
            .shortest_path(NodeId(1), NodeId(2), false)
            .expect("residential-only must remain routable");
        assert_eq!(path, vec![NodeId(1), NodeId(2)]);
        let expected = length_m * SURFACE_UNKNOWN_EDGE_PENALTY * HIGHWAY_CLASS_LOCAL;
        assert!(
            (cost - expected).abs() < 1e-6,
            "cost must be length × Unknown(1.4) × local(1.8) = {expected}, got {cost}"
        );
    }

    /// Pre-fix v8 pack edge baked as Good (untagged inferred Good) still costs as Good.
    #[test]
    fn legacy_v8_baked_good_degrades_gracefully() {
        use geo_types::Coord;
        use std::collections::HashMap;

        let mut nodes = HashMap::new();
        for (id, lat, lon) in [(1, 60.0, 10.0), (2, 60.01, 10.01)] {
            nodes.insert(
                NodeId(id),
                osm4routing::Node {
                    id: NodeId(id),
                    coord: Coord { x: lon, y: lat },
                    uses: 0,
                },
            );
        }
        let length_m = 500.0;
        // Simulate old pack: untagged tertiary stored as Good (pre-Option-C).
        let edges = vec![
            motor_edge_hw(
                "t12",
                1,
                2,
                length_m,
                Some(60.0),
                SurfaceQuality::Good,
                "tertiary",
            ),
            motor_edge_hw(
                "t21",
                2,
                1,
                length_m,
                Some(60.0),
                SurfaceQuality::Good,
                "tertiary",
            ),
        ];
        let mut graph = RouteGraph::from_parts(nodes, edges, RoutingProfile::Car);
        apply_surface_preference(
            &mut graph,
            SurfaceRoutingMode::Car,
            MotorSoftCostProfile::Car,
        );
        let (path, _, cost) = graph
            .shortest_path(NodeId(1), NodeId(2), false)
            .expect("legacy Good edge must not crash soft costs");
        assert_eq!(path, vec![NodeId(1), NodeId(2)]);
        let expected = length_m * HIGHWAY_CLASS_TERTIARY; // surface mult 1.0
        assert!(
            (cost - expected).abs() < 1e-6,
            "legacy baked Good stays Good (× tertiary class only); expected {expected}, got {cost}"
        );
    }
}
