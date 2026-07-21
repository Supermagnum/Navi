//! Offline routing: graph build, elevation reweighting, rest/safety helpers.

pub mod elevation;
pub mod graph;
pub mod osm_update;
pub mod region;
pub mod rest;
pub mod safety;
pub mod workers;

pub use graph::{RouteGraph, RoutingProfile};
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
