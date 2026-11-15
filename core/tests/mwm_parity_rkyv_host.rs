//! Host `.rkyv` side of the Phase 4 parity suite.
//!
//! ```text
//! NAVI_PACK_DIR=/path/to/ostlandet-tiles \
//!   cargo test -p driver-break-core --test mwm_parity_rkyv_host -- --ignored --nocapture
//! ```
//!
//! Writes `test-artifacts/mwm-phase4/parity/parity-rkyv-summary.json` (+ GeoJSON).

use driver_break_core::routing::graph::{RouteGraph, RouteOptions, RoutingProfile};
use driver_break_core::routing::indexed::try_load_graph_for_plan_corridor_with_pack_dirs;
use driver_break_core::routing::plan_bbox::{
    set_plan_tile_budget_at_least, trip_bbox_points, PlanEdgeClipMode,
};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

fn pack_dir() -> PathBuf {
    if let Ok(p) = std::env::var("NAVI_PACK_DIR") {
        return PathBuf::from(p);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/espa-dombas-e2e")
}

fn out_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../test-artifacts/mwm-phase4/parity")
}

fn suite_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mwm/parity-suite-ostlandet.json")
}

fn dummy_pbf(dir: &Path) -> PathBuf {
    let p = dir.join("_parity_dummy.osm.pbf");
    if !p.exists() {
        let _ = fs::write(&p, b"");
    }
    p
}

fn profile_of(s: &str) -> RoutingProfile {
    match s {
        "foot" | "hike" | "pedestrian" => RoutingProfile::Foot,
        "bike" | "bicycle" => RoutingProfile::Bicycle,
        _ => RoutingProfile::Car,
    }
}

fn haversine_m(a: (f64, f64), b: (f64, f64)) -> f64 {
    let dlat = (b.0 - a.0).to_radians();
    let dlon = (b.1 - a.1).to_radians();
    let x = (dlat / 2.0).sin().powi(2)
        + a.0.to_radians().cos() * b.0.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    2.0 * 6_378_100.0 * x.sqrt().asin()
}

fn path_length_m(g: &RouteGraph, nodes: &[osm4routing::NodeId]) -> f64 {
    let mut sum = 0.0;
    for w in nodes.windows(2) {
        let n0 = g.nodes.get(&w[0]);
        let n1 = g.nodes.get(&w[1]);
        if let (Some(a), Some(b)) = (n0, n1) {
            sum += haversine_m((a.coord.y, a.coord.x), (b.coord.y, b.coord.x));
        }
    }
    sum
}

fn write_geojson(path: &Path, id: &str, g: &RouteGraph, nodes: &[osm4routing::NodeId]) {
    let mut coords = Vec::new();
    for nid in nodes {
        if let Some(n) = g.nodes.get(nid) {
            coords.push(json!([n.coord.x, n.coord.y]));
        }
    }
    let feat = json!({
        "type": "Feature",
        "geometry": { "type": "LineString", "coordinates": coords },
        "properties": { "id": id, "router": "rkyv" }
    });
    let _ = fs::write(path, feat.to_string());
}

fn nearest_linked(g: &RouteGraph, lat: f64, lon: f64, max_m: f64) -> Option<osm4routing::NodeId> {
    let mut best: Option<(osm4routing::NodeId, f64)> = None;
    for n in g.nodes.values() {
        if !g.is_linked(n.id) {
            continue;
        }
        let d = haversine_m((lat, lon), (n.coord.y, n.coord.x));
        if d > max_m {
            continue;
        }
        if best.map(|(_, bd)| d < bd).unwrap_or(true) {
            best = Some((n.id, d));
        }
    }
    best.map(|(id, _)| id)
}

