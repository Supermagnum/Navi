//! Per-type DATEX classification against NPRA GetSituation field shapes.
//!
//! XML snippets follow the live feed's record layout (severity, lanes,
//! delays/delayTimeValue, free-text comment, coordinatesForDisplay). Counts
//! from the 2797-record snapshot are relative frequency only.

use driver_break_core::datex::{
    delay_penalize_mult, parse_situation_publication, planner_impacts, wind_penalize_mult,
    DatexImpact, DATEX_PENALIZE_MULT, NPRA_LIVE_XSI_TYPES,
};

fn wrap_record(xsi_type: &str, inner: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<d2LogicalModel xmlns="http://datex2.eu/schema/3/d2Payload"
  xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
  xmlns:sit="http://datex2.eu/schema/3/situation"
  xmlns:com="http://datex2.eu/schema/3/common"
  xmlns:loc="http://datex2.eu/schema/3/locationReferencing"
  modelBaseVersion="3">
  <payloadPublication xsi:type="sit:SituationPublication" lang="no">
    <sit:situation id="sit-{xsi_type}">
      <sit:situationRecord xsi:type="sit:{xsi_type}" id="rec-{xsi_type}" version="1">
        <com:situationRecordCreationTime>2026-09-01T00:00:00+02:00</com:situationRecordCreationTime>
        {inner}
        <sit:groupOfLocations>
          <loc:coordinatesForDisplay>
            <loc:latitude>60.56</loc:latitude>
            <loc:longitude>11.25</loc:longitude>
          </loc:coordinatesForDisplay>
        </sit:groupOfLocations>
      </sit:situationRecord>
    </sit:situation>
  </payloadPublication>
</d2LogicalModel>
"#
    )
}

fn parse_one(xsi_type: &str, inner: &str) -> driver_break_core::datex::DatexSituation {
    let xml = wrap_record(xsi_type, inner);
    let all = parse_situation_publication(&xml).unwrap_or_else(|e| panic!("parse {xsi_type}: {e}"));
    assert_eq!(
        all.len(),
        1,
        "{} must yield one record, got {}",
        xsi_type,
        all.len()
    );
    let s = all.into_iter().next().unwrap();
    assert_eq!(s.xsi_type, xsi_type);
    s
}

fn lanes_severity_comment(lanes: u32, severity: &str, comment: &str) -> String {
    format!(
        r#"<sit:severity>{severity}</sit:severity>
        <sit:impact>
          <sit:numberOfLanesRestricted>{lanes}</sit:numberOfLanesRestricted>
        </sit:impact>
        <sit:generalPublicComment>
          <sit:comment>
            <values><value lang="no">{comment}</value></values>
          </sit:comment>
        </sit:generalPublicComment>"#
    )
}

#[test]
fn inventory_lists_seventeen_live_types() {
    assert_eq!(NPRA_LIVE_XSI_TYPES.len(), 17);
}

#[test]
fn type_road_or_carriageway_or_lane_management_penalize() {
    // Representative: lane management without numeric delay → type-default Penalize.
    let s = parse_one(
        "RoadOrCarriagewayOrLaneManagement",
        &lanes_severity_comment(
            1,
            "medium",
            "Ett kjørefelt redusert. Midlertidig omlegging.",
        ),
    );
    assert_eq!(s.impact, DatexImpact::Penalize);
    assert!((s.penalize_mult - DATEX_PENALIZE_MULT).abs() < 1e-9);
}

#[test]
fn type_maintenance_works_penalize_on_lanes_or_medium_severity() {
    let with_lanes = parse_one(
        "MaintenanceWorks",
        &lanes_severity_comment(1, "low", "Bevegelig vegarbeid, ett påvirket kjørefelt."),
    );
    assert_eq!(with_lanes.impact, DatexImpact::Penalize);

    let medium = parse_one(
        "MaintenanceWorks",
        &lanes_severity_comment(0, "medium", "Vegarbeid."),
    );
    assert_eq!(medium.impact, DatexImpact::Penalize);

    let quiet = parse_one(
        "MaintenanceWorks",
        &lanes_severity_comment(0, "none", "Vegarbeid, toveis trafikk i ett løp."),
    );
    assert_eq!(quiet.impact, DatexImpact::Ignore);
}

