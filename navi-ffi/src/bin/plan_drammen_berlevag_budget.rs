//! Host Drammen→Berlevåg Stay-in-Country plan with RSS sampling.
//!
//! Knobs (env, optional):
//! - `NAVI_MEASURE_CHUNK_PAD_TAKE` (default production 3)
//! - `NAVI_MEASURE_MAX_PLAN_TILES` (default production 6)
//! - `NAVI_MEASURE_CORRIDOR_HALF_WIDTH_DEG` (default production 0.40)
//!
//! Packs: `NAVI_PACK_DIR` (Ready manifests + car rkyv). Elev: `NAVI_ELEV`.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use navi::{
    plan_car_route, set_route_plan_timing_enabled, FfiTollPolicy, FfiVehicleLimits, TravelProfile,
};

fn rss_hwm_bytes() -> (u64, u64) {
    let Ok(text) = fs::read_to_string("/proc/self/status") else {
        return (0, 0);
    };
    let mut rss = 0u64;
    let mut hwm = 0u64;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            rss = rest
                .split_whitespace()
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0)
                * 1024;
        }
        if let Some(rest) = line.strip_prefix("VmHWM:") {
            hwm = rest
                .split_whitespace()
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0)
                * 1024;
        }
    }
    (rss, hwm)
}

fn main() {
    let pack = PathBuf::from(
        env::var("NAVI_PACK_DIR").unwrap_or_else(|_| "/tmp/navi-measure-packs".into()),
    );
    let elev = PathBuf::from(env::var("NAVI_ELEV").unwrap_or_else(|_| {
        let nested = pack.join("elevation/elevation");
        if nested.is_dir() {
            nested.display().to_string()
        } else {
            pack.join("elevation").display().to_string()
        }
    }));
    let cache =
        PathBuf::from(env::var("NAVI_CACHE").unwrap_or_else(|_| "/tmp/navi-measure-cache".into()));
    let _ = fs::create_dir_all(&cache);
    let pbf = pack.join("ostlandet-latest.osm.pbf");
    if !pbf.is_file() {
        eprintln!("FATAL: missing {}", pbf.display());
        std::process::exit(2);
    }

    let label = env::var("NAVI_MEASURE_LABEL").unwrap_or_else(|_| "run".into());
    let pad_take = env::var("NAVI_MEASURE_CHUNK_PAD_TAKE").unwrap_or_else(|_| "default".into());
    let max_tiles = env::var("NAVI_MEASURE_MAX_PLAN_TILES").unwrap_or_else(|_| "default".into());
    let half_w =
        env::var("NAVI_MEASURE_CORRIDOR_HALF_WIDTH_DEG").unwrap_or_else(|_| "default".into());

    let (rss0, hwm0) = rss_hwm_bytes();
    let peak = Arc::new(AtomicU64::new(hwm0.max(rss0)));
    let stop = Arc::new(AtomicU64::new(0));
    let peak_thr = {
        let peak = Arc::clone(&peak);
        let stop = Arc::clone(&stop);
        thread::spawn(move || {
            while stop.load(Ordering::Relaxed) == 0 {
                let (rss, hwm) = rss_hwm_bytes();
                let cur = rss.max(hwm);
                peak.fetch_max(cur, Ordering::Relaxed);
                thread::sleep(Duration::from_millis(200));
            }
        })
    };

    println!("label={label}");
    println!("chunk_pad_take={pad_take}");
    println!("max_plan_tiles={max_tiles}");
    println!("corridor_half_width_deg={half_w}");
    println!("pack_dir={}", pack.display());
    println!("elev_dir={}", elev.display());
    println!("rss0_mib={:.1}", rss0 as f64 / (1024.0 * 1024.0));

    set_route_plan_timing_enabled(true);
    let t0 = Instant::now();
    let r = plan_car_route(
        pbf.display().to_string(),
        elev.display().to_string(),
        cache.display().to_string(),
        59.7401977,
        10.2015629,
        70.8578156,
        29.0860363,
        /* use_eco */ false,
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
        pack.display().to_string(),
        pack.display().to_string(),
        /* long_trip_enabled */ true,
        if env::var("NAVI_MEASURE_STAY_OFF").ok().as_deref() == Some("1") {
            None
        } else {
            Some(vec!["no".into()])
        },
        Vec::new(),
    );
    let wall_s = t0.elapsed().as_secs_f64();
    stop.store(1, Ordering::Relaxed);
    let _ = peak_thr.join();
    let (_, hwm1) = rss_hwm_bytes();
    let peak_b = peak.load(Ordering::Relaxed).max(hwm1);

    println!("wall_s={wall_s:.1}");
    println!("terminate={}", r.search_terminate_reason);
    println!("distance_km={:.2}", r.distance_km);
    println!("eta_minutes={:.1}", r.eta_minutes);
    println!("expansions={}", r.search_expansions);
    println!("pads={}", r.pad_attempts_json);
    println!("peak_rss_mib={:.1}", peak_b as f64 / (1024.0 * 1024.0));
    println!(
        "peak_rss_gib={:.3}",
        peak_b as f64 / (1024.0 * 1024.0 * 1024.0)
    );
    let fail = r.report.contains("FAIL:") || r.distance_km < 1.0;
    println!("ok={}", !fail && r.search_terminate_reason == "found");
    if let Ok(path) = env::var("NAVI_MEASURE_REPORT") {
        let _ = fs::write(&path, &r.report);
        println!("report_path={path}");
    }
    if fail || r.search_terminate_reason != "found" {
        // Prefer chunk_leg / FAIL / snap lines for diagnosis.
        for line in r.report.lines() {
            if line.contains("chunk_leg")
                || line.contains("FAIL")
                || line.contains("snap")
                || line.contains("terminate")
                || line.contains("bbox=")
                || line.contains("pack_hit")
                || line.contains("edge_clip")
            {
                println!("diag={line}");
            }
        }
    }
}
