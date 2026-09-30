//! ISO 3166-2 subdivision lookup for Norway (fylke).
//!
//! Root cause of empty `subdivision_iso`: `admin_region_at` had no subdivision
//! layer. The Ostlandet e2e `*.osm.pbf` is a stub (16 KiB zeros), so fylke cannot
//! be extracted from the regional pack. Instead we ship a compact Natural Earth
//! Admin-1 (10m) Norway asset with codes remapped to **current** ISO 3166-2:
//! Hedmark/Oppland → `no-34` Innlandet; Troms → `no-55`; Finnmark → `no-56`;
//! Nordland stays `no-18`. Natural Earth predates the 2024 county reform and
//! never carried the 2020–2023 merged `no-54` (Troms og Finnmark).
//!
//! **Coarse / informational only:** coastal cities often miss the low-res
//! polygons; municipality (kommune) is not available. Callers may use this
//! layer to gate informational notes (e.g. cloudberry), never as a hard
//! reject filter for overnight suggestions.

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
    fn current_iso_codes_tromso_alta_bodo_trondheim() {
        // NE Admin-1 is coarse: city centres on the coast often miss polygons.
        // Use the nearest inland hit beside each city (documented above).
        let (troms_iso, troms_name) = lookup(69.5992, 18.9953).expect("Tromsø hinterland");
        assert_eq!(troms_iso, "no-55", "Troms must be current ISO, not no-19/no-54");
        assert!(troms_name.to_lowercase().contains("troms"));

        let (finn_iso, finn_name) = lookup(69.9689, 23.2717).expect("Alta");
        assert_eq!(finn_iso, "no-56", "Finnmark must be current ISO, not no-20/no-54");
        assert!(finn_name.to_lowercase().contains("finnmark"));

        let (nord_iso, _) = lookup(67.2704, 14.4149).expect("Bodø hinterland");
        assert_eq!(nord_iso, "no-18");

        let (trond_iso, trond_name) = lookup(63.4305, 10.3951).expect("Trondheim");
        assert_eq!(trond_iso, "no-50");
        assert!(trond_name.to_lowercase().contains("trøndelag") || trond_name.to_lowercase().contains("trondelag"));
    }

    #[test]
    fn never_emits_pre_2024_northern_codes() {
        for &(lat, lon) in &[
            (69.5992, 18.9953),
            (68.9053, 19.0835),
            (69.9689, 23.2717),
            (70.0, 25.0),
            (66.7347, 14.7203),
        ] {
            if let Some(iso) = subdivision_iso_at(lat, lon) {
                assert!(
                    !matches!(iso.as_str(), "no-19" | "no-20" | "no-54"),
                    "legacy/merged code {iso} at {lat},{lon}"
                );
            }
        }
    }
}
