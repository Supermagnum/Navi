//! Offline routing: graph build, elevation reweighting, rest/safety helpers.

pub mod basemap;
pub mod elevation;
pub mod eta;
pub mod graph;
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
pub use eta::{
    fixed_pace_minutes, motor_path_minutes, parse_maxspeed_kmh, predeparture_eta_minutes,
    PreDeparturePace, CYCLING_MIN_PER_KM, HIKING_MIN_PER_KM,
};
pub use graph::{
    apply_official_network_preference, format_route_avoidance_report, GraphEdge, RouteGraph,
    RouteOptions, RoutingProfile, NON_NETWORK_PENALTY,
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
pub use workers::WorkerPoolPlan;
pub use safety::{
    check_overnight_candidate, DangerBarrierIndex, OvernightProximityIndex, OvernightRejectReason,
};
