//! OSM toll tag resolution and avoid-toll routing policy.
//!
//! Mode-specific keys (`toll:motor_vehicle`, `toll:bicycle`, …) override the
//! generic `toll=*` when present. See https://wiki.openstreetmap.org/wiki/Key:toll
//!
//! # `avoid_tolls` → [`TollPolicy`] migration
//!
//! The former `RouteOptions.avoid_tolls: bool` is replaced by [`TollPolicy`]:
//! - `false` → [`TollPolicy::Allow`] (toll edges at normal weight)
//! - `true` → [`TollPolicy::Penalize`] (large finite weight multiplier; default
//!   when the user enables “Avoid toll roads”)
//! - Strict absolute exclusion uses [`TollPolicy::NeverUse`] (hard filter), with
//!   adaptive bbox widen and a flagged last-resort path when no free route exists.
//!   **NeverUse is FFI / UniFFI only in this pass** — the Android “Avoid toll roads”
//!   toggle maps to Allow / Penalize only (see `docs/API.md`).
//!
//! Do not reintroduce `avoid_tolls` alongside `toll_policy`.

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

/// Finite multiplier applied to toll-edge A* costs under [`TollPolicy::Penalize`].
///
/// Large enough that free detours many times the toll length still win; finite
/// so the graph stays connected and A* finds *some* path when one exists.
pub const TOLL_AVOID_PENALTY_MULT: f64 = 50.0;

/// How the router treats OSM toll edges for this plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TollPolicy {
    /// Toll edges use normal weights (former `avoid_tolls: false`).
    #[default]
    Allow,
    /// Prefer free roads via [`TOLL_AVOID_PENALTY_MULT`] (former `avoid_tolls: true`).
    Penalize,
    /// Hard-exclude toll edges from the search (absolute never-use).
    NeverUse,
}

impl TollPolicy {
    /// Map the legacy boolean preference onto the enum (no NeverUse).
    pub fn from_avoid_tolls_bool(avoid_tolls: bool) -> Self {
        if avoid_tolls {
            Self::Penalize
        } else {
            Self::Allow
        }
    }

    pub fn as_diag_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Penalize => "penalize",
            Self::NeverUse => "never_use",
        }
    }

    /// Parse from saved-route / summary JSON.
    ///
    /// Accepts `toll_policy` string (`allow` / `penalize` / `never_use`) or the
    /// legacy `avoid_tolls` boolean when `toll_policy` is absent.
    pub fn from_summary_json(get: impl Fn(&str) -> Option<String>) -> Self {
        if let Some(raw) = get("toll_policy") {
            return Self::parse_name(&raw).unwrap_or(Self::Allow);
        }
        match get("avoid_tolls").as_deref() {
            Some("true") | Some("1") => Self::Penalize,
            Some("false") | Some("0") => Self::Allow,
            _ => Self::Allow,
        }
    }

    pub fn parse_name(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "allow" | "off" | "none" => Some(Self::Allow),
            "penalize" | "penalise" | "avoid" => Some(Self::Penalize),
            "never_use" | "never-use" | "strict" | "exclude" => Some(Self::NeverUse),
            _ => None,
        }
    }
}

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
        assert!(!applies(RoutingProfile::Bicycle, &t));
        assert!(!applies(RoutingProfile::Foot, &t));
    }

    #[test]
    fn avoid_tolls_bool_maps_to_allow_or_penalize() {
        assert_eq!(TollPolicy::from_avoid_tolls_bool(false), TollPolicy::Allow);
        assert_eq!(
            TollPolicy::from_avoid_tolls_bool(true),
            TollPolicy::Penalize
        );
    }

    #[test]
    fn summary_json_prefers_toll_policy_over_legacy_bool() {
        let map: HashMap<String, String> = HashMap::from([
            ("toll_policy".into(), "never_use".into()),
            ("avoid_tolls".into(), "false".into()),
        ]);
        assert_eq!(
            TollPolicy::from_summary_json(|k| map.get(k).cloned()),
            TollPolicy::NeverUse
        );
    }

    #[test]
    fn summary_json_legacy_avoid_tolls_true() {
        let map: HashMap<String, String> = HashMap::from([("avoid_tolls".into(), "true".into())]);
        assert_eq!(
            TollPolicy::from_summary_json(|k| map.get(k).cloned()),
            TollPolicy::Penalize
        );
    }

    #[test]
    fn generic_toll_yes() {
        let t = tags(&[("toll", "yes")]);
        assert!(applies(RoutingProfile::Car, &t));
    }

    #[test]
    fn bike_specific_no_overrides_generic_yes() {
        let t = tags(&[("toll", "yes"), ("toll:bicycle", "no")]);
        assert!(applies(RoutingProfile::Car, &t));
        assert!(!applies(RoutingProfile::Bicycle, &t));
    }
}
