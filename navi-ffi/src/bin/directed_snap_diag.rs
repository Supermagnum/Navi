//! Directed-snap diagnostic for Bergen→Stavanger (overlay disabled via stub PBF).
use std::env;
use std::path::PathBuf;
use std::time::Instant;

use driver_break_core::routing::graph::{RouteOptions, RoutingProfile, SnapRole};
use driver_break_core::routing::indexed::try_load_graph_for_plan_corridor_with_pack_dirs;
use driver_break_core::routing::plan_bbox::PlanEdgeClipMode;
use navi::{
    plan_car_route, set_route_plan_timing_enabled, FfiTollPolicy, FfiVehicleLimits, TravelProfile,
};

fn main() {
    let args: Vec<String> = env::args().collect();
    navi::init_native_logging();
    let pack_dir = PathBuf::from(arg(&args, "--pack-dir"));
    let elev = pack_dir.join("elevation");
    let cache = pack_dir.join("graph-cache-directed-snap");
    let _ = std::fs::create_dir_all(&cache);
    let _ = std::fs::create_dir_all(&elev);
    let pbf = pack_dir.join("vestlandet-latest.osm.pbf");

    let bergen: (f64, f64) = (60.388144, 5.3347434);
    let stav_centre: (f64, f64) = (58.97, 5.733);
    let stav_station: (f64, f64) = (58.9670, 5.7315);

    let pts = [(bergen.0, bergen.1), (stav_centre.0, stav_centre.1)];
    let bbox: [f64; 4] = [
        f64::min(bergen.0, stav_centre.0) - 0.05,
        f64::min(bergen.1, stav_centre.1) - 0.05,
        f64::max(bergen.0, stav_centre.0) + 0.05,
        f64::max(bergen.1, stav_centre.1) + 0.05,
    ];
    let t0 = Instant::now();
    let graph = try_load_graph_for_plan_corridor_with_pack_dirs(
        &pack_dir,
        &[pack_dir.clone()],
        &pbf,
        RoutingProfile::Car,
        Some(bbox),
        Some(&pts),
        PlanEdgeClipMode::CorridorBand,
    )
    .expect("load vestlandet corridor");
    println!(
        "pack_load_ms={:.0} nodes={} edges={} can_reach={} reachable={}",
        t0.elapsed().as_secs_f64() * 1000.0,
        graph.nodes.len(),
        graph.edges.len(),
        graph.directed_label_size(SnapRole::Origin),
        graph.directed_label_size(SnapRole::Destination),
    );

    for (name, lat, lon, role) in [
        ("bergen_origin", bergen.0, bergen.1, SnapRole::Origin),
        (
            "stav_centre_any",
            stav_centre.0,
            stav_centre.1,
            SnapRole::Any,
        ),
        (
            "stav_centre_dest",
            stav_centre.0,
            stav_centre.1,
            SnapRole::Destination,
        ),
        (
            "stav_station_dest",
            stav_station.0,
            stav_station.1,
            SnapRole::Destination,
        ),
    ] {
        let opts = RouteOptions {
            snap_role: role,
            ..Default::default()
        };
        match graph.nearest_routable_with_options_max(lat, lon, &opts, false, 750.0) {
            Ok((id, dist)) => {
                let n = &graph.nodes[&id];
                println!(
                    "snap {name} role={role:?} node={} lat={:.6} lon={:.6} dist_m={dist:.1} \
                     can_reach_main={} reachable_from_main={} out_deg={}",
                    id.0,
                    n.coord.y,
                    n.coord.x,
                    graph.directed_snap_ok(id, SnapRole::Origin),
                    graph.directed_snap_ok(id, SnapRole::Destination),
                    graph.outgoing_edge_indices(id).len(),
                );
            }
            Err(e) => println!("snap {name} FAIL nearest_m={:.1}", e.nearest_m),
        }
    }

    let oopts = RouteOptions {
        snap_role: SnapRole::Origin,
        ..Default::default()
    };
    let (oid, _) = graph
        .nearest_routable_with_options_max(bergen.0, bergen.1, &oopts, false, 750.0)
        .unwrap();
    for (label, lat, lon, role) in [
        ("centre_any", stav_centre.0, stav_centre.1, SnapRole::Any),
        (
            "centre_dest",
            stav_centre.0,
            stav_centre.1,
            SnapRole::Destination,
        ),
        (
            "station_dest",
            stav_station.0,
            stav_station.1,
            SnapRole::Destination,
        ),
    ] {
        let opts = RouteOptions {
            snap_role: role,
            ..Default::default()
        };
        let Ok((gid, _)) = graph.nearest_routable_with_options_max(lat, lon, &opts, false, 750.0)
        else {
            println!("path bergen→{label}: snap failed");
            continue;
        };
        println!(
            "path bergen→{label}: directed={} weak={} goal={}",
            graph.directed_path_exists(oid, gid),
            graph.same_weak_component(oid, gid),
            gid.0
        );
    }

    // Dead-end component size for city-centre Any snap (unreachable from main).
    {
        let opts = RouteOptions {
            snap_role: SnapRole::Any,
            ..Default::default()
        };
        if let Ok((dead, _)) = graph.nearest_routable_with_options_max(
            stav_centre.0,
            stav_centre.1,
            &opts,
            false,
            750.0,
        ) {
            let mut size = 0usize;
            for &id in graph.nodes.keys() {
                if graph.same_weak_component(dead, id)
                    && !graph.directed_snap_ok(id, SnapRole::Destination)
                    && graph.directed_snap_ok(id, SnapRole::Origin)
                {
                    size += 1;
                }
            }
            println!(
                "stav_centre_any dead_end_node={} unreachable_from_main_but_can_reach_main_count≈{size} (directed source stubs in same weak component)",
                dead.0
            );
        }
    }

    set_route_plan_timing_enabled(true);
    println!("\n=== plans (stub PBF => no ferry overlay) ===");
    for (name, elat, elon, lt) in [
        ("bergen_stav_centre", stav_centre.0, stav_centre.1, false),
        ("bergen_stav_centre_lt", stav_centre.0, stav_centre.1, true),
        ("bergen_stav_station", stav_station.0, stav_station.1, false),
    ] {
        let t1 = Instant::now();
        let r = plan_car_route(
            pbf.to_string_lossy().into(),
            elev.to_string_lossy().into(),
            cache.to_string_lossy().into(),
            bergen.0,
            bergen.1,
            elat,
            elon,
            false,
            TravelProfile::Car,
            false,
            FfiTollPolicy::Allow,
            false,
            false,
            FfiVehicleLimits {
                axle_weight_kg: None,
                bogie_weight_kg: None,
                height_m: None,
                width_m: None,
                length_m: None,
                total_weight_kg: None,
            },
            false,
            pack_dir.to_string_lossy().into(),
            pack_dir.to_string_lossy().into(),
            lt,
            None,
            Vec::new(),
        );
        let wall = t1.elapsed().as_secs_f64() * 1000.0;
        let ok = r.distance_km > 1.0 && !r.route_polyline.is_empty() && !r.report.contains("FAIL");
        let ferry = r
            .report
            .lines()
            .find(|l| l.to_ascii_lowercase().contains("ferry"))
            .unwrap_or("-");
        println!(
            "{name}\tok={ok}\twall_ms={wall:.0}\tdistance_km={:.2}\n  ferry={ferry}",
            r.distance_km
        );
        for line in r.report.lines() {
            let l = line.to_ascii_lowercase();
            if l.contains("ferry")
                || l.contains("halhjem")
                || l.contains("arsv")
                || l.contains("distance")
                || l.contains("geom")
                || l.contains("overlay")
                || l.contains("terminate")
                || line.starts_with("PLAN_PERF")
            {
                println!("  {line}");
            }
        }
        if !ok {
            for line in r.report.lines().take(15) {
                println!("  {line}");
            }
        }
    }
}

fn arg(args: &[String], key: &str) -> String {
    args.windows(2)
        .find(|w| w[0] == key)
        .map(|w| w[1].clone())
        .unwrap_or_else(|| panic!("missing {key}"))
}
