//! DATEX situation → routing impact classification and planner-facing constraints.
//!
//! Classification lives here (not in the graph builder). Only **active** situations
//! should be passed to [`planner_impacts`]; inactive / upcoming entries must stay
//! out of [`crate::routing::graph::RouteOptions`].
//!
//! NPRA's live GetSituation feed has **no** convoy/escort concept (no
//! `AuthorityOperation` convoy payload, no `WinterDrivingManagement`). This
//! module does not implement convoy handling; that would need a different source.

use super::parse::DatexSituation;

/// Routing impact derived from DATEX fields (see `docs/plugins/datex-plugin.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DatexImpact {
    /// No cost change; overlay-only.
    #[default]
    Ignore,
    /// Extra minutes on matching edges (edge stays searchable).
    Penalize,
    /// Listed to the user; no A* cost and no exclusion.
    Warn,
    /// Hard-exclude nearby edges from A* (like [`crate::routing::toll::TollPolicy::NeverUse`]).
    Block,
}

impl DatexImpact {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ignore => "ignore",
            Self::Penalize => "penalty",
            Self::Warn => "warning",
            Self::Block => "block",
        }
    }
}

/// Radius (metres) around a situation display point used to match nearby edges.
pub const DATEX_IMPACT_RADIUS_M: f64 = 250.0;

/// Finite A* cost multiplier for [`DatexImpact::Penalize`] when no numeric delay
/// (or other scale field) is present. Mirrors toll penalize.
pub const DATEX_PENALIZE_MULT: f64 = 50.0;

/// Upper clamp for delay/wind-scaled Penalize multipliers.
pub const DATEX_PENALIZE_MULT_MAX: f64 = 150.0;

/// Floor for delay/wind-scaled Penalize multipliers (a 30-second delay stays cheap).
pub const DATEX_PENALIZE_MULT_MIN: f64 = 5.0;

/// Norwegian (and a few English) phrases that indicate a **full** road closure.
///
/// Narrow keyword check — not NLP. Do **not** add bare `stengt`: it over-matches
/// partial lane text (`ett stengt kjørefelt`, `Et felt stengt`). Extend when
/// live comments surface new full-closure variants. Matching is case-insensitive
/// substring on `comment` and `locationDescription`, after
/// [`CLOSURE_RISK_EXCLUSIONS`].
pub const CLOSURE_PHRASES: &[&str] = &[
    "vegen er stengt",
    "veien er stengt",
    "vegen stengt",
    "veien stengt",
    "helt stengt",
    "helstengt",
    "sperret",
    "road closed",
    "carriageway closed",
    "fully closed",
];

/// Risk / conditional phrasing that must **not** trigger Block even when a
/// closure-adjacent word appears. Fall through to the type's non-closure rule
/// (e.g. wind-scaled Penalize for `PoorEnvironmentConditions`).
pub const CLOSURE_RISK_EXCLUSIONS: &[&str] = &[
    "fare for stengt",
    "kan bli stengt",
    "kunne bli stengt",
    "kan bli helt stengt",
    "kunne bli helt stengt",
];

/// DATEX `xsi:type` local names observed in NPRA's live GetSituation feed
/// (17 types) plus schema-valid types that have never appeared there.
///
/// Snapshot counts are relative frequency only — not a per-poll guarantee.
pub const NPRA_LIVE_XSI_TYPES: &[&str] = &[
    "RoadOrCarriagewayOrLaneManagement",
    "MaintenanceWorks",
    "GeneralNetworkManagement",
    "SpeedManagement",
    "ReroutingManagement",
    "ConstructionWorks",
    "EnvironmentalObstruction",
    "PublicEvent",
    "TransitInformation",
    "InfrastructureDamageObstruction",
    "NonWeatherRelatedRoadConditions",
    "AnimalPresenceObstruction",
    "GeneralObstruction",
    "Accident",
    "VehicleObstruction",
    "PoorEnvironmentConditions",
    "RoadsideAssistance",
];

/// Schema-valid types that NPRA's feed has not populated. Classified
/// defensively (no panic) via the generic lane/severity/closure heuristic.
pub const SCHEMA_VALID_UNUSED_XSI_TYPES: &[&str] = &[
    "AbnormalTraffic",
    "WeatherRelatedRoadConditions",
    "AuthorityOperation",
];

