//! Device/host plan smoke for indexed packs: pack-hit vs missing-pack fallback.
//!
//! Usage:
//!   navi-indexed-plan-bench \
//!     --pbf PATH --elev-dir PATH --cache-dir PATH \
//!     --start-lat F --start-lon F --end-lat F --end-lon F \
//!     [--profile car|truck|mobilehome|motorcycle|bicycle|bicycle_electric|hiking]

use std::env;

use navi::{
    plan_car_route, plan_hiking_route, set_route_plan_timing_enabled, FfiVehicleLimits,
    TravelProfile,
};

fn main() {
    let args: Vec<String> = env::args().collect();
    let pbf = arg(&args, "--pbf");
    let elev = arg(&args, "--elev-dir");
    let cache = arg(&args, "--cache-dir");
    let slat: f64 = arg(&args, "--start-lat").parse().unwrap();
    let slon: f64 = arg(&args, "--start-lon").parse().unwrap();
    let elat: f64 = arg(&args, "--end-lat").parse().unwrap();
    let elon: f64 = arg(&args, "--end-lon").parse().unwrap();
    let profile = parse_profile(&arg_or(&args, "--profile", "car"));

    let _ = std::fs::create_dir_all(&cache);
    eprintln!("planning profile={profile:?} pbf={pbf} elev={elev} cache={cache}");
    set_route_plan_timing_enabled(true);

    let t0 = std::time::Instant::now();
    let (distance_km, eta_minutes, cache_hit, report, route_polyline) = if profile
        == TravelProfile::Hiking
    {
        let waypoints_json = format!(
            r#"[{{"name":"start","lat":{slat},"lon":{slon}}},{{"name":"end","lat":{elat},"lon":{elon}}}]"#
        );
        let result = plan_hiking_route(pbf, elev, cache, waypoints_json, false, false);
        (
            result.distance_km,
            result.eta_minutes,
            result.cache_hit,
            result.report,
            result.route_polyline,
        )
    } else {
        let result = plan_car_route(
            pbf,
            elev,
            cache,
            slat,
            slon,
            elat,
            elon,
            false,
            profile,
            false,
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
        );
        (
            result.distance_km,
            result.eta_minutes,
            result.cache_hit,
            result.report,
            result.route_polyline,
        )
    };
    let wall_ms = t0.elapsed().as_secs_f64() * 1000.0;

    let pack_hit = report.contains("pack_hit=true");
    let poi_pack_hit = report.contains("poi_pack_hit=true");
    let pack_miss = report.contains("pack_hit=false");
    let poi_pack_miss = report.contains("poi_pack_hit=false");
    let fail = report.contains("FAIL:");
    let ok_route = distance_km > 1.0 && !route_polyline.is_empty() && !fail;

    println!("WALL_MS={wall_ms:.1}");
    println!("profile={profile:?}");
    println!("distance_km={distance_km:.2}");
    println!("eta_minutes={eta_minutes:.1}");
    println!("cache_hit={cache_hit}");
    println!(
        "pack_hit={}",
        if pack_hit {
            "true"
        } else if pack_miss {
            "false"
        } else {
            "absent"
        }
    );
    println!(
        "poi_pack_hit={}",
        if poi_pack_hit {
            "true"
        } else if poi_pack_miss {
            "false"
        } else {
            "absent"
        }
    );
    println!("route_ok={ok_route}");
    for line in report.lines() {
        if line.starts_with("bbox=")
            || line.starts_with("build_s=")
            || line.starts_with("pack_hit=")
            || line.starts_with("poi_pack_hit=")
            || line.starts_with("wetland_")
            || line.contains("wetland_ms")
            || line.contains("graph_build_ms")
            || line.contains("poi_barrier_ms")
            || line.contains("plan_duration_ms")
            || line.starts_with("FAIL")
            || line.starts_with("snap_")
        {
            println!("REPORT\t{line}");
        }
    }
    for key in [
        "graph_build_ms=",
        "poi_barrier_ms=",
        "plan_duration_ms=",
        "astar_ms=",
        "wetland_ms=",
    ] {
        if let Some(pos) = report.find(key) {
            let rest = &report[pos..];
            let end = rest.find(['\n', ' ', ';']).unwrap_or(rest.len().min(40));
            println!("REPORT\t{}", &rest[..end]);
        }
    }

    if !ok_route {
        eprintln!("FULL_REPORT:\n{report}");
        std::process::exit(1);
    }
}

fn parse_profile(s: &str) -> TravelProfile {
    match s.trim().to_ascii_lowercase().as_str() {
        "car" => TravelProfile::Car,
        "car_electric" | "electric_car" => TravelProfile::CarElectric,
        "motorcycle" | "moto" => TravelProfile::Motorcycle,
        "motorcycle_electric" => TravelProfile::MotorcycleElectric,
        "truck" => TravelProfile::Truck,
        "mobilehome" | "motorhome" | "rv" => TravelProfile::MobileHome,
        "bicycle" | "bike" => TravelProfile::Bicycle,
        "bicycle_electric" | "ebike" | "electric_cycle" => TravelProfile::BicycleElectric,
        "hiking" | "foot" | "walk" => TravelProfile::Hiking,
        other => panic!("unknown --profile {other}"),
    }
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
