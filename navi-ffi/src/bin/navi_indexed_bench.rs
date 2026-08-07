//! Load/validate indexed packs; report timings + version-mismatch rejection.

use std::env;
use std::path::PathBuf;
use std::time::Instant;

use driver_break_core::routing::graph::RoutingProfile;
use driver_break_core::routing::indexed::{
    load_graph_pack_bbox, load_poi_barrier_pack, PackLoadError, GRAPH_FORMAT_VERSION, MAGIC_GRAPH,
};

fn main() {
    let args: Vec<String> = env::args().collect();
    let data_dir = PathBuf::from(arg(&args, "--data-dir"));
    let stem = arg(&args, "--stem");
    let bbox = arg_opt(&args, "--bbox").map(|s| {
        let p: Vec<f64> = s.split(',').map(|x| x.parse().unwrap()).collect();
        [p[0], p[1], p[2], p[3]]
    });
    let graph = data_dir.join(format!("{stem}.navi-graph-car.rkyv"));
    let poi = data_dir.join(format!("{stem}.navi-poi-barrier.rkyv"));

    let t0 = Instant::now();
    let g = load_graph_pack_bbox(&graph, RoutingProfile::Car, bbox).expect("graph");
    let graph_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let t1 = Instant::now();
    let (pindex, barriers) = load_poi_barrier_pack(&poi).expect("poi");
    let poi_ms = t1.elapsed().as_secs_f64() * 1000.0;
    println!(
        "LOAD_OK graph_ms={graph_ms:.1} nodes={} edges={} poi_ms={poi_ms:.1} pois={} barriers_empty={}",
        g.nodes.len(),
        g.edges.len(),
        pindex.len(),
        barriers.is_empty()
    );

    // Version mismatch must not interpret payload.
    let mut bytes = std::fs::read(&graph).expect("read");
    bytes[4..8].copy_from_slice(&99u32.to_le_bytes());
    let bad = data_dir.join("bad-version.rkyv");
    std::fs::write(&bad, &bytes).unwrap();
    match load_graph_pack_bbox(&bad, RoutingProfile::Car, None) {
        Err(PackLoadError::VersionMismatch) => {
            println!("MISMATCH_OK magic_expect={MAGIC_GRAPH:#x} ver_expect={GRAPH_FORMAT_VERSION}")
        }
        Err(e) => panic!("expected VersionMismatch, got error: {e}"),
        Ok(_) => panic!("expected VersionMismatch, got Ok"),
    }
}

fn arg(args: &[String], flag: &str) -> String {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
        .unwrap_or_else(|| panic!("missing {flag}"))
}

fn arg_opt(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}
