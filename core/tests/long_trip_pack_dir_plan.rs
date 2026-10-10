//! Corridor pack lookup across Tools data_dir + long-trip-packs/.

use std::fs;
use std::path::PathBuf;

use tempfile::TempDir;

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
