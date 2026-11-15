//! Manual / optional live integration: download Østlandet leaf set + sidecars.
//!
//! Ignored in CI (large: ~1.2 GiB MWMs). Run with:
//! `NAVI_MWM_CLIENT_ENABLED=1 cargo test -p driver-break-core --test mwm_live_ostlandet -- --ignored --nocapture`

use std::path::PathBuf;

use driver_break_core::mwm::{
    activate_release, discover_mirrors, download_leaf_set, fetch_and_verify_countries_txt,
    fetch_current_catalog, leaf_set_for_bake_id, parse_region_migration_json,
    region_ui_from_catalog, set_mwm_client_enabled_override, DownloadProgress,
    COMAPS_COUNTRIES_TXT_PUBKEY_HEX,
};
use driver_break_core::pack_server::{http_get_text, DEFAULT_PACK_SERVER_BASE_URL};

#[test]
#[ignore = "live network; downloads ~1.2GiB Ostlandet MWMs + sidecars"]
fn download_ostlandet_leaf_set_live() {
    set_mwm_client_enabled_override(Some(true));
    let origin = DEFAULT_PACK_SERVER_BASE_URL;
    let catalog = fetch_current_catalog(origin).expect("fetch catalog");
    assert_eq!(catalog.schema, "navi-mwm-catalog/v2");
    assert_eq!(catalog.version, "261105");
    assert_eq!(catalog.country_groups.len(), 224);
    assert_eq!(catalog.country_group_leaf_ids().len(), 1159);

    let ui = region_ui_from_catalog(&catalog);
    assert_eq!(ui.len(), 224);

    let leaves = catalog
        .leaves_for_bake_id("europe_norway_ostlandet")
        .expect("ostlandet mapping")
        .to_vec();
    assert_eq!(leaves.len(), 9);

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
    let from_mig = leaf_set_for_bake_id(&mig, "europe_norway_ostlandet").unwrap();
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

    let data_dir = PathBuf::from(std::env::temp_dir()).join("navi-mwm-live-ostlandet");
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
    for r in &results {
        assert!(r.mwm_path.is_file(), "{}", r.leaf_id);
        if r.warning.is_none() {
            assert!(r.navi_layers_available, "{}", r.leaf_id);
        }
    }
    activate_release(&data_dir, &catalog.version).expect("activate");
    set_mwm_client_enabled_override(None);
}
