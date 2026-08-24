//! Shared download control (pause / resume / cancel) for long-running fetches.

mod control;
mod fsutil;
pub mod http;
pub mod pbf_priority;
pub mod progress;

pub use control::DownloadControl;
pub use fsutil::{available_bytes, enrich_io_error};
pub use http::{
    bearer_headers, format_reqwest_error, http_client, stream_get_to_file,
    stream_get_to_file_blocking, timeout_for_bytes, StreamDownloadOpts, StreamDownloadResult,
    DEFAULT_RETRIES,
};
pub use pbf_priority::ForegroundPlanGuard;
