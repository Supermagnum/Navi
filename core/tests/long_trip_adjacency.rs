//! Phase A1: adjacency-graph corridor (default) — four trips + graph stats.

use driver_break_core::long_trip::{
    adjacency_edge_count, adjacency_isolates, adjacency_named_links, adjacency_region_count,
    estimate_trip_disk_bytes, ordered_needed_regions_along_route_filtered,
    ordered_needed_regions_for_trip, parse_brouter_geojson, region_containing,
    warm_region_adjacency, CatalogSizeLookup, MissingCorridor, LONG_TRIP_CORRIDOR_BUFFER_KM,
};
use driver_break_core::pack_server::catalog_entries_from_ready_ids;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/long_trip")
        .join(name)
}

fn load_catalog() -> (Vec<String>, BTreeMap<String, u64>) {
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
    let cat: Cat = serde_json::from_str(&raw).expect("parse");
    let ids: Vec<String> = cat.regions.iter().map(|r| r.region_id.clone()).collect();
    let sizes: BTreeMap<String, u64> = cat
        .regions
        .iter()
        .map(|r| (r.region_id.clone(), r.bytes.unwrap_or(0)))
        .collect();
    (ids, sizes)
}

struct SizeMap(BTreeMap<String, u64>);
impl CatalogSizeLookup for SizeMap {
    fn pack_bytes_for_region(&self, region_id: &str) -> Option<u64> {
        self.0.get(region_id).copied()
    }
}

fn us_endpoints() -> ((f64, f64), (f64, f64), (f64, f64)) {
    let us: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(fixture("us_endpoints.json")).unwrap())
            .unwrap();
    let start = (
        us["points"]["red_ball_garage"]["lat"].as_f64().unwrap(),
        us["points"]["red_ball_garage"]["lon"].as_f64().unwrap(),
    );
    let dest_a = (
        us["points"]["portofino_hotel"]["lat"].as_f64().unwrap(),
        us["points"]["portofino_hotel"]["lon"].as_f64().unwrap(),
    );
    let dest_b = (
        us["points"]["north_coast_inn"]["lat"].as_f64().unwrap(),
        us["points"]["north_coast_inn"]["lon"].as_f64().unwrap(),
    );
    (start, dest_a, dest_b)
}

fn recorded_brouter_regions(
    name: &str,
    installed: &[String],
    country_iso: Option<&str>,
) -> Vec<String> {
    let path = fixture(&format!("recorded/{name}"));
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
    let route = parse_brouter_geojson(body).unwrap();
    let (ids, _) = load_catalog();
    let entries = catalog_entries_from_ready_ids(&ids);
    ordered_needed_regions_along_route_filtered(
        &route.lat_lon,
        &entries,
        installed,
        LONG_TRIP_CORRIDOR_BUFFER_KM,
        country_iso,
    )
}

#[test]
fn graph_stats_and_named_links() {
    let n = warm_region_adjacency();
    let e = adjacency_edge_count();
    let isolates = adjacency_isolates();
    let links = adjacency_named_links();
    eprintln!(
        "ADJACENCY regions={n} edges={e} isolates={}",
        isolates.len()
    );
    for id in &isolates {
        eprintln!("  isolate: {id}");
    }
    for (a, b, note) in &links {
        eprintln!("  named_link: {a} <-> {b} ({note})");
    }
    assert_eq!(n, adjacency_region_count());
    assert_eq!(n, 116);
    assert!(e > 150, "edges={e}");
    assert!(isolates.contains(&"europe/sweden/gotland"));
    assert!(links
        .iter()
        .any(|(_, _, n)| n.to_ascii_lowercase().contains("oresund")));
    assert!(
        links
            .iter()
            .all(|(a, b, _)| !a.contains("hedmark") && !b.contains("hedmark")),
        "hedmark must not appear in named links: {links:?}"
    );
}

#[test]
fn trip_klecken_innlandet_vs_recorded_brouter() {
    let installed = vec!["europe/germany/niedersachsen".into()];
    let adj =
        ordered_needed_regions_for_trip(&[(53.334, 10.045), (61.593, 10.332)], &installed, None)
            .expect("adjacency corridor");
    let brouter =
        recorded_brouter_regions("brouter_klecken_innlandet_car-eco.json", &installed, None);
    eprintln!("Klecken adjacency ({}): {adj:?}", adj.len());
    eprintln!("Klecken brouter densify ({}): {brouter:?}", brouter.len());

    for stem in [
        "schleswig-holstein",
        "denmark",
        "skane",
        "halland",
        "vastra_gotaland",
        "ostlandet",
    ] {
        assert!(
            adj.iter().any(|r| r.contains(stem)),
            "adjacency missing {stem} in {adj:?}"
        );
    }
    assert!(
        adj.iter().all(|r| !r.contains("hedmark")),
        "hedmark must not appear in corridor: {adj:?}"
    );
    // Hop-count skips Hamburg when Niedersachsen borders Schleswig-Holstein
    // directly; recorded road corridor still crosses Hamburg. Report, do not
    // force-equalize.
    let only_brouter: Vec<_> = brouter
        .iter()
        .filter(|r| !adj.iter().any(|a| a == *r))
        .cloned()
        .collect();
    let only_adj: Vec<_> = adj
        .iter()
        .filter(|r| !brouter.iter().any(|b| b == *r))
        .cloned()
        .collect();
    eprintln!("only in brouter densify: {only_brouter:?}");
    eprintln!("only in adjacency: {only_adj:?}");
    assert!(
        adj.len() >= 6,
        "expected >=6 regions after dropping niedersachsen, got {}: {adj:?}",
        adj.len()
    );
    assert_eq!(
        adj.last().map(|s| s.as_str()),
        Some("europe/norway/ostlandet")
    );
}

