//! Ordered catalog regions needed along a caller-supplied corridor.
//!
//! The corridor polyline comes from the caller (overview route, road skeleton,
//! or routed path). This module does **not** invent a straight origin→destination
//! chord — that misses Sweden on Klecken→Innlandet (Skagerrak).

use std::collections::BTreeSet;

use crate::pack_server::acquisition::{
    normalize_region_id, region_ids_match_for_catalog, resolve_area_to_catalog,
};
use crate::routing::basemap::{bbox_covers_point, region_bbox, suggest_geofabrik_path_for_point};

/// Geometry for one catalog region. Prefer [`Self::Polygon`] when available;
/// [`Self::Bbox`] is what `current.json` hosts expose today (ids only → local
/// bbox table).
#[derive(Debug, Clone)]
pub enum CatalogRegionGeom {
    /// `[min_lat, min_lon, max_lat, max_lon]`.
    Bbox([f64; 4]),
    /// Exterior ring as `(lat, lon)` vertices (closed or open).
    Polygon(Vec<(f64, f64)>),
}

impl CatalogRegionGeom {
    fn covers(&self, lat: f64, lon: f64) -> bool {
        match self {
            Self::Bbox(b) => bbox_covers_point(*b, lat, lon),
            Self::Polygon(ring) => point_in_ring(ring, lat, lon),
        }
    }

    fn area_deg2(&self) -> f64 {
        match self {
            Self::Bbox(b) => (b[2] - b[0]).max(0.0) * (b[3] - b[1]).max(0.0),
            Self::Polygon(ring) => polygon_area_deg2(ring),
        }
    }
}

/// One published (or synthetic) catalog region with geometry for corridor PIP.
#[derive(Debug, Clone)]
pub struct CatalogRegionEntry {
    pub region_id: String,
    pub geom: CatalogRegionGeom,
}

/// Build catalog entries for `ready_ids` using local bbox tables.
///
/// Ids without a known bbox are skipped (caller can supply polygons explicitly).
pub fn catalog_entries_from_ready_ids(ready_ids: &[String]) -> Vec<CatalogRegionEntry> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for raw in ready_ids {
        let id = normalize_region_id(raw);
        if id.is_empty() || !seen.insert(id.clone()) {
            continue;
        }
        let Some(bbox) = region_bbox(&id) else {
            continue;
        };
        out.push(CatalogRegionEntry {
            region_id: id,
            geom: CatalogRegionGeom::Bbox(bbox),
        });
    }
    out
}

/// Densify corridor waypoints and return ordered unique catalog regions first
/// crossed along the route, excluding `installed` (and their catalog aliases /
/// covering parents via [`region_ids_match_for_catalog`] /
/// [`crate::pack_server::path_covered_by_ready_ids`] semantics for exact id).
///
/// When several catalog geometries cover a sample, the tightest area wins;
/// path depth breaks ties. A coarse country hint from
/// [`suggest_geofabrik_path_for_point`] prefers candidates under that country
/// tree so oversized boxes (e.g. Ostlandet vs Sweden) do not steal Göteborg.
///
/// Remaining bbox false-positive risk (report): German state boxes still
/// overlap near Hamburg/Niedersachsen; Danish region boxes overlap at the
/// Storebælt approaches; Ostlandet’s east edge still overlaps western Sweden
/// when the point suggester has no country hint (open ocean / Skagerrak).
pub fn ordered_regions_along_corridor(
    waypoints: &[(f64, f64)],
    catalog: &[CatalogRegionEntry],
    installed: &[String],
    sample_step_km: f64,
) -> Vec<String> {
    let samples = densify_waypoints(waypoints, sample_step_km);
    let ready_ids: Vec<String> = catalog.iter().map(|c| c.region_id.clone()).collect();
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for &(lat, lon) in &samples {
        let Some(raw) = finest_covering(catalog, lat, lon) else {
            continue;
        };
        let Some(id) = resolve_area_to_catalog(&raw, &ready_ids) else {
            continue;
        };
        if is_installed(&id, installed) {
            continue;
        }
        if seen.insert(id.clone()) {
            out.push(id);
        }
    }
    out
}

