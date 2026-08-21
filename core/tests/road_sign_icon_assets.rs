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
        "no_sign_362_20",
        "no_sign_109",
        "no_sign_142",
    ] {
        let path = driver_break_core::icons::resolve_icon(key, IconTheme::Day, None, &bundled);
        assert!(
            !path.ends_with("unknown.svg"),
            "{key} must not fall back to unknown.svg (got {})",
            path.display()
        );
        let rgba = rasterize_key(key, IconTheme::Day, 64, 64, None, &bundled)
            .unwrap_or_else(|e| panic!("{key}: {e}"));
        assert!(rgba.len() > 64 * 64, "{key} empty raster");
    }
}

#[test]
fn all_catalog_svgs_have_bundled_icons() {
    let bundled = icons_root();
    let tags_path = bundled.join("road-signs/database/osm_tags.json");
    let raw = std::fs::read_to_string(&tags_path).expect("osm_tags.json");
    let v: serde_json::Value = serde_json::from_str(&raw).expect("json");
    let mut missing = Vec::new();
    let mut raster_fail = Vec::new();
    for sign in v["signs"].as_array().expect("signs") {
        if sign.get("svg").and_then(|x| x.as_str()).is_none() {
            continue;
        }
        let status = sign
            .get("match_status")
            .and_then(|x| x.as_str())
            .unwrap_or("");
        if status == "variable_content" || status == "not_for_navigation" {
            continue;
        }
        if sign
            .get("navi_usable_as_fixed_symbol")
            .and_then(|x| x.as_bool())
            != Some(true)
        {
            continue;
        }
        let code = sign.get("code").and_then(|x| x.as_str()).unwrap_or("");
        let mut key = String::from("no_sign_");
        for ch in code.chars() {
            if ch.is_ascii_alphanumeric() {
                key.push(ch);
            } else {
                key.push('_');
            }
        }
        let key = key.trim_matches('_').to_string();
        let path = driver_break_core::icons::resolve_icon(&key, IconTheme::Day, None, &bundled);
        if path.ends_with("unknown.svg") {
            missing.push(format!("{code} -> {key}"));
            continue;
        }
        if let Err(e) = rasterize_key(&key, IconTheme::Day, 48, 48, None, &bundled) {
            raster_fail.push(format!("{key}: {e}"));
        }
    }
    assert!(
        missing.is_empty(),
        "catalogue icons missing from bundle: {missing:?}"
    );
    assert!(
        raster_fail.is_empty(),
        "catalogue icons failed to rasterize: {raster_fail:?}"
    );
}

#[test]
fn speed_limit_cone_plates_all_resolve() {
    let bundled = icons_root();
    for key in [
        "no_sign_362_5",
        "no_sign_362_10",
        "no_sign_362_15",
        "no_sign_362_20",
        "no_sign_362_25",
        "no_sign_362_30",
        "no_sign_362_35",
        "no_sign_362_40",
        "no_sign_362_45",
        "no_sign_362_50",
        "no_sign_362_55",
        "no_sign_362_60",
        "no_sign_362_65",
        "no_sign_362_70",
        "no_sign_362_75",
        "no_sign_362_80",
        "no_sign_362_85",
        "no_sign_362_90",
        "no_sign_362_95",
        "no_sign_362_100",
        "no_sign_362_105",
        "no_sign_362_110",
    ] {
        let path = driver_break_core::icons::resolve_icon(key, IconTheme::Day, None, &bundled);
        assert!(
            !path.ends_with("unknown.svg"),
            "{key} missing (got {})",
            path.display()
        );
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