/// Parsed fields used by [`classify_impact`].
#[derive(Debug, Clone, Copy)]
pub struct DatexClassifyFields<'a> {
    pub xsi_type: &'a str,
    pub lanes_restricted: Option<u32>,
    pub severity: Option<&'a str>,
    pub comment: Option<&'a str>,
    pub location_description: Option<&'a str>,
    /// DATEX `impact/delays/delayTimeValue` in seconds, when present.
    pub delay_time_secs: Option<f64>,
    /// True when an `impact/delays` element exists (even without a numeric value).
    pub delays_present: bool,
    /// DATEX `windSpeed` (m/s) when present — used by `PoorEnvironmentConditions`.
    pub wind_speed: Option<f64>,
    /// `roadOrCarriagewayOrLaneManagementType` local name, when present.
    pub management_type: Option<&'a str>,
}

/// Result of [`classify_impact`]: bucket plus the Penalize multiplier to apply.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DatexClassification {
    pub impact: DatexImpact,
    /// Used by the planner only when [`DatexImpact::Penalize`] (legacy multiplier).
    pub penalize_mult: f64,
    /// Extra minutes for [`DatexImpact::Penalize`].
    pub penalty_minutes: f64,
    /// True when `xsi:type` is not in the known inventory (Ignore + logged warning).
    pub unrecognized_xsi_type: bool,
}

/// One planner-facing DATEX constraint (geometry + impact + match radius).
///
/// Built only from active situations via [`planner_impacts`].
#[derive(Debug, Clone, PartialEq)]
pub struct DatexPlannerConstraint {
    pub lat: f64,
    pub lon: f64,
    pub impact: DatexImpact,
    pub radius_m: f64,
    /// Source situation id (diagnostics / tests).
    pub situation_id: String,
    /// Cost multiplier when [`DatexImpact::Penalize`]. Ignored for Block.
    pub penalize_mult: f64,
    /// Extra minutes when [`DatexImpact::Penalize`].
    pub penalty_minutes: f64,
    pub road: String,
    pub place: String,
    pub validity_text: String,
    pub xsi_type: String,
}

/// Convert **active-only** situations into planner constraints.
///
/// [`DatexImpact::Ignore`] entries are dropped (no graph effect). Callers must
/// pass situations that are already active at plan time — this function does
/// **not** re-check validity windows.
pub fn planner_impacts(active_only: &[DatexSituation]) -> Vec<DatexPlannerConstraint> {
    let mut out = Vec::new();
    for s in active_only {
        if s.impact == DatexImpact::Ignore {
            continue;
        }
        if s.primary_lat_lon().is_none() {
            continue;
        }
        out.push(constraint_from_situation(s));
    }
    out
}

/// Minutes added to travel time when a record has no numeric delay.
pub fn default_penalty_minutes(xsi_type: &str, management_type: Option<&str>) -> f64 {
    match management_type.map(|s| s.trim()) {
        Some("narrowLanes") => 3.0,
        Some("intermittentShortTermClosures") => 5.0,
        Some("convoyDriving") => 8.0,
        Some("laneClosures") | Some("lanesDeviated") => 4.0,
        _ => match xsi_type {
            "Accident" => 15.0,
            "EnvironmentalObstruction" => 10.0,
            "InfrastructureDamageObstruction" => 10.0,
            "MaintenanceWorks" | "ConstructionWorks" | "Roadworks" => 5.0,
            "RoadOrCarriagewayOrLaneManagement" | "GeneralNetworkManagement" => 3.0,
            "PublicEvent" => 8.0,
            "PoorEnvironmentConditions" | "WeatherRelatedRoadConditions" => 5.0,
            "ReroutingManagement" => 5.0,
            "AbnormalTraffic"
            | "VehicleObstruction"
            | "AnimalPresenceObstruction"
            | "GeneralObstruction"
            | "NonWeatherRelatedRoadConditions"
            | "AuthorityOperation" => 5.0,
            _ => 5.0,
        },
    }
}

