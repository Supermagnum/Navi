//! Driver Break core library.
//!
//! Trusted native Rust modules for elevation download, offline routing with
//! terrain-aware reweighting, POI spatial indexing, and configurable rest/safety
//! parameters. Live ECU/OBD telemetry is intentionally out of scope; extension
//! points exist in [`ecu`] for a future isolated plugin.

pub mod bus;
pub mod config;
pub mod ecu;
pub mod icons;
pub mod poi;
pub mod routing;
pub mod search;
pub mod sensors;
pub mod storage;
pub mod tracks;

pub use config::{FuelConfig, Profile, RestConfig, SafetyConfig, VehicleLimits};
pub use poi::{PoiCategory, PoiIndex, PoiRecord};
pub use routing::elevation::{ElevationCache, ElevationDownloader, ElevationJob, ElevationService};
pub use routing::graph::{RouteGraph, RouteOptions, RoutingProfile};
pub use routing::workers::WorkerPoolPlan;
pub use search::{NameHit, NameIndex, RouteStore, SavedRoute};
pub use storage::Storage;
pub use tracks::{
    clamp_range, clamp_timeout, haversine_km, offset_lat_lon, TrackStation, TrackStore,
    UpsertOutcome, DISPLAY_RANGE_MAX_KM, DISPLAY_RANGE_MIN_KM, STATION_TIMEOUT_MAX_S,
};
