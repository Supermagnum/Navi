//! Wetland PBF vs indexed-pack timing + class agreement.
//!
//! Usage:
//!   navi-wetland-bench --pbf PATH --pack PATH [--bbox min_lat,min_lon,max_lat,max_lon]

use std::env;
use std::path::PathBuf;
use std::time::Instant;

use driver_break_core::routing::indexed::load_wetland_pack;
use driver_break_core::routing::wetland::WetlandIndex;

fn main() {
    let args: Vec<String> = env::args().collect();
    let pbf = PathBuf::from(arg(&args, "--pbf"));
    let pack = PathBuf::from(arg(&args, "--pack"));
    let bbox = parse_bbox(&arg_or(&args, "--bbox", "60.452,9.837,62.248,11.765"));

    let t0 = Instant::now();
    let w_pbf = WetlandIndex::load_from_pbf_bbox(&pbf, bbox).expect("pbf wetland load");
    let pbf_ms = t0.elapsed().as_secs_f64() * 1000.0;

    let t1 = Instant::now();
    let w_pack = load_wetland_pack(&pack, Some(bbox)).expect("pack wetland load");
    let pack_ms = t1.elapsed().as_secs_f64() * 1000.0;

    println!("WETLAND_PBF_MS={pbf_ms:.1}");
    println!("WETLAND_PACK_MS={pack_ms:.1}");
    println!("pbf_rings={}", w_pbf.ring_count());
    println!("pack_rings={}", w_pack.ring_count());
    if pack_ms > 0.0 {
        println!("speedup={:.1}x", pbf_ms / pack_ms.max(0.001));
    }

    let mut agree = 0usize;
    let mut total = 0usize;
    let mut lat = bbox[0];
    while lat <= bbox[2] + 1e-9 {
        let mut lon = bbox[1];
        while lon <= bbox[3] + 1e-9 {
            total += 1;
            if w_pbf.class_at(lat, lon) == w_pack.class_at(lat, lon) {
                agree += 1;
            }
            lon += 0.25;
        }
        lat += 0.25;
    }
    println!("CLASS_AGREE={agree}/{total}");
    if agree != total {
        std::process::exit(1);
    }
}

fn parse_bbox(s: &str) -> [f64; 4] {
    let p: Vec<f64> = s
        .split(',')
        .map(|x| x.trim().parse().expect("bbox float"))
        .collect();
    assert_eq!(p.len(), 4, "bbox needs 4 floats");
    [p[0], p[1], p[2], p[3]]
}

fn arg(args: &[String], flag: &str) -> String {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
        .unwrap_or_else(|| panic!("missing {flag}"))
}

fn arg_or(args: &[String], flag: &str, default: &str) -> String {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
        .unwrap_or_else(|| default.to_string())
}