fn is_installed(id: &str, installed: &[String]) -> bool {
    installed
        .iter()
        .any(|inst| region_ids_match_for_catalog(inst, id))
}

fn densify_waypoints(waypoints: &[(f64, f64)], step_km: f64) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    if waypoints.is_empty() {
        return out;
    }
    out.push(waypoints[0]);
    for w in waypoints.windows(2) {
        let a = w[0];
        let b = w[1];
        let d = haversine_km(a, b);
        let n = (d / step_km.max(1e-6)).ceil() as i32;
        for i in 1..=n.max(1) {
            let t = i as f64 / n.max(1) as f64;
            out.push((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
        }
    }
    out
}

fn finest_covering(catalog: &[CatalogRegionEntry], lat: f64, lon: f64) -> Option<String> {
    let mut cands: Vec<&CatalogRegionEntry> =
        catalog.iter().filter(|c| c.geom.covers(lat, lon)).collect();
    if cands.is_empty() {
        return None;
    }
    // Country-scale Denmark / Germany boxes spill into neighbours. Prefer the
    // tightest covering region; only apply Norway↔Sweden disambiguation when
    // both trees cover the sample (Ostlandet bbox vs Sweden län).
    let has_no = cands
        .iter()
        .any(|c| c.region_id.starts_with("europe/norway"));
    let has_se = cands
        .iter()
        .any(|c| c.region_id.starts_with("europe/sweden"));
    if has_no && has_se {
        if let Some(hint) = suggest_geofabrik_path_for_point(lat, lon) {
            let prefer_se = hint.starts_with("europe/sweden");
            let prefer_no = hint.starts_with("europe/norway");
            if prefer_se || prefer_no {
                let filtered: Vec<&CatalogRegionEntry> = cands
                    .iter()
                    .copied()
                    .filter(|c| {
                        if prefer_se {
                            c.region_id.starts_with("europe/sweden")
                        } else {
                            c.region_id.starts_with("europe/norway")
                        }
                    })
                    .collect();
                if !filtered.is_empty() {
                    cands = filtered;
                }
            }
        }
    }
    cands.sort_by(|a, b| {
        let aa = a.geom.area_deg2();
        let ba = b.geom.area_deg2();
        aa.partial_cmp(&ba)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| path_depth(&b.region_id).cmp(&path_depth(&a.region_id)))
            .then_with(|| a.region_id.cmp(&b.region_id))
    });
    cands.first().map(|c| c.region_id.clone())
}

fn path_depth(id: &str) -> usize {
    normalize_region_id(id).matches('/').count()
}

fn haversine_km(a: (f64, f64), b: (f64, f64)) -> f64 {
    let r = 6371.0;
    let dlat = (b.0 - a.0).to_radians();
    let dlon = (b.1 - a.1).to_radians();
    let x = (dlat / 2.0).sin().powi(2)
        + a.0.to_radians().cos() * b.0.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    2.0 * r * x.sqrt().asin()
}

