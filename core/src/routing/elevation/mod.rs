//! Copernicus DEM downloader, tile cache, and elevation lookup.

mod cache;
mod country;
mod country_polys;
mod downloader;
mod reader;
mod service;
pub mod sources;
pub mod tile_id;

pub use crate::download::DownloadControl;
pub use cache::ElevationCache;
pub use country::{iso_at as country_iso_at, lookup as country_lookup};
pub use downloader::{ElevationDownloader, ElevationJob};
pub use reader::ElevationReader;
pub use service::ElevationService;
pub use tile_id::{bbox_to_tiles, country_bbox, HgtTileId};
