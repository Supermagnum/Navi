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
        true,  // prefer official hiking networks for this corridor helper
        false, // pilgrim soft-pref off for this helper
    );
    print!("{}", r.report);
    if !r.report.contains("PASS") {
        std::process::exit(1);
    }
    // Rast-hut auto-vias: expect at least one named hut promoted on this corridor.
    if !r.report.contains("auto_vias=") {
        eprintln!("FAIL: missing auto_vias= in report");
        std::process::exit(1);
    }
    if r.report.contains("auto_vias=0") {
        eprintln!("WARN: auto_vias=0 (unexpected on Skolla→Rondvassbu with hut-rich extract)");
    } else {
        eprintln!("auto_vias_ok=true");
        // Path should pass near each accepted auto-via hut listed in breaks.
        if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&r.break_pois_json) {
            let names_line = r
                .report
                .lines()
                .find(|l| l.starts_with("auto_vias="))
                .unwrap_or("");
            let Some(names_part) = names_line.split("names=").nth(1) else {
                eprintln!("FAIL: auto_vias line missing names=");
                std::process::exit(1);
            };
            for name in names_part.split('|') {
                let name = name.trim();
                if name.is_empty() {
                    continue;
                }
                let hut = arr.iter().find(|s| {
                    s["name"]
                        .as_str()
                        .map(|n| n.eq_ignore_ascii_case(name))
                        .unwrap_or(false)
                });
                let Some(h) = hut else {
                    eprintln!("WARN: auto-via {name} not in break_pois_json");
                    continue;
                };
                let hut_lat = h["lat"].as_f64().unwrap_or(0.0);
                let hut_lon = h["lon"].as_f64().unwrap_or(0.0);
                let mut best = f64::INFINITY;
                for part in r.route_polyline.split(';') {
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
                    let d = haversine_m(hut_lat, hut_lon, lat, lon);
                    if d < best {
                        best = d;
                    }
                }
                eprintln!("auto_via_path_nearest_m name={name} m={best:.0}");
                if best > 1_200.0 {
                    eprintln!("FAIL: auto-via {name} accepted but path stays {best:.0} m away");
                    std::process::exit(1);
                }
            }
        }
    }
    // Veslefjellbua appears on some device extracts / basemap labels; optional soft check.
    if r.break_pois_json.to_lowercase().contains("veslefjell")
        || r.report.to_lowercase().contains("veslefjell")
    {
        eprintln!("veslefjellbua_ok=true");
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

fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6_378_100.0;
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
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
