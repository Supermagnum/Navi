//! Shared download control (pause / resume / cancel) for long-running fetches.

mod control;
mod fsutil;
pub mod progress;

pub use control::DownloadControl;
pub use fsutil::{available_bytes, enrich_io_error};
