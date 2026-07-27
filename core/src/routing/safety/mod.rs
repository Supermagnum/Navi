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

/// Returns `None` when the candidate is acceptable for overnight camping.
pub fn check_overnight_candidate(
    candidate_lat: f64,
    candidate_lon: f64,
    safety: &SafetyConfig,
    poi: &PoiRecord,
    building_coords: &[(f64, f64)],
    glacier_coords: &[(f64, f64)],
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
        for &(glat, glon) in glacier_coords {
            if distance_m(candidate_lat, candidate_lon, glat, glon) < safety.min_glacier_distance_m
            {
                return Some(OvernightRejectReason::TooCloseToGlacier);
            }
        }
    }

    None
}

fn distance_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    Haversine::distance(Point::new(lon1, lat1), Point::new(lon2, lat2))
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

    #[test]
    fn glacier_override_for_hut() {
        let safety = SafetyConfig::default();
        let poi = hut_poi();
        let glaciers = vec![(60.0005, 10.0005)];
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
    }

    #[test]
    fn tent_rejected_near_glacier() {
        let safety = SafetyConfig::default();
        let poi = tent_poi();
        let glaciers = vec![(60.0005, 10.0005)];
        assert_eq!(
            check_overnight_candidate(60.0, 10.0, &safety, &poi, &[], &glaciers),
            Some(OvernightRejectReason::TooCloseToGlacier)
        );
    }
}
