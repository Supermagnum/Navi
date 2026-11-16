//! Manual download hook for the elevation band measurement leaves.
//!
//! `ComapsPhase4CloseoutInstrumentedTest.phase5BandRejectRemoteLeaves` measures
//! `band_would_reject_75m` / `sidecar_hits` on Bolivia_South and China_Tibet,
//! which are not bundled with the app. Ignored in CI (network). Run once:
//!
//! `cargo test -p driver-break-core --test mwm_live_band_leaves -- --ignored --nocapture`
//!
//! It downloads both leaves and their sidecars (plus the World overlays the
//! downloader always adds) into a temp dir and prints the `adb push` commands
//! for the device layout the instrumented tests use:
//! `<externalFilesDir>/navi-mwm-test/261105/<leaf>.mwm` and
//! `<externalFilesDir>/navi-mwm-test/sidecars/<leaf>.navi-sidecar.v1.zst`.
//! Use `adb -s <serial>` on a physical device.

use driver_break_core::mwm::{
    discover_mirrors, download_leaf_set, fetch_and_verify_countries_txt, fetch_current_catalog,
    set_mwm_client_enabled_override, DownloadProgress, COMAPS_COUNTRIES_TXT_PUBKEY_HEX,
};
use driver_break_core::pack_server::DEFAULT_PACK_SERVER_BASE_URL;

const BAND_LEAVES: &[&str] = &["Bolivia_South", "China_Tibet"];
const DEVICE_ROOT: &str = "/sdcard/Android/data/no.navi.app/files/navi-mwm-test";

#[test]
#[ignore = "live network; downloads Bolivia_South and China_Tibet MWMs + sidecars"]
fn download_band_measurement_leaves_live() {
    set_mwm_client_enabled_override(Some(true));
    let origin = DEFAULT_PACK_SERVER_BASE_URL;
    let catalog = fetch_current_catalog(origin).expect("fetch catalog");

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

    let data_dir = std::env::temp_dir().join("navi-mwm-band-leaves");
    let _ = std::fs::remove_dir_all(&data_dir);
    std::fs::create_dir_all(&data_dir).unwrap();

    let leaf_ids: Vec<String> = BAND_LEAVES.iter().map(|s| (*s).to_string()).collect();
    let mut progress = DownloadProgress::default();
    let results = download_leaf_set(
        &data_dir,
        &catalog,
        &countries,
        &discovery,
        origin,
        &leaf_ids,
        &mut progress,
    )
    .expect("download leaf set");

    println!(
        "=== band leaves (release {}) ===\n# push with: adb -s <serial> ...",
        catalog.version
    );
    for r in &results {
        assert!(r.mwm_path.is_file(), "{}", r.leaf_id);
        let mwm_name = r.mwm_path.file_name().unwrap().to_string_lossy();
        println!(
            "adb push {} {DEVICE_ROOT}/{}/{mwm_name}",
            r.mwm_path.display(),
            catalog.version
        );
        if let Some(sc) = &r.sidecar_path {
            let sc_name = sc.file_name().unwrap().to_string_lossy();
            println!("adb push {} {DEVICE_ROOT}/sidecars/{sc_name}", sc.display());
        }
    }
    set_mwm_client_enabled_override(None);
}