fn point_in_ring(ring: &[(f64, f64)], lat: f64, lon: f64) -> bool {
    if ring.len() < 3 {
        return false;
    }
    // Ray cast in lon/lat degrees (adequate for small regional polygons).
    let mut inside = false;
    let mut j = ring.len() - 1;
    for i in 0..ring.len() {
        let (yi, xi) = ring[i];
        let (yj, xj) = ring[j];
        let intersect = ((yi > lat) != (yj > lat))
            && (lon < (xj - xi) * (lat - yi) / (yj - yi + f64::EPSILON) + xi);
        if intersect {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn polygon_area_deg2(ring: &[(f64, f64)]) -> f64 {
    if ring.len() < 3 {
        return f64::MAX;
    }
    let mut sum = 0.0;
    for w in ring.windows(2) {
        sum += w[0].1 * w[1].0 - w[1].1 * w[0].0;
    }
    let (a, b) = (ring.last().unwrap(), ring.first().unwrap());
    sum += a.1 * b.0 - b.1 * a.0;
    (sum.abs() * 0.5).max(1e-12)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_road_waypoints() -> Vec<(f64, f64)> {
        vec![
            (53.3340, 10.0450),
            (53.5510, 10.0000),
            (54.7830, 9.4330),
            (55.4900, 9.4700),
            (55.4000, 10.3900),
            (55.3500, 11.1300),
            (55.4100, 11.3800),
            (55.6760, 12.5680),
            (55.5700, 12.8500),
            (55.6050, 13.0000),
            (56.0500, 12.7000),
            (56.6700, 12.8600),
            (57.7100, 11.9700),
            (59.0900, 11.2500),
            (59.9100, 10.7500),
            (61.5929077, 10.3318551),
        ]
    }

    fn current_style_catalog() -> Vec<CatalogRegionEntry> {
        catalog_entries_from_ready_ids(&[
            "europe/germany/niedersachsen".into(),
            "europe/germany/hamburg".into(),
            "europe/germany/schleswig-holstein".into(),
            "europe/denmark".into(),
            "europe/sweden/skane".into(),
            "europe/sweden/halland".into(),
            "europe/sweden/vastra_gotaland".into(),
            "europe/norway/ostlandet".into(),
        ])
    }

    fn leaf_denmark_catalog() -> Vec<CatalogRegionEntry> {
        catalog_entries_from_ready_ids(&[
            "europe/germany/niedersachsen".into(),
            "europe/germany/hamburg".into(),
            "europe/germany/schleswig-holstein".into(),
            "europe/denmark/syddanmark".into(),
            "europe/denmark/sjaelland".into(),
            "europe/denmark/hovedstaden".into(),
            "europe/sweden/skane".into(),
            "europe/sweden/halland".into(),
            "europe/sweden/vastra_gotaland".into(),
            "europe/norway/ostlandet".into(),
        ])
    }

    #[test]
    fn all_road_current_catalog_lists_seven_after_niedersachsen() {
        let installed = vec!["europe/germany/niedersachsen".into()];
        let got = ordered_regions_along_corridor(
            &all_road_waypoints(),
            &current_style_catalog(),
            &installed,
            25.0,
        );
        assert_eq!(
            got,
            vec![
                "europe/germany/hamburg".to_string(),
                "europe/germany/schleswig-holstein".to_string(),
                "europe/denmark".to_string(),
                "europe/sweden/skane".to_string(),
                "europe/sweden/halland".to_string(),
                "europe/sweden/vastra_gotaland".to_string(),
                "europe/norway/ostlandet".to_string(),
            ],
            "got={got:?}"
        );
    }

    #[test]
    fn all_road_with_danish_leaves_prefers_leaves() {
        let installed = vec!["europe/germany/niedersachsen".into()];
        let got = ordered_regions_along_corridor(
            &all_road_waypoints(),
            &leaf_denmark_catalog(),
            &installed,
            25.0,
        );
        assert_eq!(
            got,
            vec![
                "europe/germany/hamburg".to_string(),
                "europe/germany/schleswig-holstein".to_string(),
                "europe/denmark/syddanmark".to_string(),
                "europe/denmark/sjaelland".to_string(),
                "europe/denmark/hovedstaden".to_string(),
                "europe/sweden/skane".to_string(),
                "europe/sweden/halland".to_string(),
                "europe/sweden/vastra_gotaland".to_string(),
                "europe/norway/ostlandet".to_string(),
            ],
            "got={got:?}"
        );
    }

    #[test]
    fn straight_klecken_to_dest_misses_sweden() {
        // Document why the caller must supply a real corridor: the chord crosses
        // the Skagerrak and never enters Skåne / Halland / Västra Götaland.
        let chord = [(53.3340, 10.0450), (61.5929077, 10.3318551)];
        let got = ordered_regions_along_corridor(
            &chord,
            &current_style_catalog(),
            &["europe/germany/niedersachsen".into()],
            25.0,
        );
        assert!(
            !got.iter()
                .any(|r| r.contains("skane") || r.contains("halland")),
            "straight chord should miss Sweden län, got {got:?}"
        );
    }
}
