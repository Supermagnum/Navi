//! Follow-up 11 DATEX rules: Fv 55 Lustravegen and Rv 5 Fodnestunnelen.

use chrono::{FixedOffset, TimeZone, Utc};
use driver_break_core::datex::{
    default_penalty_minutes, impacts_near_route_ctx, parse_situation_publication, DatexHopContext,
    DatexImpact,
};

fn wrap(record: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ns2:messageContainer xmlns="http://datex2.eu/schema/3/common"
  xmlns:ns2="http://datex2.eu/schema/3/messageContainer"
  xmlns:ns9="http://datex2.eu/schema/3/locationReferencing"
  xmlns:ns12="http://datex2.eu/schema/3/situation"
  xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
  modelBaseVersion="3">
  <ns2:payload xsi:type="ns12:SituationPublication" lang="no" modelBaseVersion="3">
    {record}
  </ns2:payload>
</ns2:messageContainer>
"#
    )
}

fn load_record(path: &str) -> driver_break_core::datex::DatexSituation {
    let rec = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let xml = wrap(&rec);
    let all = parse_situation_publication(&xml).unwrap_or_else(|e| panic!("parse {path}: {e}"));
    assert_eq!(all.len(), 1, "{path} must yield one record");
    all.into_iter().next().unwrap()
}

fn daytime_tue() -> chrono::DateTime<Utc> {
    FixedOffset::east_opt(2 * 3600)
        .unwrap()
        .with_ymd_and_hms(2026, 10, 6, 12, 0, 0)
        .unwrap()
        .with_timezone(&Utc)
}

#[test]
fn fv55_lustravegen_daytime_is_penalty_not_block() {
    let s = load_record("tests/fixtures/datex/fv55-lustravegen.xml");
    assert_eq!(s.xsi_type, "RoadOrCarriagewayOrLaneManagement");
    assert_eq!(s.management_type.as_deref(), Some("narrowLanes"));
    assert_eq!(s.impact, DatexImpact::Penalize);
    assert!(s.penalty_minutes > 0.0);
    assert!(s.is_active_at_arrival(daytime_tue()));
    let ctx = DatexHopContext {
        arrival: daytime_tue(),
        trip_uncertain: false,
        hop_bearing_deg: None,
        truck: true,
    };
    let route = vec![(61.483685, 7.642682), (61.49, 7.65)];
    let hits = impacts_near_route_ctx(&[s], &route, 5_000.0, ctx);
    assert!(
        hits.iter().any(|c| c.impact == DatexImpact::Penalize),
        "daytime Lustravegen must penalise"
    );
    assert!(
        hits.iter().all(|c| c.impact != DatexImpact::Block),
        "must not block"
    );
}

#[test]
fn rv5_fodnestunnelen_daytime_is_warning_not_block() {
    let s = load_record("tests/fixtures/datex/rv5-fodnestunnelen.xml");
    assert_eq!(s.xsi_type, "RoadOrCarriagewayOrLaneManagement");
    assert_eq!(
        s.management_type.as_deref(),
        Some("intermittentShortTermClosures")
    );
    assert_ne!(s.impact, DatexImpact::Block);
    assert!(!s.is_active_at_arrival(daytime_tue()));
    let ctx = DatexHopContext {
        arrival: daytime_tue(),
        trip_uncertain: false,
        hop_bearing_deg: None,
        truck: true,
    };
    let s_pt = s.primary_lat_lon().expect("coords");
    let route = vec![s_pt, (s_pt.0 + 0.01, s_pt.1 + 0.01)];
    let hits = impacts_near_route_ctx(&[s], &route, 5_000.0, ctx);
    assert!(
        hits.iter().any(|c| c.impact == DatexImpact::Warn),
        "daytime Fodnes must warn about night windows: {hits:?}"
    );
    assert!(hits.iter().all(|c| c.impact != DatexImpact::Block));
}

#[test]
fn default_penalty_minutes_table() {
    assert_eq!(
        default_penalty_minutes("RoadOrCarriagewayOrLaneManagement", Some("narrowLanes")),
        3.0
    );
    assert_eq!(
        default_penalty_minutes(
            "RoadOrCarriagewayOrLaneManagement",
            Some("intermittentShortTermClosures")
        ),
        5.0
    );
    assert_eq!(default_penalty_minutes("Accident", None), 15.0);
    assert_eq!(
        default_penalty_minutes("EnvironmentalObstruction", None),
        10.0
    );
    assert_eq!(default_penalty_minutes("MaintenanceWorks", None), 5.0);
    assert_eq!(default_penalty_minutes("ConstructionWorks", None), 5.0);
}
