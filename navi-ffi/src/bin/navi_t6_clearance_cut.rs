//! Diagnose T6 clearance cuts on a hop corridor (joint → dest).
//!
//! Usage:
//!   navi-t6-clearance-cut --pack-dir DIR \
//!     --start-lat 60.83648 --start-lon 9.90816 \
//!     --end-lat 60.388114 --end-lon 5.333857

use std::collections::{HashMap, HashSet, VecDeque};
use std::env;
use std::path::{Path, PathBuf};

use driver_break_core::config::VehicleLimits;
use driver_break_core::routing::graph::{GraphEdge, RouteGraph, RouteOptions, RoutingProfile};
use driver_break_core::routing::indexed::try_load_graph_for_plan_corridor_with_pack_dirs;
use driver_break_core::routing::plan_bbox::{
    set_plan_tile_budget_at_least, trip_bbox_points, PlanEdgeClipMode,
};
use osm4routing::NodeId;

fn t6() -> VehicleLimits {
    VehicleLimits {
        axle_weight_kg: Some(1661.2),
        bogie_weight_kg: None,
        height_m: Some(2.477),
        width_m: Some(2.297),
        length_m: Some(5.304),
        total_weight_kg: Some(3020.4),
    }
}

fn arg(args: &[String], key: &str) -> String {
    args.windows(2)
        .find(|w| w[0] == key)
        .map(|w| w[1].clone())
        .unwrap_or_else(|| panic!("missing {key}"))
}

fn clearance_fail(e: &GraphEdge, lim: &VehicleLimits) -> Vec<String> {
    let mut r = Vec::new();
    if let (Some(vh), Some(max)) = (lim.height_m, e.maxheight_m) {
        if vh > max {
            r.push(format!("height vehicle={vh} > maxheight_m={max}"));
        }
    }
    if let (Some(vw), Some(max)) = (lim.width_m, e.maxwidth_m) {
        if vw > max {
            r.push(format!("width vehicle={vw} > maxwidth_m={max}"));
        }
    }
    if let (Some(l), Some(max)) = (lim.length_m, e.maxlength_m) {
        if l > max {
            r.push(format!("length vehicle={l} > maxlength_m={max}"));
        }
    }
    if let (Some(tw), Some(max)) = (lim.total_weight_kg, e.maxweight_t) {
        let t = tw / 1000.0;
        if t > max {
            r.push(format!(
                "weight vehicle_t={t} > maxweight_t={max} (vehicle_kg={tw})"
            ));
        }
    }
    if let (Some(ax), Some(max)) = (lim.axle_weight_kg, e.maxaxleload_t) {
        let t = ax / 1000.0;
        if t > max {
            r.push(format!(
                "axle vehicle_t={t} > maxaxleload_t={max} (vehicle_kg={ax})"
            ));
        }
    }
    if let (Some(bg), Some(max)) = (lim.bogie_weight_kg, e.maxbogieweight_t) {
        let t = bg / 1000.0;
        if t > max {
            r.push(format!(
                "bogie vehicle_t={t} > maxbogieweight_t={max} (vehicle_kg={bg})"
            ));
        }
    }
    r
}

fn load_graph(
    dir: &Path,
    start: (f64, f64),
    dest: (f64, f64),
    profile: RoutingProfile,
) -> Option<std::sync::Arc<RouteGraph>> {
    set_plan_tile_budget_at_least(0);
    let pts = [start, dest];
    let bbox = trip_bbox_points(&pts, 0.35);
    let pbf = dir.join("ostlandet-latest.osm.pbf");
    let g = try_load_graph_for_plan_corridor_with_pack_dirs(
        dir,
        &[],
        &pbf,
        profile,
        Some(bbox),
        Some(pts.as_slice()),
        PlanEdgeClipMode::CorridorBand,
    )
    .ok()?;
    set_plan_tile_budget_at_least(0);
    Some(g)
}

