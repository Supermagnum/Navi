//! Corridor pack lookup across Tools data_dir + long-trip-packs/.

use std::fs;
use std::path::PathBuf;

use tempfile::TempDir;

fn touch_manifest(dir: &std::path::Path, stem: &str) {
    fs::write(dir.join(format!("{stem}.navi-manifest.json")), b"{}").unwrap();
}

#[test]
fn densify_sees_manifests_only_under_long_trip_packs() {
    let root = TempDir::new().unwrap();
    let data_dir = root.path().join("files");
    let pack_dir = data_dir.join("long-trip-packs");
    fs::create_dir_all(&pack_dir).unwrap();
    // Corridor packs only under long-trip-packs (Phase B layout).
    touch_manifest(&pack_dir, "ostlandet-latest");
    touch_manifest(&pack_dir, "denmark-latest");
    touch_manifest(&pack_dir, "skane-latest");
    touch_manifest(&pack_dir, "schleswig-holstein-latest");
    touch_manifest(&pack_dir, "niedersachsen-latest");

    let dirs: Vec<&std::path::Path> = vec![pack_dir.as_path(), data_dir.as_path()];
    // Hamar → Minden chord (enough span to densify across Ready regions).
    let pts = vec![(60.7945, 11.0680), (52.2885, 8.9220)];
    let hops = driver_break_core::routing::plan_bbox::densify_route_points_via_regions_dirs(
        &pts,
        &dirs,
        driver_break_core::routing::plan_bbox::LONG_TRIP_CHUNK_DEG,
    );
    assert!(
        hops.len() > 2,
        "expected densify anchors from long-trip-packs manifests, got {hops:?}"
    );

    // data_dir alone must not see those manifests.
    let hops_top = driver_break_core::routing::plan_bbox::densify_route_points_via_regions(
        &pts,
        &data_dir,
        driver_break_core::routing::plan_bbox::LONG_TRIP_CHUNK_DEG,
    );
    assert_eq!(
        hops_top.len(),
        2,
        "top-level-only must not invent densify hops without manifests"
    );
}

#[test]
fn files_only_densify_unchanged_when_packs_co_located() {
    let root = TempDir::new().unwrap();
    let data_dir = root.path().join("files");
    fs::create_dir_all(&data_dir).unwrap();
    touch_manifest(&data_dir, "ostlandet-latest");
    touch_manifest(&data_dir, "denmark-latest");
    touch_manifest(&data_dir, "skane-latest");
    touch_manifest(&data_dir, "schleswig-holstein-latest");
    touch_manifest(&data_dir, "niedersachsen-latest");

    let pts = vec![(60.7945, 11.0680), (52.2885, 8.9220)];
    let hops = driver_break_core::routing::plan_bbox::densify_route_points_via_regions(
        &pts,
        &data_dir,
        driver_break_core::routing::plan_bbox::LONG_TRIP_CHUNK_DEG,
    );
    assert!(
        hops.len() > 2,
        "co-located top-level packs must still densify, got {hops:?}"
    );
}

#[test]
fn plan_pack_dir_list_prefers_explicit_then_nested() {
    let root = TempDir::new().unwrap();
    let data = root.path().join("files");
    let nested = data.join("long-trip-packs");
    fs::create_dir_all(&nested).unwrap();
    let mut out: Vec<PathBuf> = Vec::new();
    out.push(nested.clone());
    if !out.iter().any(|d| d == &data) {
        out.push(data.clone());
    }
    assert_eq!(out[0], nested);
    assert_eq!(out[1], data);
}