#[test]
fn type_general_network_management_penalize_only_when_lanes() {
    let lights = parse_one(
        "GeneralNetworkManagement",
        &lanes_severity_comment(0, "low", "Midlertidige trafikklys."),
    );
    assert_eq!(lights.impact, DatexImpact::Ignore);

    let restricted = parse_one(
        "GeneralNetworkManagement",
        &lanes_severity_comment(1, "low", "Manuell dirigering."),
    );
    assert_eq!(restricted.impact, DatexImpact::Penalize);
}

#[test]
fn type_speed_management_ignore() {
    let s = parse_one(
        "SpeedManagement",
        &lanes_severity_comment(0, "none", "Fartsgrense 50 km/t."),
    );
    assert_eq!(s.impact, DatexImpact::Ignore);
}

#[test]
fn type_rerouting_management_penalize() {
    let s = parse_one(
        "ReroutingManagement",
        &lanes_severity_comment(0, "low", "Følg omkjøringsskilt."),
    );
    assert_eq!(s.impact, DatexImpact::Penalize);
}

#[test]
fn type_construction_works_penalize_unless_closure_text() {
    let open = parse_one(
        "ConstructionWorks",
        &lanes_severity_comment(0, "medium", "Anleggsarbeid pågår."),
    );
    assert_eq!(open.impact, DatexImpact::Penalize);
    assert_eq!(open.lanes_restricted, Some(0));

    let closed = parse_one(
        "ConstructionWorks",
        &lanes_severity_comment(0, "low", "Vegen er stengt."),
    );
    assert_eq!(closed.impact, DatexImpact::Block);
}

#[test]
fn type_environmental_obstruction_block() {
    let s = parse_one(
        "EnvironmentalObstruction",
        &lanes_severity_comment(0, "high", "Steinsprang. Fallen tree / landslip."),
    );
    assert_eq!(s.impact, DatexImpact::Block);
}

#[test]
fn type_public_event_block_when_lanes_or_closed() {
    let open = parse_one(
        "PublicEvent",
        &lanes_severity_comment(0, "low", "Arrangement langs vegen."),
    );
    assert_eq!(open.impact, DatexImpact::Penalize);

    let lanes = parse_one(
        "PublicEvent",
        &lanes_severity_comment(2, "low", "Vegen er stengt for arrangement."),
    );
    assert_eq!(lanes.impact, DatexImpact::Block);
}

#[test]
fn type_transit_information_ignore() {
    let s = parse_one(
        "TransitInformation",
        &lanes_severity_comment(0, "none", "Ferjetabell endret."),
    );
    assert_eq!(s.impact, DatexImpact::Ignore);
}

#[test]
fn type_infrastructure_damage_obstruction_block_on_closure_or_lanes() {
    let closed = parse_one(
        "InfrastructureDamageObstruction",
        &lanes_severity_comment(0, "high", "damagedRoadSurface. Vegen er stengt."),
    );
    assert_eq!(closed.impact, DatexImpact::Block);

    let two_lanes = parse_one(
        "InfrastructureDamageObstruction",
        &lanes_severity_comment(2, "high", "Midlertidig omkjøring via Eidsfoss"),
    );
    assert_eq!(two_lanes.impact, DatexImpact::Block);
}

#[test]
fn type_infrastructure_damage_passable_stays_penalize() {
    // National-snapshot false positives: still open with lights/speed, or
    // partial reopen with "kan passere" — must not hard-Block.
    let lights = parse_one(
        "InfrastructureDamageObstruction",
        &lanes_severity_comment(
            0,
            "none",
            "Skade på vegnett.|Lysregulering, fartsgrense 30 km/t.",
        ),
    );
    assert_eq!(lights.impact, DatexImpact::Penalize);

    let can_pass = parse_one(
        "InfrastructureDamageObstruction",
        &lanes_severity_comment(
            0,
            "unknown",
            "Personbiler og andre kjøretøy opptil opp til 32 tonns totalvekt kan passere.",
        ),
    );
    assert_eq!(can_pass.impact, DatexImpact::Penalize);
}

#[test]
fn regression_partial_lane_stengt_does_not_block_from_text() {
    let one_lane = parse_one(
        "MaintenanceWorks",
        &lanes_severity_comment(1, "low", "Vegarbeid, ett stengt kjørefelt.|Lysregulering."),
    );
    assert_eq!(one_lane.impact, DatexImpact::Penalize);

    let felt = parse_one(
        "GeneralNetworkManagement",
        &lanes_severity_comment(0, "low", "Et felt stengt."),
    );
    assert_ne!(felt.impact, DatexImpact::Block);
    assert_eq!(felt.impact, DatexImpact::Ignore);
}

