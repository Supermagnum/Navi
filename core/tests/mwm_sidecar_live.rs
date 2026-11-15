//! Live / performance sidecar reads (ignored in CI).
//!
//! ```text
//! cargo test -p driver-break-core --test mwm_sidecar_live -- --ignored --nocapture
//! ```

use std::path::PathBuf;
use std::time::Instant;

use driver_break_core::mwm::{
    fetch_current_catalog, set_mwm_client_enabled_override, SidecarReader,
};
use driver_break_core::pack_server::{http_get_bytes, DEFAULT_PACK_SERVER_BASE_URL};

fn download_sidecar(origin: &str, rel: &str, dest: &PathBuf) {
    let url = format!("{}/mwm/{rel}", origin.trim_end_matches('/'));
    let bytes = http_get_bytes(&url, std::time::Duration::from_secs(600)).expect("download");
    std::fs::write(dest, bytes).unwrap();
}

fn rss_kib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            let kib: u64 = rest.split_whitespace().next()?.parse().ok()?;
            return Some(kib);
        }
    }
    None
}

#[test]
#[ignore = "live network; downloads Ostlandet + Bolivia sidecars"]
fn read_ostlandet_sidecars_and_report_rss() {
    set_mwm_client_enabled_override(Some(true));
    let origin = DEFAULT_PACK_SERVER_BASE_URL;
    let catalog = fetch_current_catalog(origin).expect("catalog");
    assert_eq!(catalog.version, "261105");
    let leaves = catalog
        .leaves_for_bake_id("europe_norway_ostlandet")
        .expect("ostlandet")
        .to_vec();
    assert_eq!(leaves.len(), 9);

    let dir = PathBuf::from(std::env::temp_dir()).join("navi-mwm-sidecar-live");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let mut largest_ost: Option<(String, u64)> = None;
    for id in &leaves {
        let leaf = catalog.leaf(id).expect("leaf");
        let rel = leaf.sidecar_url.as_deref().expect("sidecar_url");
        let dest = dir.join(format!("{id}.navi-sidecar.v1.zst"));
        download_sidecar(origin, rel, &dest);
        let meta_len = std::fs::metadata(&dest).unwrap().len();
        match &largest_ost {
            None => largest_ost = Some((id.clone(), meta_len)),
            Some((_, n)) if meta_len > *n => largest_ost = Some((id.clone(), meta_len)),
            _ => {}
        }
        let t0 = Instant::now();
        let rss0 = rss_kib();
        let r = SidecarReader::open_path(&dest).expect("open");
        let counts = r.inspect_counts().expect("inspect");
        let _ = r.attribution_json().expect("attr");
        let elapsed = t0.elapsed();
        let rss1 = rss_kib();
        println!(
            "ostlandet leaf={id} sidecar_bytes={meta_len} wetland={} soft={} hard={} boardwalk={} elev_f={} elev_p={} load_ms={:.1} rss_kib_before={rss0:?} after={rss1:?}",
            counts.wetland_records,
            counts.soft_records,
            counts.hard_records,
            counts.boardwalk_records,
            counts.elevation_features,
            counts.elevation_points,
            elapsed.as_secs_f64() * 1000.0
        );
    }

    // Large non-Østlandet leaf (Bolivia_South ~7.4 MiB sidecar).
    let bolivia = catalog
        .leaf("Bolivia_South")
        .expect("Bolivia_South in live catalog");
    let dest = dir.join("Bolivia_South.navi-sidecar.v1.zst");
    download_sidecar(origin, bolivia.sidecar_url.as_deref().unwrap(), &dest);
    let meta_len = std::fs::metadata(&dest).unwrap().len();
    let t0 = Instant::now();
    let rss0 = rss_kib();
    let r = SidecarReader::open_path(&dest).unwrap();
    let counts = r.inspect_counts().unwrap();
    let _ = r.attribution_json().unwrap();
    let elapsed = t0.elapsed();
    let rss1 = rss_kib();
    println!(
        "large leaf=Bolivia_South sidecar_bytes={meta_len} wetland={} elev_f={} elev_p={} load_ms={:.1} rss_kib_before={rss0:?} after={rss1:?}",
        counts.wetland_records,
        counts.elevation_features,
        counts.elevation_points,
        elapsed.as_secs_f64() * 1000.0
    );
    if let Some((id, n)) = largest_ost {
        println!("largest_ostlandet_sidecar={id} bytes={n}");
    }
    set_mwm_client_enabled_override(None);
}
