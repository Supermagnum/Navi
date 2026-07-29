//! Offline ISO-based driving-hours pack resolution.
//!
//! Uses the shared country ring table via
//! [`crate::routing::elevation::country_iso_at`] (same offline admin detector
//! used as the Geofabrik-`.poly` fallback elsewhere) — not a second bespoke
//! geometry approach.

use crate::config::JurisdictionDrivingHoursPack;
use crate::routing::elevation::country_iso_at as iso_at;

/// ISO codes where Navi applies the **EU Regulation EC 561/2006** parameter pack.
///
/// Membership (verified 2026-07-29 against project jurisdiction docs + UK GOV.UK
/// drivers' hours guidance):
///
/// - **EU member states** (subset covered by the offline ring table)
/// - **EEA / closely aligned:** Norway (`no`), Iceland (`is`), Liechtenstein (`li`)
/// - **Switzerland (`ch`)** — treated as EC 561-shaped for Navi's single EU pack
///   (bilateral / international HGV practice; not a separate AETR row yet)
///
/// **Explicitly excluded:**
///
/// - **`gb` (United Kingdom)** — UK applies *assimilated* EC 561/2006 as it has
///   effect in domestic law, with UK national derogations and a distinct AETR
///   split for some international journeys
///   ([GOV.UK drivers' hours guidance](https://www.gov.uk/guidance/drivers-hours-goods-vehicles/1-assimilated-and-aetr-rules-on-drivers-hours)).
///   That is **not** the same as silently applying Navi's EU EC 561 pack.
///   Until a dedicated UK pack exists, resolve `gb` →
///   [`JurisdictionDrivingHoursPack::Unknown`] (decline-by-default).
/// - **AETR-only non-EU signatories** (e.g. Ukraine, Turkey) — not in this list;
///   add keyed packs later rather than inventing EU numbers.
const EC561_FAMILY: &[&str] = &[
    // EU (offline rings present)
    "at", "be", "bg", "cy", "cz", "de", "dk", "ee", "es", "fi", "fr", "gr", "hr", "hu", "ie", "it",
    "lt", "lu", "lv", "mt", "nl", "pl", "pt", "ro", "se", "si", "sk", //
    // EEA / aligned
    "no", "is", "li", "ch",
];

/// Resolve a driving-hours pack from a corridor start (or GPS) position.
///
/// Unmatched / unsupported jurisdictions →
/// [`JurisdictionDrivingHoursPack::Unknown`] (decline-by-default).
pub fn resolve_driving_hours_pack_at(lat: f64, lon: f64) -> JurisdictionDrivingHoursPack {
    match iso_at(lat, lon) {
        Some("us") => JurisdictionDrivingHoursPack::Fmcsa,
        Some(code) if EC561_FAMILY.contains(&code) => JurisdictionDrivingHoursPack::Ec561,
        // Includes `gb` and any ISO we can detect but have no pack for.
        Some(_) | None => JurisdictionDrivingHoursPack::Unknown,
    }
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

    /// Regression: UK must not inherit the EU EC 561 pack via a coarse GB bbox.
    #[test]
    fn london_gb_declines_ec561_pack() {
        assert_eq!(
            resolve_driving_hours_pack_at(51.5074, -0.1278),
            JurisdictionDrivingHoursPack::Unknown,
            "gb must decline until a dedicated UK pack exists"
        );
    }

    #[test]
    fn dublin_ie_is_ec561() {
        assert_eq!(
            resolve_driving_hours_pack_at(53.35, -6.26),
            JurisdictionDrivingHoursPack::Ec561
        );
    }

    #[test]
    fn near_no_se_border_oslo_side_stays_ec561() {
        // Still inside Norway ring / EC 561 family.
        assert_eq!(
            resolve_driving_hours_pack_at(59.91, 10.75),
            JurisdictionDrivingHoursPack::Ec561
        );
    }
}
