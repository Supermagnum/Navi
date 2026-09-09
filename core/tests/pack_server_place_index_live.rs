//! Live pack-server install + Geofabrik PBF + place-index (no Android / emulator).
//!
//! Drives the same core path Tools uses after a green **Download region**:
//! `plan_region_acquisition` → `ensure_place_index_after_pack_install` → FTS search.
//!
//! Requires network access to the pack host chain and download.geofabrik.de.
//! Downloads are large (packs + PBF). Run explicitly:
//!
//! ```bash
//! cargo test -p driver-break-core --test pack_server_place_index_live -- --ignored --nocapture
//! ```
//!
//! Optional: `NAVI_PLACE_INDEX_LIVE_DIR=/path` to reuse a download cache across runs.

use std::path::PathBuf;
use std::time::Instant;

use driver_break_core::pack_server::{
    ensure_place_index_after_pack_install, plan_region_acquisition, PLACE_INDEX_DB_NAME,
};
use driver_break_core::search::NameIndex;

fn data_root() -> PathBuf {
    if let Ok(p) = std::env::var("NAVI_PLACE_INDEX_LIVE_DIR") {
        let p = PathBuf::from(p.trim());
        if !p.as_os_str().is_empty() {
            return p;
        }
    }
    std::env::temp_dir().join("navi-pack-place-index-live")
}

fn run_region(region_id: &str, query: &str, expect_substr: &str, force_rebuild: bool) {
    let root = data_root().join(region_id.replace('/', "_"));
    let _ = std::fs::create_dir_all(&root);
    eprintln!(
        "=== region={region_id} data_dir={} force_rebuild={force_rebuild} ===",
        root.display()
    );

    let t0 = Instant::now();
    let plan = plan_region_acquisition(region_id, None, Some(root.as_path()));
    eprintln!(
        "pack install: execute_local={} data_source={} reason={}",
        plan.execute_local_convert,
        plan.data_source.as_str(),
        plan.log_message
    );
    assert!(
        !plan.execute_local_convert,
        "expected pack-server install for {region_id}, got local: {}",
        plan.log_message
    );
    eprintln!(
        "pack install wall_ms={:.1}",
        t0.elapsed().as_secs_f64() * 1000.0
    );

    let t1 = Instant::now();
    eprintln!("place-index: start (Geofabrik PBF + NameIndex)");
    let report = ensure_place_index_after_pack_install(&root, region_id, force_rebuild)
        .unwrap_or_else(|e| panic!("place index failed for {region_id}: {e}"));
    eprintln!("{}", report.to_report_string());
    eprintln!(
        "place-index wall_ms={:.1} (pbf_ms={:.1} index_ms={:.1})",
        t1.elapsed().as_secs_f64() * 1000.0,
        report.pbf_ms,
        report.index_ms
    );
    assert!(report.pbf_bytes >= 1_000_000, "PBF too small");
    assert!(
        report.indexed > 0 || report.cache_hit,
        "expected indexed rows or cache hit"
    );
    let db = root.join(PLACE_INDEX_DB_NAME);
    assert!(NameIndex::has_entries(&db), "place_index.db has no rows");
    let idx = NameIndex::open(&db).expect("open place index");
    let hits = idx.search(query, 10).expect("search");
    eprintln!(
        "search query={query:?} hits={}",
        hits.iter()
            .map(|h| format!("{} ({})", h.name, h.kind))
            .collect::<Vec<_>>()
            .join("; ")
    );
    assert!(
        hits.iter().any(|h| h
            .name
            .to_lowercase()
            .contains(&expect_substr.to_lowercase())),
        "expected a hit containing {expect_substr:?}, got {:?}",
        hits.iter().map(|h| &h.name).collect::<Vec<_>>()
    );
}

#[test]
#[ignore = "live network: pack server + Geofabrik; large downloads"]
fn live_vestlandet_pack_then_place_index() {
    run_region("europe/norway/vestlandet", "Bergen", "Bergen", true);
}

#[test]
#[ignore = "live network: pack server + Geofabrik; large downloads"]
fn live_sweden_pack_then_place_index() {
    // Sweden product regions are län (PMT-splitter: europe/sweden/<lan>), not
    // the Geofabrik country extract. Never use europe/sweden for this test.
    use driver_break_core::pack_server::discover_pack_catalog;
    let cat = discover_pack_catalog(None);
    if let Some(reason) = cat.unreachable_reason.as_ref() {
        panic!("pack catalog unreachable: {reason}");
    }
    // Prefer smaller counties when several are published.
    const PREFERRED: &[&str] = &[
        "europe/sweden/gotland",
        "europe/sweden/blekinge",
        "europe/sweden/halland",
        "europe/sweden/kronoberg",
        "europe/sweden/stockholm",
    ];
    let lans: Vec<_> = cat
        .ready_region_ids
        .into_iter()
        .filter(|id| id.starts_with("europe/sweden/"))
        .collect();
    let region_id = PREFERRED
        .iter()
        .find(|p| lans.iter().any(|id| id == *p))
        .copied()
        .or_else(|| lans.first().map(|s| s.as_str()))
        .unwrap_or_else(|| {
            panic!(
                "no Sweden län packs in catalog (europe/sweden/*). \
                 PMT-splitter defines 21 län; live current.json only has \
                 country europe/sweden — refuse country-level Sweden for this test"
            )
        });
    eprintln!(
        "picked Sweden län {region_id} (from {} published län)",
        lans.len().max(1)
    );
    let leaf = region_id.rsplit('/').next().unwrap_or(region_id);
    let query = match leaf {
        "skane" => "Malmö",
        "stockholm" => "Stockholm",
        "gotland" => "Visby",
        "vastra-gotaland" => "Göteborg",
        "norrbotten" => "Luleå",
        other => other,
    };
    run_region(region_id, query, query, true);
}

#[test]
#[ignore = "live network: pack server + Geofabrik; large downloads"]
fn live_bremen_pack_then_place_index() {
    run_region("europe/germany/bremen", "Bremen", "Bremen", true);
}

#[test]
#[ignore = "live network: pack server + Geofabrik; re-download / re-index"]
fn live_bremen_update_reindexes() {
    // First install (or reuse cache dir).
    run_region("europe/germany/bremen", "Bremen", "Bremen", true);
    let root = data_root().join("europe_germany_bremen");
    let db = root.join(PLACE_INDEX_DB_NAME);
    let before = std::fs::metadata(&db)
        .expect("db after first")
        .modified()
        .ok();

    // Update path: packs again + force_rebuild place index.
    eprintln!("=== UPDATE path re-run for europe/germany/bremen ===");
    let plan = plan_region_acquisition("europe/germany/bremen", None, Some(root.as_path()));
    assert!(!plan.execute_local_convert, "{}", plan.log_message);
    let report = ensure_place_index_after_pack_install(&root, "europe/germany/bremen", true)
        .expect("reindex");
    eprintln!("update place-index: {}", report.to_report_string());
    assert!(!report.cache_hit, "update must rebuild, not cache-hit");
    assert!(report.indexed > 0);
    let after = std::fs::metadata(&db)
        .expect("db after update")
        .modified()
        .ok();
    eprintln!("mtime before={before:?} after={after:?}");
    let idx = NameIndex::open(&db).expect("open");
    let hits = idx.search("Bremen", 5).expect("search");
    assert!(hits.iter().any(|h| h.name.contains("Bremen")));
}