/// Structured DATEX management values that mean the carriageway is fully closed.
pub fn is_full_closure_management(management_type: Option<&str>) -> bool {
    matches!(
        management_type.map(|s| s.trim()),
        Some("closed") | Some("carriagewayClosures")
    )
}

/// Works / lane / short-stop / convoy / lights — never a hard Block.
pub fn is_never_block_management(management_type: Option<&str>) -> bool {
    matches!(
        management_type.map(|s| s.trim()),
        Some("narrowLanes")
            | Some("intermittentShortTermClosures")
            | Some("convoyDriving")
            | Some("laneClosures")
            | Some("lanesDeviated")
            | Some("hardShoulderRunning")
            | Some("carPoolLaneInOperation")
            | Some("turnRestrictions")
            | Some("useOfSpecifiedLane")
            | Some("rollingRoadBlock")
            | Some("tidalFlowOperation")
            | Some("oppositeCarriagewayUsage")
            | Some("contraflow")
            | Some("contraflowReversible")
    )
}

pub fn constraint_from_situation(s: &super::parse::DatexSituation) -> DatexPlannerConstraint {
    let (lat, lon) = s.primary_lat_lon().unwrap_or((0.0, 0.0));
    DatexPlannerConstraint {
        lat,
        lon,
        impact: s.impact,
        radius_m: DATEX_IMPACT_RADIUS_M,
        situation_id: s.id.clone(),
        penalize_mult: s.penalize_mult.max(1.0),
        penalty_minutes: s.penalty_minutes,
        road: s.road_number.clone().unwrap_or_default(),
        place: s.location_description.clone().unwrap_or_default(),
        validity_text: s.validity_text(),
        xsi_type: s.xsi_type.clone(),
    }
}

fn ignore() -> DatexClassification {
    DatexClassification {
        impact: DatexImpact::Ignore,
        penalize_mult: DATEX_PENALIZE_MULT,
        penalty_minutes: 0.0,
        unrecognized_xsi_type: false,
    }
}

#[allow(dead_code)]
fn warn_class(minutes: f64) -> DatexClassification {
    DatexClassification {
        impact: DatexImpact::Warn,
        penalize_mult: DATEX_PENALIZE_MULT,
        penalty_minutes: minutes.max(0.0),
        unrecognized_xsi_type: false,
    }
}

fn penalize(mult: f64, minutes: f64) -> DatexClassification {
    DatexClassification {
        impact: DatexImpact::Penalize,
        penalize_mult: mult.clamp(DATEX_PENALIZE_MULT_MIN, DATEX_PENALIZE_MULT_MAX),
        penalty_minutes: minutes.max(0.0),
        unrecognized_xsi_type: false,
    }
}

fn block() -> DatexClassification {
    DatexClassification {
        impact: DatexImpact::Block,
        penalize_mult: DATEX_PENALIZE_MULT,
        penalty_minutes: 0.0,
        unrecognized_xsi_type: false,
    }
}

/// `true` when this type must stay [`DatexImpact::Ignore`] regardless of
/// severity, lanes, delay, or free-text. Speed limits and ferry timetables
/// must not reroute.
pub fn is_structurally_ignore(xsi_type: &str) -> bool {
    matches!(
        xsi_type,
        "SpeedManagement" | "TransitInformation" | "RoadsideAssistance"
    )
}

/// Known inventory: NPRA live types + schema-valid unused types we classify
/// defensively. Future/typo types return false.
pub fn is_known_situation_xsi_type(xsi_type: &str) -> bool {
    NPRA_LIVE_XSI_TYPES.contains(&xsi_type)
        || SCHEMA_VALID_UNUSED_XSI_TYPES.contains(&xsi_type)
        || xsi_type == "Roadworks"
}

/// Map DATEX `delayTimeValue` (seconds) to a Penalize cost multiplier.
///
/// Graduated so a 30-second delay is much cheaper than a 30-minute one.
/// 30 minutes lands at [`DATEX_PENALIZE_MULT`]; longer delays continue up to
/// [`DATEX_PENALIZE_MULT_MAX`].
pub fn delay_penalize_mult(delay_secs: f64) -> f64 {
    let minutes = delay_secs.max(0.0) / 60.0;
    (DATEX_PENALIZE_MULT_MIN + (DATEX_PENALIZE_MULT - DATEX_PENALIZE_MULT_MIN) * (minutes / 30.0))
        .clamp(DATEX_PENALIZE_MULT_MIN, DATEX_PENALIZE_MULT_MAX)
}

