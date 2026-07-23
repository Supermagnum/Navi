//! Offline Protomaps basemap (PMTiles) region keys and download helpers.

mod downloader;
mod extract;
mod http_backend;
mod range_coalesce;
mod regions;

pub use downloader::{PmtilesDownloader, PmtilesJob};
pub use extract::{
    extract_bbox_to_file, resolve_planet_url_blocking, DEFAULT_EXTRACT_MAX_ZOOM,
    PROTOMAPS_BUILDS_METADATA_URL, PROTOMAPS_BUILD_BASE_URL, PROTOMAPS_PLANET_FALLBACK_URL,
};
pub use regions::{
    bbox_covers_point, default_pmtiles_base_url, default_pmtiles_planet_url,
    geofabrik_path_to_region_key, region_bbox, region_pmtiles_url, sanitize_region_key,
    DEFAULT_PMTILES_BASE_URL,
};