#[test]
fn regression_fare_for_stengt_does_not_block() {
    let inner = r#"<sit:severity>none</sit:severity>
        <sit:impact>
          <sit:numberOfLanesRestricted>0</sit:numberOfLanesRestricted>
        </sit:impact>
        <sit:generalPublicComment>
          <sit:comment>
            <values><value lang="no">Sterk vind, fare for stengt veg.</value></values>
          </sit:comment>
        </sit:generalPublicComment>
        <sit:wind>
          <sit:windSpeed>17.3</sit:windSpeed>
        </sit:wind>"#;
    let s = parse_one("PoorEnvironmentConditions", inner);
    assert_eq!(s.impact, DatexImpact::Penalize);
    assert_eq!(s.wind_speed, Some(17.3));
    assert!((s.penalize_mult - wind_penalize_mult(17.3)).abs() < 1e-9);

    let conditional = parse_one(
        "MaintenanceWorks",
        &lanes_severity_comment(0, "none", "Veien kan bli stengt i kortere perioder"),
    );
    assert_ne!(conditional.impact, DatexImpact::Block);
}

#[test]
fn type_non_weather_related_road_conditions_penalize() {
    let s = parse_one(
        "NonWeatherRelatedRoadConditions",
        &lanes_severity_comment(0, "medium", "Glatt vegbane (slipperyRoad)."),
    );
    assert_eq!(s.impact, DatexImpact::Penalize);
}

#[test]
fn type_animal_presence_obstruction_penalize() {
    let s = parse_one(
        "AnimalPresenceObstruction",
        &lanes_severity_comment(0, "low", "Dyr i vegen (animalsOnTheRoad)."),
    );
    assert_eq!(s.impact, DatexImpact::Penalize);
}

#[test]
fn type_general_obstruction_penalize_escalates_on_closure_text() {
    let object = parse_one(
        "GeneralObstruction",
        &lanes_severity_comment(0, "low", "Gjenstand i vegen (objectOnTheRoad)."),
    );
    assert_eq!(object.impact, DatexImpact::Penalize);

    let blocked = parse_one(
        "GeneralObstruction",
        &lanes_severity_comment(0, "unknown", "Vegen er stengt pga. gjenstand."),
    );
    assert_eq!(blocked.impact, DatexImpact::Block);
}

#[test]
fn type_accident_block_even_when_severity_unknown() {
    let s = parse_one("Accident", &lanes_severity_comment(0, "unknown", "Ulykke."));
    assert_eq!(s.impact, DatexImpact::Block);
}

#[test]
fn type_vehicle_obstruction_penalize() {
    let s = parse_one(
        "VehicleObstruction",
        &lanes_severity_comment(1, "low", "brokenDownVehicle."),
    );
    assert_eq!(s.impact, DatexImpact::Penalize);
}

#[test]
fn type_poor_environment_conditions_penalize_scales_with_wind() {
    let inner = r#"<sit:severity>medium</sit:severity>
        <sit:impact>
          <sit:numberOfLanesRestricted>0</sit:numberOfLanesRestricted>
        </sit:impact>
        <sit:generalPublicComment>
          <sit:comment>
            <values><value lang="no">Sterk vind (strongWinds).</value></values>
          </sit:comment>
        </sit:generalPublicComment>
        <sit:wind>
          <sit:windSpeed>20</sit:windSpeed>
        </sit:wind>"#;
    let s = parse_one("PoorEnvironmentConditions", inner);
    assert_eq!(s.impact, DatexImpact::Penalize);
    assert_eq!(s.wind_speed, Some(20.0));
    assert!((s.penalize_mult - DATEX_PENALIZE_MULT).abs() < 1e-9);
}

#[test]
fn type_roadside_assistance_ignore() {
    let s = parse_one(
        "RoadsideAssistance",
        &lanes_severity_comment(0, "none", "vehicleRecovery."),
    );
    assert_eq!(s.impact, DatexImpact::Ignore);
}

