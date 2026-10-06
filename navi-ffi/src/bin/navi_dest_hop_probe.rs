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

fn campaign_vehicle() -> driver_break_core::config::VehicleLimits {
    driver_break_core::config::VehicleLimits {
        axle_weight_kg: Some(1661.2),
        bogie_weight_kg: None,
        height_m: Some(2.477),
        width_m: Some(2.297),
        length_m: Some(5.304),
        total_weight_kg: Some(3020.4),
    }
}

fn load_corridor_graph(
    dir: &Path,
    start: (f64, f64),
    dest: (f64, f64),
    profile: RoutingProfile,
) -> Option<std::sync::Arc<RouteGraph>> {
    use driver_break_core::routing::indexed::try_load_graph_for_plan_corridor_with_pack_dirs;
    use driver_break_core::routing::plan_bbox::{
        set_plan_tile_budget_at_least, trip_bbox_points, PlanEdgeClipMode,
    };

    set_plan_tile_budget_at_least(0);
    let pts = [start, dest];
    let bbox = trip_bbox_points(&pts, 0.35);
    let g = try_load_graph_for_plan_corridor_with_pack_dirs(
        dir,
        &[],
        &dummy_pbf(dir),
        profile,
        Some(bbox),
        Some(pts.as_slice()),
        PlanEdgeClipMode::CorridorBand,
    )
    .ok()?;
    set_plan_tile_budget_at_least(0);
    Some(g)
}

fn count_option_filters(g: &RouteGraph, opts: &RouteOptions) -> (u64, u64, u64, u64, u64, u64) {
    let mut h = 0u64;
    let mut w = 0u64;
    let mut axle = 0u64;
    let mut len = 0u64;
    let mut wt = 0u64;
    let mut datex = 0u64;
    let Some(ref lim) = opts.vehicle else {
        let datex_n = g
            .edges
            .iter()
            .filter(|e| {
                opts.datex_impacts.iter().any(|c| {
                    c.impact == driver_break_core::datex::DatexImpact::Block
                        && driver_break_core::routing::graph::edge_distance_m(e, c.lat, c.lon)
                            <= c.radius_m
                })
            })
            .count() as u64;
        return (0, 0, 0, 0, 0, datex_n);
    };
    for e in &g.edges {
        if let (Some(vh), Some(max)) = (lim.height_m, e.maxheight_m) {
            if vh > max {
                h += 1;
            }
        }
        if let (Some(vw), Some(max)) = (lim.width_m, e.maxwidth_m) {
            if vw > max {
                w += 1;
            }
        }
        if let (Some(ax), Some(max)) = (lim.axle_weight_kg, e.maxaxleload_t) {
            if ax / 1000.0 > max {
                axle += 1;
            }
        }
        if let (Some(l), Some(max)) = (lim.length_m, e.maxlength_m) {
            if l > max {
                len += 1;
            }
        }
        if let (Some(tw), Some(max)) = (lim.total_weight_kg, e.maxweight_t) {
            if tw / 1000.0 > max {
                wt += 1;
            }
        }
        if opts.datex_impacts.iter().any(|c| {
            c.impact == driver_break_core::datex::DatexImpact::Block
                && driver_break_core::routing::graph::edge_distance_m(e, c.lat, c.lon) <= c.radius_m
        }) {
            datex += 1;
        }
    }
    (h, w, axle, len, wt, datex)
}

fn dest_block_sample(g: &RouteGraph, dest: NodeId, opts: &RouteOptions, n: usize) {
    let mut shown = 0usize;
    for e in &g.edges {
        if e.target != dest && e.source != dest {
            continue;
        }
        let lim = opts.vehicle.as_ref();
        let mut reasons: Vec<&str> = Vec::new();
        if e.access_forbidden {
            reasons.push("access_forbidden");
        }
        if let Some(lim) = lim {
            if let (Some(vh), Some(max)) = (lim.height_m, e.maxheight_m) {
                if vh > max {
                    reasons.push("height");
                }
            }
            if let (Some(vw), Some(max)) = (lim.width_m, e.maxwidth_m) {
                if vw > max {
                    reasons.push("width");
                }
            }
            if let (Some(ax), Some(max)) = (lim.axle_weight_kg, e.maxaxleload_t) {
                if ax / 1000.0 > max {
                    reasons.push("axle");
                }
            }
            if let (Some(l), Some(max)) = (lim.length_m, e.maxlength_m) {
                if l > max {
                    reasons.push("length");
                }
            }
            if let (Some(tw), Some(max)) = (lim.total_weight_kg, e.maxweight_t) {
                if tw / 1000.0 > max {
                    reasons.push("weight");
                }
            }
        }
        if reasons.is_empty() {
            continue;
        }
        println!(
            "  dest_incident_blocked {}->{} hwy={:?} ref={:?} name={:?} \
             maxh={:?} maxw={:?} maxaxle={:?} maxweight={:?} maxlength={:?} reasons={:?}",
            e.source.0,
            e.target.0,
            e.highway,
            e.road_ref,
            e.name,
            e.maxheight_m,
            e.maxwidth_m,
            e.maxaxleload_t,
            e.maxweight_t,
            e.maxlength_m,
            reasons
        );
        shown += 1;
        if shown >= n {
            break;
        }
    }
}

