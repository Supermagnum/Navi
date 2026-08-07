//! Phase 1c PoC: rkyv + memmap2 zero-copy routing graph (successor to R*Tree 1b).
//!
//! Variant A: plain graph (length weights). Variant B: same layout + per-edge
//! elevation delta (Δh) for live eco arithmetic (no DEM at plan time).
//!
//! Usage:
//!   rkyv-mmap-graph-poc build --pbf PATH --out PATH --variant a|b \
//!       [--bbox minLat,minLon,maxLat,maxLon] [--elev-dir PATH]
//!   rkyv-mmap-graph-poc bench --pbf PATH --out PATH --variant a|b \
//!       --start-lat F --start-lon F --end-lat F --end-lon F [--elev-dir PATH]
//!
//! `build` is offline preprocess. `bench` compares cold PBF graph_build to
//! mmap+rkyv access (no owned RouteGraph materialization) on the same OD.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use driver_break_core::config::{EcoConfig, Profile};
use driver_break_core::routing::elevation::{ElevationCache, ElevationService};
use driver_break_core::routing::graph::{max_waypoint_snap_m, RouteGraph, RoutingProfile};
use memmap2::Mmap;
use pathfinding::prelude::astar;
use rkyv::rancor::Error as RkyvError;
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};

const MAGIC: u32 = 0x4E_56_52_4B; // "NVRK"
const VARIANT_A: u8 = 1;
const VARIANT_B: u8 = 2;

#[derive(Archive, RkyvSerialize, RkyvDeserialize, Debug, Clone)]
struct FlatGraphPack {
    magic: u32,
    variant: u8,
    has_delta_h: bool,
    node_ids: Vec<i64>,
    node_lats: Vec<f64>,
    node_lons: Vec<f64>,
    edge_src: Vec<u32>,
    edge_tgt: Vec<u32>,
    edge_length_m: Vec<f64>,
    edge_base_weight: Vec<f64>,
    /// Metres; 0 when unknown / Variant A.
    edge_delta_h_m: Vec<f32>,
    /// CSR: adj_offsets len = nodes+1; adj_edges holds edge indices.
    adj_offsets: Vec<u32>,
    adj_edges: Vec<u32>,
}

fn usage() -> ! {
    eprintln!(
        "usage:\n  rkyv-mmap-graph-poc build --pbf PATH --out PATH --variant a|b [--bbox ...] [--elev-dir PATH]\n  rkyv-mmap-graph-poc bench --pbf PATH --out PATH --variant a|b --start-lat F --start-lon F --end-lat F --end-lon F [--elev-dir PATH]"
    );
    std::process::exit(2);
}

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}

fn parse_bbox(s: &str) -> [f64; 4] {
    let p: Vec<f64> = s
        .split(',')
        .map(|x| x.trim().parse().expect("bbox float"))
        .collect();
    assert_eq!(p.len(), 4, "bbox needs 4 floats");
    [p[0], p[1], p[2], p[3]]
}

fn trip_bbox(slat: f64, slon: f64, elat: f64, elon: f64) -> [f64; 4] {
    let lat_span = (slat - elat).abs();
    let lon_span = (slon - elon).abs();
    let pad = (lat_span.max(lon_span) * 0.35).clamp(0.35, 2.5);
    [
        slat.min(elat) - pad,
        slon.min(elon) - pad,
        slat.max(elat) + pad,
        slon.max(elon) + pad,
    ]
}

fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6_371_000.0_f64;
    let p1 = lat1.to_radians();
    let p2 = lat2.to_radians();
    let dp = (lat2 - lat1).to_radians();
    let dl = (lon2 - lon1).to_radians();
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