#[test]
fn transit_information_and_speed_management_never_escalate() {
    // Structurally exempt: even if future NPRA data adds severity, lanes, or
    // closure phrasing, these types must not reach the planner as Penalize/Block.
    let speed = parse_one(
        "SpeedManagement",
        &lanes_severity_comment(4, "highest", "Vegen er stengt. Fartsgrense 30 km/t."),
    );
    assert_eq!(speed.impact, DatexImpact::Ignore);
    assert!(
        planner_impacts(std::slice::from_ref(&speed)).is_empty(),
        "SpeedManagement must not reach the planner"
    );

    let transit = parse_one(
        "TransitInformation",
        &lanes_severity_comment(3, "highest", "Vegen er stengt. Ferje innstilt."),
    );
    assert_eq!(transit.impact, DatexImpact::Ignore);
    assert!(
        planner_impacts(std::slice::from_ref(&transit)).is_empty(),
        "TransitInformation must not reach the planner"
    );
}

#[test]
fn lane_management_delay_time_value_drives_penalize_multiplier() {
    // Same lane count; only delayTimeValue differs. 30 min vs 30 s.
    let with_delay = |secs: &str| {
        format!(
            r#"<sit:severity>medium</sit:severity>
        <sit:impact>
          <sit:delays>
            <sit:delayTimeValue>{secs}</sit:delayTimeValue>
            <sit:delaysType>delays</sit:delaysType>
          </sit:delays>
          <sit:numberOfLanesRestricted>1</sit:numberOfLanesRestricted>
        </sit:impact>"#
        )
    };

    let long = parse_one("RoadOrCarriagewayOrLaneManagement", &with_delay("1800"));
    let short = parse_one("RoadOrCarriagewayOrLaneManagement", &with_delay("30"));

    assert_eq!(long.impact, DatexImpact::Penalize);
    assert_eq!(short.impact, DatexImpact::Penalize);
    assert_eq!(long.lanes_restricted, Some(1));
    assert_eq!(short.lanes_restricted, Some(1));
    assert_eq!(long.delay_time_secs, Some(1800.0));
    assert_eq!(short.delay_time_secs, Some(30.0));
    assert!(
        long.penalize_mult > short.penalize_mult,
        "delay must drive multiplier: 1800s={} 30s={}",
        long.penalize_mult,
        short.penalize_mult
    );
    assert!((long.penalize_mult - delay_penalize_mult(1800.0)).abs() < 1e-9);
    assert!((short.penalize_mult - delay_penalize_mult(30.0)).abs() < 1e-9);

    let constraints = planner_impacts(std::slice::from_ref(&long));
    assert_eq!(constraints.len(), 1);
    assert!((constraints[0].penalize_mult - long.penalize_mult).abs() < 1e-9);
}

#[test]
fn lane_management_delays_without_value_falls_back_to_lane_severity() {
    let inner = r#"<sit:severity>none</sit:severity>
        <sit:impact>
          <sit:delays>
            <sit:delaysType>delays</sit:delaysType>
          </sit:delays>
          <sit:numberOfLanesRestricted>0</sit:numberOfLanesRestricted>
        </sit:impact>"#;
    let s = parse_one("RoadOrCarriagewayOrLaneManagement", inner);
    assert!(s.delays_present);
    assert!(s.delay_time_secs.is_none());
    assert_eq!(s.impact, DatexImpact::Ignore);
}

#[test]
fn unrecognized_xsi_type_does_not_crash_and_defaults_to_ignore() {
    let s = parse_one(
        "CompletelyFutureSituationType",
        &lanes_severity_comment(4, "highest", "Vegen er stengt."),
    );
    assert!(
        s.unrecognized_xsi_type,
        "must be flagged, not silently dropped"
    );
    assert_eq!(s.impact, DatexImpact::Ignore);
    assert_eq!(s.xsi_type, "CompletelyFutureSituationType");
    assert!(
        planner_impacts(std::slice::from_ref(&s)).is_empty(),
        "unrecognized type must not reach the planner"
    );
}

#[test]
fn schema_valid_unused_types_do_not_panic() {
    let abnormal = parse_one(
        "AbnormalTraffic",
        &lanes_severity_comment(1, "medium", "Kø."),
    );
    assert!(!abnormal.unrecognized_xsi_type);
    assert_eq!(abnormal.impact, DatexImpact::Penalize);

    let weather = parse_one(
        "WeatherRelatedRoadConditions",
        &lanes_severity_comment(0, "none", "Vått føre."),
    );
    assert!(!weather.unrecognized_xsi_type);
    assert_eq!(weather.impact, DatexImpact::Ignore);
}
