//! Fast "near the planned corridor?" pre-filter for overnight building loads.
//!
//! Used to drop buildings far from the route path before the exact 150 m
//! allemannsretten distance check. This is intentionally looser than
//! [`crate::config::SAFETY_MIN_BUILDING_DISTANCE_M`].

use geo::{Distance, Haversine, Point};
use rstar::RTree;

use crate::config::OVERNIGHT_BUILDING_CORRIDOR_MARGIN_M;

/// Default sample spacing along the corridor when building the index (metres).
const SAMPLE_STEP_M: f64 = 200.0;

/// Spatial index of a route corridor for generous proximity tests.
#[derive(Debug, Clone)]
pub struct CorridorBand {
    /// Sample points as `[lon, lat]`.
    tree: RTree<[f64; 2]>,
    /// Dense `(lat, lon)` vertices used for segment refinement near hits.
    vertices: Vec<(f64, f64)>,
    margin_m: f64,
    /// Axis-aligned envelope expanded by `margin_m` for cheap rejects.
    /// `[min_lat, min_lon, max_lat, max_lon]`.
    envelope: [f64; 4],
}

impl CorridorBand {
    /// Build from route vertices `(lat, lon)`. Empty input yields a band that
    /// never matches (caller should fall back to bbox-only behaviour).
    pub fn from_lat_lon(coords: &[(f64, f64)], margin_m: f64) -> Self {
        let margin_m = margin_m.max(1.0);
        let vertices = densify_lat_lon(coords, SAMPLE_STEP_M);
        let pts: Vec<[f64; 2]> = vertices.iter().map(|&(lat, lon)| [lon, lat]).collect();
        let envelope = envelope_for_vertices(&vertices, margin_m);
        Self {
            tree: RTree::bulk_load(pts),
            vertices,
            margin_m,
            envelope,
        }
    }

    /// Convenience: [`OVERNIGHT_BUILDING_CORRIDOR_MARGIN_M`].
    pub fn overnight_buildings(coords: &[(f64, f64)]) -> Self {
        Self::from_lat_lon(coords, OVERNIGHT_BUILDING_CORRIDOR_MARGIN_M)
    }

    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }

    pub fn margin_m(&self) -> f64 {
        self.margin_m
    }

    pub fn sample_count(&self) -> usize {
        self.vertices.len()
    }

    /// Expanded rectangular envelope of the corridor band.
    pub fn envelope(&self) -> [f64; 4] {
        self.envelope
    }

    /// Cheap reject against the expanded corridor envelope.
    pub fn in_envelope(&self, lat: f64, lon: f64) -> bool {
        let [min_lat, min_lon, max_lat, max_lon] = self.envelope;
        lat >= min_lat && lat <= max_lat && lon >= min_lon && lon <= max_lon
    }

    /// True when `(lat, lon)` is within `margin_m` of the corridor polyline.
    pub fn contains(&self, lat: f64, lon: f64) -> bool {
        if self.vertices.is_empty() {
            return false;
        }
        if !self.in_envelope(lat, lon) {
            return false;
        }
        // Cheap reject via nearest sample, then refine against adjacent segments.
        let Some(nn) = self.tree.nearest_neighbor(&[lon, lat]) else {
            return false;
        };
        let nn_lat = nn[1];
        let nn_lon = nn[0];
        let d_nn = Haversine::distance(Point::new(lon, lat), Point::new(nn_lon, nn_lat));
        // Samples are ~SAMPLE_STEP_M apart; anything farther than margin + step
        // from the nearest sample cannot be within margin of the polyline.
        if d_nn > self.margin_m + SAMPLE_STEP_M {
            return false;
        }
        if d_nn <= self.margin_m {
            return true;
        }
        // Boundary band: refine against a few segments around the nearest sample.
        let mut best = d_nn;
        let Some(idx) = self.vertices.iter().position(|&(vlat, vlon)| {
            (vlat - nn_lat).abs() < 1e-12 && (vlon - nn_lon).abs() < 1e-12
        }) else {
            return d_nn <= self.margin_m;
        };
        let lo = idx.saturating_sub(2);
        let hi = (idx + 3).min(self.vertices.len());
        for w in self.vertices[lo..hi].windows(2) {
            let d = dist_point_to_segment_m(lat, lon, w[0].0, w[0].1, w[1].0, w[1].1);
            if d < best {
                best = d;
                if best <= self.margin_m {
                    return true;
                }
            }
        }
        best <= self.margin_m
    }

    /// Keep only points inside the corridor band.
    pub fn filter_lat_lon(&self, points: &[(f64, f64)]) -> Vec<(f64, f64)> {
        points
            .iter()
            .copied()
            .filter(|&(lat, lon)| self.contains(lat, lon))
            .collect()
    }
}

/// Densify (and lightly thin) corridor vertices so samples are ~`step_m` apart.
///
/// Sparse waypoint polylines must be interpolated; otherwise nearest-sample
/// rejects falsely exclude points that sit mid-segment.
fn densify_lat_lon(coords: &[(f64, f64)], step_m: f64) -> Vec<(f64, f64)> {
    let step_m = step_m.max(1.0);
    let mut out = Vec::new();
    let Some(&first) = coords.first() else {
        return out;
    };
    out.push(first);
    for w in coords.windows(2) {
        let (a_lat, a_lon) = w[0];
        let (b_lat, b_lon) = w[1];
        let seg_m = Haversine::distance(Point::new(a_lon, a_lat), Point::new(b_lon, b_lat));
        if seg_m < 1.0 {
            continue;
        }
        let n = (seg_m / step_m).floor() as usize;
        for i in 1..=n {
            let t = (i as f64 * step_m) / seg_m;
            if t >= 1.0 {
                break;
            }
            out.push((a_lat + t * (b_lat - a_lat), a_lon + t * (b_lon - a_lon)));
        }
        if out.last() != Some(&w[1]) {
            out.push(w[1]);
        }
    }
    out
}

