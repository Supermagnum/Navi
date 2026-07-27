//! Offline routing: graph build, elevation reweighting, rest/safety helpers.

pub mod basemap;
pub mod ebike_route;
pub mod elevation;
pub mod eta;
pub mod graph;
pub mod guidance_path;
pub mod osm_update;
pub mod region;
pub mod rest;
pub mod safety;
pub mod workers;

pub use basemap::{
    bbox_covers_point, default_pmtiles_base_url, default_pmtiles_planet_url,
    geofabrik_path_to_region_key, region_bbox, region_pmtiles_url, resolve_planet_url_blocking,
    PmtilesDownloader, PmtilesJob, DEFAULT_EXTRACT_MAX_ZOOM, DEFAULT_PMTILES_BASE_URL,
    PROTOMAPS_PLANET_FALLBACK_URL,
};
pub use ebike_route::{
    analyze_ebike_route, analyze_ev_car_route, format_ebike_route_report,
    format_ebike_route_report_with_path_grade, format_ev_car_route_report,
    grade_exceeds_capability, path_max_climb_grade_pct, path_mechanical_energy_j,
    steep_segments_over_capability,
};
pub use eta::{
    edge_speed_kmh, fixed_pace_minutes, highway_class_display_label, highway_fallback_kmh,
    motor_path_minutes, parse_maxspeed_kmh, predeparture_eta_minutes, PreDeparturePace,
    CYCLING_MIN_PER_KM, HIKING_MIN_PER_KM,
};
pub use graph::{
    apply_official_network_preference, format_route_avoidance_report, GraphEdge, RouteGraph,
    RouteOptions, RoutingProfile, NON_NETWORK_PENALTY,
};
pub use guidance_path::{
    build_maneuvers, build_sim_samples, maneuvers_to_json, samples_to_json, RouteManeuver,
    SimSample,
};
pub use osm_update::{
    apply_pending_update, apply_update_plan, bind_geofabrik_extract, check_for_updates,
    decide_update_plan, format_update_plan, set_weekly_reminder_opt_in, weekly_reminder_due,
    GeofabrikState, RegionExtractMeta, UpdateApplyResult, UpdatePlan,
    STALENESS_FULL_REDOWNLOAD_DAYS, WEEKLY_CHECK_REMINDER_DAYS,
};
pub use region::{
    provision_region, provision_region_with_elev_tar, RegionProvision, CORRIDOR_BBOX,
};
pub use rest::{
    car_break_interval_hours, car_style_daily_hours, choose_daily_overnight_rest,
    choose_hiking_overnight, commit_truck_multi_day_plan, commit_truck_trip, cycling_daily_km,
    evaluate_fmcsa_trip, evaluate_truck_trip, hiking_samples_from_coords, max_daily_distance_km,
    motor_break_interval_km, motor_daily_budget, plan_fmcsa_multi_day, plan_hiking_multi_day,
    plan_motor_multi_day, plan_truck_multi_day, resolve_driving_hours_pack_at,
    truck_break_distances_km, truck_break_duration_minutes, truck_break_interval_km,
    truck_day_cap_hours, truck_effective_break_parts, truck_mandatory_break_after_hours,
    truck_max_daily_driving_hours, truck_max_fortnightly_driving_hours,
    truck_max_weekly_driving_hours, truck_required_breaks, uses_motor_multi_day, uses_truck_rest,
    BreakKind, HikingDaySegment, HikingMultiDayPlan, HikingOvernightStop, HikingRouteSample,
    MotorDailyBudget, MotorDaySegment, MotorMultiDayPlan, MotorOvernightCandidate,
    MotorOvernightKind, MotorOvernightStop, TruckDaySegment, TruckDutyEvaluation,
    TruckMultiDayPlan, TruckOvernightKind, TruckOvernightRest, TruckRestCandidate,
    TruckRestFacility, OVERNIGHT_NEAR_HUT_MAX_M,
};
pub use safety::{
    check_overnight_candidate, DangerBarrierIndex, OvernightProximityIndex, OvernightRejectReason,
};
pub use workers::WorkerPoolPlan;
