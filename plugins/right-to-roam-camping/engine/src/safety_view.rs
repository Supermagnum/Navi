//! Overnight distance checks without linking `driver-break-core` (wasm guest).

use serde::{Deserialize, Serialize};

/// Subset of SafetyConfig the camping engine needs.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct OvernightSafety {
    pub min_building_distance_m: f64,
    pub min_glacier_distance_m: f64,
}

impl Default for OvernightSafety {
    fn default() -> Self {
        Self {
            min_building_distance_m: 150.0,
            min_glacier_distance_m: 1_000.0,
        }
    }
}

#[cfg(feature = "native")]
impl From<&driver_break_core::config::SafetyConfig> for OvernightSafety {
    fn from(s: &driver_break_core::config::SafetyConfig) -> Self {
        Self {
            min_building_distance_m: s.min_building_distance_m,
            min_glacier_distance_m: s.min_glacier_distance_m,
        }
    }
}

fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const R: f64 = 6_371_000.0;
    let p1 = lat1.to_radians();
    let p2 = lat2.to_radians();
    let dp = (lat2 - lat1).to_radians();
    let dl = (lon2 - lon1).to_radians();
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * R * a.sqrt().asin()
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

fn point_to_segment_m(lat: f64, lon: f64, a: [f64; 2], b: [f64; 2]) -> f64 {
    // a/b are [lon, lat]
    let (lat0, lon0) = (a[1], a[0]);
    let (lat1, lon1) = (b[1], b[0]);
    let dx = lon1 - lon0;
    let dy = lat1 - lat0;
    if dx.abs() < f64::EPSILON && dy.abs() < f64::EPSILON {
        return haversine_m(lat, lon, lat0, lon0);
    }
    let t = ((lon - lon0) * dx + (lat - lat0) * dy) / (dx * dx + dy * dy);
    let t = t.clamp(0.0, 1.0);
    haversine_m(lat, lon, lat0 + t * dy, lon0 + t * dx)
}

fn min_glacier_m(lat: f64, lon: f64, rings: &[Vec<[f64; 2]>]) -> Option<f64> {
    if rings.is_empty() {
        return None;
    }
    let mut min = f64::INFINITY;
    for ring in rings {
        if point_in_ring([lon, lat], ring) {
            return Some(0.0);
        }
        for w in ring.windows(2) {
            min = min.min(point_to_segment_m(lat, lon, w[0], w[1]));
        }
    }
    Some(min)
}

/// Wild-camp overnight hard filters. Returns reject reason slug or `None`.
pub fn wild_overnight_reject(
    lat: f64,
    lon: f64,
    safety: &OvernightSafety,
    buildings: &[(f64, f64)],
    glacier_rings: &[Vec<[f64; 2]>],
) -> Option<&'static str> {
    for &(blat, blon) in buildings {
        if haversine_m(lat, lon, blat, blon) < safety.min_building_distance_m {
            return Some("too_close_to_building");
        }
    }
    if let Some(d) = min_glacier_m(lat, lon, glacier_rings) {
        if d < safety.min_glacier_distance_m {
            return Some("too_close_to_glacier");
        }
    }
    None
}