fn graph_to_flat(graph: &RouteGraph, elev: Option<&ElevationService>) -> FlatGraphPack {
    let mut node_ids = Vec::with_capacity(graph.nodes.len());
    let mut node_lats = Vec::with_capacity(graph.nodes.len());
    let mut node_lons = Vec::with_capacity(graph.nodes.len());
    let mut id_to_idx: HashMap<i64, u32> = HashMap::with_capacity(graph.nodes.len());
    for (id, node) in &graph.nodes {
        let idx = node_ids.len() as u32;
        id_to_idx.insert(id.0, idx);
        node_ids.push(id.0);
        node_lats.push(node.coord.y);
        node_lons.push(node.coord.x);
    }

    let mut edge_src = Vec::with_capacity(graph.edges.len());
    let mut edge_tgt = Vec::with_capacity(graph.edges.len());
    let mut edge_length_m = Vec::with_capacity(graph.edges.len());
    let mut edge_base_weight = Vec::with_capacity(graph.edges.len());
    let mut edge_delta_h_m = Vec::with_capacity(graph.edges.len());
    let mut missing_delta = 0usize;

    for e in &graph.edges {
        let s = *id_to_idx.get(&e.source.0).expect("src");
        let t = *id_to_idx.get(&e.target.0).expect("tgt");
        edge_src.push(s);
        edge_tgt.push(t);
        edge_length_m.push(e.length_m);
        edge_base_weight.push(e.base_weight);
        let dh = if let Some(elev) = elev {
            match (
                elev.get_elevation(e.start_lat, e.start_lon),
                elev.get_elevation(e.end_lat, e.end_lon),
            ) {
                (Some(a), Some(b)) => (b - a) as f32,
                _ => {
                    missing_delta += 1;
                    0.0
                }
            }
        } else {
            0.0
        };
        if elev.is_some() {
            edge_delta_h_m.push(dh);
        }
    }

    if elev.is_none() {
        edge_delta_h_m.clear();
    }

    let n = node_ids.len();
    let mut buckets: Vec<Vec<u32>> = vec![Vec::new(); n];
    for (ei, &s) in edge_src.iter().enumerate() {
        buckets[s as usize].push(ei as u32);
    }
    let mut adj_offsets = Vec::with_capacity(n + 1);
    let mut adj_edges = Vec::with_capacity(graph.edges.len());
    adj_offsets.push(0);
    for b in &buckets {
        adj_edges.extend_from_slice(b);
        adj_offsets.push(adj_edges.len() as u32);
    }

    if elev.is_some() {
        eprintln!(
            "delta_h: edges={} missing_endpoint_dem={missing_delta}",
            edge_delta_h_m.len()
        );
    }

    FlatGraphPack {
        magic: MAGIC,
        variant: if elev.is_some() { VARIANT_B } else { VARIANT_A },
        has_delta_h: elev.is_some(),
        node_ids,
        node_lats,
        node_lons,
        edge_src,
        edge_tgt,
        edge_length_m,
        edge_base_weight,
        edge_delta_h_m,
        adj_offsets,
        adj_edges,
    }
}

fn build(pbf: &Path, out: &Path, variant: u8, bbox: Option<[f64; 4]>, elev_dir: Option<&Path>) {
    eprintln!("building RouteGraph from {} …", pbf.display());
    let t0 = Instant::now();
    let graph = match bbox {
        Some(b) => {
            RouteGraph::build_from_pbf_bbox(pbf, RoutingProfile::Car, b).expect("bbox build")
        }
        None => RouteGraph::build_from_pbf(pbf, RoutingProfile::Car).expect("full build"),
    };
    eprintln!(
        "graph_build_ms={:.1} nodes={} edges={}",
        t0.elapsed().as_secs_f64() * 1000.0,
        graph.nodes.len(),
        graph.edges.len()
    );

    let elev_svc;
    let elev_ref = if variant == VARIANT_B {
        let dir = elev_dir.expect("--elev-dir required for variant b");
        elev_svc = ElevationService::new(ElevationCache::new(dir));
        if let Some(b) = bbox {
            let _ = elev_svc.warm_bbox(b);
        }
        Some(&elev_svc)
    } else {
        None
    };

    let t1 = Instant::now();
    let pack = graph_to_flat(&graph, elev_ref);
    let bytes = rkyv::to_bytes::<RkyvError>(&pack).expect("rkyv serialize");
    fs::write(out, &bytes).expect("write archive");
    eprintln!(
        "serialize_ms={:.1} bytes={} out={}",
        t1.elapsed().as_secs_f64() * 1000.0,
        bytes.len(),
        out.display()
    );
}