fn run_ablate_one(
    label: &str,
    g: &RouteGraph,
    start: (f64, f64),
    dest: (f64, f64),
    opts: &RouteOptions,
) {
    let t_bind = Instant::now();
    let mut opts = opts.clone();
    g.bind_datex_overlay(&mut opts);
    let bind_ms = t_bind.elapsed().as_millis();
    let t_snap = Instant::now();
    let (Ok((s, sm)), Ok((d, dm))) = (
        g.nearest_routable_with_options(start.0, start.1, &opts, false),
        g.nearest_routable_with_options(dest.0, dest.1, &opts, false),
    ) else {
        println!("{label} SNAP FAIL nodes={} bind_ms={bind_ms}", g.nodes.len());
        return;
    };
    let snap_ms = t_snap.elapsed().as_millis();
    let t_dir = Instant::now();
    let weak = g.same_weak_component(s, d);
    let dir_ok = weak && g.directed_reachable_with_options(s, d, &opts);
    let dir_ms = t_dir.elapsed().as_millis();
    let t_ast = Instant::now();
    let stats = g.shortest_path_with_options_stats(s, d, false, &opts);
    let astar_ms = t_ast.elapsed().as_millis();
    let exp = stats.expansions.max(1);
    let us = (astar_ms as f64) * 1000.0 / (exp as f64);
    println!(
        "{label} nodes={} edges={} bind_ms={bind_ms} snap_m={sm:.1}/{dm:.1} snap_ms={snap_ms} \
         weak={weak} directed_ok={dir_ok} dir_ms={dir_ms} astar_ms={astar_ms} \
         expansions={} terminate={} us_per_expansion={us:.1} path={}",
        g.nodes.len(),
        g.edges.len(),
        stats.expansions,
        stats.terminate_reason,
        stats.path.is_some()
    );
    if !dir_ok {
        dest_block_sample(g, d, &opts, 12);
    }
}

