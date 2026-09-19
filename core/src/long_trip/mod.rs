//! Long-trip mode: ORS preliminary corridor, ordered region acquisition, and
//! storage checks.
//!
//! Orchestration is core + Android (not a WASM plugin). Pack downloads for this
//! mode may be gated to unmetered Wi‑Fi/Ethernet on the host; that gate must not
//! change ordinary Tools downloads.

mod estimate;
mod neighbours;
mod orchestrate;
mod ors;
mod volume;

pub use estimate::{
    estimate_trip_disk_bytes, CatalogSizeLookup, SpaceCheck, SpaceReport, PBF_KEEP_RATIO,
    PLACE_INDEX_RATIO, STAGING_SAFETY_FACTOR,
};
pub use neighbours::{
    avoid_country_ids_for_allowed, land_neighbours_iso, neighbour_table_is_symmetric,
    ors_country_id,
};
pub use orchestrate::{
    LongTripError, LongTripPlan, RegionDownloader, RegionIndexer, RegionTripState,
    TripOrchestrator, VolumeSource, LONG_TRIP_CORRIDOR_BUFFER_KM,
};
pub use ors::{
    build_directions_request_body, parse_directions_geojson, request_directions, OrsConfig,
    OrsError, OrsRoute, DEFAULT_ORS_BASE_URL, ORS_DISCLOSURE, ORS_MAX_DISTANCE_M,
    ORS_MAX_WAYPOINTS,
};
pub use volume::{StorageVolume, VolumeId};

use crate::pack_server::{ordered_regions_along_corridor, CatalogRegionEntry, ReadyRegion};

/// Map an ORS (or other) corridor polyline through the catalog, dropping
/// installed regions. Order = first crossing along the route.
///
/// When `country_iso` is set, keep only catalog ids under that country prefix
/// (hard stay-inside-country for the preliminary region list).
pub fn ordered_needed_regions_along_route(
    route_lat_lon: &[(f64, f64)],
    catalog: &[CatalogRegionEntry],
    installed: &[String],
    sample_step_km: f64,
) -> Vec<String> {
    ordered_needed_regions_along_route_filtered(
        route_lat_lon,
        catalog,
        installed,
        sample_step_km,
        None,
    )
}

/// Like [`ordered_needed_regions_along_route`] with an optional ISO country filter.
pub fn ordered_needed_regions_along_route_filtered(
    route_lat_lon: &[(f64, f64)],
    catalog: &[CatalogRegionEntry],
    installed: &[String],
    sample_step_km: f64,
    country_iso: Option<&str>,
) -> Vec<String> {
    let mut needed =
        ordered_regions_along_corridor(route_lat_lon, catalog, installed, sample_step_km);
    if let Some(iso) = country_iso {
        let prefix = country_catalog_prefix(iso);
        needed.retain(|id| id.starts_with(&prefix) || id == prefix.trim_end_matches('/'));
    }
    needed
}

/// Classify catalog coverage for a needed-region list under an optional
/// country filter (ISO alpha-2 lowercase).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogCoverage {
    /// Every needed id is published (has a catalog entry with geometry).
    Complete,
    /// Some needed stems are absent from `current.json`.
    NotPublished {
        regions: Vec<String>,
        /// Always local Geofabrik conversion for unpublished leaves.
        fallback: &'static str,
    },
    /// Catalog has zero regions under the country tree (e.g. no `north-america/us/`).
    NoCountryCoverage { country_iso: String },
}

pub const LOCAL_BAKE_FALLBACK: &str = "local-bake (Geofabrik extract + on-device convert)";

/// Inspect whether `needed` ids appear in `catalog_ids`. When `country_iso` is
/// set and the catalog has no regions under that country prefix, return
/// [`CatalogCoverage::NoCountryCoverage`].
pub fn classify_catalog_coverage(
    needed: &[String],
    catalog_ids: &[String],
    country_iso: Option<&str>,
) -> CatalogCoverage {
    if let Some(iso) = country_iso {
        let prefix = country_catalog_prefix(iso);
        let any = catalog_ids.iter().any(|id| id.starts_with(&prefix));
        if !any {
            return CatalogCoverage::NoCountryCoverage {
                country_iso: iso.to_ascii_lowercase(),
            };
        }
    }
    let mut missing = Vec::new();
    for n in needed {
        let ok = catalog_ids.iter().any(|c| {
            crate::pack_server::region_ids_match_for_catalog(c, n)
                || c.starts_with(&format!("{n}/"))
                || n.starts_with(&format!("{c}/"))
        });
        if !ok {
            missing.push(n.clone());
        }
    }
    if missing.is_empty() {
        CatalogCoverage::Complete
    } else {
        CatalogCoverage::NotPublished {
            regions: missing,
            fallback: LOCAL_BAKE_FALLBACK,
        }
    }
}

fn country_catalog_prefix(iso: &str) -> String {
    match iso.trim().to_ascii_lowercase().as_str() {
        "us" | "usa" => "north-america/us".into(),
        "no" | "nor" => "europe/norway".into(),
        "se" | "swe" => "europe/sweden".into(),
        "de" | "deu" => "europe/germany".into(),
        "dk" | "dnk" => "europe/denmark".into(),
        other => format!("country/{other}"),
    }
}

/// Build catalog entries from ready regions, attaching local bboxes when known.
pub fn catalog_entries_from_ready_regions(regions: &[ReadyRegion]) -> Vec<CatalogRegionEntry> {
    let ids: Vec<String> = regions.iter().map(|r| r.region_id.clone()).collect();
    crate::pack_server::catalog_entries_from_ready_ids(&ids)
}

/// True when two region bboxes touch or overlap (expanded by a small epsilon).
pub fn regions_bbox_adjacent(a: &[f64; 4], b: &[f64; 4], eps_deg: f64) -> bool {
    let a0 = [
        a[0] - eps_deg,
        a[1] - eps_deg,
        a[2] + eps_deg,
        a[3] + eps_deg,
    ];
    !(a0[2] < b[0] || b[2] < a0[0] || a0[3] < b[1] || b[3] < a0[1])
}
