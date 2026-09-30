//! Admin region resolution for jurisdiction packs (camping, horse, …).
//!
//! Distinct from [`crate::routing::elevation::country_iso_at`]: this path applies
//! **territory overrides** for ISO 3166-1 codes that Natural Earth Admin-0 does
//! not expose separately, and leaves ISO 3166-2 subdivision empty until an
//! OSM `admin_level=4` (or equivalent) layer exists.
//!
//! # Territory overrides (SJ, GI)
//!
//! Natural Earth's Admin-0 asset used by [`country_iso_at`](crate::routing::elevation::country_iso_at)
//! has **no `sj` polygon** and **no `gi` polygon** (verified against
//! `country_polys.bin` on 2026-09). Svalbard / Jan Mayen therefore resolve as
//! `no`, and Gibraltar is absorbed into Spain — wrong for right-to-roam packs
//! (`docs/plugins/right-to-roam-camping-spec.md` §3.1).
//!
//! Override sources (axis-aligned boxes; not cadastral boundaries):
//! - **Svalbard archipelago + Bjørnøya:** Svalbard Treaty geography /
//!   Wikipedia extent ≈ 74°–81°N, 10°–35°E; Bjørnøya ≈ 74.5°N, 19°E.
//! - **Jan Mayen:** ≈ 71.0°N, 8.3°W (west of mainland Norway longitudes).
//! - **Gibraltar:** ≈ 36.14°N, 5.35°W.
//!
//! Points inside a **margin ring** around an override (but not the inner box)
//! return **unknown**, never the Natural Earth parent code.

use crate::routing::elevation::country_iso_at;

/// Resolved admin region for a WGS84 point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminRegion {
    /// ISO 3166-1 alpha-2, lowercase. `None` = unknown or ambiguous.
    pub country_iso: Option<String>,
    /// ISO 3166-2 (e.g. `no-18`, `de-bb`, `gb-sct`). Always `None` until a
    /// subdivision layer exists.
    pub subdivision_iso: Option<String>,
}

impl AdminRegion {
    pub fn unknown() -> Self {
        Self {
            country_iso: None,
            subdivision_iso: None,
        }
    }

    pub fn country(iso: &str) -> Self {
        Self {
            country_iso: Some(iso.to_ascii_lowercase()),
            subdivision_iso: None,
        }
    }
}

#[derive(Clone, Copy)]
struct BoxDeg {
    min_lat: f64,
    max_lat: f64,
    min_lon: f64,
    max_lon: f64,
}

impl BoxDeg {
    fn contains(self, lat: f64, lon: f64) -> bool {
        lat >= self.min_lat && lat <= self.max_lat && lon >= self.min_lon && lon <= self.max_lon
    }

    fn expand(self, margin_deg: f64) -> Self {
        Self {
            min_lat: self.min_lat - margin_deg,
            max_lat: self.max_lat + margin_deg,
            min_lon: self.min_lon - margin_deg,
            max_lon: self.max_lon + margin_deg,
        }
    }
}

