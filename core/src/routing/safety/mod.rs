//! Overnight safety filters and dangerous linear barriers for break access.

mod barriers;
mod overnight;

pub use barriers::DangerBarrierIndex;
pub use overnight::OvernightProximityIndex;

use geo::{Distance, Haversine, Point};

use crate::config::SafetyConfig;
use crate::poi::{PoiCategory, PoiRecord};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OvernightRejectReason {
    TooCloseToBuilding,
    TooCloseToGlacier,
}

impl OvernightRejectReason {
    /// User-facing exclusion text for plan cards / pause labels.
    pub fn user_message(self) -> &'static str {
        match self {
            Self::TooCloseToBuilding => "Excluded: too close to a building",
            Self::TooCloseToGlacier => "Excluded: within 1 km of a glacier",
        }
    }
}

/// Returns `None` when the candidate is acceptable for overnight camping.
///
/// Glacier distance is measured to the nearest ring **edge** (or zero when the
/// candidate sits inside a glacier polygon) — not to the ring centroid.
pub fn check_overnight_candidate(
    candidate_lat: f64,
    candidate_lon: f64,
    safety: &SafetyConfig,
    poi: &PoiRecord,
    building_coords: &[(f64, f64)],
    glacier_rings: &[Vec<[f64; 2]>],
) -> Option<OvernightRejectReason> {
    let is_established = poi.categories.contains(&PoiCategory::OvernightFacility)
        || poi.categories.contains(&PoiCategory::Cabin)
        || poi.categories.contains(&PoiCategory::NetworkHut);

    for &(blat, blon) in building_coords {
        if distance_m(candidate_lat, candidate_lon, blat, blon) < safety.min_building_distance_m {
            return Some(OvernightRejectReason::TooCloseToBuilding);
        }
    }

    if !is_established {
        if let Some(d) =
            min_distance_to_glacier_rings_m(candidate_lat, candidate_lon, glacier_rings)
        {
            if d < safety.min_glacier_distance_m {
                return Some(OvernightRejectReason::TooCloseToGlacier);
            }
        }
    }

    None
}

/// Minimum metres to any glacier ring edge; `0` when inside a ring. `None` if no rings.
pub fn min_distance_to_glacier_rings_m(lat: f64, lon: f64, rings: &[Vec<[f64; 2]>]) -> Option<f64> {
    if rings.is_empty() {
        return None;
    }
    let mut min = f64::INFINITY;
    for ring in rings {
        if barriers::point_in_glacier_ring([lon, lat], ring) {
            return Some(0.0);
        }
        for w in ring.windows(2) {
            min = min.min(point_to_segment_m(lat, lon, w[0], w[1]));
        }
    }
    Some(min)
}

fn distance_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    Haversine::distance(Point::new(lon1, lat1), Point::new(lon2, lat2))
}