fn nearest_idx(archived: &ArchivedFlatGraphPack, lat: f64, lon: f64) -> Option<usize> {
    let n = archived.node_lats.len();
    if n == 0 {
        return None;
    }
    let snap_max = max_waypoint_snap_m(RoutingProfile::Car);
    let mut best = 0usize;
    let mut best_d = f64::INFINITY;
    for i in 0..n {
        let d = haversine_m(
            lat,
            lon,
            archived.node_lats[i].into(),
            archived.node_lons[i].into(),
        );
        if d < best_d {
            best_d = d;
            best = i;
        }
    }
    if best_d <= snap_max {
        Some(best)
    } else {
        None
    }
}

fn cost_to_u64(cost: f64) -> u64 {
    (cost.max(0.0) * 1000.0).round() as u64
}

fn edge_cost(
    archived: &ArchivedFlatGraphPack,
    edge_i: usize,
    use_eco: bool,
    eco: &EcoConfig,
) -> f64 {
    if !use_eco || !archived.has_delta_h {
        return archived.edge_base_weight[edge_i].into();
    }
    let len: f64 = archived.edge_length_m[edge_i].into();
    let dh: f64 = f32::from(archived.edge_delta_h_m[edge_i]) as f64;
    eco.segment_energy_joules(len, dh)
}

fn plan_archived(
    archived: &ArchivedFlatGraphPack,
    start: usize,
    goal: usize,
    use_eco: bool,
    eco: &EcoConfig,
) -> Option<(Vec<usize>, f64)> {
    let goal_lat: f64 = archived.node_lats[goal].into();
    let goal_lon: f64 = archived.node_lons[goal].into();
    let result = astar(
        &start,
        |&n| {
            let a = u32::from(archived.adj_offsets[n]) as usize;
            let b = u32::from(archived.adj_offsets[n + 1]) as usize;
            (a..b).map(move |k| {
                let ei = u32::from(archived.adj_edges[k]) as usize;
                let tgt = u32::from(archived.edge_tgt[ei]) as usize;
                let c = edge_cost(archived, ei, use_eco, eco);
                (tgt, cost_to_u64(c))
            })
        },
        |&n| {
            let lat: f64 = archived.node_lats[n].into();
            let lon: f64 = archived.node_lons[n].into();
            cost_to_u64(haversine_m(lat, lon, goal_lat, goal_lon) * 0.1)
        },
        |&n| n == goal,
    )?;
    let (path, _cost) = result;
    let mut dist = 0.0_f64;
    for w in path.windows(2) {
        let a = u32::from(archived.adj_offsets[w[0]]) as usize;
        let b = u32::from(archived.adj_offsets[w[0] + 1]) as usize;
        for k in a..b {
            let ei = u32::from(archived.adj_edges[k]) as usize;
            if u32::from(archived.edge_tgt[ei]) as usize == w[1] {
                dist += f64::from(archived.edge_length_m[ei]);
                break;
            }
        }
    }
    Some((path, dist / 1000.0))
}

fn touch_all_edges(archived: &ArchivedFlatGraphPack) -> f64 {
    let mut acc = 0.0_f64;
    let has_dh = archived.has_delta_h;
    for i in 0..archived.edge_length_m.len() {
        acc += f64::from(archived.edge_length_m[i]);
        if has_dh {
            acc += f32::from(archived.edge_delta_h_m[i]) as f64 * 1e-9;
        }
    }
    acc
}

fn arith_reweight_ms(archived: &ArchivedFlatGraphPack, eco: &EcoConfig) -> (f64, f64) {
    let t0 = Instant::now();
    let mut sum = 0.0_f64;
    for i in 0..archived.edge_length_m.len() {
        let len: f64 = archived.edge_length_m[i].into();
        let dh: f64 = f32::from(archived.edge_delta_h_m[i]) as f64;
        sum += eco.segment_energy_joules(len, dh);
    }
    (t0.elapsed().as_secs_f64() * 1000.0, sum)
}

