//! Plan a Truck route from the device's current LocationManager fix.
//!
//! Start coordinates MUST be supplied via NAVI_START_LAT / NAVI_START_LON from a
//! live `adb shell dumpsys location` (or equivalent) fused/gps last fix.
//! Do not hardcode Espa/Atnbrufossen, DNT corridors, or other synthetic starts.

use std::env;
use std::path::PathBuf;

fn main() {
    let start_lat: f64 = env::var("NAVI_START_LAT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            eprintln!("FATAL: set NAVI_START_LAT from live dumpsys location (no hardcoded start)");
            std::process::exit(2);
        });
    let start_lon: f64 = env::var("NAVI_START_LON")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            eprintln!("FATAL: set NAVI_START_LON from live dumpsys location (no hardcoded start)");
            std::process::exit(2);
        });
    // Destination must be chosen only after start is known (env override).
    let end_lat: f64 = env::var("NAVI_END_LAT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            eprintln!("FATAL: set NAVI_END_LAT after confirming live start (no assumed corridor)");
            std::process::exit(2);
        });
    let end_lon: f64 = env::var("NAVI_END_LON")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            eprintln!("FATAL: set NAVI_END_LON after confirming live start (no assumed corridor)");
            std::process::exit(2);
        });

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../core/target/integration-fixtures");
    let pbf = env::var("NAVI_PBF")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("ostlandet-latest.osm.pbf"));
    let elev = env::var("NAVI_ELEV")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("elevation"));
    let cache = env::var("NAVI_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("navi-truck-live-gps-cache"));
    let _ = std::fs::create_dir_all(&cache);

    println!("gps_source=Android_LocationManager_dumpsys");
    println!("start_lat={start_lat:.6} start_lon={start_lon:.6}");
    println!("end_lat={end_lat:.6} end_lon={end_lon:.6}");
    println!("pbf={}", pbf.display());

    let profile = match env::var("NAVI_PROFILE")
        .unwrap_or_else(|_| "Truck".into())
        .as_str()
    {
        "Car" => navi::TravelProfile::Car,
        "TruckElectric" => navi::TravelProfile::TruckElectric,
        "MobileHome" => navi::TravelProfile::MobileHome,
        _ => navi::TravelProfile::Truck,
    };
    println!("profile={profile:?}");

    let r = navi::plan_car_route(
        pbf.display().to_string(),
        elev.display().to_string(),
        cache.display().to_string(),
        start_lat,
        start_lon,
        end_lat,
        end_lon,
        false,
        profile,
        false,
        false,
        false,
        navi::FfiVehicleLimits {
            axle_weight_kg: None,
            bogie_weight_kg: None,
            height_m: None,
            width_m: None,
            length_m: None,
            total_weight_kg: None,
        },
        false,
    );
    println!("distance_km={:.3}", r.distance_km);
    println!("eta_minutes={:.1}", r.eta_minutes);
    println!("driving_hours={:.2}", r.eta_minutes / 60.0);
    let n_breaks = serde_json::from_str::<Vec<serde_json::Value>>(&r.break_pois_json)
        .map(|v| v.len())
        .unwrap_or(0);
    println!("break_poi_count={n_breaks}");
    println!("--- report ---");
    print!("{}", r.report);
    if !r.report.contains("PASS") {
        std::process::exit(1);
    }
}