#[test]
#[ignore = "needs ostlandet .rkyv tiles under NAVI_PACK_DIR"]
fn parity_suite_rkyv_host() {
    let dir = pack_dir();
    assert!(
        dir.is_dir(),
        "missing pack dir {} — set NAVI_PACK_DIR",
        dir.display()
    );
    let suite: Value = serde_json::from_str(&fs::read_to_string(suite_path()).unwrap()).unwrap();
    let routes = suite["routes"].as_array().unwrap();
    let out = out_dir();
    let gj_dir = out.join("geojson");
    fs::create_dir_all(&gj_dir).unwrap();

    let mut results = Vec::new();
    let mut ok_count = 0usize;
    for r in routes {
        let id = r["id"].as_str().unwrap();
        let profile = profile_of(r["profile"].as_str().unwrap());
        let from = (
            r["from"]["lat"].as_f64().unwrap(),
            r["from"]["lon"].as_f64().unwrap(),
        );
        let to = (
            r["to"]["lat"].as_f64().unwrap(),
            r["to"]["lon"].as_f64().unwrap(),
        );
        let t0 = Instant::now();
        set_plan_tile_budget_at_least(0);
        let pts = [from, to];
        let bbox = trip_bbox_points(&pts, 0.35);
        let loaded = try_load_graph_for_plan_corridor_with_pack_dirs(
            &dir,
            &[],
            &dummy_pbf(&dir),
            profile,
            Some(bbox),
            Some(pts.as_slice()),
            PlanEdgeClipMode::CorridorBand,
        );
        set_plan_tile_budget_at_least(0);
        let (code, length_m, time_s, point_count, err) = match loaded {
            Ok(g) => {
                let snap_m = match profile {
                    RoutingProfile::Car => 2500.0,
                    RoutingProfile::Foot | RoutingProfile::Bicycle => 800.0,
                    RoutingProfile::Truck => 2500.0,
                };
                match (
                    nearest_linked(&g, from.0, from.1, snap_m),
                    nearest_linked(&g, to.0, to.1, snap_m),
                ) {
                    (None, _) => (
                        "SNAP_FAIL",
                        0.0,
                        0.0,
                        0usize,
                        Some("start snap failed".into()),
                    ),
                    (_, None) => (
                        "SNAP_FAIL",
                        0.0,
                        0.0,
                        0usize,
                        Some("dest snap failed".into()),
                    ),
                    (Some(s), Some(d)) => {
                        match g.shortest_path_with_options(s, d, false, &RouteOptions::default()) {
                            Some((path, _edges, cost)) => {
                                let len = path_length_m(&g, &path);
                                let gj = gj_dir.join(format!("{id}.rkyv.geojson"));
                                write_geojson(&gj, id, &g, &path);
                                ok_count += 1;
                                ("OK", len, cost, path.len(), None)
                            }
                            None => ("NO_ROUTE", 0.0, 0.0, 0, Some("shortest_path none".into())),
                        }
                    }
                }
            }
            Err(e) => ("LOAD_FAIL", 0.0, 0.0, 0, Some(format!("{e}"))),
        };
        let build_s = t0.elapsed().as_secs_f64();
        results.push(json!({
            "id": id,
            "profile": r["profile"],
            "tags": r["tags"],
            "code": code,
            "length_m": length_m,
            "time_s": time_s,
            "build_s": build_s,
            "point_count": point_count,
            "error": err,
            "geojson": gj_dir.join(format!("{id}.rkyv.geojson")).display().to_string(),
        }));
        eprintln!("rkyv {id}: {code} len={length_m:.0} build={build_s:.2}s");
    }

    let summary = json!({
        "router": "rkyv",
        "host": true,
        "pack_dir": dir.display().to_string(),
        "route_count": routes.len(),
        "ok_count": ok_count,
        "results": results,
    });
    fs::write(
        out.join("parity-rkyv-summary.json"),
        serde_json::to_string_pretty(&summary).unwrap(),
    )
    .unwrap();
    assert!(
        ok_count >= 10,
        "expected at least 10 OK .rkyv routes, got {ok_count}"
    );
}
