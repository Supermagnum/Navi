//! Follow-up 53 step 1: node counts for the long stretch and cases h, j.
//! Needs NAVI_GATE_PACKS. Does not change product code.

use driver_break_core::routing::corridor_skeleton::{
    estimated_direct_search_nodes, estimated_path_covering_nodes, vm_hwm_mb, vm_rss_mb,
};
use driver_break_core::routing::graph::RoutingProfile;
use driver_break_core::routing::indexed::{
    corridor_cache_clear, try_load_graph_for_plan_corridor_with_pack_dirs,
};
use driver_break_core::routing::plan_bbox::{
    corridor_band_bboxes, direct_search_clip_bboxes, start_end_aabb, PlanEdgeClipMode,
    CORRIDOR_BAND_STEP_DEG, CORRIDOR_EDGE_HALF_WIDTH_DEG,
};
use std::path::{Path, PathBuf};

const STRETCH: ((f64, f64), (f64, f64)) = ((60.27656, 10.81650), (59.80326, 9.39866));
const H: ((f64, f64), (f64, f64)) = ((59.913330, 10.738970), (59.955924, 11.049112));
const J: ((f64, f64), (f64, f64)) = ((55.676098, 12.568337), (55.6517, 12.2922));

fn measure(label: &str, packs: &Path, start: (f64, f64), end: (f64, f64)) {
    let dirs = [packs];
    let pts = [start, end];
    let rss0 = vm_rss_mb();
    let hwm0 = vm_hwm_mb();
    let today = estimated_path_covering_nodes(&dirs, RoutingProfile::Car, &pts);
    let clips = direct_search_clip_bboxes(start, end);
    let cheap = estimated_direct_search_nodes(&dirs, RoutingProfile::Car, start, end);
    eprintln!(
        "{label} estimate_today={today} (full tiles on eighth-samples of the chord) \
         cheap_clipped={cheap} rss0={rss0} hwm0={hwm0}"
    );

    let aabb = start_end_aabb(start, end);
    let pbf = packs.join("ostlandet-latest.osm.pbf");
    let dummy = if pbf.is_file() {
        pbf
    } else {
        PathBuf::from("missing.osm.pbf")
    };

    corridor_cache_clear();
    let t = std::time::Instant::now();
    let aabb_g = try_load_graph_for_plan_corridor_with_pack_dirs(
        packs,
        &[packs.to_path_buf()],
        &dummy,
        RoutingProfile::Car,
        Some(aabb),
        Some(&pts),
        PlanEdgeClipMode::TripAabb,
    );
    let aabb_ms = t.elapsed().as_millis();
    let aabb_rss = vm_rss_mb();
    let aabb_hwm = vm_hwm_mb();
    match aabb_g {
        Ok(g) => eprintln!(
            "{label} start_end_aabb nodes={} edges={} load_ms={aabb_ms} rss={aabb_rss} hwm={aabb_hwm} \
             shape=axis-aligned start-end box margin=0",
            g.nodes.len(),
            g.edges.len()
        ),
        Err(e) => eprintln!("{label} start_end_aabb FAIL {e:?} load_ms={aabb_ms} rss={aabb_rss}"),
    }

    let band = corridor_band_bboxes(&pts, CORRIDOR_EDGE_HALF_WIDTH_DEG, CORRIDOR_BAND_STEP_DEG);
    let band_box = {
        let min_lat = band.iter().map(|b| b[0]).fold(f64::INFINITY, f64::min);
        let min_lon = band.iter().map(|b| b[1]).fold(f64::INFINITY, f64::min);
        let max_lat = band.iter().map(|b| b[2]).fold(f64::NEG_INFINITY, f64::max);
        let max_lon = band.iter().map(|b| b[3]).fold(f64::NEG_INFINITY, f64::max);
        [min_lat, min_lon, max_lat, max_lon]
    };
    corridor_cache_clear();
    let t = std::time::Instant::now();
    let band_g = try_load_graph_for_plan_corridor_with_pack_dirs(
        packs,
        &[packs.to_path_buf()],
        &dummy,
        RoutingProfile::Car,
        Some(band_box),
        Some(&pts),
        PlanEdgeClipMode::CorridorBand,
    );
    let band_ms = t.elapsed().as_millis();
    let band_rss = vm_rss_mb();
    let band_hwm = vm_hwm_mb();
    match band_g {
        Ok(g) => eprintln!(
            "{label} hop_corridor_band nodes={} edges={} load_ms={band_ms} rss={band_rss} hwm={band_hwm} \
             shape=0.40deg squares along the chord step=0.20deg",
            g.nodes.len(),
            g.edges.len()
        ),
        Err(e) => eprintln!("{label} hop_corridor_band FAIL {e:?} load_ms={band_ms} rss={band_rss}"),
    }

    corridor_cache_clear();
    let t = std::time::Instant::now();
    let dir_g = try_load_graph_for_plan_corridor_with_pack_dirs(
        packs,
        &[packs.to_path_buf()],
        &dummy,
        RoutingProfile::Car,
        Some(start_end_aabb(start, end)),
        Some(&pts),
        PlanEdgeClipMode::CorridorBand,
    );
    let dir_ms = t.elapsed().as_millis();
    let dir_rss = vm_rss_mb();
    let dir_hwm = vm_hwm_mb();
    match dir_g {
        Ok(g) => eprintln!(
            "{label} direct_band nodes={} edges={} load_ms={dir_ms} rss={dir_rss} hwm={dir_hwm} \
             clips={}",
            g.nodes.len(),
            g.edges.len(),
            clips.len()
        ),
        Err(e) => eprintln!("{label} direct_band FAIL {e:?} load_ms={dir_ms} rss={dir_rss}"),
    }
}

#[test]
#[ignore = "needs NAVI_GATE_PACKS"]
fn fu53_clip_counts() {
    let packs = PathBuf::from(std::env::var("NAVI_GATE_PACKS").expect("NAVI_GATE_PACKS"));
    assert!(packs.is_dir(), "{}", packs.display());
    measure("stretch", &packs, STRETCH.0, STRETCH.1);
    measure("h", &packs, H.0, H.1);
    measure("j", &packs, J.0, J.1);
}
