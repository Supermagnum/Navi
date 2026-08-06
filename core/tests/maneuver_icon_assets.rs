//! Fail loudly when maneuver icon keys lack SVG assets in the Android lean pack
//! (`app/src/main/assets/icons`) or the full core set (`core/src/icons`).
//!
//! The lean pack under `src/main/assets` ships in **all** Android build types
//! (debug and release; there are no product flavors). Missing `nav_*` files
//! become `unknown.svg` at runtime — exactly the silent gap the multi-RA
//! hardware pass exposed.

use std::collections::HashSet;
use std::path::PathBuf;

use driver_break_core::nav::ManeuverKind;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .canonicalize()
        .expect("repo root")
}

fn lean_icons_dir() -> PathBuf {
    repo_root().join("app/src/main/assets/icons")
}

fn core_icons_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/icons")
}

/// Every icon stem the maneuver / approach UI may request (day+night `_bk`/`_wh`).
fn required_maneuver_stems() -> Vec<&'static str> {
    vec![
        "nav_left_1",
        "nav_left_2",
        "nav_left_3",
        "nav_right_1",
        "nav_right_2",
        "nav_right_3",
        "nav_straight",
        "nav_keep_left",
        "nav_keep_right",
        "nav_exit_left",
        "nav_exit_right",
        "nav_merge_left",
        "nav_merge_right",
        "nav_turnaround_left",
        "nav_turnaround_right",
        "nav_destination",
        "nav_roundabout_r1",
        "nav_roundabout_r2",
        "nav_roundabout_r3",
        "nav_roundabout_r4",
        "nav_roundabout_r5",
        "nav_roundabout_r6",
        "nav_roundabout_r7",
        "nav_roundabout_r8",
        "nav_roundabout_l1",
        "nav_roundabout_l2",
        "nav_roundabout_l3",
        "nav_roundabout_l4",
        "nav_roundabout_l5",
        "nav_roundabout_l6",
        "nav_roundabout_l7",
        "nav_roundabout_l8",
    ]
}

#[test]
fn android_lean_pack_has_all_maneuver_nav_icons() {
    let lean = lean_icons_dir();
    assert!(
        lean.is_dir(),
        "lean icon pack dir missing: {}",
        lean.display()
    );
    let mut missing = Vec::new();
    for stem in required_maneuver_stems() {
        for suffix in ["_bk.svg", "_wh.svg"] {
            let p = lean.join(format!("{stem}{suffix}"));
            if !p.is_file() {
                missing.push(p.display().to_string());
            }
        }
    }
    assert!(
        missing.is_empty(),
        "Android lean pack (ships in release APK) missing maneuver icons:\n  {}",
        missing.join("\n  ")
    );
}

#[test]
fn core_icon_set_has_all_maneuver_nav_icons() {
    let core = core_icons_dir();
    let mut missing = Vec::new();
    for stem in required_maneuver_stems() {
        for suffix in ["_bk.svg", "_wh.svg"] {
            let p = core.join(format!("{stem}{suffix}"));
            if !p.is_file() {
                missing.push(p.display().to_string());
            }
        }
    }
    assert!(
        missing.is_empty(),
        "core icon set missing maneuver icons:\n  {}",
        missing.join("\n  ")
    );
}

#[test]
fn maneuver_kind_icon_keys_are_covered() {
    let required: HashSet<_> = required_maneuver_stems().into_iter().collect();
    let kinds = [
        ManeuverKind::SlightLeft,
        ManeuverKind::Left,
        ManeuverKind::SharpLeft,
        ManeuverKind::SlightRight,
        ManeuverKind::Right,
        ManeuverKind::SharpRight,
        ManeuverKind::Straight,
        ManeuverKind::ExitLeft,
        ManeuverKind::ExitRight,
        ManeuverKind::MergeLeft,
        ManeuverKind::MergeRight,
        ManeuverKind::UTurn,
        ManeuverKind::Destination,
        ManeuverKind::KeepLeft,
        ManeuverKind::KeepRight,
        ManeuverKind::Unknown,
    ];
    for k in kinds {
        let key = k.icon_key();
        assert!(
            required.contains(key),
            "ManeuverKind::{k:?} icon_key `{key}` not in lean-pack required list"
        );
    }
    for exit in 1..=8u8 {
        let key = ManeuverKind::roundabout_icon_key(Some(exit));
        assert!(
            required.contains(key),
            "roundabout_icon_key({exit}) `{key}` not in required list"
        );
    }
}
