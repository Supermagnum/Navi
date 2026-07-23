//! Host helper: plan Skolla → Harlandshytta → Eldåbu → Rondvassbu on Ostlandet foot graph.

use std::env;
use std::path::PathBuf;

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../core/target/integration-fixtures");
    let pbf = env::var("NAVI_PBF").map(PathBuf::from).unwrap_or_else(|_| root.join("ostlandet-latest.osm.pbf"));
    let elev = env::var("NAVI_ELEV").map(PathBuf::from).unwrap_or_else(|_| root.join("elevation"));
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
    );
    print!("{}", r.report);
    if !r.report.contains("PASS") {
        std::process::exit(1);
    }
    let _ = std::fs::write(root.join("skolla_rondvassbu.polyline.txt"), &r.route_polyline);
    let _ = std::fs::write(root.join("skolla_rondvassbu.breaks.json"), &r.break_pois_json);
    eprintln!("distance_km={}", r.distance_km);
    eprintln!("breaks={}", r.break_pois_json);
}