/// Result of the territory override pass (before Natural Earth).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerritoryOverride {
    /// Confident territory ISO (lowercase).
    Confident(&'static str),
    /// Near an override edge — must not fall through to a parent code.
    Ambiguous,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OverrideHit {
    Confident(&'static str),
    Ambiguous,
    Miss,
}

struct TerritoryBox {
    iso: &'static str,
    inners: &'static [BoxDeg],
    margin_deg: f64,
}

/// Svalbard (incl. Bjørnøya) + Jan Mayen → `sj`.
const SJ_OVERRIDE: TerritoryBox = TerritoryBox {
    iso: "sj",
    inners: &[
        // Spitsbergen / Nordaustlandet / Edgeøya / … and Bjørnøya (74.4°N, 19°E).
        BoxDeg {
            min_lat: 74.20,
            max_lat: 80.85,
            min_lon: 10.0,
            max_lon: 34.5,
        },
        // Jan Mayen (west of mainland longitudes — cannot hit Tromsø).
        BoxDeg {
            min_lat: 70.82,
            max_lat: 71.20,
            min_lon: -9.15,
            max_lon: -7.85,
        },
    ],
    margin_deg: 0.25,
};

/// Gibraltar → `gi`.
const GI_OVERRIDE: TerritoryBox = TerritoryBox {
    iso: "gi",
    inners: &[BoxDeg {
        min_lat: 36.109,
        max_lat: 36.155,
        min_lon: -5.367,
        max_lon: -5.338,
    }],
    margin_deg: 0.02,
};

const OVERRIDES: &[TerritoryBox] = &[SJ_OVERRIDE, GI_OVERRIDE];

fn override_hit(o: &TerritoryBox, lat: f64, lon: f64) -> OverrideHit {
    if o.inners.iter().any(|b| b.contains(lat, lon)) {
        return OverrideHit::Confident(o.iso);
    }
    if o
        .inners
        .iter()
        .any(|b| b.expand(o.margin_deg).contains(lat, lon))
    {
        return OverrideHit::Ambiguous;
    }
    OverrideHit::Miss
}

/// Territory override only (no Natural Earth). Used by tests and by
/// [`admin_region_at`].
pub fn territory_override_at(lat: f64, lon: f64) -> Option<TerritoryOverride> {
    if !lat.is_finite() || !lon.is_finite() {
        return None;
    }
    for o in OVERRIDES {
        match override_hit(o, lat, lon) {
            OverrideHit::Confident(iso) => return Some(TerritoryOverride::Confident(iso)),
            OverrideHit::Ambiguous => return Some(TerritoryOverride::Ambiguous),
            OverrideHit::Miss => {}
        }
    }
    None
}

/// Resolve country (+ optional subdivision) for jurisdiction pack selection.
///
/// Territory overrides run **before** Natural Earth. Ambiguous override margins
/// yield [`AdminRegion::unknown`] (never the parent NE code).
pub fn admin_region_at(lat: f64, lon: f64) -> AdminRegion {
    match territory_override_at(lat, lon) {
        Some(TerritoryOverride::Confident(iso)) => return AdminRegion::country(iso),
        Some(TerritoryOverride::Ambiguous) => return AdminRegion::unknown(),
        None => {}
    }

    if !lat.is_finite() || !lon.is_finite() {
        return AdminRegion::unknown();
    }

    match country_iso_at(lat, lon) {
        Some(iso) => {
            let mut r = AdminRegion::country(iso);
            // Prefer warmed OSM admin_level=4 ISO3166-2 when available.
            if let Some(sub) = crate::admin_subdivision::subdivision_iso_at(lat, lon) {
                r.subdivision_iso = Some(sub);
            }
            r
        }
        None => AdminRegion::unknown(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn longyearbyen_is_sj_not_no() {
        assert_eq!(
            territory_override_at(78.2232, 15.6267),
            Some(TerritoryOverride::Confident("sj"))
        );
        let r = admin_region_at(78.2232, 15.6267);
        assert_eq!(r.country_iso.as_deref(), Some("sj"));
        assert!(r.subdivision_iso.is_none());
    }

    #[test]
    fn bjornoya_is_sj() {
        assert_eq!(
            territory_override_at(74.45, 19.05),
            Some(TerritoryOverride::Confident("sj"))
        );
    }

    #[test]
    fn jan_mayen_is_sj() {
        assert_eq!(
            territory_override_at(70.984, -8.516),
            Some(TerritoryOverride::Confident("sj"))
        );
    }

    #[test]
    fn tromso_not_claimed_by_sj_override() {
        // Mainland Tromsø must not hit SJ boxes; full NE → `no` is covered by
        // country_iso_* integration tests (Natural Earth warm is slow).
        assert_eq!(territory_override_at(69.6492, 18.9553), None);
    }

    #[test]
    fn gibraltar_centre_is_gi() {
        assert_eq!(
            territory_override_at(36.1408, -5.3536),
            Some(TerritoryOverride::Confident("gi"))
        );
        assert_eq!(
            admin_region_at(36.1408, -5.3536).country_iso.as_deref(),
            Some("gi")
        );
    }

    #[test]
    fn svalbard_margin_is_ambiguous_unknown_not_no() {
        // Just south of the Svalbard inner box, inside the margin ring.
        assert_eq!(
            territory_override_at(74.05, 19.0),
            Some(TerritoryOverride::Ambiguous)
        );
        let r = admin_region_at(74.05, 19.0);
        assert!(
            r.country_iso.is_none(),
            "margin must not fall through to NE parent, got {r:?}"
        );
    }

    #[test]
    fn subdivision_field_is_none_on_override_hits() {
        for (lat, lon) in [
            (78.2232, 15.6267),
            (74.45, 19.05),
            (70.984, -8.516),
            (36.1408, -5.3536),
        ] {
            let r = admin_region_at(lat, lon);
            assert!(r.subdivision_iso.is_none(), "{lat},{lon} → {r:?}");
            assert!(r.country_iso.is_some(), "{lat},{lon}");
        }
    }
}
