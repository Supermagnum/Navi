//! Copernicus DEM downloader, tile cache, and elevation lookup.

mod cache;
mod country;
mod downloader;
mod reader;
mod service;
pub mod sources;
pub mod tile_id;

pub use cache::ElevationCache;
pub use downloader::{DownloadControl, ElevationDownloader, ElevationJob};
pub use reader::ElevationReader;
pub use service::ElevationService;
pub use tile_id::{bbox_to_tiles, country_bbox, HgtTileId};