/// Scale Penalize by DATEX `windSpeed` (m/s). 20 m/s lands at
/// [`DATEX_PENALIZE_MULT`].
pub fn wind_penalize_mult(wind_mps: f64) -> f64 {
    let t = wind_mps.max(0.0) / 20.0;
    (DATEX_PENALIZE_MULT_MIN + (DATEX_PENALIZE_MULT - DATEX_PENALIZE_MULT_MIN) * t)
        .clamp(DATEX_PENALIZE_MULT_MIN, DATEX_PENALIZE_MULT_MAX)
}

/// Classify impact from `xsi:type` plus parsed DATEX fields.
///
/// See `docs/plugins/datex-plugin.md` for the type → bucket table.
pub fn classify_impact(fields: &DatexClassifyFields<'_>) -> DatexClassification {
    let xsi = fields.xsi_type;

    if is_structurally_ignore(xsi) {
        return ignore();
    }

    if !is_known_situation_xsi_type(xsi) {
        log::warn!(
            target: "NaviDatex",
            "unrecognized DATEX xsi:type={xsi}; classifying Ignore"
        );
        return DatexClassification {
            impact: DatexImpact::Ignore,
            penalize_mult: DATEX_PENALIZE_MULT,
            penalty_minutes: 0.0,
            unrecognized_xsi_type: true,
        };
    }

    let minutes = fields
        .delay_time_secs
        .map(|s| s.max(0.0) / 60.0)
        .unwrap_or_else(|| default_penalty_minutes(xsi, fields.management_type));
    let delay_mult = fields
        .delay_time_secs
        .map(delay_penalize_mult)
        .unwrap_or(DATEX_PENALIZE_MULT);

    // Only a structured full closure blocks. Free-text closure phrases are not enough.
    if is_full_closure_management(fields.management_type)
        && !is_never_block_management(fields.management_type)
        && !matches!(
            xsi,
            "MaintenanceWorks" | "ConstructionWorks" | "Roadworks" | "SpeedManagement"
        )
    {
        return block();
    }

    match xsi {
        "RoadOrCarriagewayOrLaneManagement" => penalize(delay_mult, minutes),
        "MaintenanceWorks" | "ConstructionWorks" | "Roadworks" => penalize(delay_mult, minutes),
        "GeneralNetworkManagement" => penalize(delay_mult, minutes),
        "ReroutingManagement" => penalize(delay_mult, minutes),
        "EnvironmentalObstruction" | "Accident" => penalize(delay_mult, minutes),
        "InfrastructureDamageObstruction"
        | "PublicEvent"
        | "NonWeatherRelatedRoadConditions"
        | "AnimalPresenceObstruction"
        | "VehicleObstruction"
        | "GeneralObstruction" => penalize(delay_mult, minutes),
        "PoorEnvironmentConditions" => {
            let m = fields
                .wind_speed
                .map(wind_penalize_mult)
                .unwrap_or(delay_mult);
            penalize(m, minutes)
        }
        "AbnormalTraffic" | "WeatherRelatedRoadConditions" | "AuthorityOperation" => {
            penalize(delay_mult, minutes)
        }
        _ => {
            log::warn!(
                target: "NaviDatex",
                "DATEX xsi:type={xsi} on known list but unmatched; classifying Ignore"
            );
            ignore()
        }
    }
}

#[allow(dead_code)]
fn lanes_positive(fields: &DatexClassifyFields<'_>) -> bool {
    fields.lanes_restricted.is_some_and(|n| n >= 1)
}

/// Case-insensitive substring match against [`CLOSURE_PHRASES`], plus
/// `closed` combined with `road` / `vegen` / `vei`.
///
/// Returns `false` when [`CLOSURE_RISK_EXCLUSIONS`] match (risk/conditional
/// language), even if a closure phrase would otherwise hit.
pub fn text_indicates_closure(text: Option<&str>) -> bool {
    let Some(raw) = text else {
        return false;
    };
    let t = raw.to_ascii_lowercase();
    if CLOSURE_RISK_EXCLUSIONS.iter().any(|p| t.contains(p)) {
        return false;
    }
    if CLOSURE_PHRASES.iter().any(|p| t.contains(p)) {
        return true;
    }
    t.contains("closed") && (t.contains("road") || t.contains("vegen") || t.contains("vei"))
}

