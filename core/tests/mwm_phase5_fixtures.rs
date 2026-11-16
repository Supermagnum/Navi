//! Shape checks for the Phase 5 fixtures (no native engine needed).

use driver_break_core::mwm::comaps_search::is_known_category_key;
use serde_json::Value;
use std::collections::HashSet;

const SEARCH_SUITE: &str = include_str!("fixtures/mwm/search-suite-ostlandet.json");
const PARITY_SUITE: &str = include_str!("fixtures/mwm/parity-suite-ostlandet.json");

fn load(s: &str) -> Value {
    serde_json::from_str(s).expect("fixture is valid JSON")
}

#[test]
fn search_suite_shape() {
    let suite = load(SEARCH_SUITE);
    assert_eq!(suite["schema"], "navi-search-suite/v1");
    let queries = suite["queries"].as_array().expect("queries array");
    assert!(queries.len() >= 40, "got {}", queries.len());

    let mut ids = HashSet::new();
    let mut reverse = 0;
    let mut kinds = HashSet::new();
    for q in queries {
        let id = q["id"].as_str().expect("id");
        assert!(ids.insert(id.to_string()), "duplicate id {id}");
        let kind = q["kind"].as_str().expect("kind");
        let conf = q["confidence"].as_str().expect("confidence");
        assert!(matches!(conf, "high" | "medium"), "{id}: {conf}");
        match kind {
            "free_text" => {
                assert!(!q["query"].as_str().unwrap().is_empty(), "{id}");
                assert!(
                    q.get("expect_top_any").is_some()
                        || q.get("expect_address_any").is_some()
                        || q.get("min_results").is_some(),
                    "{id}: no expectation"
                );
            }
            "category" => {
                let key = q["query"].as_str().unwrap();
                assert!(is_known_category_key(key), "{id}: unknown key {key}");
                assert!(
                    q["near"]["lat"].is_f64() && q["near"]["lon"].is_f64(),
                    "{id}"
                );
            }
            "reverse" => {
                reverse += 1;
                let lat = q["lat"].as_f64().unwrap();
                let lon = q["lon"].as_f64().unwrap();
                // Ostlandet bounding box (generous).
                assert!(
                    (58.5..=62.5).contains(&lat) && (8.0..=12.5).contains(&lon),
                    "{id}"
                );
                assert!(
                    !q["expect_street_any"].as_array().unwrap().is_empty(),
                    "{id}"
                );
            }
            other => panic!("{id}: unknown kind {other}"),
        }
        kinds.insert(kind.to_string());
    }
    assert_eq!(reverse, 5);
    assert_eq!(kinds.len(), 3);

    // Required topic coverage.
    let tags: HashSet<&str> = queries
        .iter()
        .flat_map(|q| q["tags"].as_array().unwrap())
        .filter_map(|t| t.as_str())
        .collect();
    for needed in [
        "town",
        "street",
        "address",
        "postcode",
        "cabin",
        "shelter",
        "water",
        "fuel",
        "ev",
        "misspelling",
        "aeoa",
        "reverse",
    ] {
        assert!(tags.contains(needed), "missing tag {needed}");
    }
}

#[test]
fn parity_suite_expect_values() {
    let suite = load(PARITY_SUITE);
    let routes = suite["routes"].as_array().expect("routes");
    let mut ids = HashSet::new();
    for r in routes {
        let id = r["id"].as_str().unwrap();
        assert!(ids.insert(id.to_string()), "duplicate id {id}");
        let expect = r["expect"].as_str().unwrap_or("ok");
        assert!(
            matches!(expect, "ok" | "end_not_on_road" | "start_not_on_road"),
            "{id}: {expect}"
        );
    }
    let by_id = |id: &str| routes.iter().find(|r| r["id"] == id).unwrap();

    // A2: Valdresflye car runs on Rv51; the off-road variant must not auto-snap.
    assert_eq!(by_id("valdresflye_car")["expect"], "ok");
    let off = by_id("valdresflye_car_offroad");
    assert_eq!(off["expect"], "end_not_on_road");
    assert_eq!(off["profile"], "car");
    assert!(off["tags"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t == "end_not_on_road"));
}
