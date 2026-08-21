//! Host-side extract of compact live-hazard JSON layers from a regional PBF.
//!
//! Usage:
//!   cargo run -p navi-ffi --bin live-hazard-extract --release -- \
//!     /path/to/ostlandet-latest.osm.pbf /path/to/out_dir
//!
//! Writes signs.json, cameras.json, children.json, bumps.json and prints load
//! stats + a micro-benchmark of cone queries (no Android / UniFFI tick).

use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use driver_break_core::routing::live_hazard::{
    nearest_live_children_warning, nearest_live_sign_style_warning,
    nearest_live_speed_camera_warning, LiveHazardIndex, LIVE_HAZARD_CONE_M,
};

fn main() {
    let mut args = env::args().skip(1);
    let pbf = PathBuf::from(args.next().expect("pbf path"));
    let out = PathBuf::from(args.next().expect("out dir"));
    fs::create_dir_all(&out).expect("mkdir out");

    eprintln!("loading {}", pbf.display());
    let t0 = Instant::now();
    let (index, stats) = LiveHazardIndex::load_from_pbf(&pbf).expect("load_from_pbf");
    let load_ms = t0.elapsed().as_secs_f64() * 1000.0;

    let signs = index.signs_json();
    let cameras = index.cameras_json();
    let children = index.children_json();
    let bumps = index.bumps_json();
    fs::write(out.join("signs.json"), &signs).expect("write signs");
    fs::write(out.join("cameras.json"), &cameras).expect("write cameras");
    fs::write(out.join("children.json"), &children).expect("write children");
    fs::write(out.join("bumps.json"), &bumps).expect("write bumps");

    let utf8 = signs.len() + cameras.len() + children.len() + bumps.len();
    println!(
        "LOAD signs={} children={} cameras={} bumps={} compact_utf8_est={} layer_utf8={} cone_m={} load_ms={:.1}",
        stats.signs,
        stats.children,
        stats.cameras,
        stats.bumps,
        stats.compact_json_utf8,
        utf8,
        LIVE_HAZARD_CONE_M,
        load_ms
    );

    // Vallset corridor sample point (same as on-device overhead test).
    let lat = 60.680_804_625_204_44;
    let lon = 11.345_380_193_660_88;
    let heading = Some(160.0);
    let window = index.windowed(lat, lon);
    let iters = 200u32;
    for _ in 0..10 {
        let _ = nearest_live_sign_style_warning(&window, lat, lon, heading);
        let _ = nearest_live_children_warning(&window, lat, lon, heading);
        let _ = nearest_live_speed_camera_warning(&window, lat, lon, heading, true);
    }
    let t1 = Instant::now();
    for _ in 0..iters {
        let _ = nearest_live_sign_style_warning(&window, lat, lon, heading);
        let _ = nearest_live_children_warning(&window, lat, lon, heading);
        let _ = nearest_live_speed_camera_warning(&window, lat, lon, heading, true);
    }
    let tick_ms = t1.elapsed().as_secs_f64() * 1000.0 / f64::from(iters);
    println!(
        "HOST_COMPACT_TICK_ms_mean={:.4} window_signs={} window_children={} window_cams={} window_bumps={}",
        tick_ms,
        window.signs.len(),
        window.children.len(),
        window.cameras.len(),
        window.bumps.len()
    );
}