#[test]
fn trip_kautokeino_roros_norway_only() {
    // Kautokeino ~69.01/23.04, Roros ~62.57/11.38
    let needed =
        ordered_needed_regions_for_trip(&[(69.01, 23.04), (62.57, 11.38)], &[], Some("no"))
            .expect("NO corridor");
    eprintln!("Kautokeino→Roros adjacency: {needed:?}");
    assert!(!needed.is_empty());
    for r in &needed {
        assert!(
            r.starts_with("europe/norway"),
            "Norway-only leaked {r} in {needed:?}"
        );
    }
    assert!(needed.iter().any(|r| r.contains("nord-norge")));
    assert!(needed
        .iter()
        .any(|r| r.contains("trondelag") || r.contains("ostlandet")));
}

#[test]
fn trip_us_a_and_b_real_geometry_storage() {
    let (start, dest_a, dest_b) = us_endpoints();
    assert_eq!(
        region_containing(start.0, start.1, Some("us")),
        Some("north-america/us/new-york")
    );
    assert_eq!(
        region_containing(dest_a.0, dest_a.1, Some("us")),
        Some("north-america/us/california/socal")
    );
    assert_eq!(
        region_containing(dest_b.0, dest_b.1, Some("us")),
        Some("north-america/us/california/norcal")
    );

    let a = ordered_needed_regions_for_trip(&[start, dest_a], &[], Some("us")).unwrap();
    let b = ordered_needed_regions_for_trip(&[start, dest_b], &[], Some("us")).unwrap();
    eprintln!("US A Red Ball→Portofino ({}): {a:?}", a.len());
    eprintln!("US B Red Ball→Crescent ({}): {b:?}", b.len());

    assert!(a.len() >= 5, "US A too short: {a:?}");
    assert!(b.len() >= 5, "US B too short: {b:?}");
    assert!(a.last().unwrap().contains("socal"));
    assert!(b.last().unwrap().contains("norcal"));
    for r in a.iter().chain(b.iter()) {
        assert!(r.starts_with("north-america/us/"), "non-state leaf {r}");
        assert!(
            !r.contains("us-midwest")
                && !r.contains("us-northeast")
                && !r.contains("us-pacific")
                && !r.contains("us-south")
                && !r.contains("us-west"),
            "multi-state aggregate in corridor: {r}"
        );
    }

    let (_, sizes) = load_catalog();
    let lookup = SizeMap(sizes);
    let est_a = estimate_trip_disk_bytes(&a, &lookup, 512u64 * 1024 * 1024 * 1024);
    let est_b = estimate_trip_disk_bytes(&b, &lookup, 512u64 * 1024 * 1024 * 1024);
    eprintln!("US A storage512={est_a:?}");
    eprintln!("US B storage512={est_b:?}");
    // REAL: every region has a catalog byte size from current.json.
    for id in a.iter().chain(b.iter()) {
        assert!(
            lookup.pack_bytes_for_region(id).unwrap_or(0) > 0,
            "missing real catalog bytes for {id}"
        );
    }
}

#[test]
fn trip_with_via_concatenates_and_dedupes() {
    // Klecken → Copenhagen-ish (denmark) → Innlandet: via should not duplicate DK.
    let needed = ordered_needed_regions_for_trip(
        &[(53.334, 10.045), (55.676, 12.568), (61.593, 10.332)],
        &["europe/germany/niedersachsen".into()],
        None,
    )
    .unwrap();
    eprintln!("via trip: {needed:?}");
    let mut seen = std::collections::BTreeSet::new();
    for r in &needed {
        assert!(seen.insert(r.clone()), "duplicate {r} in {needed:?}");
    }
    assert!(needed.iter().any(|r| r.contains("denmark")));
    assert_eq!(
        needed.last().map(|s| s.as_str()),
        Some("europe/norway/ostlandet")
    );
}

#[test]
fn gotland_fails_missing_corridor() {
    let err = ordered_needed_regions_for_trip(&[(57.63, 18.29), (59.33, 18.07)], &[], Some("se"))
        .unwrap_err();
    assert!(matches!(err, MissingCorridor::NoPath { .. }), "{err}");
}
