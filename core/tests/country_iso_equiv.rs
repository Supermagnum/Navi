//! Equivalence: wholly-inside grid shortcut vs exact PIP + coastal snap.
//!
//! Enable the heavy ostlandet edge walk with `NAVI_COUNTRY_ISO_EQUIV=1`.

use driver_break_core::routing::elevation::{
    country_iso_at, iso_at_exact_path, warm_country_polys, COASTAL_SNAP_TOLERANCE_M,
};
use driver_break_core::routing::graph::RoutingProfile;
use driver_break_core::routing::indexed::try_load_graph_for_plan_bbox;
use std::path::PathBuf;

#[test]
fn grid_shortcut_matches_exact_on_one_million_random_points() {
    let _ = warm_country_polys();
    // Deterministic LCG — no external RNG dependency.
    let mut state: u64 = 0xC0FFEE_u64;
    let mut next = || {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        state
    };
    let n = 1_000_000usize;
    let mut mismatches = 0usize;
    for _ in 0..n {
        let r1 = (next() as f64) / (u64::MAX as f64);
        let r2 = (next() as f64) / (u64::MAX as f64);
        // Global land-ish band; oceans exercise coastal snap equally.
        let lat = -60.0 + r1 * 120.0;
        let lon = -180.0 + r2 * 360.0;
        let a = country_iso_at(lat, lon);
        let b = iso_at_exact_path(lat, lon);
        if a != b {
            mismatches += 1;
            if mismatches <= 10 {
                eprintln!("mismatch lat={lat} lon={lon} fast={a:?} exact={b:?}");
            }
        }
    }
    assert_eq!(
        mismatches, 0,
        "grid shortcut diverged on {mismatches}/{n} points (snap tol {COASTAL_SNAP_TOLERANCE_M} m)"
    );
}

#[test]
#[ignore = "equiv: set NAVI_COUNTRY_ISO_EQUIV=1 (needs ostlandet car tiles)"]
fn grid_shortcut_matches_exact_on_every_ostlandet_edge_endpoint() {
    assert_eq!(
        std::env::var("NAVI_COUNTRY_ISO_EQUIV").ok().as_deref(),
        Some("1")
    );
    let data = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/espa-dombas-e2e");
    let pbf = data.join("ostlandet-latest.osm.pbf");
    let bbox = [59.0_f64, 9.5, 62.5, 12.5];
    let graph = try_load_graph_for_plan_bbox(&data, &pbf, RoutingProfile::Car, Some(bbox))
        .expect("load ostlandet car graph");
    let mut mismatches = 0usize;
    let mut checked = 0usize;
    for e in &graph.edges {
        for (lat, lon) in [
            (e.start_lat, e.start_lon),
            (
                (e.start_lat + e.end_lat) * 0.5,
                (e.start_lon + e.end_lon) * 0.5,
            ),
            (e.end_lat, e.end_lon),
        ] {
            checked += 1;
            let a = country_iso_at(lat, lon);
            let b = iso_at_exact_path(lat, lon);
            if a != b {
                mismatches += 1;
                if mismatches <= 10 {
                    eprintln!("edge mismatch lat={lat} lon={lon} fast={a:?} exact={b:?}");
                }
            }
        }
    }
    eprintln!("ostlandet_edge_points_checked={checked} mismatches={mismatches}");
    assert_eq!(mismatches, 0);
}
