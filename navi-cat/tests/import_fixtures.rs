//! Importer coverage against `testdata/cat/` (AnyTone, OSM corridor, OpenRepeater).
//! Results recorded in docs/cat-test.md.

use std::fs;
use std::path::PathBuf;

use navi_cat::importers::{
    decode_cps_bytes, ensure_cat_import_dir, import_anytone_channel_csv, import_anytone_offset_csv,
    import_openrepeater_json, import_osm_json, import_repeaterbook, CAT_IMPORT_REL,
};
use navi_cat::repeater::{is_aprs, RepeaterDb, RepeaterSource};

fn cat_testdata() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata/cat")
}

#[test]
fn anytone_channel_utf8_filters_and_dedupes() {
    let path = cat_testdata().join("anytone/channel.csv");
    let csv = fs::read_to_string(&path).expect("channel.csv");
    let db = RepeaterDb::open_memory().unwrap();
    let n = import_anytone_channel_csv(&db, &csv).unwrap();
    // 5 analog duplex + 2 unique DMR (Gjovik TG rows deduped; Kongs kept); APRS+simplex out
    assert_eq!(n, 7, "expected 7 sites after APRS/simplex/DMR dedupe");
    let near = db.query_near(60.563, 11.257, 150.0, None);
    // CSV-only has no position → not in distance auto-tune set
    assert!(near.is_empty());
}

#[test]
fn anytone_channel_windows1252_same_count() {
    let path = cat_testdata().join("anytone/channel_windows1252.csv");
    let bytes = fs::read(&path).expect("channel_windows1252.csv");
    let csv = decode_cps_bytes(&bytes);
    let db = RepeaterDb::open_memory().unwrap();
    let n = import_anytone_channel_csv(&db, &csv).unwrap();
    assert_eq!(n, 7);
}

#[test]
fn anytone_offset_header_only_ok() {
    let path = cat_testdata().join("anytone/offset.csv");
    let csv = fs::read_to_string(&path).expect("offset.csv");
    let db = RepeaterDb::open_memory().unwrap();
    let n = import_anytone_offset_csv(&db, &csv).unwrap();
    assert_eq!(n, 0);
}

#[test]
fn non_networked_osm_fixture_loads() {
    let path = cat_testdata().join("non_networked.json");
    let json = fs::read_to_string(&path).expect("non_networked.json");
    let db = RepeaterDb::open_memory().unwrap();
    let n = import_osm_json(&db, &json).unwrap();
    assert_eq!(n, 18);
    let near = db.query_near(60.563, 11.257, 150.0, None);
    assert!(near.iter().all(|s| !is_aprs(s)));
    assert!(near.iter().any(|s| s.callsign == "LA6GR"));
    assert!(near
        .iter()
        .any(|s| matches!(s.source, RepeaterSource::Osm)));
}

#[test]
fn openrepeater_norway_empty_ok() {
    let path = cat_testdata().join("openrepeater/norway_attempt.json");
    let json = fs::read_to_string(&path).expect("norway_attempt.json");
    let db = RepeaterDb::open_memory().unwrap();
    let n = import_openrepeater_json(&db, &json).unwrap();
    assert_eq!(n, 0);
}

#[test]
fn repeaterbook_stays_disabled() {
    let db = RepeaterDb::open_memory().unwrap();
    assert!(import_repeaterbook(&db, "Innlandet").is_err());
}

#[test]
fn cat_import_rel_stable() {
    assert_eq!(CAT_IMPORT_REL, "cat/import");
    let tmp = tempfile::tempdir().unwrap();
    let dir = ensure_cat_import_dir(tmp.path()).unwrap();
    assert!(dir.is_dir());
}