fn run_ablate_matrix(
    dir: &Path,
    data_dir: Option<&Path>,
    hop_name: &str,
    start: (f64, f64),
    dest: (f64, f64),
) {
    use driver_break_core::routing::graph::{MotorSoftCostProfile, SurfaceRoutingMode};

    println!("===== ablate {hop_name} =====");
    let datex = if let Some(dd) = data_dir {
        let now = chrono::Utc::now();
        match driver_break_core::datex::load_plan_datex_situations(dd, now) {
            Some(all) => {
                // Reconstruct the long-plan ~53 list: 25 km of the campaign
                // corridor (not hop-local 5 km, not nationwide). Overlay bind
                // is still O(E x that list) once; A* must stay ~2 us/exp.
                let corridor: Vec<(f64, f64)> = vec![
                    (53.079686, 10.587198),
                    (53.55, 10.0),
                    (55.3, 9.5),
                    (59.91, 10.75),
                    (61.75, 9.54),
                    (61.86914, 9.10551),
                    (61.67732, 8.30020),
                    (61.44338, 7.46140),
                ];
                driver_break_core::datex::impacts_near_route(&all, &corridor, 25_000.0, now)
            }
            None => Vec::new(),
        }
    } else {
        Vec::new()
    };
    println!(
        "datex_impacts={} data_dir={}",
        datex.len(),
        data_dir
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "none".into())
    );
    for (i, c) in datex.iter().enumerate() {
        println!(
            "  datex[{i}] {:?} id={} lat={:.5} lon={:.5} r_m={:.0} mult={}",
            c.impact, c.situation_id, c.lat, c.lon, c.radius_m, c.penalize_mult
        );
    }

    let g_car = match load_corridor_graph(dir, start, dest, RoutingProfile::Car) {
        Some(g) => g,
        None => {
            println!("{hop_name} LOAD FAIL car");
            return;
        }
    };
    let g_truck = match load_corridor_graph(dir, start, dest, RoutingProfile::Truck) {
        Some(g) => g,
        None => {
            println!("{hop_name} LOAD FAIL truck");
            return;
        }
    };
    println!(
        "loaded car_nodes={} truck_nodes={} car_edges={} truck_edges={}",
        g_car.nodes.len(),
        g_truck.nodes.len(),
        g_car.edges.len(),
        g_truck.edges.len()
    );

    let mut steps: Vec<(&str, &RouteGraph, RouteOptions)> = Vec::new();

    let o0 = RouteOptions::default();
    steps.push(("0_car_default", &g_car, o0.clone()));

    let o1 = RouteOptions::default();
    steps.push(("1_truck_graph_default_opts", &g_truck, o1.clone()));

    let mut o2 = RouteOptions::default();
    o2.surface_routing_mode = Some(SurfaceRoutingMode::Car);
    steps.push(("2_truck+surface_car", &g_truck, o2.clone()));

    let mut o3 = o2.clone();
    o3.motor_soft = Some((SurfaceRoutingMode::Car, MotorSoftCostProfile::MobileHome));
    steps.push(("3_+motor_soft_mobile_home", &g_truck, o3.clone()));

    let mut o4 = o3.clone();
    o4.vehicle = Some(campaign_vehicle());
    steps.push(("4_+vehicle_campaign", &g_truck, o4.clone()));

    let mut o4h = o3.clone();
    o4h.vehicle = Some(driver_break_core::config::VehicleLimits {
        axle_weight_kg: None,
        bogie_weight_kg: None,
        height_m: Some(2.477),
        width_m: None,
        length_m: None,
        total_weight_kg: None,
    });
    steps.push(("4a_height_only", &g_truck, o4h.clone()));

    let mut o4w = o3.clone();
    o4w.vehicle = Some(driver_break_core::config::VehicleLimits {
        axle_weight_kg: None,
        bogie_weight_kg: None,
        height_m: None,
        width_m: Some(2.297),
        length_m: None,
        total_weight_kg: None,
    });
    steps.push(("4b_width_only", &g_truck, o4w.clone()));

    let mut o4x = o3.clone();
    o4x.vehicle = Some(driver_break_core::config::VehicleLimits {
        axle_weight_kg: Some(1661.2),
        bogie_weight_kg: None,
        height_m: None,
        width_m: None,
        length_m: None,
        total_weight_kg: None,
    });
    steps.push(("4c_axle_only", &g_truck, o4x.clone()));

    let mut o4l = o3.clone();
    o4l.vehicle = Some(driver_break_core::config::VehicleLimits {
        axle_weight_kg: None,
        bogie_weight_kg: None,
        height_m: None,
        width_m: None,
        length_m: Some(5.304),
        total_weight_kg: None,
    });
    steps.push(("4d_length_only", &g_truck, o4l.clone()));

    let mut o4t = o3.clone();
    o4t.vehicle = Some(driver_break_core::config::VehicleLimits {
        axle_weight_kg: None,
        bogie_weight_kg: None,
        height_m: None,
        width_m: None,
        length_m: None,
        total_weight_kg: Some(3020.4),
    });
    steps.push(("4e_weight_only", &g_truck, o4t.clone()));

    let mut o5 = o4.clone();
    o5.datex_impacts = datex.clone();
    steps.push(("5_+datex", &g_truck, o5.clone()));

    let mut o6 = o5.clone();
    o6.avoid_tunnels = false;
    o6.avoid_motorways = false;
    o6.avoid_ferries = false;
    steps.push(("6_app_like_no_avoids", &g_truck, o6.clone()));

    let (h, w, axle, len, wt, dx) = count_option_filters(&g_truck, &o5);
    println!(
        "filter_edge_counts height={h} width={w} axle={axle} length={len} weight={wt} datex_block={dx}"
    );
    if dx > 0 {
        for e in &g_truck.edges {
            let hit = o5.datex_impacts.iter().any(|c| {
                c.impact == driver_break_core::datex::DatexImpact::Block
                    && driver_break_core::routing::graph::edge_distance_m(e, c.lat, c.lon)
                        <= c.radius_m
            });
            if !hit {
                continue;
            }
            println!(
                "  datex_block_edge {}->{} hwy={:?} ref={:?} name={:?} ({:.5},{:.5})-({:.5},{:.5})",
                e.source.0,
                e.target.0,
                e.highway,
                e.road_ref,
                e.name,
                e.start_lat,
                e.start_lon,
                e.end_lat,
                e.end_lon
            );
        }
    }

    for (name, g, opts) in &steps {
        run_ablate_one(&format!("{hop_name}/{name}"), g, start, dest, opts);
    }

    println!("{hop_name}/7_begin_plan_cancel_checks (same as 5, with plan_id)");
    let _guard = driver_break_core::download::plan_cancel::begin_plan();
    run_ablate_one(
        &format!("{hop_name}/7_with_plan_id"),
        &g_truck,
        start,
        dest,
        &o5,
    );
    drop(_guard);

    println!("{hop_name}/8_nice+5");
    driver_break_core::routing::workers::WorkerPoolPlan::lower_current_thread_priority();
    run_ablate_one(
        &format!("{hop_name}/8_after_nice"),
        &g_truck,
        start,
        dest,
        &o5,
    );
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

    if mode == "ablate" || mode == "all" {
        let data_dir = std::env::var("NAVI_PROBE_DATA_DIR")
            .ok()
            .map(PathBuf::from);
        let hop13_a = (60.64909_f64, 10.52005);
        let hop13_b = (61.25957_f64, 10.22110);
        let which = std::env::var("NAVI_PROBE_HOP").unwrap_or_else(|_| "both".into());
        if which == "both" || which == "13" {
            run_ablate_matrix(&dir, data_dir.as_deref(), "hop13", hop13_a, hop13_b);
        }
        if which == "both" || which == "17" {
            run_ablate_matrix(&dir, data_dir.as_deref(), "hop17", hop17, dest);
        }
    }
}
