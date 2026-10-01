//! Shared download control (pause / resume / cancel) for long-running fetches.

mod control;
mod fsutil;
pub mod http;
pub mod pbf_priority;
pub mod phase_timing;
pub mod plan_cancel;
pub mod progress;

pub use control::DownloadControl;
pub use fsutil::{available_bytes, enrich_io_error};
pub use http::{
    bearer_headers, dated_pbf_url_from_redirect_location_with_dir, format_reqwest_error,
    geofabrik_dated_pbf_url, http_client, is_geofabrik_latest_pbf_url, is_geofabrik_osm_pbf_url,
    newest_dated_pbf_href_from_geofabrik_html, recent_geofabrik_yymmdd_candidates,
    resolve_geofabrik_latest_to_dated_url, resolve_geofabrik_latest_to_dated_url_blocking,
    shared_http_client, stream_get_to_file, stream_get_to_file_blocking, timeout_for_bytes,
    StreamDownloadOpts, StreamDownloadResult, DEFAULT_RETRIES, GEOFABRIK_EXTRACT_RETRIES,
};
pub use pbf_priority::ForegroundPlanGuard;
