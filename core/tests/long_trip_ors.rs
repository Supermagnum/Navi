//! Phase 2: ORS preliminary corridor → ordered catalog regions (fixture-backed).

use driver_break_core::long_trip::{
    build_directions_request_body, classify_catalog_coverage, ors_country_id, CatalogCoverage,
    LONG_TRIP_CORRIDOR_BUFFER_KM, ORS_DISCLOSURE,
};
use driver_break_core::pack_server::{catalog_entries_from_ready_ids, ReadyRegion};
use serde::Deserialize;
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/long_trip")
        .join(name)
}

fn load_catalog_ids() -> (Vec<String>, Vec<(String, u64)>) {
    #[derive(Deserialize)]
    struct Cat {
        regions: Vec<Reg>,
    }
    #[derive(Deserialize)]
    struct Reg {
        region_id: String,
        bytes: Option<u64>,
    }
    let raw = std::fs::read_to_string(fixture("current.json")).expect("current.json");
    let cat: Cat = serde_json::from_str(&raw).expect("parse current");
    let ids: Vec<String> = cat.regions.iter().map(|r| r.region_id.clone()).collect();
    let sizes: Vec<(String, u64)> = cat
        .regions
        .iter()
        .map(|r| (r.region_id.clone(), r.bytes.unwrap_or(0)))
        .collect();
    (ids, sizes)
}

fn regions_from_recorded_brouter(
    name: &str,
    installed: &[String],
    country_iso: Option<&str>,
) -> (Vec<String>, f64) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/long_trip/recorded")
        .join(name);
    let wrap: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        wrap.get("navi_fixture").and_then(|x| x.as_str()),
        Some("recorded")
    );
    let body = wrap
        .get("response_body")
        .and_then(|b| b.as_str())
        .expect("response_body");
    let route = driver_break_core::long_trip::parse_brouter_geojson(body).unwrap();
    let (ids, _) = load_catalog_ids();
    let entries = catalog_entries_from_ready_ids(&ids);
    let needed = driver_break_core::long_trip::ordered_needed_regions_along_route_filtered(
        &route.lat_lon,
        &entries,
        installed,
        LONG_TRIP_CORRIDOR_BUFFER_KM,
        country_iso,
    );
    (needed, route.distance_m)
}

#[test]
fn disclosure_string_mentions_third_party() {
    let d = ORS_DISCLOSURE.to_ascii_lowercase();
    assert!(d.contains("third-party") || d.contains("third party"));
    assert!(d.contains("openrouteservice") || d.contains("open route"));
    assert!(d.contains("brouter"));
}

#[test]
fn klecken_innlandet_seven_regions_after_dropping_niedersachsen() {
    let installed = vec!["europe/germany/niedersachsen".into()];
    let (needed, dist_m) =
        regions_from_recorded_brouter("brouter_klecken_innlandet_car-eco.json", &installed, None);
    assert!(dist_m > 0.0);
    // Property: unique, ordered, no installed, destination-side Norway last-ish.
    let mut seen = std::collections::BTreeSet::new();
    for r in &needed {
        assert!(seen.insert(r.clone()), "duplicate {r}");
        assert_ne!(r, "europe/germany/niedersachsen");
    }
    assert!(
        needed.iter().any(|r| r.contains("hamburg")),
        "expected hamburg in {needed:?}"
    );
    assert!(
        needed
            .iter()
            .any(|r| r.contains("denmark") || r.contains("schleswig")),
        "expected DK/SH in {needed:?}"
    );
    assert!(
        needed
            .last()
            .map(|r| r.contains("ostlandet") || r.contains("norway"))
            .unwrap_or(false),
        "destination region should be last: {needed:?}"
    );
    // Property checks (exact count can grow when catalog adds leaves like hedmark).
    assert!(
        needed.len() >= 7,
        "Klecken→Innlandet expected ≥7 missing regions, got {}: {needed:?}",
        needed.len()
    );
    for stem in [
        "hamburg",
        "schleswig-holstein",
        "denmark",
        "skane",
        "halland",
        "vastra_gotaland",
        "ostlandet",
    ] {
        assert!(
            needed.iter().any(|r| r.contains(stem)),
            "missing stem {stem} in {needed:?}"
        );
    }
}

