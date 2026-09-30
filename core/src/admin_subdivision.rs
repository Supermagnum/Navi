//! ISO 3166-2 subdivision lookup for Norway (fylke).
//!
//! Root cause of empty `subdivision_iso`: `admin_region_at` had no subdivision
//! layer. The Ostlandet e2e `*.osm.pbf` is a stub (16 KiB zeros), so fylke cannot
//! be extracted from the regional pack. Instead we ship a compact Natural Earth
//! Admin-1 (10m) Norway asset (legacy fylke codes mapped to current ISO where
//! needed, e.g. Hedmark/Oppland → `no-34` Innlandet).

use std::sync::OnceLock;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
struct FylkeFeature {
    iso: String,
    name: String,
    /// Outer rings as `[lon, lat]`.
    rings: Vec<Vec<[f64; 2]>>,
}

#[derive(Debug, Clone)]
struct FylkeRing {
    iso: String,
    name: String,
    ring: Vec<[f64; 2]>,
    min_lon: f64,
    max_lon: f64,
    min_lat: f64,
    max_lat: f64,
}

struct SubdivisionIndex {
    rings: Vec<FylkeRing>,
}

static INDEX: OnceLock<SubdivisionIndex> = OnceLock::new();

const ASSET: &str = include_str!("admin_subdivision/data/norway_fylke_ne10m.json");

fn index() -> &'static SubdivisionIndex {
    INDEX.get_or_init(|| {
        let features: Vec<FylkeFeature> =
            serde_json::from_str(ASSET).expect("norway_fylke_ne10m.json");
        let mut rings = Vec::new();
        for f in features {
            for ring in f.rings {
                if ring.len() < 3 {
                    continue;
                }
                let (min_lon, max_lon, min_lat, max_lat) = ring_bounds(&ring);
                rings.push(FylkeRing {
                    iso: f.iso.clone(),
                    name: f.name.clone(),
                    ring,
                    min_lon,
                    max_lon,
                    min_lat,
                    max_lat,
                });
            }
        }
        SubdivisionIndex { rings }
    })
}

/// Number of baked fylke rings.
pub fn subdivision_ring_count() -> usize {
    index().rings.len()
}

/// Optional warm hook (no-op for the baked asset; kept for embedder API stability).
pub fn warm_subdivisions_from_pbf(_pbf: impl AsRef<std::path::Path>) -> anyhow::Result<usize> {
    Ok(subdivision_ring_count())
}

/// ISO 3166-2 for the containing Norwegian fylke, if any.
pub fn subdivision_iso_at(lat: f64, lon: f64) -> Option<String> {
    lookup(lat, lon).map(|(iso, _)| iso)
}

/// Display name for the containing fylke.
pub fn subdivision_name_at(lat: f64, lon: f64) -> Option<String> {
    lookup(lat, lon).map(|(_, name)| name)
}

fn lookup(lat: f64, lon: f64) -> Option<(String, String)> {
    if !lat.is_finite() || !lon.is_finite() {
        return None;
    }
    let idx = index();
    let p = [lon, lat];
    let mut best: Option<(&FylkeRing, f64)> = None;
    for r in &idx.rings {
        if lon < r.min_lon || lon > r.max_lon || lat < r.min_lat || lat > r.max_lat {
            continue;
        }
        if !point_in_ring(p, &r.ring) {
            continue;
        }
        let area = ring_area_abs(&r.ring);
        if best.map(|(_, a)| area < a).unwrap_or(true) {
            best = Some((r, area));
        }
    }
    best.map(|(r, _)| (r.iso.clone(), r.name.clone()))
}

fn ring_bounds(ring: &[[f64; 2]]) -> (f64, f64, f64, f64) {
    let mut min_lon = f64::INFINITY;
    let mut max_lon = f64::NEG_INFINITY;
    let mut min_lat = f64::INFINITY;
    let mut max_lat = f64::NEG_INFINITY;
    for p in ring {
        min_lon = min_lon.min(p[0]);
        max_lon = max_lon.max(p[0]);
        min_lat = min_lat.min(p[1]);
        max_lat = max_lat.max(p[1]);
    }
    (min_lon, max_lon, min_lat, max_lat)
}

fn ring_area_abs(ring: &[[f64; 2]]) -> f64 {
    if ring.len() < 3 {
        return f64::INFINITY;
    }
    let mut s = 0.0;
    for i in 0..ring.len() - 1 {
        s += ring[i][0] * ring[i + 1][1] - ring[i + 1][0] * ring[i][1];
    }
    (s * 0.5).abs()
}

fn point_in_ring(p: [f64; 2], ring: &[[f64; 2]]) -> bool {
    if ring.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = ring.len() - 1;
    for i in 0..ring.len() {
        let pi = ring[i];
        let pj = ring[j];
        let intersect = ((pi[1] > p[1]) != (pj[1] > p[1]))
            && (p[0] < (pj[0] - pi[0]) * (p[1] - pi[1]) / (pj[1] - pi[1] + f64::EPSILON) + pi[0]);
        if intersect {
            inside = !inside;
        }
        j = i;
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lillehammer_is_innlandet() {
        let (iso, name) = lookup(61.13796, 10.59777).expect("Lillehammer fylke");
        assert_eq!(iso, "no-34");
        assert!(name.to_lowercase().contains("innlandet"));
    }

    #[test]
    fn nordland_centroid_resolves_for_cloudberry() {
        // Natural Earth label point for Nordland (coastal cities can miss polygons).
        let (iso, _) = lookup(66.7347, 14.7203).expect("Nordland");
        assert_eq!(iso, "no-18");
    }

    #[test]
    fn troms_centroid_resolves_for_cloudberry() {
        let (iso, _) = lookup(68.9053, 19.0835).expect("Troms");
        assert_eq!(iso, "no-19");
    }
}
