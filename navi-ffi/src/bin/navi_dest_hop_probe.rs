//! Isolate the Bevensen dest hop: load real ostlandet + vestlandet car tiles
//! and report snap / component / directed connectivity. Host-only; no APK.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::time::Instant;

use driver_break_core::routing::graph::{RouteGraph, RouteOptions, RoutingProfile};
use driver_break_core::routing::indexed::{load_graph_pack_clips, merge_tile_graphs};
use osm4routing::NodeId;

fn haversine_m(alat: f64, alon: f64, blat: f64, blon: f64) -> f64 {
    let r = 6_371_000.0;
    let dlat = (blat - alat).to_radians();
    let dlon = (blon - alon).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + alat.to_radians().cos() * blat.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

fn load_stem(dir: &Path, files: &[&str], clip: [f64; 4], profile: RoutingProfile) -> RouteGraph {
    let clips = [clip];
    let mut parts = Vec::new();
    for f in files {
        let p = dir.join(f);
        match load_graph_pack_clips(&p, profile, Some(&clips)) {
            Ok(g) => {
                eprintln!("  loaded {f} nodes={} edges={}", g.nodes.len(), g.edges.len());
                parts.push(g);
            }
            Err(e) => eprintln!("  skip {f}: {e}"),
        }
    }
    merge_tile_graphs(parts, profile)
}

fn node_pack(id: NodeId, ost: &HashSet<i64>, vest: &HashSet<i64>) -> &'static str {
    let o = ost.contains(&id.0);
    let v = vest.contains(&id.0);
    match (o, v) {
        (true, true) => "both",
        (true, false) => "ostlandet",
        (false, true) => "vestlandet",
        (false, false) => "neither",
    }
}

fn bfs_undirected(g: &RouteGraph, start: NodeId, goal: NodeId) -> Option<Vec<NodeId>> {
    let mut adj: HashMap<i64, Vec<i64>> = HashMap::new();
    for e in &g.edges {
        adj.entry(e.source.0).or_default().push(e.target.0);
        adj.entry(e.target.0).or_default().push(e.source.0);
    }
    let mut prev: HashMap<i64, i64> = HashMap::new();
    let mut q = VecDeque::new();
    q.push_back(start.0);
    prev.insert(start.0, start.0);
    while let Some(u) = q.pop_front() {
        if u == goal.0 {
            let mut path = vec![u];
            while path[path.len() - 1] != start.0 {
                let p = prev[&path[path.len() - 1]];
                path.push(p);
            }
            path.reverse();
            return Some(path.into_iter().map(NodeId).collect());
        }
        for &v in adj.get(&u).into_iter().flatten() {
            if prev.contains_key(&v) {
                continue;
            }
            prev.insert(v, u);
            q.push_back(v);
        }
    }
    None
}

fn directed_hop_ok(g: &RouteGraph, a: NodeId, b: NodeId) -> bool {
    g.outgoing_edge_indices(a)
        .iter()
        .any(|&i| g.edges[i].target == b)
}

