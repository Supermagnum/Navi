//! Approximate country bounding boxes for elevation country jobs.
//!
//! Host UI should prefer Geofabrik `.poly` boundaries when available; these boxes
//! are a conservative fallback for tile enumeration.

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
];
