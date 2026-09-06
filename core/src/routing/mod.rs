//! Offline routing: graph build, elevation reweighting, rest/safety helpers.

pub mod access;
pub mod basemap;
pub mod conditional;
pub mod dnt_winter;
pub mod ebike_route;
pub mod elevation;
pub mod eta;
pub mod graph;
pub mod guidance_path;
pub mod hiking_hybrid;
pub mod indexed;
pub mod live_hazard;
pub mod osm_update;
pub mod plan_bbox;
pub mod region;
pub mod region_lock;
pub mod rest;
pub mod road_sign;
pub mod safety;
pub mod speed_camera;
pub mod terrain;
pub mod toll;
pub mod wetland;
pub mod workers;

pub use basemap::{
    bbox_covers_point, default_pmtiles_base_url, default_pmtiles_planet_url,
    geofabrik_path_to_region_key, pbf_stem_to_geofabrik_path, point_covered_by_regions,
    region_bbox, region_pmtiles_url, resolve_planet_url_blocking, suggest_geofabrik_path_for_point,
    PmtilesDownloader, PmtilesJob, DEFAULT_EXTRACT_MAX_ZOOM, DEFAULT_PMTILES_BASE_URL,
    PROTOMAPS_PLANET_FALLBACK_URL,
};
pub use ebike_route::{
    analyze_ebike_route, analyze_ev_car_route, format_ebike_route_report,
    format_ebike_route_report_with_path_grade, format_eco_energy_breakdown_report,
    format_ev_car_route_report, grade_exceeds_capability, path_eco_energy_breakdown,
    path_max_climb_grade_pct, path_mechanical_energy_j, steep_segments_over_capability,
    EcoEnergyBreakdown,
};
pub use eta::{
    edge_speed_kmh, fixed_pace_minutes, highway_class_display_label, highway_fallback_kmh,
    motor_path_minutes, motor_path_minutes_from_edges, parse_maxspeed_kmh,
    predeparture_eta_minutes, PreDeparturePace, CYCLING_MIN_PER_KM, HIKING_MIN_PER_KM,
};
pub use graph::{
    apply_official_network_preference, format_route_avoidance_report, max_waypoint_snap_m,
    GraphEdge, PathSearchStats, RouteGraph, RouteOptions, RoutingProfile, SnapTooFar,
    WetlandApplyStats, NON_NETWORK_PENALTY,
};
pub use guidance_path::{
    build_maneuvers, build_maneuvers_from_edges, build_maneuvers_with_options, build_sim_samples,
    build_sim_samples_from_edges, build_sim_samples_from_lat_lon, build_sim_samples_with_options,
    maneuvers_to_json, navit_roundabout_icon, navit_roundabout_sector, probe_roundabout_icon,
    probe_roundabout_spans, samples_to_json, RoundaboutIconProbe, RoundaboutSpan, RouteManeuver,
    SimSample,
};
pub use hiking_hybrid::{
    plan_hybrid_hiking_path, HikingWaypoint, HybridHikingPath, RouteSegment, SegmentKind,
    OFF_TRAIL_ADVISORY,
};
pub use osm_update::{
    apply_pending_update, apply_update_plan, bind_geofabrik_extract, check_for_updates,
    decide_update_plan, format_update_plan, geofabrik_latest_pbf_url, geofabrik_updates_base,
    set_weekly_reminder_opt_in, weekly_reminder_due, GeofabrikState, RegionExtractMeta,
    UpdateApplyResult, UpdatePlan, STALENESS_FULL_REDOWNLOAD_DAYS, WEEKLY_CHECK_REMINDER_DAYS,
};
pub use region::{
    provision_region, provision_region_with_elev_tar, RegionProvision, CORRIDOR_BBOX,
};
pub use region_lock::{
    acquire_plan_fallback, cleanup_spills_for_pid, convert_lock_held,
    holding_convert_lock_on_thread, is_convert_in_progress_err, recover_stale, region_id_for_pbf,
    region_lock_path, try_acquire_convert, ConvertAcquire, RegionLockGuard, RegionLockKind,
    RegionLockPhase, REGION_CONVERT_IN_PROGRESS,
};
pub use rest::{
    car_break_interval_hours, car_style_daily_hours, choose_daily_overnight_rest,
    choose_hiking_overnight, commit_truck_multi_day_plan, commit_truck_trip, cycling_daily_km,
    evaluate_fmcsa_trip, evaluate_truck_trip, hiking_samples_from_coords, max_daily_distance_km,
    motor_break_interval_km, motor_daily_budget, plan_fmcsa_multi_day, plan_hiking_multi_day,
    plan_motor_multi_day, plan_truck_multi_day, resolve_driving_hours_pack_at,
    soft_break_distances_km, soft_break_interval_km_fallback, soft_car_break_interval_hours,
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
    check_overnight_candidate, min_distance_to_glacier_rings_m, DangerBarrierIndex,
    OvernightProximityIndex, OvernightRejectReason,
};
pub use terrain::{least_cost_path, TerrainPath, TERRAIN_CELL_M, TERRAIN_MAX_GAP_M};
pub use toll::{toll_applies_for_profile, TollPolicy, TOLL_AVOID_PENALTY_MULT};
pub use wetland::{
    classify_wetland_value, tags_indicate_boardwalk, WetlandClass, WetlandIndex,
    WETLAND_SOFT_COST_MULT,
};
pub use workers::WorkerPoolPlan;
