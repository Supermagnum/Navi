//! OSM toll tag resolution per routing profile.
//!
//! Mode-specific keys (`toll:motor_vehicle`, `toll:bicycle`, …) override the
//! generic `toll=*` when present. See https://wiki.openstreetmap.org/wiki/Key:toll

use crate::routing::graph::RoutingProfile;

/// Keys that apply to car / motorcycle-class routing.
const CAR_TOLL_KEYS: &[&str] = &["toll:motor_vehicle", "toll:motorcar", "toll:motorcycle"];

/// Truck / HGV also honour `toll:hgv`.
const TRUCK_TOLL_KEYS: &[&str] = &[
    "toll:motor_vehicle",
    "toll:motorcar",
    "toll:motorcycle",
    "toll:hgv",
];

const BICYCLE_TOLL_KEYS: &[&str] = &["toll:bicycle"];
const FOOT_TOLL_KEYS: &[&str] = &["toll:foot"];

fn is_truthy_toll(raw: &str) -> bool {
    matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "yes" | "true" | "1" | "toll"
    )
}

fn mode_toll_keys(profile: RoutingProfile) -> &'static [&'static str] {
    match profile {
        RoutingProfile::Car => CAR_TOLL_KEYS,
        RoutingProfile::Truck => TRUCK_TOLL_KEYS,
        RoutingProfile::Bicycle => BICYCLE_TOLL_KEYS,
        RoutingProfile::Foot => FOOT_TOLL_KEYS,
    }
}

/// Whether this way is a toll road for `profile` (for avoid-toll filtering).
///
/// If any mode-specific key for the profile is set, those values decide (OR of
/// truthy specifics). Otherwise fall back to generic `toll`.
pub fn toll_applies_for_profile<'a>(
    profile: RoutingProfile,
    get: impl Fn(&str) -> Option<&'a str>,
) -> bool {
    let mut any_specific = false;
    let mut any_yes = false;
    for key in mode_toll_keys(profile) {
        if let Some(v) = get(key) {
            any_specific = true;
            if is_truthy_toll(v) {
                any_yes = true;
            }
        }
    }
    if any_specific {
        return any_yes;
    }
    get("toll").map(is_truthy_toll).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn tags(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).into(), (*v).into()))
            .collect()
    }

    fn applies(profile: RoutingProfile, map: &HashMap<String, String>) -> bool {
        toll_applies_for_profile(profile, |k| map.get(k).map(String::as_str))
    }

    #[test]
    fn motor_vehicle_only_tolls_car_not_bike_or_foot() {
        let t = tags(&[("toll:motor_vehicle", "yes")]);
        assert!(applies(RoutingProfile::Car, &t));
        assert!(applies(RoutingProfile::Truck, &t));
        assert!(!applies(RoutingProfile::Bicycle, &t));
        assert!(!applies(RoutingProfile::Foot, &t));
    }

    #[test]
    fn generic_toll_with_bicycle_no_exempts_bike() {
        let t = tags(&[("toll", "yes"), ("toll:bicycle", "no")]);
        assert!(applies(RoutingProfile::Car, &t));
        assert!(!applies(RoutingProfile::Bicycle, &t));
        assert!(applies(RoutingProfile::Foot, &t));
    }

    #[test]
    fn bicycle_only_toll() {
        let t = tags(&[("toll:bicycle", "yes")]);
        assert!(!applies(RoutingProfile::Car, &t));
        assert!(applies(RoutingProfile::Bicycle, &t));
        assert!(!applies(RoutingProfile::Foot, &t));
    }

    #[test]
    fn foot_override_off_generic_toll() {
        let t = tags(&[("toll", "yes"), ("toll:foot", "no")]);
        assert!(applies(RoutingProfile::Car, &t));
        assert!(!applies(RoutingProfile::Foot, &t));
    }

    #[test]
    fn hgv_specific_for_truck() {
        let t = tags(&[("toll:hgv", "yes")]);
        assert!(!applies(RoutingProfile::Car, &t));
        assert!(applies(RoutingProfile::Truck, &t));
    }
}
