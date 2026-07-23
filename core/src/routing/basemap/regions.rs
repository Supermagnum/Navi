//! Geofabrik path → region_key + bbox for offline PMTiles extracts from the
//! public Protomaps planet build (HTTP range extract — no project hosting).

use crate::routing::basemap::extract::{
    resolve_planet_url_blocking, PROTOMAPS_PLANET_FALLBACK_URL,
};
use crate::routing::elevation::country_lookup;

/// Default planet PMTiles URL (resolved at call time when possible).
///
/// Prefer [`default_pmtiles_planet_url`] which hits Protomaps builds metadata.
pub const DEFAULT_PMTILES_BASE_URL: &str = PROTOMAPS_PLANET_FALLBACK_URL;

pub fn default_pmtiles_base_url() -> &'static str {
    DEFAULT_PMTILES_BASE_URL
}

/// Resolve the current public Protomaps planet URL (network), else fallback.
pub fn default_pmtiles_planet_url() -> String {
    resolve_planet_url_blocking()
}

/// Map a Geofabrik path (e.g. `europe/norway/ostlandet`) to a stable file stem.
pub fn geofabrik_path_to_region_key(path: &str) -> String {
    let trimmed = path.trim().trim_matches('/');
    if trimmed.is_empty() {
        return "unknown".to_string();
    }
    sanitize_region_key(&trimmed.replace('/', "_"))
}

pub fn sanitize_region_key(key: &str) -> String {
    key.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_ascii_lowercase()
}

/// Approximate bbox `[min_lat, min_lon, max_lat, max_lon]` for a Geofabrik path.
pub fn region_bbox(geofabrik_path: &str) -> Option<[f64; 4]> {
    let path = geofabrik_path.trim().trim_matches('/').to_ascii_lowercase();
    if let Some(bbox) = NORWAY_LANDSDEL
        .iter()
        .find(|(p, _)| *p == path)
        .map(|(_, b)| *b)
    {
        return Some(bbox);
    }
    if let Some(rest) = path.strip_prefix("europe/") {
        let leaf = rest.split('/').next().unwrap_or(rest);
        if let Some(code) = geofabrik_leaf_to_iso(leaf) {
            return country_lookup(code);
        }
    }
    if let Some(rest) = path.strip_prefix("north-america/") {
        let leaf = rest.split('/').next().unwrap_or(rest);
        if leaf == "us" || leaf.starts_with("us/") {
            return country_lookup("us");
        }
    }
    if path == "russia" || path.starts_with("russia/") {
        return Some(RUSSIA_BBOX);
    }
    None
}

fn geofabrik_leaf_to_iso(leaf: &str) -> Option<&'static str> {
    match leaf {
        "norway" => Some("no"),
        "sweden" => Some("se"),
        "finland" => Some("fi"),
        "germany" => Some("de"),
        "switzerland" => Some("ch"),
        "austria" => Some("at"),
        "france" => Some("fr"),
        "great-britain" => Some("gb"),
        _ => None,
    }
}

/// Legacy helper: when `base` is a planet URL, return it unchanged; otherwise
/// append `/{region_key}.pmtiles` (old pre-cut hosting shape).
pub fn region_pmtiles_url(base: &str, region_key: &str) -> String {
    let base = base.trim_end_matches('/');
    if base.contains("build.protomaps.com") && base.ends_with(".pmtiles") {
        return base.to_string();
    }
    let key = sanitize_region_key(region_key);
    format!("{base}/{key}.pmtiles")
}

pub fn bbox_covers_point(bbox: [f64; 4], lat: f64, lon: f64) -> bool {
    lat >= bbox[0] && lat <= bbox[2] && lon >= bbox[1] && lon <= bbox[3]
}

const NORWAY_LANDSDEL: &[(&str, [f64; 4])] = &[
    ("europe/norway", [57.9, 4.5, 71.5, 31.5]),
    ("europe/norway/ostlandet", [58.5, 7.5, 62.8, 13.5]),
    ("europe/norway/vestlandet", [58.0, 4.0, 63.5, 8.5]),
    ("europe/norway/trondelag", [62.5, 8.5, 65.5, 14.5]),
    ("europe/norway/nord-norge", [64.5, 10.0, 71.5, 31.5]),
    ("europe/norway/sorlandet", [57.8, 5.5, 59.5, 10.0]),
    // Small Oslo window for e2e / instrumented basemap tests (fast extract).
    ("test/oslo", [59.85, 10.6, 59.98, 10.9]),
];

const RUSSIA_BBOX: [f64; 4] = [41.0, 19.0, 82.0, 180.0];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_key_from_geofabrik_path() {
        assert_eq!(
            geofabrik_path_to_region_key("europe/norway/ostlandet"),
            "europe_norway_ostlandet"
        );
    }

    #[test]
    fn ostlandet_bbox_covers_oslo() {
        let bbox = region_bbox("europe/norway/ostlandet").unwrap();
        assert!(bbox_covers_point(bbox, 59.91, 10.75));
        assert!(!bbox_covers_point(bbox, 69.65, 18.96));
    }

    #[test]
    fn planet_url_passthrough() {
        let u = "https://build.protomaps.com/20260722.pmtiles";
        assert_eq!(region_pmtiles_url(u, "europe_norway_ostlandet"), u);
    }
}