/// Approximate T6-only directed BFS (clearance + access_forbidden).
fn reachable_t6(g: &RouteGraph, start: NodeId, lim: &VehicleLimits) -> HashSet<NodeId> {
    let mut seen = HashSet::new();
    let mut q = VecDeque::new();
    seen.insert(start);
    q.push_back(start);
    while let Some(u) = q.pop_front() {
        for &idx in g.outgoing_edge_indices(u) {
            let e = &g.edges[idx];
            if e.access_forbidden {
                continue;
            }
            if !clearance_fail(e, lim).is_empty() {
                continue;
            }
            let v = e.target;
            if seen.insert(v) {
                q.push_back(v);
            }
        }
    }
    seen
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let dir = PathBuf::from(arg(&args, "--pack-dir"));
    let slat: f64 = arg(&args, "--start-lat").parse().unwrap();
    let slon: f64 = arg(&args, "--start-lon").parse().unwrap();
    let elat: f64 = arg(&args, "--end-lat").parse().unwrap();
    let elon: f64 = arg(&args, "--end-lon").parse().unwrap();
    let start = (slat, slon);
    let dest = (elat, elon);
    let lim = t6();

    for profile in [RoutingProfile::Car, RoutingProfile::Truck] {
        println!("===== profile={profile:?} =====");
        let Some(g) = load_graph(&dir, start, dest, profile) else {
            println!("LOAD FAIL");
            continue;
        };
        println!("nodes={} edges={}", g.nodes.len(), g.edges.len());

        let opts0 = RouteOptions::default();
        let mut opts_t6 = RouteOptions::default();
        opts_t6.vehicle = Some(lim.clone());

        let (Ok((s, sm)), Ok((d, dm))) = (
            g.nearest_routable_with_options(start.0, start.1, &opts0, false),
            g.nearest_routable_with_options(dest.0, dest.1, &opts0, false),
        ) else {
            println!("SNAP FAIL without limits");
            continue;
        };
        println!(
            "snap_no_lim start_m={sm:.1} dest_m={dm:.1} s={} d={}",
            s.0, d.0
        );
        match (
            g.nearest_routable_with_options(start.0, start.1, &opts_t6, false),
            g.nearest_routable_with_options(dest.0, dest.1, &opts_t6, false),
        ) {
            (Ok((s2, sm2)), Ok((d2, dm2))) => {
                let weak2 = g.same_weak_component(s2, d2);
                let dir2 = weak2 && g.directed_reachable_with_options(s2, d2, &opts_t6);
                println!(
                    "snap_t6 start_m={sm2:.1} dest_m={dm2:.1} s={} d={} weak={weak2} directed={dir2}",
                    s2.0, d2.0
                );
                if let Some(n) = g.nodes.get(&d) {
                    println!(
                        "snap_nolim_dest_coord lat={:.6} lon={:.6}",
                        n.coord.y, n.coord.x
                    );
                }
                if let Some(n) = g.nodes.get(&d2) {
                    println!(
                        "snap_t6_dest_coord lat={:.6} lon={:.6}",
                        n.coord.y, n.coord.x
                    );
                }
                // How does planning fail with T6 snaps?
                let dir_mixed = g.same_weak_component(s2, d)
                    && g.directed_reachable_with_options(s2, d, &opts_t6);
                println!("t6_start_to_nolim_dest directed={dir_mixed} (simulates bad dest snap)");
            }
            (a, b) => println!("snap_t6 FAIL start={a:?} dest={b:?}"),
        }

        let weak0 = g.same_weak_component(s, d);
        let dir0 = weak0 && g.directed_reachable_with_options(s, d, &opts0);
        let dir_t6 = weak0 && g.directed_reachable_with_options(s, d, &opts_t6);
        println!("no_lim weak={weak0} directed={dir0}; t6 directed={dir_t6}");

        for (name, partial) in [
            (
                "height",
                VehicleLimits {
                    height_m: lim.height_m,
                    ..Default::default()
                },
            ),
            (
                "width",
                VehicleLimits {
                    width_m: lim.width_m,
                    ..Default::default()
                },
            ),
            (
                "length",
                VehicleLimits {
                    length_m: lim.length_m,
                    ..Default::default()
                },
            ),
            (
                "weight",
                VehicleLimits {
                    total_weight_kg: lim.total_weight_kg,
                    ..Default::default()
                },
            ),
            (
                "axle",
                VehicleLimits {
                    axle_weight_kg: lim.axle_weight_kg,
                    ..Default::default()
                },
            ),
        ] {
            let mut o = RouteOptions::default();
            o.vehicle = Some(partial);
            let ok = g.same_weak_component(s, d) && g.directed_reachable_with_options(s, d, &o);
            println!("  ablate_{name}_only directed_ok={ok}");
        }

        let mut by_reason: HashMap<String, u64> = HashMap::new();
        let mut excluded_n = 0u64;
        for e in &g.edges {
            let fails = clearance_fail(e, &lim);
            if fails.is_empty() {
                continue;
            }
            excluded_n += 1;
            for f in &fails {
                let key = f.split(' ').next().unwrap_or("?").to_string();
                *by_reason.entry(key).or_default() += 1;
            }
        }
        println!("excluded_edges={excluded_n} reasons={by_reason:?}");

        if let Some((path, edges, _cost)) = g.shortest_path_with_options(s, d, false, &opts0) {
            println!("undirected_path_nodes={} edges={}", path.len(), edges.len());
            let mut path_blocked = 0u64;
            for &ei in &edges {
                let e = &g.edges[ei];
                let fails = clearance_fail(e, &lim);
                if fails.is_empty() {
                    continue;
                }
                path_blocked += 1;
                if path_blocked <= 40 {
                    println!(
                        "PATH_BLOCK way={} ref={:?} name={:?} hwy={:?} \
                         lat={:.5} lon={:.5} -> lat={:.5} lon={:.5} \
                         maxw={:?} maxh={:?} maxweight_t={:?} maxaxle_t={:?} maxlength={:?} \
                         oneway={} tunnel={} | {}",
                        e.id,
                        e.road_ref,
                        e.name,
                        e.highway,
                        e.start_lat,
                        e.start_lon,
                        e.end_lat,
                        e.end_lon,
                        e.maxwidth_m,
                        e.maxheight_m,
                        e.maxweight_t,
                        e.maxaxleload_t,
                        e.maxlength_m,
                        e.is_oneway,
                        e.is_tunnel,
                        fails.join("; ")
                    );
                }
            }
            println!("path_blocked_edges={path_blocked}");
        }

        let reach = reachable_t6(&g, s, &lim);
        println!(
            "t6_reachable_nodes={} t6_reaches_dest={}",
            reach.len(),
            reach.contains(&d)
        );

        let mut cut: Vec<&GraphEdge> = Vec::new();
        for e in &g.edges {
            if !reach.contains(&e.source) || reach.contains(&e.target) {
                continue;
            }
            if e.access_forbidden {
                continue;
            }
            if clearance_fail(e, &lim).is_empty() {
                continue;
            }
            cut.push(e);
        }
        let mut seen_ways = HashSet::new();
        let mut printed = 0u64;
        println!("clearance_cut_edges={}", cut.len());
        for e in cut {
            let key = format!(
                "{}|{:?}|{:?}|{:?}",
                e.id, e.maxwidth_m, e.maxheight_m, e.maxweight_t
            );
            if !seen_ways.insert(key) {
                continue;
            }
            let fails = clearance_fail(e, &lim);
            printed += 1;
            if printed <= 80 {
                println!(
                    "CUT way={} ref={:?} name={:?} hwy={:?} \
                     lat={:.5} lon={:.5} maxw={:?} maxh={:?} maxweight_t={:?} \
                     maxaxle_t={:?} maxlength={:?} oneway={} | {}",
                    e.id,
                    e.road_ref,
                    e.name,
                    e.highway,
                    (e.start_lat + e.end_lat) / 2.0,
                    (e.start_lon + e.end_lon) / 2.0,
                    e.maxwidth_m,
                    e.maxheight_m,
                    e.maxweight_t,
                    e.maxaxleload_t,
                    e.maxlength_m,
                    e.is_oneway,
                    fails.join("; ")
                );
            }
        }
        println!("cut_unique_ways={printed}");
    }
}
