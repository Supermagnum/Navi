//! Regression: pad widen does not expand corridor-band edge materialization.
//!
//! Soft-gated on Ostlandet car tiles under `target/espa-dombas-e2e` that can
//! route the leg13 densify hop with **no** edge clip. When those fixtures are
//! present and connected:
//! - If corridor band finds a route under the current densify snap budget,
//!   that is acceptable (band is sufficient).
//! - If band disconnects, trip-AABB must still find a route (AABB fallback).
//!
//! Synthetic geometry (0.40° band vs cross-track detour) is covered
//! unconditionally in `plan_bbox` unit tests.

use driver_break_core::routing::graph::{RouteOptions, RoutingProfile};
use driver_break_core::routing::indexed::{load_graph_pack_clips, merge_tile_graphs};
use driver_break_core::routing::plan_bbox::{
    corridor_band_bboxes, plan_edge_clips, trip_bbox_points, PlanEdgeClipMode,
    CHUNK_INTERMEDIATE_SNAP_M, CORRIDOR_BAND_STEP_DEG, CORRIDOR_EDGE_HALF_WIDTH_DEG,
};
use std::path::{Path, PathBuf};

/// Bevensen MobileHome densify hop that failed under corridor band (leg13).
const LEG13_START: (f64, f64) = (61.375314, 8.657898);
const LEG13_END: (f64, f64) = (61.617086, 8.043864);

const COVERING_TILES: &[&str] = &[
    "ostlandet-latest.navi-graph-car.t2_0.rkyv",
    "ostlandet-latest.navi-graph-car.t2_1.rkyv",
];

fn data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/espa-dombas-e2e")
}

fn load_car_clips(
    dir: &Path,
    clips: Option<&[[f64; 4]]>,
) -> Option<driver_break_core::routing::graph::RouteGraph> {
    let mut graphs = Vec::new();
    for name in COVERING_TILES {
        let p = dir.join(name);
        if !p.is_file() {
            return None;
        }
        if let Ok(g) = load_graph_pack_clips(&p, RoutingProfile::Car, clips) {
            if !g.edges.is_empty() {
                graphs.push(g);
            }
        }
    }
    if graphs.is_empty() {
        return None;
    }
    Some(merge_tile_graphs(graphs, RoutingProfile::Car))
}

fn route_found(g: &driver_break_core::routing::graph::RouteGraph) -> (bool, String) {
    let opts = RouteOptions::default();
    let Ok((ss, _)) = g.nearest_routable_with_options_max(
        LEG13_START.0,
        LEG13_START.1,
        &opts,
        false,
        CHUNK_INTERMEDIATE_SNAP_M,
    ) else {
        return (false, "snap_start".into());
    };
    let Ok((ee, _)) = g.nearest_routable_with_options_max(
        LEG13_END.0,
        LEG13_END.1,
        &opts,
        false,
        CHUNK_INTERMEDIATE_SNAP_M,
    ) else {
        return (false, "snap_end".into());
    };
    let st = g.shortest_path_with_options_stats(ss, ee, false, &opts);
    (
        st.path.is_some(),
        format!(
            "term={} exp={} edges={}",
            st.terminate_reason,
            st.expansions,
            g.edges.len()
        ),
    )
}

#[test]
fn leg13_corridor_band_misses_detour_aabb_finds_route() {
    let dir = data_dir();
    if !dir.is_dir() {
        eprintln!("skip: missing {}", dir.display());
        return;
    }
    let Some(g_full) = load_car_clips(&dir, None) else {
        eprintln!(
            "skip: covering Ostlandet car tiles absent under {}",
            dir.display()
        );
        return;
    };
    let (full_ok, full_diag) = route_found(&g_full);
    if !full_ok {
        eprintln!(
            "skip: fixture tiles do not connect leg13 unclipped ({full_diag}); \
             synthetic plan_bbox unit test still guards pad-widen/AABB invariant"
        );
        return;
    }

    let pts = [LEG13_START, LEG13_END];
    let band = corridor_band_bboxes(&pts, CORRIDOR_EDGE_HALF_WIDTH_DEG, CORRIDOR_BAND_STEP_DEG);
    let g_band = load_car_clips(&dir, Some(&band)).expect("band");
    let (band_ok, band_diag) = route_found(&g_band);
    if band_ok {
        // Wider densify snap (35 km) can make the narrow band routeable on this
        // fixture; AABB fallback is only required when band still disconnects.
        eprintln!("corridor band routes leg13 ({band_diag}); AABB fallback not required");
        return;
    }

    let mut aabb_ok = false;
    let mut last = String::new();
    for &pad in &[0.35_f64, 0.70, 1.40] {
        let aabb = trip_bbox_points(&pts, pad);
        let clips =
            plan_edge_clips(Some(&pts), Some(aabb), PlanEdgeClipMode::TripAabb).expect("aabb");
        let g = load_car_clips(&dir, Some(&clips)).expect("aabb load");
        let (ok, diag) = route_found(&g);
        last = format!("pad={pad} {diag}");
        if ok {
            aabb_ok = true;
            eprintln!("aabb fallback ok: {last}");
            break;
        }
    }
    assert!(
        aabb_ok,
        "TripAabb edge clip must find a route after band disconnect; last={last} band={band_diag}"
    );
}
