//! Offline evidence accuracy for Natural Earth country polygons.
//!
//! Ground truth is Nominatim (`country_iso_expected.json`). Points are never
//! moved and expected ISO codes are never rewritten to make a dataset pass.
//! Known Natural Earth vs OSM admin disagreements near borders are listed
//! explicitly below.

use driver_break_core::routing::elevation::{
    country_iso_at, dist_to_foreign_border_m, COASTAL_SNAP_TOLERANCE_M,
};
use serde::Deserialize;
use std::collections::HashSet;
use std::path::PathBuf;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/long_trip/country_iso_expected.json")
}

#[derive(Deserialize)]
struct ExpectedFile {
    points: Vec<ExpectedRow>,
    navi_fixture: String,
}

#[derive(Deserialize)]
struct ExpectedRow {
    id: String,
    lat: f64,
    lon: f64,
    expected_iso: String,
    #[serde(default)]
    border_pair: Option<String>,
}

/// Natural Earth Admin-0 (50m) disagrees with Nominatim/OSM admin at these
/// restored near-border evidence points. Distances are to the expected
/// country's NE boundary (metres). Do not "fix" by moving the points.
const KNOWN_NE50_NOMINATIM_DISPUTES: &[&str] = &[
    "karigasniemi_fi",  // Nom fi; NE no (~0.7 km)
    "storskog_no",      // Nom ru; NE no (~3.7 km)
    "borisoglebsky_ru", // Nom ru; NE no (~2.0 km)
    "sumas_us",         // Nom us; NE ca (~0.2 km)
    "brownsville_us",   // Nom us; NE mx (~2.3 km)
    "point_roberts_us", // Nom us; NE ca (exclave; NE omits US land)
    "agua_prieta_mx",   // Nom mx; NE us (~0.01 km)
    "blaine_us",        // Nom us; NE ca (~0.08 km)
    "el_paso_us",       // Nom us; NE mx (~0.9 km)
    // Restored densification (Task A) — same NE/OSM border jitter:
    "peace_arch_us",
    "point_roberts_tyee_us",
    "el_paso_bridge_us",
    "krusaa_border_dk", // Nominatim returned de at this sample; NE says dk
    "sweetgrass_us",
    "portal_us",
    "eagle_pass_us",
    "presidio_us",
    "riksgransen_se",
];

#[test]
fn evidence_fixture_is_recorded_and_large_enough() {
    let raw = std::fs::read_to_string(fixture()).unwrap();
    let file: ExpectedFile = serde_json::from_str(&raw).unwrap();
    assert_eq!(file.navi_fixture, "recorded");
    assert!(
        file.points.len() >= 100,
        "need ≥100 evidence points, got {}",
        file.points.len()
    );
    let near_border = file
        .points
        .iter()
        .filter(|p| p.border_pair.is_some())
        .count();
    assert!(
        near_border * 2 >= file.points.len(),
        "at least half should carry border_pair (near-border), got {near_border}/{}",
        file.points.len()
    );
}

#[test]
fn at_least_half_fixture_within_5km_of_foreign_border() {
    let file: ExpectedFile =
        serde_json::from_str(&std::fs::read_to_string(fixture()).unwrap()).unwrap();
    let mut within_1 = 0usize;
    let mut within_2 = 0usize;
    let mut within_5 = 0usize;
    for p in &file.points {
        let d = dist_to_foreign_border_m(p.lat, p.lon, &p.expected_iso).unwrap_or(f64::INFINITY);
        if d <= 1_000.0 {
            within_1 += 1;
        }
        if d <= 2_000.0 {
            within_2 += 1;
        }
        if d <= 5_000.0 {
            within_5 += 1;
        }
    }
    eprintln!(
        "border_distance_hist within_1km={within_1} within_2km={within_2} within_5km={within_5} / {}",
        file.points.len()
    );
    assert!(
        within_5 * 2 >= file.points.len(),
        "need ≥half within 5 km of a foreign border, got {within_5}/{}",
        file.points.len()
    );
}

#[test]
fn country_iso_at_matches_evidence_except_known_ne_disputes() {
    let file: ExpectedFile =
        serde_json::from_str(&std::fs::read_to_string(fixture()).unwrap()).unwrap();
    let known: HashSet<&str> = KNOWN_NE50_NOMINATIM_DISPUTES.iter().copied().collect();
    let mut misses = Vec::new();
    let mut known_hits = Vec::new();
    for p in &file.points {
        let got = country_iso_at(p.lat, p.lon);
        if got == Some(p.expected_iso.as_str()) {
            continue;
        }
        let d_exp = dist_to_foreign_border_m(p.lat, p.lon, &p.expected_iso);
        let line = format!(
            "{} expected={} got={:?} @ {},{} dist_to_expected_border_or_foreign={:?}",
            p.id, p.expected_iso, got, p.lat, p.lon, d_exp
        );
        if known.contains(p.id.as_str()) {
            known_hits.push(line);
        } else {
            misses.push(line);
        }
    }
    eprintln!(
        "known_ne50_disputes={} coastal_snap_tol_m={}",
        known_hits.len(),
        COASTAL_SNAP_TOLERANCE_M
    );
    for k in &known_hits {
        eprintln!("KNOWN_DISPUTE {k}");
    }
    assert!(
        misses.is_empty(),
        "unexpected country_iso_at mismatches ({}):\n{}",
        misses.len(),
        misses.join("\n")
    );
    // Every listed dispute must still be a real miss (list stays honest).
    assert_eq!(
        known_hits.len(),
        KNOWN_NE50_NOMINATIM_DISPUTES.len(),
        "known dispute list stale: expected {} misses, got {}\n{}",
        KNOWN_NE50_NOMINATIM_DISPUTES.len(),
        known_hits.len(),
        known_hits.join("\n")
    );
}
