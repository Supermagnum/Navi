//! Manual / optional live integration: download Østlandet leaf set + sidecars.
//!
//! Ignored in CI (large: ~1.2 GiB MWMs). Run with:
//! `cargo test -p driver-break-core --test mwm_live_ostlandet -- --ignored --nocapture`
//!
//! Flag defaults ON; force classic path with `NAVI_MWM_CLIENT_ENABLED=0`.

use std::time::Instant;

use driver_break_core::mwm::{
    activate_release, check_local_mwm_fingerprint, discover_mirrors, download_leaf_set,
    fetch_and_verify_countries_txt, fetch_current_catalog, fingerprint_from_catalog_leaf,
    leaf_set_for_bake_id, parse_region_migration_json, region_ui_from_catalog,
    set_mwm_client_enabled_override, sha256_hex_of_path, DownloadProgress, FingerprintStatus,
    COMAPS_COUNTRIES_TXT_PUBKEY_HEX,
};
use driver_break_core::pack_server::{http_get_text, DEFAULT_PACK_SERVER_BASE_URL};

#[test]
#[ignore = "live network; downloads ~1.2GiB Ostlandet MWMs + sidecars"]
fn download_ostlandet_leaf_set_live() {
    set_mwm_client_enabled_override(Some(true));
    let started = Instant::now();
    let origin = DEFAULT_PACK_SERVER_BASE_URL;
    let catalog = fetch_current_catalog(origin).expect("fetch catalog");
    assert_eq!(catalog.schema, "navi-mwm-catalog/v2");
    assert_eq!(catalog.version, "261105");
    assert_eq!(catalog.country_groups.len(), 224);
    assert_eq!(catalog.country_group_leaf_ids().len(), 1159);

    let ui = region_ui_from_catalog(&catalog);
    assert_eq!(ui.len(), 224);

    let raw = catalog
        .leaves_for_bake_id("europe_norway_ostlandet")
        .expect("ostlandet mapping")
        .to_vec();
    let leaves = driver_break_core::mwm::install_leaves_for_region("europe_norway_ostlandet", &raw);
    assert!(
        leaves.iter().all(|id| id.starts_with("Norway_")),
        "Østlandet install must not include foreign leaves: {leaves:?} (raw {raw:?})"
    );
    assert!(
        leaves.iter().any(|id| id == "Norway_Hedmark"),
        "Hedmark missing from {leaves:?}"
    );

    let mig_url = format!(
        "{}/mwm/{}",
        origin,
        catalog
            .region_migration_url
            .as_deref()
            .unwrap_or("261105/region-migration.json")
    );
    let mig_text = http_get_text(&mig_url, std::time::Duration::from_secs(120)).unwrap();
    let mig = parse_region_migration_json(&mig_text).unwrap();
    let from_mig_raw = leaf_set_for_bake_id(&mig, "europe_norway_ostlandet").unwrap();
    let from_mig =
        driver_break_core::mwm::install_leaves_for_region("europe_norway_ostlandet", &from_mig_raw);
    assert_eq!(from_mig, leaves);

    let discovery = discover_mirrors(
        &catalog.comaps.metaserver_url,
        &catalog.comaps.map_series,
        Some(catalog.comaps.release_version),
    )
    .expect("comaps discovery");
    let pubkey = if catalog.comaps.countries_txt_pubkey_hex.is_empty() {
        COMAPS_COUNTRIES_TXT_PUBKEY_HEX
    } else {
        catalog.comaps.countries_txt_pubkey_hex.as_str()
    };
    let countries = fetch_and_verify_countries_txt(
        &discovery,
        Some(&catalog.comaps.countries_txt_sha256),
        Some(catalog.comaps.countries_txt_bytes),
        pubkey,
    )
    .expect("countries.txt verify");

    let data_dir = std::env::temp_dir().join("navi-mwm-live-ostlandet");
    let _ = std::fs::remove_dir_all(&data_dir);
    std::fs::create_dir_all(&data_dir).unwrap();

    let mut progress = DownloadProgress::default();
    let results = download_leaf_set(
        &data_dir,
        &catalog,
        &countries,
        &discovery,
        origin,
        &leaves,
        &mut progress,
    )
    .expect("download leaf set");
    assert_eq!(results.len(), leaves.len());

    let mut total_mwm: u64 = 0;
    let mut total_sidecar: u64 = 0;
    println!(
        "=== Ostlandet live download report (release {}) ===",
        catalog.version
    );
    for r in &results {
        assert!(r.mwm_path.is_file(), "{}", r.leaf_id);
        let mwm_len = std::fs::metadata(&r.mwm_path).unwrap().len();
        let mwm_sha = sha256_hex_of_path(&r.mwm_path).expect("mwm sha");
        total_mwm += mwm_len;

        let leaf = catalog.leaf(&r.leaf_id).expect("leaf in catalog");
        let expected = fingerprint_from_catalog_leaf(leaf).expect("fingerprint fields");
        let fp = check_local_mwm_fingerprint(&r.leaf_id, &r.mwm_path, &expected).unwrap();
        assert!(
            matches!(fp, FingerprintStatus::Match),
            "fingerprint mismatch for {}: {:?}",
            r.leaf_id,
            fp
        );

        let (sc_len, sc_sha) = if let Some(ref sc) = r.sidecar_path {
            assert!(sc.is_file(), "sidecar {}", r.leaf_id);
            let n = std::fs::metadata(sc).unwrap().len();
            let sha = sha256_hex_of_path(sc).expect("sidecar sha");
            total_sidecar += n;
            (Some(n), Some(sha))
        } else {
            (None, None)
        };

        if r.warning.is_none() {
            assert!(r.navi_layers_available, "{}", r.leaf_id);
        }

        println!(
            "leaf={}\tmwm_bytes={}\tmwm_sha256={}\tsidecar_bytes={:?}\tsidecar_sha256={:?}\tfp={:?}\tnavi_layers={}\twarning={:?}",
            r.leaf_id,
            mwm_len,
            mwm_sha,
            sc_len,
            sc_sha,
            fp,
            r.navi_layers_available,
            r.warning
        );
    }
    activate_release(&data_dir, &catalog.version).expect("activate");
    let elapsed = started.elapsed();
    println!(
        "TOTAL mwm_bytes={} sidecar_bytes={} sum={} elapsed_secs={:.1} data_dir={}",
        total_mwm,
        total_sidecar,
        total_mwm + total_sidecar,
        elapsed.as_secs_f64(),
        data_dir.display()
    );
    // Catalog metadata expectation for Østlandet on 261105.
    assert_eq!(total_mwm, 1_228_846_805);
    assert_eq!(total_sidecar, 6_038_192);
    set_mwm_client_enabled_override(None);
}