fn envelope_for_vertices(vertices: &[(f64, f64)], margin_m: f64) -> [f64; 4] {
    if vertices.is_empty() {
        return [0.0, 0.0, 0.0, 0.0];
    }
    let mut min_lat = f64::INFINITY;
    let mut max_lat = f64::NEG_INFINITY;
    let mut min_lon = f64::INFINITY;
    let mut max_lon = f64::NEG_INFINITY;
    let mut mid_lat = 0.0;
    for &(lat, lon) in vertices {
        min_lat = min_lat.min(lat);
        max_lat = max_lat.max(lat);
        min_lon = min_lon.min(lon);
        max_lon = max_lon.max(lon);
        mid_lat += lat;
    }
    mid_lat /= vertices.len() as f64;
    let d_lat = margin_m / 111_320.0;
    let d_lon = margin_m / (111_320.0 * mid_lat.to_radians().cos().max(0.2));
    [
        min_lat - d_lat,
        min_lon - d_lon,
        max_lat + d_lat,
        max_lon + d_lon,
    ]
}

/// Local ENU point-to-segment distance (metres). Kept here so `poi` does not
/// depend on `routing::graph`.
fn dist_point_to_segment_m(
    lat: f64,
    lon: f64,
    a_lat: f64,
    a_lon: f64,
    b_lat: f64,
    b_lon: f64,
) -> f64 {
    let a = Point::new(a_lon, a_lat);
    let b = Point::new(b_lon, b_lat);
    let ab = Haversine::distance(a, b);
    if ab < 1.0 {
        return Haversine::distance(Point::new(lon, lat), a);
    }
    let mid_lat = (a_lat + b_lat) / 2.0;
    let m_per_deg_lat = 111_320.0;
    let m_per_deg_lon = 111_320.0 * mid_lat.to_radians().cos();
    let ax = a_lon * m_per_deg_lon;
    let ay = a_lat * m_per_deg_lat;
    let bx = b_lon * m_per_deg_lon;
    let by = b_lat * m_per_deg_lat;
    let px = lon * m_per_deg_lon;
    let py = lat * m_per_deg_lat;
    let abx = bx - ax;
    let aby = by - ay;
    let t = ((px - ax) * abx + (py - ay) * aby) / (abx * abx + aby * aby);
    let t = t.clamp(0.0, 1.0);
    let qx = ax + t * abx;
    let qy = ay + t * aby;
    let dx = px - qx;
    let dy = py - qy;
    (dx * dx + dy * dy).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SAFETY_MIN_BUILDING_DISTANCE_M;

    /// ~111.32 m per 0.001° latitude.
    fn offset_north(lat: f64, lon: f64, metres: f64) -> (f64, f64) {
        (lat + metres / 111_320.0, lon)
    }

    #[test]
    fn keeps_building_within_exact_threshold() {
        let corridor = vec![(61.0, 10.0), (61.1, 10.0)];
        let band = CorridorBand::overnight_buildings(&corridor);
        let (lat, lon) = offset_north(61.05, 10.0, SAFETY_MIN_BUILDING_DISTANCE_M * 0.9);
        assert!(
            band.contains(lat, lon),
            "points inside the real 150 m threshold must survive the pre-filter"
        );
    }

    #[test]
    fn keeps_building_near_150m_boundary() {
        let corridor = vec![(61.0, 10.0), (61.1, 10.0)];
        let band = CorridorBand::overnight_buildings(&corridor);
        let (lat, lon) = offset_north(61.05, 10.0, SAFETY_MIN_BUILDING_DISTANCE_M);
        assert!(
            band.contains(lat, lon),
            "150 m boundary case must remain inside the {OVERNIGHT_BUILDING_CORRIDOR_MARGIN_M} m band"
        );
    }

    #[test]
    fn drops_building_far_from_corridor() {
        let corridor = vec![(61.0, 10.0), (61.1, 10.0)];
        let band = CorridorBand::overnight_buildings(&corridor);
        // ~5 km east of the N-S corridor — well outside a 1.5 km band.
        let lat: f64 = 61.05;
        let lon = 10.0 + 5_000.0 / (111_320.0 * lat.to_radians().cos());
        assert!(!band.contains(lat, lon));
    }

    #[test]
    fn filter_preserves_near_drops_far() {
        let corridor = vec![(61.0, 10.0), (61.2, 10.0)];
        let band = CorridorBand::from_lat_lon(&corridor, 1_500.0);
        let near = offset_north(61.1, 10.0, 100.0);
        // Offset east — north would still lie on the N–S corridor.
        let lat = 61.1_f64;
        let far = (lat, 10.0 + 5_000.0 / (111_320.0 * lat.to_radians().cos()));
        let kept = band.filter_lat_lon(&[near, far]);
        assert_eq!(kept, vec![near]);
    }
}
