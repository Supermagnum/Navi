//! Approximate country bounding boxes for elevation country jobs, plus shared
//! offline ISO point-in-polygon detection ([`iso_at`]).
//!
//! Host UI should prefer Geofabrik `.poly` / OSM admin boundaries when available
//! for a bound extract; the rings in [`super::country_polys`] are the on-device
//! fallback used by HOS jurisdiction resolution and available for other callers.

pub use super::country_polys::iso_at;

pub fn lookup(code: &str) -> Option<[f64; 4]> {
    let key = code.to_ascii_lowercase();
    COUNTRIES
        .iter()
        .find(|(c, _)| *c == key)
        .map(|(_, bbox)| *bbox)
}

const COUNTRIES: &[(&str, [f64; 4])] = &[
    ("no", [57.9, 4.5, 71.5, 31.5]),
    ("se", [55.0, 10.0, 69.5, 24.5]),
    ("fi", [59.5, 19.0, 70.5, 32.0]),
    ("de", [47.0, 5.5, 55.5, 15.5]),
    ("ch", [45.5, 5.5, 48.0, 10.8]),
    ("at", [46.0, 9.0, 49.5, 17.5]),
    ("fr", [41.0, -5.5, 51.5, 10.0]),
    ("gb", [49.5, -8.5, 61.0, 2.0]),
    ("us", [24.0, -125.0, 50.0, -66.0]),
    ("ie", [51.4, -10.5, 55.5, -5.9]),
    ("is", [63.2, -24.6, 66.6, -13.4]),
    ("be", [49.4, 2.5, 51.6, 6.5]),
    ("nl", [50.7, 3.3, 53.6, 7.3]),
    ("dk", [54.5, 8.0, 57.8, 12.8]),
    ("pl", [49.0, 14.0, 54.9, 24.2]),
    ("it", [36.6, 6.6, 47.1, 18.6]),
    ("es", [35.9, -9.4, 43.8, 3.4]),
    ("pt", [36.9, -9.6, 42.2, -6.1]),
];
