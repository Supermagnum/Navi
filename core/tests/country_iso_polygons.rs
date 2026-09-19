//! Offline evidence accuracy for Natural Earth country polygons.

use driver_break_core::routing::elevation::country_iso_at;
use serde::Deserialize;
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
fn country_iso_at_matches_all_evidence_points() {
    let file: ExpectedFile =
        serde_json::from_str(&std::fs::read_to_string(fixture()).unwrap()).unwrap();
    let mut misses = Vec::new();
    for p in &file.points {
        let got = country_iso_at(p.lat, p.lon);
        if got != Some(p.expected_iso.as_str()) {
            misses.push(format!(
                "{} expected={} got={:?} @ {},{}",
                p.id, p.expected_iso, got, p.lat, p.lon
            ));
        }
    }
    assert!(
        misses.is_empty(),
        "country_iso_at mismatches ({}):\n{}",
        misses.len(),
        misses.join("\n")
    );
}
