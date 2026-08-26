//! Offline Protomaps basemap (PMTiles) region keys and download helpers.

mod downloader;
mod extract;
mod http_backend;
mod range_coalesce;
mod regions;
mod tile_read;

pub use downloader::{PmtilesDownloader, PmtilesJob};
pub use extract::{
    extract_bbox_to_file, resolve_planet_url_blocking, validate_completed_pmtiles,
    DEFAULT_EXTRACT_MAX_ZOOM, MIN_FULL_REGION_BASEMAP_BYTES, PROTOMAPS_BUILDS_METADATA_URL,
    PROTOMAPS_BUILD_BASE_URL, PROTOMAPS_PLANET_FALLBACK_URL,
};
pub use regions::{
    bbox_covers_point, default_pmtiles_base_url, default_pmtiles_planet_url,
    geofabrik_path_to_region_key, pbf_stem_to_geofabrik_path, point_covered_by_regions,
    region_bbox, region_pmtiles_url, sanitize_region_key, suggest_geofabrik_path_for_point,
    DEFAULT_PMTILES_BASE_URL,
};
pub use tile_read::read_pmtiles_tile;
