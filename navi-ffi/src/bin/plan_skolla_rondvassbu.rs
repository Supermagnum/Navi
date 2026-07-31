//! Host helper: plan Skolla → Harlandshytta → Eldåbu → Rondvassbu on Ostlandet foot graph.

use std::env;
use std::path::PathBuf;

fn main() {
    let root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../core/target/integration-fixtures");
    let pbf = env::var("NAVI_PBF")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("ostlandet-latest.osm.pbf"));
    let elev = env::var("NAVI_ELEV")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("elevation"));
    let cache = env::var("NAVI_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("graph-cache-foot-ostlandet"));
    let wps = r#"[
      {"name":"Skolla","lat":61.2430347,"lon":10.8170385},
      {"name":"Harlandshytta","lat":61.4342578,"lon":10.6561922},
      {"name":"Eldåbu","lat":61.7562897,"lon":9.9793564},
      {"name":"Rondvassbu","lat":61.8804325,"lon":9.7959854}
    ]"#;
    eprintln!("planning on {} …", pbf.display());
    let r = navi::plan_hiking_route(
        pbf.display().to_string(),
        elev.display().to_string(),
        cache.display().to_string(),
        wps.to_string(),
        true, // prefer official hiking networks for this corridor helper
    );
    print!("{}", r.report);
    if !r.report.contains("PASS") {
        std::process::exit(1);
    }
    let _ = std::fs::write(
        root.join("skolla_rondvassbu.polyline.txt"),
        &r.route_polyline,
    );
    let _ = std::fs::write(
        root.join("skolla_rondvassbu.breaks.json"),
        &r.break_pois_json,
    );
    let samples = if r.sim_samples_json.len() > 2 {
        r.sim_samples_json.clone()
    } else {
        // Staged overlay polyline densified at hiking pace when planner returned none.
        densify_overlay_sim_samples(&r.route_polyline)
    };
    let _ = std::fs::write(root.join("skolla_rondvassbu.sim_samples.json"), &samples);
    eprintln!("distance_km={}", r.distance_km);
    eprintln!("breaks={}", r.break_pois_json);
    eprintln!("sim_samples_bytes={}", samples.len());
}

fn densify_overlay_sim_samples(polyline: &str) -> String {
    use driver_break_core::routing::{
        build_sim_samples_from_lat_lon, samples_to_json, HIKING_MIN_PER_KM,
    };
    let mut coords: Vec<(f64, f64)> = Vec::new();
    for part in polyline.split(';') {
        let mut it = part.split(',');
        let (Some(lon_s), Some(lat_s)) = (it.next(), it.next()) else {
            continue;
        };
        let Ok(lon) = lon_s.trim().parse::<f64>() else {
            continue;
        };
        let Ok(lat) = lat_s.trim().parse::<f64>() else {
            continue;
        };
        coords.push((lat, lon));
    }
    let speed = 60.0 / HIKING_MIN_PER_KM;
    samples_to_json(&build_sim_samples_from_lat_lon(
        &coords,
        speed,
        Some("path"),
    ))
}