fn dem_reweight_ms(pbf: &Path, bbox: [f64; 4], elev_dir: &Path, eco: &EcoConfig) -> f64 {
    let mut graph =
        RouteGraph::build_from_pbf_bbox(pbf, RoutingProfile::Car, bbox).expect("cold for reweight");
    let elev = ElevationService::new(ElevationCache::new(elev_dir));
    let _ = elev.warm_bbox(bbox);
    let t0 = Instant::now();
    graph.apply_eco_reweighting(&elev, eco);
    t0.elapsed().as_secs_f64() * 1000.0
}

fn bench(
    pbf: &Path,
    archive_path: &Path,
    variant: u8,
    slat: f64,
    slon: f64,
    elat: f64,
    elon: f64,
    elev_dir: Option<&Path>,
) {
    let bbox = trip_bbox(slat, slon, elat, elon);
    eprintln!(
        "bbox={:.3},{:.3},{:.3},{:.3} variant={} snap_max_m={:.0}",
        bbox[0],
        bbox[1],
        bbox[2],
        bbox[3],
        if variant == VARIANT_B { "B" } else { "A" },
        max_waypoint_snap_m(RoutingProfile::Car)
    );

    eprintln!("=== COLD PBF graph_build (build_from_pbf_bbox) ===");
    let t_cold = Instant::now();
    let cold = RouteGraph::build_from_pbf_bbox(pbf, RoutingProfile::Car, bbox).expect("cold build");
    let cold_build_ms = t_cold.elapsed().as_secs_f64() * 1000.0;
    eprintln!(
        "cold_graph_build_ms={cold_build_ms:.1} nodes={} edges={}",
        cold.nodes.len(),
        cold.edges.len()
    );

    eprintln!("=== INDEXED rkyv+memmap2 zero-copy access ===");
    let t_map = Instant::now();
    let file = fs::File::open(archive_path).expect("open archive");
    let mmap = unsafe { Mmap::map(&file).expect("mmap") };
    let archived =
        rkyv::access::<ArchivedFlatGraphPack, RkyvError>(&mmap[..]).expect("rkyv access");
    let mmap_access_ms = t_map.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(u32::from(archived.magic), MAGIC, "bad magic");
    eprintln!(
        "mmap_access_ms={mmap_access_ms:.3} nodes={} edges={} has_delta_h={} file_bytes={}",
        archived.node_ids.len(),
        archived.edge_length_m.len(),
        archived.has_delta_h,
        mmap.len()
    );

    let t_touch = Instant::now();
    let touch_acc = touch_all_edges(archived);
    let touch_all_ms = t_touch.elapsed().as_secs_f64() * 1000.0;
    eprintln!("touch_all_edges_ms={touch_all_ms:.1} checksum={touch_acc:.3}");

    let eco_car = EcoConfig::for_profile(Profile::Car);
    let t_plan = Instant::now();
    let start = nearest_idx(archived, slat, slon).expect("snap start");
    let goal = nearest_idx(archived, elat, elon).expect("snap goal");
    let (path, dist_km) = plan_archived(archived, start, goal, false, &eco_car).expect("no path");
    let first_plan_ms = t_plan.elapsed().as_secs_f64() * 1000.0;
    eprintln!(
        "first_plan_ms={first_plan_ms:.1} dist_km={dist_km:.2} path_nodes={}",
        path.len()
    );

    // Phase 0 bar mirrors 1b: time to graph ready for A* (no owned RouteGraph).
    // mmap+rkyv::access is that gate; touch_all is a full page-in upper bound.
    let indexed_load_ms = mmap_access_ms;
    let speedup = cold_build_ms / indexed_load_ms.max(0.001);
    let pass_abs = indexed_load_ms <= 2000.0;
    let pass_ratio = speedup >= 10.0;
    let go = pass_abs && pass_ratio;
    eprintln!(
        "RESULT variant={} indexed_load_ms={indexed_load_ms:.3} (mmap+rkyv access) touch_all_ms={touch_all_ms:.1} first_plan_ms={first_plan_ms:.1} speedup={speedup:.1}x phase0_abs_le_2s={pass_abs} phase0_ge_10x={pass_ratio} PHASE1C={}",
        if variant == VARIANT_B { "B" } else { "A" },
        if go { "GO" } else { "NO-GO" }
    );

    if variant == VARIANT_B {
        let elev = elev_dir.expect("--elev-dir for variant B eco compare");
        eprintln!("=== Variant B eco: DEM reweight vs Δh arithmetic ===");
        let dem_ms = dem_reweight_ms(pbf, bbox, elev, &eco_car);
        eprintln!("dem_reweight_only_ms={dem_ms:.1} (after cold bbox build; Car EcoConfig)");

        let (car_arith_ms, car_sum) = arith_reweight_ms(archived, &eco_car);
        let eco_moto = EcoConfig::for_profile(Profile::Motorcycle);
        let (moto_arith_ms, moto_sum) = arith_reweight_ms(archived, &eco_moto);
        eprintln!(
            "delta_h_arith_ms car={car_arith_ms:.3} sum_j={car_sum:.0} motorcycle={moto_arith_ms:.3} sum_j={moto_sum:.0}"
        );
        assert!(
            (car_sum - moto_sum).abs() > 1.0,
            "Car and Motorcycle energy from shared Δh should differ"
        );

        let t_eco_plan = Instant::now();
        let (path_e, dist_e) =
            plan_archived(archived, start, goal, true, &eco_car).expect("eco path");
        let eco_plan_ms = t_eco_plan.elapsed().as_secs_f64() * 1000.0;
        eprintln!(
            "eco_plan_ms={eco_plan_ms:.1} dist_km={dist_e:.2} path_nodes={} (live Δh→energy in A*)",
            path_e.len()
        );
        eprintln!(
            "ECO_COMPARE dem_reweight_ms={dem_ms:.1} delta_h_arith_car_ms={car_arith_ms:.3} speedup_vs_dem={:.1}x",
            dem_ms / car_arith_ms.max(0.001)
        );
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        usage();
    }
    let variant = match arg_value(&args, "--variant")
        .unwrap_or_else(|| "a".into())
        .to_ascii_lowercase()
        .as_str()
    {
        "a" | "plain" => VARIANT_A,
        "b" | "delta" | "elev" => VARIANT_B,
        other => panic!("unknown --variant {other}"),
    };
    match args[1].as_str() {
        "build" => {
            let pbf = PathBuf::from(arg_value(&args, "--pbf").expect("--pbf"));
            let out = PathBuf::from(arg_value(&args, "--out").expect("--out"));
            let bbox = arg_value(&args, "--bbox").map(|s| parse_bbox(&s));
            let elev = arg_value(&args, "--elev-dir").map(PathBuf::from);
            if variant == VARIANT_B && elev.is_none() {
                panic!("variant b requires --elev-dir");
            }
            build(&pbf, &out, variant, bbox, elev.as_deref());
        }
        "bench" => {
            let pbf = PathBuf::from(arg_value(&args, "--pbf").expect("--pbf"));
            let out = PathBuf::from(arg_value(&args, "--out").expect("--out"));
            let slat: f64 = arg_value(&args, "--start-lat")
                .expect("--start-lat")
                .parse()
                .unwrap();
            let slon: f64 = arg_value(&args, "--start-lon")
                .expect("--start-lon")
                .parse()
                .unwrap();
            let elat: f64 = arg_value(&args, "--end-lat")
                .expect("--end-lat")
                .parse()
                .unwrap();
            let elon: f64 = arg_value(&args, "--end-lon")
                .expect("--end-lon")
                .parse()
                .unwrap();
            let elev = arg_value(&args, "--elev-dir").map(PathBuf::from);
            if variant == VARIANT_B && elev.is_none() {
                panic!("variant b requires --elev-dir");
            }
            bench(&pbf, &out, variant, slat, slon, elat, elon, elev.as_deref());
        }
        _ => usage(),
    }
}
