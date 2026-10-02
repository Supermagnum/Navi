//! Host-side Raufoss↔Bergen / control matrix against installed v9 packs.
//!
//! Usage:
//!   cargo run -p navi-ffi --release --bin region_to_region_perf_matrix -- \
//!     --pack-dir /path/to/long-trip-packs \
//!     --elev-dir /path/to/elevation

use std::env;
use std::path::PathBuf;
use std::time::Instant;

use navi::{
    plan_car_route, set_route_plan_timing_enabled, FfiTollPolicy, FfiVehicleLimits, TravelProfile,
};

fn main() {
    let args: Vec<String> = env::args().collect();
    navi::init_native_logging();
    let pack_dir = PathBuf::from(arg(&args, "--pack-dir"));
    let elev = arg_or(
        &args,
        "--elev-dir",
        pack_dir.join("elevation").to_string_lossy().as_ref(),
    );
    let only = arg_or(&args, "--only", "");
    let cache = pack_dir.join("graph-cache-r2r-perf");
    let _ = std::fs::create_dir_all(&cache);
    let ost_pbf = pack_dir.join("ostlandet-latest.osm.pbf");
    let vest_pbf = pack_dir.join("vestlandet-latest.osm.pbf");
    assert!(
        pack_dir.join("ostlandet-latest.navi-manifest.json").is_file(),
        "missing ostlandet manifest in {}",
        pack_dir.display()
    );
    assert!(
        pack_dir.join("vestlandet-latest.navi-manifest.json").is_file(),
        "missing vestlandet manifest in {}",
        pack_dir.display()
    );

    set_route_plan_timing_enabled(true);
    println!(
        "route\teco\tpack_hit\twall_ms\tplan_ms\tpack_load_ms\teco_reweight_ms\tastar_ms\texpansions\tnodes\tedges\tdistance_km\tpeak_rss_mb\troute_ok"
    );

    let cases: &[(&str, &PathBuf, f64, f64, f64, f64, bool)] = &[
        (
            "raufoss_dombas",
            &ost_pbf,
            60.7277483,
            10.6109403,
            62.0755,
            9.1278,
            false,
        ),
        (
            "raufoss_dombas_eco",
            &ost_pbf,
            60.7277483,
            10.6109403,
            62.0755,
            9.1278,
            true,
        ),
        (
            "bergen_forde",
            &vest_pbf,
            60.388144,
            5.3347434,
            61.4522,
            5.8570,
            false,
        ),
        (
            "bergen_forde_eco",
            &vest_pbf,
            60.388144,
            5.3347434,
            61.4522,
            5.8570,
            true,
        ),
        (
            "raufoss_bergen",
            &ost_pbf,
            60.7277483,
            10.6109403,
            60.388144,
            5.3347434,
            false,
        ),
        (
            "raufoss_bergen_eco",
            &ost_pbf,
            60.7277483,
            10.6109403,
            60.388144,
            5.3347434,
            true,
        ),
        (
            "raufoss_bergen_eco_warm",
            &ost_pbf,
            60.7277483,
            10.6109403,
            60.388144,
            5.3347434,
            true,
        ),
    ];

    for &(name, pbf, slat, slon, elat, elon, eco) in cases {
        if !only.is_empty() && name != only {
            continue;
        }
        eprintln!("START case={name} eco={eco}");
        let t0 = Instant::now();
        let r = plan_car_route(
            pbf.to_string_lossy().into(),
            elev.clone(),
            cache.to_string_lossy().into(),
            slat,
            slon,
            elat,
            elon,
            eco,
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
            false,
            None,
            Vec::new(),
        );
        let wall_ms = t0.elapsed().as_secs_f64() * 1000.0;
        let report = &r.report;
        let ok = r.distance_km > 1.0 && !r.route_polyline.is_empty() && !report.contains("FAIL");
        println!(
            "{name}\t{eco}\t{}\t{wall_ms:.0}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.2}\t{}\t{ok}",
            report.contains("pack_hit=true"),
            extract(report, "plan_duration_ms"),
            extract(report, "pack_load_ms"),
            extract(report, "eco_reweight_ms"),
            extract(report, "astar_ms"),
            extract(report, "expansions"),
            extract_token(report, "nodes="),
            extract_token(report, "edges="),
            r.distance_km,
            extract(report, "peak_rss_mb"),
        );
        eprintln!("--- REPORT {name} eco={eco} wall_ms={wall_ms:.0} ---\n{report}");
    }
}

fn extract(report: &str, key: &str) -> String {
    for part in report.split(['\n', ' ', ';', '|']) {
        if let Some(rest) = part.strip_prefix(&format!("{key}=")) {
            return rest.to_string();
        }
    }
    "-".into()
}

fn extract_token(report: &str, key: &str) -> String {
    for part in report.split(['\n', ' ', ';', '|']) {
        if let Some(rest) = part.strip_prefix(key) {
            return rest.to_string();
        }
    }
    "-".into()
}

fn arg(args: &[String], key: &str) -> String {
    args.windows(2)
        .find(|w| w[0] == key)
        .map(|w| w[1].clone())
        .unwrap_or_else(|| panic!("missing {key}"))
}

fn arg_or(args: &[String], key: &str, default: &str) -> String {
    args.windows(2)
        .find(|w| w[0] == key)
        .map(|w| w[1].clone())
        .unwrap_or_else(|| default.to_string())
}
