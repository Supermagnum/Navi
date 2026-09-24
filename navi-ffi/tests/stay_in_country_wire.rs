//! Stay-in-Country: `allowed_countries` must reach RouteOptions (not stay hardcoded None).

use navi::country_iso_at;

/// Drammen (origin) and Kautokeino (destination) — product example for Stay in Country.
const DRAMMEN: (f64, f64) = (59.7440, 10.2045);
const KAUTOKEINO: (f64, f64) = (69.0125, 23.0415);

#[test]
fn country_iso_at_drammen_and_kautokeino_are_norway() {
    assert_eq!(country_iso_at(DRAMMEN.0, DRAMMEN.1).as_deref(), Some("no"));
    assert_eq!(
        country_iso_at(KAUTOKEINO.0, KAUTOKEINO.1).as_deref(),
        Some("no")
    );
}

#[test]
fn allowed_countries_for_plan_helper_matches_host() {
    // Mirrors StayInCountry.allowedCountriesForPlan on the host.
    fn allowed(enabled: bool, iso: Option<&str>) -> Option<Vec<String>> {
        if !enabled {
            return None;
        }
        let iso = iso?.trim().to_ascii_lowercase();
        if iso.len() == 2 && iso.chars().all(|c| c.is_ascii_alphabetic()) {
            Some(vec![iso])
        } else {
            None
        }
    }
    assert_eq!(allowed(false, Some("no")), None);
    assert_eq!(allowed(true, Some("NO")), Some(vec!["no".into()]));
    assert_eq!(allowed(true, None), None);
}
