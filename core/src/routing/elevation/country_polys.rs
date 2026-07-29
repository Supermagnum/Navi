//! Simplified offline ISO country rings for point-in-polygon lookup.
//!
//! Shared by elevation country helpers and HOS jurisdiction resolution so both
//! use one admin-region detector (not a second bespoke geometry path).
//!
//! Rings are coarse mainland approximations (lon, lat), sufficient for
//! offline pack selection. Overseas territories are intentionally omitted.
//! Prefer Geofabrik `.poly` / OSM admin boundaries when a host has them for a
//! bound extract; these rings are the on-device fallback.

use geo::{point, Contains, Coord, LineString, Polygon};

type LonLat = (f64, f64);

fn poly(ring: &[LonLat]) -> Polygon {
    let mut coords: Vec<Coord> = ring
        .iter()
        .map(|(lon, lat)| Coord { x: *lon, y: *lat })
        .collect();
    if let (Some(first), Some(last)) = (coords.first().copied(), coords.last().copied()) {
        if first != last {
            coords.push(first);
        }
    }
    Polygon::new(LineString::new(coords), vec![])
}

/// (ISO-3166-1 alpha-2 lowercase, exterior ring lon/lat).
///
/// Order matters when rings could theoretically overlap at coarse resolution:
/// more specific / smaller countries should be checked first by callers that
/// iterate this table in order (see [`iso_at`]).
const COUNTRY_RINGS: &[(&str, &[LonLat])] = &[
    // Small / nested first
    (
        "li",
        &[(9.47, 47.05), (9.63, 47.05), (9.63, 47.27), (9.47, 47.27)],
    ),
    (
        "lu",
        &[(5.73, 49.44), (6.53, 49.44), (6.53, 50.19), (5.73, 50.19)],
    ),
    (
        "mt",
        &[
            (14.18, 35.80),
            (14.58, 35.80),
            (14.58, 36.09),
            (14.18, 36.09),
        ],
    ),
    (
        "cy",
        &[
            (32.25, 34.55),
            (34.60, 34.55),
            (34.60, 35.70),
            (32.25, 35.70),
        ],
    ),
    (
        "be",
        &[(2.54, 49.49), (6.40, 49.49), (6.40, 51.51), (2.54, 51.51)],
    ),
    (
        "nl",
        &[(3.35, 50.75), (7.23, 50.75), (7.23, 53.55), (3.35, 53.55)],
    ),
    (
        "dk",
        &[(8.05, 54.55), (12.70, 54.55), (12.70, 57.80), (8.05, 57.80)],
    ),
    (
        "ch",
        &[(5.95, 45.82), (10.50, 45.82), (10.50, 47.81), (5.95, 47.81)],
    ),
    (
        "at",
        &[(9.50, 46.37), (17.20, 46.37), (17.20, 49.02), (9.50, 49.02)],
    ),
    (
        "si",
        &[
            (13.38, 45.42),
            (16.60, 45.42),
            (16.60, 46.88),
            (13.38, 46.88),
        ],
    ),
    (
        "hr",
        &[
            (13.45, 42.35),
            (19.45, 42.35),
            (19.45, 46.55),
            (13.45, 46.55),
        ],
    ),
    (
        "sk",
        &[
            (16.83, 47.73),
            (22.57, 47.73),
            (22.57, 49.62),
            (16.83, 49.62),
        ],
    ),
    (
        "cz",
        &[
            (12.09, 48.55),
            (18.87, 48.55),
            (18.87, 51.06),
            (12.09, 51.06),
        ],
    ),
    (
        "hu",
        &[
            (16.11, 45.74),
            (22.90, 45.74),
            (22.90, 48.59),
            (16.11, 48.59),
        ],
    ),
    (
        "ee",
        &[
            (21.80, 57.50),
            (28.25, 57.50),
            (28.25, 59.70),
            (21.80, 59.70),
        ],
    ),
    (
        "lv",
        &[
            (20.95, 55.65),
            (28.25, 55.65),
            (28.25, 58.10),
            (20.95, 58.10),
        ],
    ),
    (
        "lt",
        &[
            (20.90, 53.90),
            (26.85, 53.90),
            (26.85, 56.45),
            (20.90, 56.45),
        ],
    ),
    (
        "ie",
        &[
            (-10.50, 51.40),
            (-5.95, 51.40),
            (-5.95, 55.45),
            (-10.50, 55.45),
        ],
    ),
    (
        "pt",
        &[
            (-9.55, 36.95),
            (-6.15, 36.95),
            (-6.15, 42.20),
            (-9.55, 42.20),
        ],
    ),
    (
        "bg",
        &[
            (22.35, 41.20),
            (28.65, 41.20),
            (28.65, 44.25),
            (22.35, 44.25),
        ],
    ),
    (
        "ro",
        &[
            (20.25, 43.60),
            (29.75, 43.60),
            (29.75, 48.30),
            (20.25, 48.30),
        ],
    ),
    (
        "gr",
        &[
            (19.35, 34.80),
            (29.65, 34.80),
            (29.65, 41.80),
            (19.35, 41.80),
        ],
    ),
    (
        "pl",
        &[
            (14.10, 49.00),
            (24.15, 49.00),
            (24.15, 54.90),
            (14.10, 54.90),
        ],
    ),
    (
        "it",
        &[(6.60, 36.60), (18.55, 36.60), (18.55, 47.10), (6.60, 47.10)],
    ),
    (
        "es",
        &[(-9.35, 35.95), (3.35, 35.95), (3.35, 43.80), (-9.35, 43.80)],
    ),
    (
        "de",
        &[(5.85, 47.25), (15.05, 47.25), (15.05, 55.10), (5.85, 55.10)],
    ),
    (
        "fr",
        &[(-5.15, 42.30), (8.25, 42.30), (8.25, 51.15), (-5.15, 51.15)],
    ),
    (
        "se",
        &[
            (11.00, 55.20),
            (24.20, 55.20),
            (24.20, 69.10),
            (11.00, 69.10),
        ],
    ),
    (
        "fi",
        &[
            (20.50, 59.70),
            (31.60, 59.70),
            (31.60, 70.10),
            (20.50, 70.10),
        ],
    ),
    (
        "is",
        &[
            (-24.55, 63.25),
            (-13.45, 63.25),
            (-13.45, 66.60),
            (-24.55, 66.60),
        ],
    ),
    // Norway mainland (coarse); Svalbard omitted intentionally
    (
        "no",
        &[(4.50, 57.90), (31.20, 57.90), (31.20, 71.20), (4.50, 71.20)],
    ),
    // UK — detected so callers can *decline* EU EC 561 pack (assimilated UK
    // rules / national derogations are not the same as Navi's EU pack).
    (
        "gb",
        &[(-8.20, 49.85), (1.80, 49.85), (1.80, 58.70), (-8.20, 58.70)],
    ),
    // Contiguous US (Alaska/Hawaii omitted for HOS start classification)
    (
        "us",
        &[
            (-125.00, 24.40),
            (-66.90, 24.40),
            (-66.90, 49.40),
            (-125.00, 49.40),
        ],
    ),
];

/// Resolve ISO-3166-1 alpha-2 (lowercase) for a WGS84 point via ring containment.
///
/// Returns the first matching country in [`COUNTRY_RINGS`] order (small states
/// before large neighbours). `None` if outside the offline pack table.
pub fn iso_at(lat: f64, lon: f64) -> Option<&'static str> {
    let p = point!(x: lon, y: lat);
    for (code, ring) in COUNTRY_RINGS {
        let polygon = poly(ring);
        if polygon.contains(&p) {
            return Some(*code);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oslo_is_norway() {
        assert_eq!(iso_at(59.91, 10.75), Some("no"));
    }

    #[test]
    fn london_is_gb() {
        assert_eq!(iso_at(51.5074, -0.1278), Some("gb"));
    }

    #[test]
    fn kansas_is_us() {
        assert_eq!(iso_at(39.0, -98.0), Some("us"));
    }

    #[test]
    fn mid_atlantic_is_none() {
        assert_eq!(iso_at(35.0, -40.0), None);
    }

    #[test]
    fn liechtenstein_before_neighbours() {
        assert_eq!(iso_at(47.14, 9.52), Some("li"));
    }
}
