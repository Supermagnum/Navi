//! FTS place-name checks for common Norwegian car/hiking queries.
//!
//! Requires `core/target/integration-fixtures/ostlandet-latest.osm.pbf` (or
//! `NAME_INDEX_PBF` / `NAME_INDEX_DB` env overrides).

use std::path::{Path, PathBuf};

use driver_break_core::search::NameIndex;

fn fixture_pbf() -> PathBuf {
    if let Ok(p) = std::env::var("NAME_INDEX_PBF") {
        return PathBuf::from(p);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/integration-fixtures/ostlandet-latest.osm.pbf")
}

fn index_db_path() -> PathBuf {
    if let Ok(p) = std::env::var("NAME_INDEX_DB") {
        return PathBuf::from(p);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/integration-fixtures/place_index_search_check.db")
}

fn ensure_index(pbf: &Path, db: &Path) -> NameIndex {
    if db.exists() && NameIndex::is_current_schema(db) {
        return NameIndex::open(db).expect("open existing place index");
    }
    assert!(
        pbf.exists(),
        "OSM extract missing at {} — download/index region before blaming search logic",
        pbf.display()
    );
    if db.exists() {
        let _ = std::fs::remove_file(db);
    }
    let mut idx = NameIndex::open(db).expect("create place index");
    let n = idx.load_from_pbf(pbf).expect("index pbf");
    eprintln!("Indexed {n} named nodes from {}", pbf.display());
    idx
}

fn first_hit(idx: &NameIndex, query: &str) -> Option<(String, String, f64, f64)> {
    let hits = idx.search(query, 8).expect("search");
    hits.into_iter()
        .next()
        .map(|h| (h.name, h.kind, h.lat, h.lon))
}

#[test]
#[ignore = "needs ostlandet (or NAME_INDEX_*) place index fixtures"]
fn car_and_hiking_name_search_samples() {
    let pbf = fixture_pbf();
    let db = index_db_path();
    let idx = ensure_index(&pbf, &db);

    let cases: &[(&str, &str)] = &[
        ("Mer Extra Flisa", "car"),
        ("Circle K Mjøsstranda", "car"),
        ("Esso Express Hanestad", "car"),
        ("Vannbruksmuseum", "car"),
        ("Bjørnhollia", "hiking"),
        ("Haverdalsvegen 1740", "hiking"),
    ];

    let mut failures = Vec::new();
    for (q, profile) in cases {
        match first_hit(&idx, q) {
            Some((name, kind, lat, lon)) => {
                eprintln!(
                    "PASS [{profile}] {q:?} -> name={name:?} kind={kind} lat={lat:.5} lon={lon:.5}"
                );
            }
            None => {
                eprintln!(
                    "FAIL [{profile}] {q:?} -> zero results (index exists: {})",
                    db.exists()
                );
                failures.push(*q);
            }
        }
    }
    assert!(
        failures.is_empty(),
        "unresolved queries: {failures:?} (pbf={}, db={})",
        pbf.display(),
        db.display()
    );
}

#[test]
#[ignore = "needs ostlandet (or NAME_INDEX_*) place index fixtures"]
fn baberg_duplicate_farms_include_kommune_context() {
    let pbf = fixture_pbf();
    let db = index_db_path();
    let idx = ensure_index(&pbf, &db);
    let hits = idx.search("Båberg", 20).expect("search");
    let labels: Vec<String> = hits
        .iter()
        .map(|h| {
            driver_break_core::search::format_place_display(&h.name, &h.sub_area, &h.municipality)
        })
        .collect();
    eprintln!("Båberg hits: {labels:?}");
    let gjovik = hits.iter().any(|h| {
        h.name == "Båberg"
            && h.municipality.eq_ignore_ascii_case("Gjøvik")
            && h.sub_area.to_lowercase().contains("brattberg")
    });
    let ringsaker = hits.iter().any(|h| {
        h.name == "Båberg"
            && h.municipality.eq_ignore_ascii_case("Ringsaker")
            && h.sub_area.to_lowercase().contains("løken")
    });
    assert!(
        gjovik && ringsaker,
        "expected both Båberg farms with Brattberg/Gjøvik and Løken/Ringsaker, got {labels:?}"
    );
}
