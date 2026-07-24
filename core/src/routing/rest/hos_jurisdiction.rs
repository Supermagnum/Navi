//! Offline bbox-based driving-hours pack resolution (interim admin detection).

use crate::config::JurisdictionDrivingHoursPack;
use crate::routing::elevation::country_lookup;

/// ISO codes treated as EC 561 / EEA-aligned for bbox detection.
const EC561_FAMILY: &[&str] = &["no", "se", "fi", "de", "ch", "at", "fr", "gb"];

fn point_in_bbox(lat: f64, lon: f64, bbox: [f64; 4]) -> bool {
    lat >= bbox[0] && lat <= bbox[2] && lon >= bbox[1] && lon <= bbox[3]
}

/// Resolve a driving-hours pack from a corridor start (or GPS) position.
///
/// Uses offline country bboxes already shipped for elevation jobs. Unmatched
/// coordinates → [`JurisdictionDrivingHoursPack::Unknown`] (decline-by-default).
pub fn resolve_driving_hours_pack_at(lat: f64, lon: f64) -> JurisdictionDrivingHoursPack {
    if let Some(bbox) = country_lookup("us") {
        if point_in_bbox(lat, lon, bbox) {
            return JurisdictionDrivingHoursPack::Fmcsa;
        }
    }
    for code in EC561_FAMILY {
        if let Some(bbox) = country_lookup(code) {
            if point_in_bbox(lat, lon, bbox) {
                return JurisdictionDrivingHoursPack::Ec561;
            }
        }
    }
    JurisdictionDrivingHoursPack::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn norway_point_resolves_ec561() {
        assert_eq!(
            resolve_driving_hours_pack_at(59.91, 10.75),
            JurisdictionDrivingHoursPack::Ec561
        );
    }

    #[test]
    fn kansas_point_resolves_fmcsa() {
        assert_eq!(
            resolve_driving_hours_pack_at(39.0, -98.0),
            JurisdictionDrivingHoursPack::Fmcsa
        );
    }

    #[test]
    fn mid_atlantic_declines_unknown() {
        assert_eq!(
            resolve_driving_hours_pack_at(35.0, -40.0),
            JurisdictionDrivingHoursPack::Unknown
        );
    }
}
