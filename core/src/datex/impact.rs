//! DATEX situation → routing impact classification and planner-facing constraints.
//!
//! Classification lives here (not in the graph builder). Only **active** situations
//! should be passed to [`planner_impacts`]; inactive / upcoming entries must stay
//! out of [`crate::routing::graph::RouteOptions`].

use super::parse::DatexSituation;

/// Routing impact derived from DATEX fields (see `docs/plugins/datex-plugin.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DatexImpact {
    /// No cost change; overlay-only.
    #[default]
    Ignore,
    /// Prefer alternatives via a finite cost multiplier (edge stays searchable).
    Penalize,
    /// Hard-exclude nearby edges from A* (like [`crate::routing::toll::TollPolicy::NeverUse`]).
    Block,
}

impl DatexImpact {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ignore => "ignore",
            Self::Penalize => "penalize",
            Self::Block => "block",
        }
    }
}

/// Radius (metres) around a situation display point used to match nearby edges.
pub const DATEX_IMPACT_RADIUS_M: f64 = 250.0;

/// Finite A* cost multiplier for [`DatexImpact::Penalize`] edges (mirrors toll).
pub const DATEX_PENALIZE_MULT: f64 = 50.0;

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
        let Some((lat, lon)) = s.primary_lat_lon() else {
            continue;
        };
        out.push(DatexPlannerConstraint {
            lat,
            lon,
            impact: s.impact,
            radius_m: DATEX_IMPACT_RADIUS_M,
            situation_id: s.id.clone(),
        });
    }
    out
}

/// Classify impact from parsed DATEX fields (first match wins).
///
/// See `docs/plugins/datex-plugin.md` for the field→bucket table.
pub fn classify_impact(
    lanes_restricted: Option<u32>,
    severity: Option<&str>,
    comment: Option<&str>,
    location_description: Option<&str>,
) -> DatexImpact {
    if text_indicates_closure(comment) || text_indicates_closure(location_description) {
        return DatexImpact::Block;
    }
    if lanes_restricted.is_some_and(|n| n >= 2) {
        return DatexImpact::Block;
    }
    if lanes_restricted.is_some_and(|n| n >= 1) {
        return DatexImpact::Penalize;
    }
    if severity_is_elevated(severity) {
        return DatexImpact::Penalize;
    }
    DatexImpact::Ignore
}

fn text_indicates_closure(text: Option<&str>) -> bool {
    let Some(raw) = text else {
        return false;
    };
    let t = raw.to_ascii_lowercase();
    // Norwegian + English closure cues (word-ish; "stengt" covers "vegen er stengt").
    t.contains("stengt")
        || t.contains("sperret")
        || t.contains("road closed")
        || t.contains("carriageway closed")
        || t.contains("fully closed")
        || (t.contains("closed")
            && (t.contains("road") || t.contains("vegen") || t.contains("vei")))
}

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
            impact,
        }
    }

    #[test]
    fn stengt_comment_is_block() {
        assert_eq!(
            classify_impact(Some(0), Some("none"), Some("Vegen er stengt."), None),
            DatexImpact::Block
        );
    }

    #[test]
    fn two_lanes_is_block() {
        assert_eq!(
            classify_impact(Some(2), Some("low"), Some("Vegarbeid."), None),
            DatexImpact::Block
        );
    }

    #[test]
    fn one_lane_is_penalize() {
        assert_eq!(
            classify_impact(Some(1), Some("low"), Some("ett påvirket kjørefelt"), None),
            DatexImpact::Penalize
        );
    }

    #[test]
    fn none_severity_zero_lanes_is_ignore() {
        assert_eq!(
            classify_impact(Some(0), Some("none"), Some("Fartsgrense 50 km/t"), None),
            DatexImpact::Ignore
        );
    }

    #[test]
    fn planner_impacts_drops_ignore_and_keeps_block() {
        let ignore = sit("a", DatexImpact::Ignore);
        let block = DatexSituation {
            id: "b".into(),
            impact: DatexImpact::Block,
            comment: Some("stengt".into()),
            ..ignore.clone()
        };
        let got = planner_impacts(&[ignore, block]);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].situation_id, "b");
        assert_eq!(got[0].impact, DatexImpact::Block);
    }
}