/// Local equirectangular metres from `(lat,lon)` to segment `a`–`b` (`[lon,lat]`).
fn point_to_segment_m(lat: f64, lon: f64, a: [f64; 2], b: [f64; 2]) -> f64 {
    let lat0 = lat.to_radians();
    let m_per_deg_lat = 111_320.0_f64;
    let m_per_deg_lon = 111_320.0 * lat0.cos();
    let px = lon * m_per_deg_lon;
    let py = lat * m_per_deg_lat;
    let ax = a[0] * m_per_deg_lon;
    let ay = a[1] * m_per_deg_lat;
    let bx = b[0] * m_per_deg_lon;
    let by = b[1] * m_per_deg_lat;
    let dx = bx - ax;
    let dy = by - ay;
    let len2 = dx * dx + dy * dy;
    let t = if len2 < 1e-6 {
        0.0
    } else {
        (((px - ax) * dx + (py - ay) * dy) / len2).clamp(0.0, 1.0)
    };
    let qx = ax + t * dx;
    let qy = ay + t * dy;
    (px - qx).hypot(py - qy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn hut_poi() -> PoiRecord {
        PoiRecord {
            osm_id: 1,
            lat: 60.0,
            lon: 10.0,
            categories: vec![PoiCategory::OvernightFacility, PoiCategory::Cabin],
            icon_key: "tourism-alpine_hut".into(),
            tags: HashMap::new(),
            name: Some("Hut".into()),
        }
    }

    fn tent_poi() -> PoiRecord {
        PoiRecord {
            osm_id: 2,
            lat: 60.0,
            lon: 10.0,
            categories: vec![PoiCategory::TentSite],
            icon_key: "tourism-camp_site".into(),
            tags: HashMap::new(),
            name: Some("Camp".into()),
        }
    }

    /// Tiny closed ring around `(lat, lon)` (~110 m half-side).
    fn tiny_ring(lat: f64, lon: f64) -> Vec<[f64; 2]> {
        let d = 0.001;
        vec![
            [lon - d, lat - d],
            [lon + d, lat - d],
            [lon + d, lat + d],
            [lon - d, lat + d],
            [lon - d, lat - d],
        ]
    }

    #[test]
    fn glacier_override_for_hut() {
        let safety = SafetyConfig::default();
        let poi = hut_poi();
        let glaciers = vec![tiny_ring(60.0005, 10.0005)];
        assert!(check_overnight_candidate(60.0, 10.0, &safety, &poi, &[], &glaciers).is_none());
    }

    #[test]
    fn building_rejection() {
        let safety = SafetyConfig::default();
        let poi = hut_poi();
        let buildings = vec![(60.0001, 10.0001)];
        assert_eq!(
            check_overnight_candidate(60.0, 10.0, &safety, &poi, &buildings, &[]),
            Some(OvernightRejectReason::TooCloseToBuilding)
        );
        assert_eq!(
            OvernightRejectReason::TooCloseToBuilding.user_message(),
            "Excluded: too close to a building"
        );
    }

    #[test]
    fn tent_rejected_near_glacier() {
        let safety = SafetyConfig::default();
        let poi = tent_poi();
        let glaciers = vec![tiny_ring(60.0005, 10.0005)];
        assert_eq!(
            check_overnight_candidate(60.0, 10.0, &safety, &poi, &[], &glaciers),
            Some(OvernightRejectReason::TooCloseToGlacier)
        );
        assert_eq!(
            OvernightRejectReason::TooCloseToGlacier.user_message(),
            "Excluded: within 1 km of a glacier"
        );
    }

    #[test]
    fn edge_distance_not_centroid() {
        // Long thin glacier: centroid far north; candidate south of south edge.
        // Centroid check would under-exclude; edge check must exclude within 1 km.
        let ring = vec![
            [10.0, 60.02],
            [10.01, 60.02],
            [10.01, 60.00],
            [10.0, 60.00],
            [10.0, 60.02],
        ];
        let south_edge = 60.00;
        // ~500 m south of south edge
        let candidate_lat = south_edge - (500.0 / 111_320.0);
        let candidate_lon = 10.005;
        let d = min_distance_to_glacier_rings_m(
            candidate_lat,
            candidate_lon,
            std::slice::from_ref(&ring),
        )
        .expect("dist");
        assert!(
            (400.0..700.0).contains(&d),
            "expected ~500 m to south edge, got {d}"
        );
        let centroid_lat = 60.01;
        let centroid_d = distance_m(candidate_lat, candidate_lon, centroid_lat, 10.005);
        assert!(
            centroid_d > 1_000.0,
            "centroid distance {centroid_d} should exceed 1 km (would wrongly allow)"
        );
        let poi = tent_poi();
        let safety = SafetyConfig::default();
        assert_eq!(
            check_overnight_candidate(candidate_lat, candidate_lon, &safety, &poi, &[], &[ring]),
            Some(OvernightRejectReason::TooCloseToGlacier)
        );
    }
}