fn run_cfg(
    label: &str,
    dir: &Path,
    ost_files: &[&str],
    vest_files: &[&str],
    primary_first: bool,
    start: (f64, f64),
    dest: (f64, f64),
    clip: [f64; 4],
    profile: RoutingProfile,
) {
    println!("=== {label} profile={profile:?} primary_first_vestlandet={primary_first} ===");
    let t0 = Instant::now();
    let ost = load_stem(dir, ost_files, clip, profile);
    let vest = load_stem(dir, vest_files, clip, profile);
    let ost_ids: HashSet<i64> = ost.nodes.keys().map(|n| n.0).collect();
    let vest_ids: HashSet<i64> = vest.nodes.keys().map(|n| n.0).collect();
    let both = ost_ids.intersection(&vest_ids).count();
    println!(
        "ost nodes={} edges={} vest nodes={} edges={} shared_node_ids={}",
        ost.nodes.len(),
        ost.edges.len(),
        vest.nodes.len(),
        vest.edges.len(),
        both
    );

    let mut rv55 = 0u32;
    for g in [&ost, &vest] {
        for e in &g.edges {
            let r = e.road_ref.as_deref().unwrap_or("");
            if r.contains("55") {
                rv55 += 1;
            }
        }
    }
    println!("rv55-ish edges in either pack (pre-merge, ref contains 55): {rv55}");

    let parts = if primary_first {
        vec![vest, ost]
    } else {
        vec![ost, vest]
    };
    let g = merge_tile_graphs(parts, profile);
    println!(
        "merged nodes={} edges={} load_ms={}",
        g.nodes.len(),
        g.edges.len(),
        t0.elapsed().as_millis()
    );

    let opts = RouteOptions::default();
    let snap_start = g.nearest_routable_with_options(start.0, start.1, &opts, false);
    let snap_dest = g.nearest_routable_with_options(dest.0, dest.1, &opts, false);
    let (Ok((s, sm)), Ok((d, dm))) = (snap_start, snap_dest) else {
        println!("SNAP FAIL start={snap_start:?} dest={snap_dest:?}");
        return;
    };
    let sll = g.node_lat_lon(s).unwrap_or(start);
    let dll = g.node_lat_lon(d).unwrap_or(dest);
    let weak = g.same_weak_component(s, d);
    let dir_ok = weak && g.directed_reachable_with_options(s, d, &opts);
    println!(
        "snap_start={:.5},{:.5} snap_start_m={sm:.1} pack={} id={}",
        sll.0,
        sll.1,
        node_pack(s, &ost_ids, &vest_ids),
        s.0
    );
    println!(
        "snap_dest={:.5},{:.5} snap_dest_m={dm:.1} pack={} id={}",
        dll.0,
        dll.1,
        node_pack(d, &ost_ids, &vest_ids),
        d.0
    );
    println!(
        "comp_start={} comp_dest={} weak_ok={weak} directed_ok={dir_ok}",
        g.weak_component_id(s),
        g.weak_component_id(d)
    );

    // Rv 55 edges near the Sognefjell / county border (~61.55, 8.05).
    let mut n55 = 0u32;
    let mut n55_oneway = 0u32;
    let mut n55_fwd = 0u32;
    let mut n55_rev = 0u32;
    for e in &g.edges {
        let r = e.road_ref.as_deref().unwrap_or("");
        if !(r.contains("55") || e.name.as_deref().unwrap_or("").contains("Sognefjell")) {
            continue;
        }
        let mid_lat = (e.start_lat + e.end_lat) * 0.5;
        let mid_lon = (e.start_lon + e.end_lon) * 0.5;
        if haversine_m(mid_lat, mid_lon, 61.55, 8.05) > 25_000.0 {
            continue;
        }
        n55 += 1;
        if e.is_oneway {
            n55_oneway += 1;
        }
        let west = e.end_lon < e.start_lon;
        if west {
            n55_fwd += 1;
        } else {
            n55_rev += 1;
        }
    }
    println!(
        "rv55/sognefjell edges within 25km of 61.55,8.05: n={n55} oneway={n55_oneway} \
         westbound={n55_fwd} eastbound={n55_rev}"
    );

    if weak && !dir_ok {
        if let Some(path) = bfs_undirected(&g, s, d) {
            println!("undirected path hops={}", path.len().saturating_sub(1));
            let mut shown = 0u32;
            for w in path.windows(2) {
                let a = w[0];
                let b = w[1];
                let fwd = directed_hop_ok(&g, a, b);
                if fwd {
                    continue;
                }
                let rev = directed_hop_ok(&g, b, a);
                let edge = g.edges.iter().find(|e| {
                    (e.source == a && e.target == b) || (e.source == b && e.target == a)
                });
                if shown < 12 {
                    if let Some(e) = edge {
                        println!(
                            "  DIRECTED_BREAK {:.5},{:.5}->{:.5},{:.5} ref={:?} name={:?} \
                             hwy={:?} oneway={} access_forbidden={} cond={:?} \
                             reverse_edge_exists={rev} src_pack={} tgt_pack={}",
                            e.start_lat,
                            e.start_lon,
                            e.end_lat,
                            e.end_lon,
                            e.road_ref,
                            e.name,
                            e.highway,
                            e.is_oneway,
                            e.access_forbidden,
                            e.motor_vehicle_conditional,
                            node_pack(e.source, &ost_ids, &vest_ids),
                            node_pack(e.target, &ost_ids, &vest_ids)
                        );
                    } else {
                        println!(
                            "  DIRECTED_BREAK node {} -> {} no packed edge either way rev={rev}",
                            a.0, b.0
                        );
                    }
                    shown += 1;
                }
            }
            if shown == 0 {
                println!(
                    "  undirected path is fully directed; directed_ok false is not a simple one-way on that path"
                );
            }
        } else {
            println!("no undirected path despite weak_ok (component id collision?)");
        }
    }
    println!("result={}", if dir_ok { "directed_ok" } else { "disconnected" });
    println!();
}

fn main() {
    let dir = PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("usage: navi-dest-hop-probe PACK_DIR"),
    );
    let ost_files = [
        "ostlandet-latest.navi-graph-car.t2_0.rkyv",
        "ostlandet-latest.navi-graph-car.t2_1.rkyv",
        "ostlandet-latest.navi-graph-car.t3_0.rkyv",
        "ostlandet-latest.navi-graph-car.t3_1.rkyv",
    ];
    let vest_files = [
        "vestlandet-latest.navi-graph-car.t3_2.rkyv",
        "vestlandet-latest.navi-graph-car.t3_3.rkyv",
        "vestlandet-latest.navi-graph-car.t3_4.rkyv",
        "vestlandet-latest.navi-graph-car.t4_2.rkyv",
        "vestlandet-latest.navi-graph-car.t4_3.rkyv",
        "vestlandet-latest.navi-graph-car.t4_4.rkyv",
    ];
    // Hop 17 (this densify) and FU3 dest hop (Sognefjell).
    let hops = [
        (
            "hop17",
            (61.67732, 8.30020),
            (61.44338, 7.46140),
        ),
        (
            "fu3_sognefjell",
            (61.61687, 8.04346),
            (61.44338, 7.46140),
        ),
    ];
    for (name, start, dest) in hops {
        let clip = [
            f64::min(start.0, dest.0) - 0.40,
            f64::min(start.1, dest.1) - 0.40,
            f64::max(start.0, dest.0) + 0.40,
            f64::max(start.1, dest.1) + 0.40,
        ];
        for primary_vest in [false, true] {
            let label = format!(
                "{name} primary={}",
                if primary_vest {
                    "vestlandet"
                } else {
                    "ostlandet"
                }
            );
            run_cfg(
                &label,
                &dir,
                &ost_files,
                &vest_files,
                primary_vest,
                start,
                dest,
                clip,
                RoutingProfile::Car,
            );
        }
    }
}
