//! Isolate the Bevensen dest hop: load real ostlandet + vestlandet car tiles
//! and report snap / component / directed connectivity. Host-only; no APK.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::time::Instant;

use driver_break_core::routing::graph::{RouteGraph, RouteOptions, RoutingProfile};
use driver_break_core::routing::indexed::{load_graph_pack_clips, merge_tile_graphs};
use driver_break_core::routing::plan_bbox::{
    plan_edge_clips, trip_bbox_points, PlanEdgeClipMode,
};
use osm4routing::NodeId;

fn haversine_m(alat: f64, alon: f64, blat: f64, blon: f64) -> f64 {
    let r = 6_371_000.0;
    let dlat = (blat - alat).to_radians();
    let dlon = (blon - alon).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + alat.to_radians().cos() * blat.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

fn load_stem_clips(
    dir: &Path,
    files: &[&str],
    clips: &[[f64; 4]],
    profile: RoutingProfile,
) -> RouteGraph {
    let mut parts = Vec::new();
    for f in files {
        let p = dir.join(f);
        match load_graph_pack_clips(&p, profile, Some(clips)) {
            Ok(g) => {
                eprintln!("  loaded {f} nodes={} edges={}", g.nodes.len(), g.edges.len());
                parts.push(g);
            }
            Err(e) => eprintln!("  skip {f}: {e}"),
        }
    }
    merge_tile_graphs(parts, profile)
}

fn run_listed_multi(
    label: &str,
    dir: &Path,
    ost_files: &[&str],
    vest_files: &[&str],
    primary_vest: bool,
    start: (f64, f64),
    dest: (f64, f64),
    clips: &[[f64; 4]],
) {
    println!("=== {label} clips={} primary_vest={primary_vest} ===", clips.len());
    let t0 = Instant::now();
    let ost = load_stem_clips(dir, ost_files, clips, RoutingProfile::Car);
    let vest = load_stem_clips(dir, vest_files, clips, RoutingProfile::Car);
    let parts = if primary_vest {
        vec![vest, ost]
    } else {
        vec![ost, vest]
    };
    let g = merge_tile_graphs(parts, RoutingProfile::Car);
    let opts = RouteOptions::default();
    let snap_start = g.nearest_routable_with_options(start.0, start.1, &opts, false);
    let snap_dest = g.nearest_routable_with_options(dest.0, dest.1, &opts, false);
    let (Ok((s, sm)), Ok((d, dm))) = (snap_start, snap_dest) else {
        println!("SNAP FAIL start={snap_start:?} dest={snap_dest:?} nodes={}", g.nodes.len());
        return;
    };
    let weak = g.same_weak_component(s, d);
    let dir_ok = weak && g.directed_reachable_with_options(s, d, &opts);
    println!(
        "nodes={} edges={} snap_start_m={sm:.1} snap_dest_m={dm:.1} weak_ok={weak} directed_ok={dir_ok} load_ms={}",
        g.nodes.len(),
        g.edges.len(),
        t0.elapsed().as_millis()
    );
    println!(
        "result={}",
        if dir_ok {
            "directed_ok"
        } else {
            "disconnected"
        }
    );
    println!();
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

fn dummy_pbf(dir: &Path) -> PathBuf {
    // try_load only needs the stem from the filename.
    dir.join("ostlandet-latest.osm.pbf")
}

fn run_app_corridor(
    label: &str,
    dir: &Path,
    start: (f64, f64),
    dest: (f64, f64),
    mode: driver_break_core::routing::plan_bbox::PlanEdgeClipMode,
    fill_budget: bool,
) {
    use driver_break_core::routing::indexed::try_load_graph_for_plan_corridor_with_pack_dirs;
    use driver_break_core::routing::plan_bbox::{
        set_plan_tile_budget_at_least, trip_bbox_points, PlanEdgeClipMode,
        MAX_PLAN_TILES_MULTI_STEM,
    };

    set_plan_tile_budget_at_least(0);
    if fill_budget {
        set_plan_tile_budget_at_least(MAX_PLAN_TILES_MULTI_STEM);
    }
    let pts = [start, dest];
    let pad = 0.35_f64;
    let bbox = trip_bbox_points(&pts, pad);
    println!(
        "=== {label} mode={mode:?} pad={pad} bbox={:.3},{:.3},{:.3},{:.3} ===",
        bbox[0], bbox[1], bbox[2], bbox[3]
    );
    let t0 = Instant::now();
    let pbf = dummy_pbf(dir);
    match try_load_graph_for_plan_corridor_with_pack_dirs(
        dir,
        &[],
        &pbf,
        RoutingProfile::Car,
        Some(bbox),
        Some(pts.as_slice()),
        mode,
    ) {
        Ok(g) => {
            let opts = RouteOptions::default();
            let snap_start = g.nearest_routable_with_options(start.0, start.1, &opts, false);
            let snap_dest = g.nearest_routable_with_options(dest.0, dest.1, &opts, false);
            let (Ok((s, sm)), Ok((d, dm))) = (snap_start, snap_dest) else {
                println!(
                    "SNAP FAIL start={snap_start:?} dest={snap_dest:?} nodes={} load_ms={}",
                    g.nodes.len(),
                    t0.elapsed().as_millis()
                );
                return;
            };
            let weak = g.same_weak_component(s, d);
            let dir_ok = weak && g.directed_reachable_with_options(s, d, &opts);
            println!(
                "nodes={} edges={} snap_start_m={sm:.1} snap_dest_m={dm:.1} \
                 weak_ok={weak} directed_ok={dir_ok} load_ms={}",
                g.nodes.len(),
                g.edges.len(),
                t0.elapsed().as_millis()
            );
            println!(
                "result={}",
                if dir_ok {
                    "directed_ok"
                } else {
                    "disconnected"
                }
            );
        }
        Err(e) => println!("LOAD FAIL {e} load_ms={}", t0.elapsed().as_millis()),
    }
    set_plan_tile_budget_at_least(0);
    println!();
}

fn run_hop_astar(
    label: &str,
    dir: &Path,
    start: (f64, f64),
    dest: (f64, f64),
    surface_car: bool,
) {
    use driver_break_core::routing::graph::SurfaceRoutingMode;
    use driver_break_core::routing::indexed::try_load_graph_for_plan_corridor_with_pack_dirs;
    use driver_break_core::routing::plan_bbox::{
        set_plan_tile_budget_at_least, trip_bbox_points, PlanEdgeClipMode,
    };

    set_plan_tile_budget_at_least(0);
    let pts = [start, dest];
    let bbox = trip_bbox_points(&pts, 0.35);
    println!("=== {label} surface_car={surface_car} ===");
    let t_load = Instant::now();
    let g = match try_load_graph_for_plan_corridor_with_pack_dirs(
        dir,
        &[],
        &dummy_pbf(dir),
        RoutingProfile::Car,
        Some(bbox),
        Some(pts.as_slice()),
        PlanEdgeClipMode::CorridorBand,
    ) {
        Ok(g) => g,
        Err(e) => {
            println!("LOAD FAIL {e}");
            return;
        }
    };
    let load_ms = t_load.elapsed().as_millis();
    let mut opts = RouteOptions::default();
    if surface_car {
        opts.surface_routing_mode = Some(SurfaceRoutingMode::Car);
    }
    let (Ok((s, sm)), Ok((d, dm))) = (
        g.nearest_routable_with_options(start.0, start.1, &opts, false),
        g.nearest_routable_with_options(dest.0, dest.1, &opts, false),
    ) else {
        println!("SNAP FAIL nodes={}", g.nodes.len());
        return;
    };
    println!(
        "load_ms={load_ms} nodes={} edges={} snap_start_m={sm:.1} snap_dest_m={dm:.1}",
        g.nodes.len(),
        g.edges.len()
    );
    let t_ast = Instant::now();
    let stats = g.shortest_path_with_options_stats(s, d, false, &opts);
    let astar_ms = t_ast.elapsed().as_millis();
    let exp = stats.expansions.max(1);
    println!(
        "astar_ms={astar_ms} expansions={} terminate={} us_per_expansion={:.1} path={}",
        stats.expansions,
        stats.terminate_reason,
        (astar_ms as f64) * 1000.0 / (exp as f64),
        stats.path.is_some()
    );
    set_plan_tile_budget_at_least(0);
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
    let hop17 = (61.67732_f64, 8.30020);
    let dest = (61.44338_f64, 7.46140);
    let mode = std::env::var("NAVI_PROBE_MODE").unwrap_or_else(|_| "app".to_string());

    if mode == "aabb" || mode == "all" {
        // Isolated AABB merge of listed tiles (does not match the emulator).
        let hops = [
            ("hop17", hop17, dest),
            ("fu3_sognefjell", (61.61687, 8.04346), dest),
        ];
        for (name, start, d) in hops {
            let clip = [
                f64::min(start.0, d.0) - 0.40,
                f64::min(start.1, d.1) - 0.40,
                f64::max(start.0, d.0) + 0.40,
                f64::max(start.1, d.1) + 0.40,
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
                    d,
                    clip,
                    RoutingProfile::Car,
                );
            }
        }
    }

    if mode == "app" || mode == "all" {
        use driver_break_core::routing::plan_bbox::PlanEdgeClipMode;
        let pts = [hop17, dest];
        let bbox = trip_bbox_points(&pts, 0.35);
        let band_clips = plan_edge_clips(Some(pts.as_slice()), Some(bbox), PlanEdgeClipMode::CorridorBand)
            .unwrap_or_default();
        let aabb_clips =
            plan_edge_clips(Some(pts.as_slice()), Some(bbox), PlanEdgeClipMode::TripAabb)
                .unwrap_or_default();
        println!(
            "--- listed tiles + emulator corridor-band edge clips (must fail hop 17) ---"
        );
        println!("band_clips={}", band_clips.len());
        run_listed_multi(
            "hop17_listed_band",
            &dir,
            &ost_files,
            &vest_files,
            true,
            hop17,
            dest,
            &band_clips,
        );
        println!("--- listed tiles + trip-AABB edge clips (must pass) ---");
        run_listed_multi(
            "hop17_listed_aabb",
            &dir,
            &ost_files,
            &vest_files,
            true,
            hop17,
            dest,
            &aabb_clips,
        );
        println!("--- try_load CorridorBand vs TripAabb (pack dir as present) ---");
        run_app_corridor(
            "hop17_band",
            &dir,
            hop17,
            dest,
            PlanEdgeClipMode::CorridorBand,
            false,
        );
        run_app_corridor(
            "hop17_aabb",
            &dir,
            hop17,
            dest,
            PlanEdgeClipMode::TripAabb,
            true,
        );
    }

    if mode == "astar" || mode == "all" {
        // Last emulator plan (pid 11623): hop 13 after i=12 success.
        let hop13_a = (60.64909_f64, 10.52005);
        let hop13_b = (61.25957_f64, 10.22110);
        run_hop_astar("hop13_default", &dir, hop13_a, hop13_b, false);
        run_hop_astar("hop13_surface_car", &dir, hop13_a, hop13_b, true);
        // Prior densify hop 12 (Oslo → Ostlandet joint) as a second mid-route sample.
        let hop12_a = (59.9100_f64, 10.7500);
        let hop12_b = (60.7950_f64, 11.0680);
        run_hop_astar("hop12_default", &dir, hop12_a, hop12_b, false);
        run_hop_astar("hop12_surface_car", &dir, hop12_a, hop12_b, true);
    }
}