#[allow(dead_code)]
fn severity_is_elevated(severity: Option<&str>) -> bool {
    matches!(
        severity.map(|s| s.trim().to_ascii_lowercase()).as_deref(),
        Some("medium") | Some("high") | Some("highest")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datex::parse::SituationKind;

    fn sit(id: &str, impact: DatexImpact) -> DatexSituation {
        DatexSituation {
            id: id.into(),
            kind: SituationKind::Roadworks,
            xsi_type: "MaintenanceWorks".into(),
            geometry: vec![(60.0, 10.0)],
            valid_from: None,
            valid_to: None,
            road_number: None,
            location_description: None,
            comment: None,
            severity: Some("none".into()),
            lanes_restricted: Some(0),
            delay_time_secs: None,
            delays_present: false,
            wind_speed: None,
            impact,
            penalize_mult: DATEX_PENALIZE_MULT,
            penalty_minutes: 0.0,
            unrecognized_xsi_type: false,
            management_type: None,
            vehicle_types: Vec::new(),
            direction: None,
            valid_periods: Vec::new(),
        }
    }

    fn fields(xsi: &str) -> DatexClassifyFields<'_> {
        DatexClassifyFields {
            xsi_type: xsi,
            lanes_restricted: None,
            severity: None,
            comment: None,
            location_description: None,
            delay_time_secs: None,
            delays_present: false,
            wind_speed: None,
            management_type: None,
        }
    }

    #[test]
    fn stengt_comment_does_not_block_construction() {
        let mut f = fields("ConstructionWorks");
        f.comment = Some("Vegen er stengt.");
        assert_eq!(classify_impact(&f).impact, DatexImpact::Penalize);
    }

    #[test]
    fn partial_lane_stengt_text_does_not_block_from_phrase_alone() {
        assert!(!text_indicates_closure(Some(
            "Vegarbeid, ett stengt kjørefelt."
        )));
        assert!(!text_indicates_closure(Some("Et felt stengt.")));
        let mut f = fields("MaintenanceWorks");
        f.lanes_restricted = Some(1);
        f.severity = Some("low");
        f.comment = Some("Vegarbeid, ett stengt kjørefelt.|Lysregulering.");
        assert_eq!(classify_impact(&f).impact, DatexImpact::Penalize);
        let mut f2 = fields("GeneralNetworkManagement");
        f2.lanes_restricted = Some(0);
        f2.severity = Some("low");
        f2.comment = Some("Et felt stengt.");
        // No lanes>0 and no strong closure → still a network-management warning/penalty.
        assert_eq!(classify_impact(&f2).impact, DatexImpact::Penalize);
    }

    #[test]
    fn risk_phrasing_fare_for_stengt_does_not_block() {
        assert!(!text_indicates_closure(Some(
            "Sterk vind, fare for stengt veg."
        )));
        assert!(!text_indicates_closure(Some(
            "Veien kan bli stengt i kortere perioder"
        )));
        assert!(!text_indicates_closure(Some(
            "veien kunne bli stengt i korte perioder"
        )));
        let mut f = fields("PoorEnvironmentConditions");
        f.severity = Some("none");
        f.lanes_restricted = Some(0);
        f.comment = Some("Sterk vind, fare for stengt veg.");
        f.wind_speed = Some(17.3);
        let c = classify_impact(&f);
        assert_eq!(c.impact, DatexImpact::Penalize);
        assert!((c.penalize_mult - wind_penalize_mult(17.3)).abs() < 1e-9);
    }

    #[test]
    fn infrastructure_damage_passable_is_penalize_not_block() {
        let mut pass = fields("InfrastructureDamageObstruction");
        pass.lanes_restricted = Some(0);
        pass.severity = Some("none");
        pass.comment =
            Some("Personbiler og andre kjøretøy opptil opp til 32 tonns totalvekt kan passere.");
        assert_eq!(classify_impact(&pass).impact, DatexImpact::Penalize);

        let mut lights = fields("InfrastructureDamageObstruction");
        lights.lanes_restricted = Some(0);
        lights.severity = Some("none");
        lights.comment = Some("Skade på vegnett.|Lysregulering, fartsgrense 30 km/t.");
        assert_eq!(classify_impact(&lights).impact, DatexImpact::Penalize);

        let mut closed = fields("InfrastructureDamageObstruction");
        closed.lanes_restricted = Some(0);
        closed.comment = Some("Skade på vegnett, vegen er stengt.");
        assert_eq!(classify_impact(&closed).impact, DatexImpact::Penalize);

        let mut two_lanes = fields("InfrastructureDamageObstruction");
        two_lanes.lanes_restricted = Some(2);
        two_lanes.comment = Some("Skade på vegnett.|Midlertidig omkjøring via Eidsfoss");
        assert_eq!(classify_impact(&two_lanes).impact, DatexImpact::Penalize);
    }

    #[test]
    fn stengt_i_periode_is_not_a_full_closure_phrase() {
        assert!(!text_indicates_closure(Some(
            "Vegarbeid.|Stengt i periode på 0,5 timer."
        )));
        assert!(text_indicates_closure(Some(
            "Veien er helt stengt mellom kl 12"
        )));
        assert!(text_indicates_closure(Some("Helstengt fra 10:00-13:00")));
    }

    #[test]
    fn maintenance_stengt_text_is_penalty_not_block() {
        let mut f = fields("MaintenanceWorks");
        f.lanes_restricted = Some(2);
        f.severity = Some("low");
        f.comment = Some("Vegarbeid, vegen er stengt.|Omkjøring er skiltet.");
        assert_eq!(classify_impact(&f).impact, DatexImpact::Penalize);
    }

    #[test]
    fn maintenance_two_lanes_without_closure_text_is_penalize() {
        let mut f = fields("MaintenanceWorks");
        f.lanes_restricted = Some(2);
        f.severity = Some("low");
        f.comment = Some("Vegarbeid.");
        assert_eq!(classify_impact(&f).impact, DatexImpact::Penalize);
    }

    #[test]
    fn one_lane_maintenance_is_penalize() {
        let mut f = fields("MaintenanceWorks");
        f.lanes_restricted = Some(1);
        f.severity = Some("low");
        assert_eq!(classify_impact(&f).impact, DatexImpact::Penalize);
    }

    #[test]
    fn none_severity_zero_lanes_maintenance_is_penalty() {
        let mut f = fields("MaintenanceWorks");
        f.lanes_restricted = Some(0);
        f.severity = Some("none");
        f.comment = Some("Fartsgrense 50 km/t");
        assert_eq!(classify_impact(&f).impact, DatexImpact::Penalize);
    }

    #[test]
    fn delay_scale_thirty_minutes_exceeds_thirty_seconds() {
        let short = delay_penalize_mult(30.0);
        let long = delay_penalize_mult(1800.0);
        assert!(
            long > short,
            "30 min ({long}) must penalize more than 30 s ({short})"
        );
        assert!((long - DATEX_PENALIZE_MULT).abs() < 1e-9);
        assert!(short < 10.0);
    }

    #[test]
    fn planner_impacts_drops_ignore_and_keeps_block() {
        let ignore_s = sit("a", DatexImpact::Ignore);
        let block = DatexSituation {
            id: "b".into(),
            impact: DatexImpact::Block,
            comment: Some("stengt".into()),
            ..ignore_s.clone()
        };
        let got = planner_impacts(&[ignore_s, block]);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].situation_id, "b");
        assert_eq!(got[0].impact, DatexImpact::Block);
    }

    #[test]
    fn planner_impacts_copies_delay_scaled_multiplier() {
        let s = DatexSituation {
            impact: DatexImpact::Penalize,
            penalize_mult: delay_penalize_mult(30.0),
            xsi_type: "RoadOrCarriagewayOrLaneManagement".into(),
            ..sit("d", DatexImpact::Penalize)
        };
        let got = planner_impacts(&[s]);
        assert_eq!(got.len(), 1);
        assert!((got[0].penalize_mult - delay_penalize_mult(30.0)).abs() < 1e-9);
    }
}
