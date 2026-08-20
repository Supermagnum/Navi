//! Rasterize vendored Norwegian road-sign icons (NLOD catalogue).

use driver_break_core::icons::{rasterize_key, IconTheme};
use driver_break_core::routing::road_sign::load_catalog;
use std::collections::HashMap;
use std::path::PathBuf;

fn icons_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/icons")
}

#[test]
fn road_sign_samples_rasterize() {
    let bundled = icons_root();
    for key in [
        "no_sign_100_1",
        "no_sign_104_1",
        "no_sign_366",
        "no_sign_640_10",
        "no_sign_755",
    ] {
        let rgba = rasterize_key(key, IconTheme::Day, 64, 64, None, &bundled)
            .unwrap_or_else(|e| panic!("{key}: {e}"));
        assert!(rgba.len() > 64 * 64, "{key} empty raster");
    }
}

#[test]
fn excluded_signs_not_in_catalog_match() {
    let cat = load_catalog().expect("catalog");
    let mut tags = HashMap::new();
    tags.insert("traffic_sign".into(), "NO:812".into());
    assert!(cat.match_node_tags(&tags).is_none());
    tags.insert("traffic_sign".into(), "NO:140".into());
    assert!(cat.match_node_tags(&tags).is_none());
}
