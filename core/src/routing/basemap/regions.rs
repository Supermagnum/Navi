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

/// Area of a lat/lon bbox in square degrees (for picking the tightest landsdel).
fn bbox_area_deg2(bbox: [f64; 4]) -> f64 {
    (bbox[2] - bbox[0]).max(0.0) * (bbox[3] - bbox[1]).max(0.0)
}

/// Most-specific Geofabrik landsdel/country path whose approximate bbox covers
/// `(lat, lon)`. Prefers Norway landsdel extracts over `europe/norway`.
///
/// Returns `None` when no known table entry covers the point.
pub fn suggest_geofabrik_path_for_point(lat: f64, lon: f64) -> Option<&'static str> {
    let mut best: Option<(&'static str, f64)> = None;
    for (path, bbox) in NORWAY_LANDSDEL {
        if *path == "test/oslo" {
            continue;
        }
        if !bbox_covers_point(*bbox, lat, lon) {
            continue;
        }
        let area = bbox_area_deg2(*bbox);
        match best {
            None => best = Some((*path, area)),
            Some((_, best_area)) if area < best_area => best = Some((*path, area)),
            _ => {}
        }
    }
    best.map(|(p, _)| p)
}

/// Whether `(lat, lon)` falls inside any of the given Geofabrik path bboxes.
pub fn point_covered_by_regions(lat: f64, lon: f64, geofabrik_paths: &[&str]) -> bool {
    geofabrik_paths.iter().any(|path| {
        region_bbox(path)
            .map(|bbox| bbox_covers_point(bbox, lat, lon))
            .unwrap_or(false)
    })
}

/// Map a downloaded PBF leaf stem (`ostlandet-latest`) to a Geofabrik path.
pub fn pbf_stem_to_geofabrik_path(stem: &str) -> Option<String> {
    let leaf = stem
        .trim()
        .trim_end_matches(".osm.pbf")
        .trim_end_matches("-latest")
        .trim_end_matches("_latest")
        .to_ascii_lowercase();
    if leaf.is_empty() {
        return None;
    }
    match leaf.as_str() {
        "norway" => Some("europe/norway".into()),
        "ostlandet" | "oppland" => Some("europe/norway/ostlandet".into()),
        "vestlandet" => Some("europe/norway/vestlandet".into()),
        "trondelag" => Some("europe/norway/trondelag".into()),
        "nord-norge" | "nord_norge" => Some("europe/norway/nord-norge".into()),
        "sorlandet" => Some("europe/norway/sorlandet".into()),
        other => {
            // Already a full path with slashes replaced? Prefer known table hit.
            let as_path = other.replace('_', "/");
            if region_bbox(&as_path).is_some() {
                Some(as_path)
            } else if region_bbox(&format!("europe/norway/{other}")).is_some() {
                Some(format!("europe/norway/{other}"))
            } else {
                None
            }
        }
    }
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
    fn suggest_landsdel_prefers_specific_over_country() {
        assert_eq!(
            suggest_geofabrik_path_for_point(59.91, 10.75),
            Some("europe/norway/ostlandet")
        );
        assert_eq!(
            suggest_geofabrik_path_for_point(69.65, 18.96),
            Some("europe/norway/nord-norge")
        );
        // Bergen — Vestlandet only (Preikestolen sits in Vestlandet∩Sørlandet overlap).
        assert_eq!(
            suggest_geofabrik_path_for_point(60.3913, 5.3221),
            Some("europe/norway/vestlandet")
        );
        // Just west of Ostlandet min_lon 7.5.
        assert_eq!(
            suggest_geofabrik_path_for_point(60.4, 7.4),
            Some("europe/norway/vestlandet")
        );
    }

    #[test]
    fn coverage_uses_downloaded_paths_only() {
        let ost = ["europe/norway/ostlandet"];
        assert!(point_covered_by_regions(59.91, 10.75, &ost));
        assert!(!point_covered_by_regions(69.65, 18.96, &ost));
        assert!(point_covered_by_regions(
            69.65,
            18.96,
            &["europe/norway/ostlandet", "europe/norway/nord-norge"]
        ));
    }

    #[test]
    fn pbf_stem_maps_to_geofabrik_path() {
        assert_eq!(
            pbf_stem_to_geofabrik_path("ostlandet-latest"),
            Some("europe/norway/ostlandet".into())
        );
        assert_eq!(
            pbf_stem_to_geofabrik_path("oppland-latest.osm.pbf"),
            Some("europe/norway/ostlandet".into())
        );
        assert_eq!(
            pbf_stem_to_geofabrik_path("norway-latest"),
            Some("europe/norway".into())
        );
    }

    #[test]
    fn planet_url_passthrough() {
        let u = "https://build.protomaps.com/20260722.pmtiles";
        assert_eq!(region_pmtiles_url(u, "europe_norway_ostlandet"), u);
    }
}
