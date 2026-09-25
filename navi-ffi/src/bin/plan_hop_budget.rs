//! Probe one densify hop under measure knobs (RSS + terminate).
//!
//! Env: NAVI_PACK_DIR, NAVI_ELEV, NAVI_CACHE, NAVI_START_LAT/LON, NAVI_END_LAT/LON,
//! NAVI_MEASURE_* knobs, NAVI_MEASURE_LABEL.

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
    let cache = PathBuf::from(
        env::var("NAVI_CACHE").unwrap_or_else(|_| "/tmp/navi-measure-cache-hop".into()),
    );
    let _ = fs::create_dir_all(&cache);
    let pbf = pack.join("ostlandet-latest.osm.pbf");
    let slat: f64 = env::var("NAVI_START_LAT").unwrap().parse().unwrap();
    let slon: f64 = env::var("NAVI_START_LON").unwrap().parse().unwrap();
    let elat: f64 = env::var("NAVI_END_LAT").unwrap().parse().unwrap();
    let elon: f64 = env::var("NAVI_END_LON").unwrap().parse().unwrap();
    let label = env::var("NAVI_MEASURE_LABEL").unwrap_or_else(|_| "hop".into());

    let (rss0, hwm0) = rss_hwm_bytes();
    let peak = Arc::new(AtomicU64::new(hwm0.max(rss0)));
    let stop = Arc::new(AtomicU64::new(0));
    let peak_thr = {
        let peak = Arc::clone(&peak);
        let stop = Arc::clone(&stop);
        thread::spawn(move || {
            while stop.load(Ordering::Relaxed) == 0 {
                let (rss, hwm) = rss_hwm_bytes();
                peak.fetch_max(rss.max(hwm), Ordering::Relaxed);
                thread::sleep(Duration::from_millis(100));
            }
        })
    };

    // Force chunk-leg pad take by spanning just under LONG_TRIP so we call
    // plan_car_route once; pass long_trip=false and rely on is_chunk path via
    // plan_car_route — actually plan_car_route never sets is_chunk. Use full
    // plan with long_trip so densify collapses to one hop when start/end are
    // already one hop apart (< chunk after densify).
    set_route_plan_timing_enabled(true);
    let t0 = Instant::now();
    // Direct single-leg plan: long_trip false keeps one A* with full pad schedule
    // unless we set take via env on chunk path. For hop probe we want CHUNK pad
    // take: call with long_trip true on a span that densifies to exactly this hop
    // by using the hop endpoints as OD (span < 1.15 → no densify).
    // NAVI_MEASURE_LONG_TRIP=0 disables chunk densify; NAVI_MEASURE_STAY_OFF=1
    // clears allowed_countries (cross-border corridors).
    let long_trip = env::var("NAVI_MEASURE_LONG_TRIP")
        .map(|v| v != "0" && !v.eq_ignore_ascii_case("false"))
        .unwrap_or(true);
    let allowed = if env::var("NAVI_MEASURE_STAY_OFF").ok().as_deref() == Some("1") {
        None
    } else {
        Some(vec!["no".into()])
    };
    let r = plan_car_route(
        pbf.display().to_string(),
        elev.display().to_string(),
        cache.display().to_string(),
        slat,
        slon,
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
        pack.display().to_string(),
        pack.display().to_string(),
        long_trip,
        allowed,
        Vec::new(),
    );
    let wall_s = t0.elapsed().as_secs_f64();
    stop.store(1, Ordering::Relaxed);
    let _ = peak_thr.join();
    let peak_b = peak.load(Ordering::Relaxed).max(rss_hwm_bytes().1);

    println!("label={label}");
    println!("od={slat:.5},{slon:.5}->{elat:.5},{elon:.5}");
    println!(
        "chunk_pad_take={}",
        env::var("NAVI_MEASURE_CHUNK_PAD_TAKE").unwrap_or_else(|_| "default".into())
    );
    println!(
        "max_plan_tiles={}",
        env::var("NAVI_MEASURE_MAX_PLAN_TILES").unwrap_or_else(|_| "default".into())
    );
    println!(
        "corridor_half_width_deg={}",
        env::var("NAVI_MEASURE_CORRIDOR_HALF_WIDTH_DEG").unwrap_or_else(|_| "default".into())
    );
    println!("wall_s={wall_s:.1}");
    println!("terminate={}", r.search_terminate_reason);
    println!("distance_km={:.2}", r.distance_km);
    println!("pads={}", r.pad_attempts_json);
    println!("peak_rss_mib={:.1}", peak_b as f64 / (1024.0 * 1024.0));
    println!(
        "ok={}",
        r.search_terminate_reason == "found" && r.distance_km > 1.0
    );
    for line in r.report.lines() {
        if line.contains("FAIL")
            || line.contains("snap")
            || line.contains("bbox=")
            || line.contains("edge_clip")
            || line.contains("pack_hit")
            || line.contains("terminate")
        {
            println!("diag={line}");
        }
    }
}