#[test]
fn kautokeino_roros_norway_only_request_and_regions() {
    let body =
        build_directions_request_body(&[[23.04, 69.01], [11.38, 62.57]], Some(&["no".into()]))
            .unwrap();
    let avoid = body["options"]["avoid_countries"].as_array().unwrap();
    let ids: Vec<u64> = avoid.iter().filter_map(|v| v.as_u64()).collect();
    for iso in ["se", "fi", "ru"] {
        let id = ors_country_id(iso).unwrap() as u64;
        assert!(
            ids.contains(&id),
            "missing neighbour {iso} id {id} in {ids:?}"
        );
    }
    let feats = body["options"]["avoid_features"].as_array().unwrap();
    assert!(feats.iter().any(|v| v == "ferries"));

    let (needed, _) =
        regions_from_recorded_brouter("brouter_kautokeino_roros_car-eco.json", &[], Some("no"));
    assert!(!needed.is_empty());
    for r in &needed {
        assert!(
            r.starts_with("europe/norway"),
            "Norway-only corridor leaked {r} in {needed:?}"
        );
    }
}

#[test]
fn error_fixtures_map_to_typed_ors_errors() {
    use driver_break_core::long_trip::{parse_directions_geojson, OrsError};
    let rate = r#"{"error":{"code":429,"message":"Rate limit exceeded"}}"#;
    assert!(matches!(
        parse_directions_geojson(rate),
        Err(OrsError::RateLimited) | Err(OrsError::RequestTooLarge { .. })
    ));
    let big =
        r#"{"error":{"code":2004,"message":"Request parameters exceed the maximum distance"}}"#;
    assert!(matches!(
        parse_directions_geojson(big),
        Err(OrsError::RequestTooLarge { .. })
    ));
    let none = r#"{"error":{"code":2010,"message":"Could not find routable point"}}"#;
    assert!(matches!(
        parse_directions_geojson(none),
        Err(OrsError::NoRoute)
    ));
}

#[test]
fn no_straight_line_fallback_symbol() {
    // Integration guard alongside ors.rs unit test.
    let ors = include_str!("../src/long_trip/ors.rs");
    assert!(!ors.contains("straight_line_between"));
    assert!(!ors.contains("fallback_chord"));
}

#[test]
#[ignore = "live ORS: set OPENROUTESERVICE_API_KEY"]
fn live_ors_directions_smoke() {
    let key = std::env::var("OPENROUTESERVICE_API_KEY").expect("OPENROUTESERVICE_API_KEY");
    let cfg = driver_break_core::long_trip::OrsConfig::from_parts(
        key,
        driver_break_core::long_trip::DEFAULT_ORS_BASE_URL,
    );
    let route = driver_break_core::long_trip::request_directions(
        &cfg,
        &[(53.334, 10.045), (53.551, 10.0)],
        None,
    )
    .expect("live ORS");
    assert!(route.lat_lon.len() >= 2);
    assert!(route.distance_m > 0.0);
}

#[test]
fn catalog_coverage_complete_for_us_fixture() {
    let (ids, _) = load_catalog_ids();
    let us: Vec<_> = ids
        .iter()
        .filter(|i| i.starts_with("north-america/us/"))
        .cloned()
        .collect();
    assert!(
        !us.is_empty(),
        "fixture current.json must publish US regions"
    );
    let cov = classify_catalog_coverage(&["north-america/us/new-york".into()], &ids, Some("us"));
    assert_eq!(cov, CatalogCoverage::Complete);
}

// Silence unused ReadyRegion import path when sizes used later.
#[allow(dead_code)]
fn _ready(id: &str) -> ReadyRegion {
    ReadyRegion {
        region_id: id.into(),
        generation: None,
        bytes: Some(1),
        manifest_url: None,
    }
}
