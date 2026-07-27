//! Geometry / road-audit / Atnosen comparison helpers for eco validation.
//!
//! Run: `cargo test -p driver-break-core --test route_geometry_audit -- --nocapture --ignored`

mod helpers;

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use driver_break_core::config::EcoConfig;
use driver_break_core::poi::PoiCategory;
use driver_break_core::routing::elevation::{ElevationCache, ElevationService};
use driver_break_core::routing::graph::{RouteGraph, RoutingProfile};
use helpers::{
    haversine_m, nearest_node, path_edge_indices, route_metrics, sample_route_points,
    CombinedPoiIndex, TestReport,
};

const START_LAT: f64 = 60.562_191_4;
const START_LON: f64 = 11.256_123_9;
const END_LAT: f64 = 61.851_250_0;
const END_LON: f64 = 10.233_842_0;
const ATNOSEN_LAT: f64 = 61.729_384_8;
const ATNOSEN_LON: f64 = 10.817_000_0;

const QUESTIONABLE: &[&str] = &[
    "track",
    "path",
    "footway",
    "bridleway",
    "steps",
    "cycleway",
    "pedestrian",
    "service",
];

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("integration-fixtures")
}

fn passat_eco() -> EcoConfig {
    EcoConfig {
        drag_coefficient: 0.28,
        frontal_area_m2: 2.2,
        mass_kg: 1500.0,
        ..EcoConfig::default()
    }
}

fn nearest_routable(
    graph: &RouteGraph,
    lat: f64,
    lon: f64,
    require_outgoing: bool,
) -> (osm4routing::NodeId, f64, f64, f64) {
    use rayon::prelude::*;
    use std::collections::HashSet;
    let allowed: HashSet<_> = if require_outgoing {
        graph.edges.iter().map(|e| e.source).collect()
    } else {
        let mut s = HashSet::new();
        for e in &graph.edges {
            s.insert(e.source);
            s.insert(e.target);
        }
        s
    };
    allowed
        .par_iter()
        .filter_map(|id| graph.nodes.get(id))
        .map(|node| {
            let d = haversine_m(lat, lon, node.coord.y, node.coord.x);
            (node.id, node.coord.y, node.coord.x, d)
        })
        .min_by(|a, b| a.3.partial_cmp(&b.3).unwrap_or(std::cmp::Ordering::Equal))
        .expect("no routable nodes")
}

fn path_coords(graph: &RouteGraph, path: &[osm4routing::NodeId]) -> Vec<(f64, f64)> {
    path.iter()
        .filter_map(|id| graph.nodes.get(id).map(|n| (n.coord.y, n.coord.x)))
        .collect()
}

fn write_geojson_linestring(path: &std::path::Path, coords: &[(f64, f64)], name: &str) {
    let mut coords_json = String::from("[");
    for (i, (lat, lon)) in coords.iter().enumerate() {
        if i > 0 {
            coords_json.push(',');
        }
        // GeoJSON is lon,lat
        coords_json.push_str(&format!("[{lon:.6},{lat:.6}]"));
    }
    coords_json.push(']');
    let body = format!(
        r#"{{"type":"FeatureCollection","features":[{{"type":"Feature","properties":{{"name":"{name}"}},"geometry":{{"type":"LineString","coordinates":{coords_json}}}}}]}}"#
    );
    let _ = fs::write(path, body);
}

fn fetch_osrm_via_atnosen() -> anyhow::Result<(f64, Vec<(f64, f64)>)> {
    let url = format!(
        "https://router.project-osrm.org/route/v1/driving/{START_LON},{START_LAT};{ATNOSEN_LON},{ATNOSEN_LAT};{END_LON},{END_LAT}?overview=full&geometries=geojson"
    );
    let rt = tokio::runtime::Runtime::new()?;
    let body: serde_json::Value = rt.block_on(async {
        reqwest::Client::new()
            .get(&url)
            .header("User-Agent", "NaviGeometryAudit/1.0")
            .send()
            .await?
            .error_for_status()?
            .json()
            .await
    })?;
    let route = &body["routes"][0];
    let dist = route["distance"].as_f64().unwrap_or(0.0);
    let coords = route["geometry"]["coordinates"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|c| {
            let arr = c.as_array()?;
            Some((arr[1].as_f64()?, arr[0].as_f64()?))
        })
        .collect();
    Ok((dist, coords))
}

fn fetch_osrm_direct() -> anyhow::Result<(f64, Vec<(f64, f64)>)> {
    let url = format!(
        "https://router.project-osrm.org/route/v1/driving/{START_LON},{START_LAT};{END_LON},{END_LAT}?overview=full&geometries=geojson"
    );
    let rt = tokio::runtime::Runtime::new()?;
    let body: serde_json::Value = rt.block_on(async {
        reqwest::Client::new()
            .get(&url)
            .header("User-Agent", "NaviGeometryAudit/1.0")
            .send()
            .await?
            .error_for_status()?
            .json()
            .await
    })?;
    let route = &body["routes"][0];
    let dist = route["distance"].as_f64().unwrap_or(0.0);
    let coords = route["geometry"]["coordinates"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|c| {
            let arr = c.as_array()?;
            Some((arr[1].as_f64()?, arr[0].as_f64()?))
        })
        .collect();
    Ok((dist, coords))
}

/// Sample every Nth point and report max distance from the other polyline (Hausdorff-ish one-way).
fn max_point_to_polyline_m(pts: &[(f64, f64)], line: &[(f64, f64)]) -> f64 {
    if line.len() < 2 || pts.is_empty() {
        return f64::INFINITY;
    }
    let step = (pts.len() / 200).max(1);
    let mut max_d = 0.0_f64;
    for p in pts.iter().step_by(step) {
        let mut best = f64::INFINITY;
        for w in line.windows(2) {
            // approximate: min distance to segment endpoints / mid (coarse)
            let d0 = haversine_m(p.0, p.1, w[0].0, w[0].1);
            let d1 = haversine_m(p.0, p.1, w[1].0, w[1].1);
            let mid = ((w[0].0 + w[1].0) / 2.0, (w[0].1 + w[1].1) / 2.0);
            let dm = haversine_m(p.0, p.1, mid.0, mid.1);
            best = best.min(d0).min(d1).min(dm);
        }
        max_d = max_d.max(best);
    }
    max_d
}

#[test]
#[ignore = "network + large graph audit"]
fn route_geometry_and_road_audit() {
    if let Err(e) = run() {
        panic!("{e:#}");
    }
}

fn run() -> anyhow::Result<()> {
    let fixtures = fixture_dir();
    let ostlandet = fixtures.join("ostlandet-latest.osm.pbf");
    anyhow::ensure!(ostlandet.exists(), "missing {}", ostlandet.display());
    let mut report = TestReport::with_title("Route geometry audit — Atnosen + road types");

    let graph = RouteGraph::build_from_pbf(&ostlandet, RoutingProfile::Car)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let start = nearest_routable(&graph, START_LAT, START_LON, true);
    let goal = nearest_routable(&graph, END_LAT, END_LON, false);
    let atn = nearest_routable(&graph, ATNOSEN_LAT, ATNOSEN_LON, false);

    // Direct
    let direct = graph
        .shortest_path(start.0, goal.0, false)
        .ok_or_else(|| anyhow::anyhow!("no direct route"))?;
    let direct_edges = path_edge_indices(&graph, &direct.0);
    let direct_coords = path_coords(&graph, &direct.0);
    let direct_m: f64 = direct_edges.iter().map(|&i| graph.edges[i].length_m).sum();

    // Via Atnosen
    let a = graph
        .shortest_path(start.0, atn.0, false)
        .ok_or_else(|| anyhow::anyhow!("no start->Atnosen"))?;
    let b = graph
        .shortest_path(atn.0, goal.0, false)
        .ok_or_else(|| anyhow::anyhow!("no Atnosen->goal"))?;
    let mut via_nodes = a.0.clone();
    if via_nodes.last() == b.0.first() {
        via_nodes.extend(b.0.iter().skip(1).copied());
    } else {
        via_nodes.extend(b.0.iter().copied());
    }
    let via_edges = path_edge_indices(&graph, &via_nodes);
    let via_coords = path_coords(&graph, &via_nodes);
    let via_m: f64 = via_edges.iter().map(|&i| graph.edges[i].length_m).sum();

    write_geojson_linestring(
        &fixtures.join("navi_direct.geojson"),
        &direct_coords,
        "navi-direct",
    );
    write_geojson_linestring(
        &fixtures.join("navi_via_atnosen.geojson"),
        &via_coords,
        "navi-via-atnosen",
    );

    report.section("Navi distances");
    report.line(&format!(
        "Direct: {:.2} km ({} nodes, {} edges)",
        direct_m / 1000.0,
        direct.0.len(),
        direct_edges.len()
    ));
    report.line(&format!(
        "Via Atnosen: {:.2} km ({} nodes); detour {:+.2} km",
        via_m / 1000.0,
        via_nodes.len(),
        (via_m - direct_m) / 1000.0
    ));
    report.line(&format!(
        "Leg split: start->Atnosen {:.2} km + Atnosen->goal {:.2} km",
        a.1 / 1000.0,
        b.1 / 1000.0
    ));

    // OSRM
    report.section("OSRM comparison");
    let (osrm_direct_m, osrm_direct_coords) = fetch_osrm_direct()?;
    let (osrm_via_m, osrm_via_coords) = fetch_osrm_via_atnosen()?;
    write_geojson_linestring(
        &fixtures.join("osrm_direct.geojson"),
        &osrm_direct_coords,
        "osrm-direct",
    );
    write_geojson_linestring(
        &fixtures.join("osrm_via_atnosen.geojson"),
        &osrm_via_coords,
        "osrm-via-atnosen",
    );
    report.line(&format!(
        "OSRM direct: {:.2} km; Navi-OSRM delta {:+.2} km",
        osrm_direct_m / 1000.0,
        (direct_m - osrm_direct_m) / 1000.0
    ));
    report.line(&format!(
        "OSRM via Atnosen: {:.2} km; Navi-OSRM delta {:+.2} km",
        osrm_via_m / 1000.0,
        (via_m - osrm_via_m) / 1000.0
    ));

    let navi_to_osrm = max_point_to_polyline_m(&via_coords, &osrm_via_coords);
    let osrm_to_navi = max_point_to_polyline_m(&osrm_via_coords, &via_coords);
    report.line(&format!(
        "Via-path coarse max deviation: Navi→OSRM {:.1} km, OSRM→Navi {:.1} km",
        navi_to_osrm / 1000.0,
        osrm_to_navi / 1000.0
    ));

    // Find farthest Navi via points from OSRM via (likely divergence loci)
    report.section("Via-Atnosen divergence loci (Navi points far from OSRM)");
    let mut far: Vec<(f64, f64, f64)> = Vec::new();
    let step = (via_coords.len() / 100).max(1);
    for p in via_coords.iter().step_by(step) {
        let mut best = f64::INFINITY;
        for q in osrm_via_coords
            .iter()
            .step_by(5.max(osrm_via_coords.len() / 500))
        {
            best = best.min(haversine_m(p.0, p.1, q.0, q.1));
        }
        if best > 2_000.0 {
            far.push((p.0, p.1, best));
        }
    }
    far.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap());
    for (lat, lon, d) in far.iter().take(12) {
        report.line(&format!(
            "  Navi point ({lat:.5},{lon:.5}) ~{:.1} km from nearest OSRM via sample",
            d / 1000.0
        ));
    }
    if far.is_empty() {
        report.line("No Navi via samples >2 km from OSRM via — paths are topologically close.");
    } else {
        report.section("Geometry notes");
        report.line(
            "Residual multi-km deviations may remain (destination approach / local alternatives).",
        );
        report.line(
            "Kolomoen E6->Rv3: car graph now emits reverse edges for two-way OSM ways (car_backward).",
        );
        report.line(
            "Previous Moelv west-Mjosa loop was caused by missing northbound Rv3 (ways drawn southbound).",
        );
    }

    // Road-type audit on direct
    report.section("Direct-route road-type audit");
    let mut by_hw: HashMap<String, (f64, usize)> = HashMap::new();
    let mut questionable: Vec<String> = Vec::new();
    let mut private_ish: Vec<String> = Vec::new();
    for &i in &direct_edges {
        let e = &graph.edges[i];
        let hw = e.highway.clone().unwrap_or_else(|| "(none)".into());
        let ent = by_hw.entry(hw.clone()).or_insert((0.0, 0));
        ent.0 += e.length_m;
        ent.1 += 1;
        if QUESTIONABLE.iter().any(|q| hw == *q) {
            questionable.push(format!(
                "{} {} {:.0}m ({:.5},{:.5})->({:.5},{:.5})",
                e.id, hw, e.length_m, e.start_lat, e.start_lon, e.end_lat, e.end_lon
            ));
        }
        // GraphEdge does not store access=; flag service/track as review.
        if hw == "service" || hw == "track" {
            private_ish.push(format!("{} {} {:.0}m id={}", hw, e.id, e.length_m, e.id));
        }
    }
    let mut hw_rows: Vec<_> = by_hw.into_iter().collect();
    hw_rows.sort_by(|a, b| b.1 .0.partial_cmp(&a.1 .0).unwrap());
    for (hw, (m, n)) in &hw_rows {
        report.line(&format!(
            "  {hw}: {:.1} km across {n} edges ({:.1}%)",
            m / 1000.0,
            100.0 * m / direct_m
        ));
    }
    report.line(&format!(
        "Questionable highway classes (track/path/service/...): {} edges",
        questionable.len()
    ));
    for q in questionable.iter().take(25) {
        report.line(&format!("  FLAG {q}"));
    }
    if questionable.len() > 25 {
        report.line(&format!("  ... and {} more", questionable.len() - 25));
    }
    let q_m: f64 = direct_edges
        .iter()
        .filter(|&&i| {
            graph.edges[i]
                .highway
                .as_deref()
                .is_some_and(|h| QUESTIONABLE.contains(&h))
        })
        .map(|&i| graph.edges[i].length_m)
        .sum();
    report.line(&format!(
        "Questionable class distance: {:.2} km ({:.2}% of route)",
        q_m / 1000.0,
        100.0 * q_m / direct_m
    ));
    if q_m < 500.0 {
        report.line("AUDIT: essentially clean for car (questionable < 0.5 km).");
    } else {
        report.line("AUDIT: non-trivial questionable mileage — review FLAG lines.");
    }

    // Via-Atnosen road-type audit (post reverse-edge: rule out track/service shortcut)
    report.section("Via-Atnosen road-type audit");
    let mut via_by_hw: HashMap<String, (f64, usize)> = HashMap::new();
    let mut via_questionable: Vec<String> = Vec::new();
    for &i in &via_edges {
        let e = &graph.edges[i];
        let hw = e.highway.clone().unwrap_or_else(|| "(none)".into());
        let ent = via_by_hw.entry(hw.clone()).or_insert((0.0, 0));
        ent.0 += e.length_m;
        ent.1 += 1;
        if QUESTIONABLE.iter().any(|q| hw == *q) {
            via_questionable.push(format!(
                "{} {} {:.0}m ({:.5},{:.5})->({:.5},{:.5})",
                e.id, hw, e.length_m, e.start_lat, e.start_lon, e.end_lat, e.end_lon
            ));
        }
    }
    let mut via_hw_rows: Vec<_> = via_by_hw.into_iter().collect();
    via_hw_rows.sort_by(|a, b| b.1 .0.partial_cmp(&a.1 .0).unwrap());
    for (hw, (m, n)) in &via_hw_rows {
        report.line(&format!(
            "  {hw}: {:.1} km across {n} edges ({:.1}%)",
            m / 1000.0,
            100.0 * m / via_m
        ));
    }
    let via_q_m: f64 = via_edges
        .iter()
        .filter(|&&i| {
            graph.edges[i]
                .highway
                .as_deref()
                .is_some_and(|h| QUESTIONABLE.contains(&h))
        })
        .map(|&i| graph.edges[i].length_m)
        .sum();
    report.line(&format!(
        "Questionable class distance: {:.2} km ({:.2}% of via)",
        via_q_m / 1000.0,
        100.0 * via_q_m / via_m
    ));
    for q in via_questionable.iter().take(20) {
        report.line(&format!("  FLAG {q}"));
    }
    if via_questionable.len() > 20 {
        report.line(&format!("  ... and {} more", via_questionable.len() - 20));
    }
    // Flag Navi via edges whose midpoint is >2 km from OSRM (possible shortcut corridor)
    report.line("Divergence-corridor questionable edges (midpoint >2 km from OSRM):");
    let mut div_q = 0usize;
    let mut div_q_m = 0.0_f64;
    for &i in &via_edges {
        let e = &graph.edges[i];
        let mid = (
            (e.start_lat + e.end_lat) / 2.0,
            (e.start_lon + e.end_lon) / 2.0,
        );
        let mut best = f64::INFINITY;
        for q in osrm_via_coords
            .iter()
            .step_by(5.max(osrm_via_coords.len() / 500))
        {
            best = best.min(haversine_m(mid.0, mid.1, q.0, q.1));
        }
        if best <= 2_000.0 {
            continue;
        }
        let hw = e.highway.as_deref().unwrap_or("(none)");
        if !QUESTIONABLE.contains(&hw) {
            continue;
        }
        div_q += 1;
        div_q_m += e.length_m;
        if div_q <= 15 {
            report.line(&format!(
                "  DIV-FLAG {} {} {:.0}m ~{:.1}km from OSRM ({:.5},{:.5})",
                e.id,
                hw,
                e.length_m,
                best / 1000.0,
                mid.0,
                mid.1
            ));
        }
    }
    if div_q == 0 {
        report.line("  none — no track/service/path in the >2 km divergence band.");
    } else {
        report.line(&format!(
            "  {div_q} edges / {:.2} km questionable in divergence band — flag for filter hardening.",
            div_q_m / 1000.0
        ));
    }
    if via_q_m < 500.0 && div_q == 0 {
        report.line("VIA AUDIT: clean — shorter-than-OSRM length is not a track/service shortcut.");
    } else if via_q_m < 500.0 {
        report.line("VIA AUDIT: low questionable mileage overall; review DIV-FLAG lines.");
    } else {
        report.line("VIA AUDIT: non-trivial questionable mileage — review FLAG lines.");
    }

    // Eco re-score via vs direct under Passat
    report.section("Eco energy on Navi geometries (Passat, regen=0)");
    let elev = ElevationService::new(ElevationCache::new(fixtures.join("elevation")));
    let eco = passat_eco();
    let m_dir = route_metrics(&graph, &direct_edges, &elev, &eco, true);
    let m_via = route_metrics(&graph, &via_edges, &elev, &eco, true);
    report.log_route_metrics("Direct", &m_dir, direct.1);
    report.log_route_metrics("Via Atnosen", &m_via, a.1 + b.1);
    report.line(&format!(
        "Energy delta via-direct: {:+.0} J ({:+.2} kWh)",
        m_via.energy_j - m_dir.energy_j,
        (m_via.energy_j - m_dir.energy_j) / 3.6e6
    ));

    // Water POI diagnostic for hiking corridor samples
    report.section("Water POI diagnostic (hedmark+oppland index)");
    let hedmark = fixtures.join("hedmark-latest.osm.pbf");
    let oppland = fixtures.join("oppland-latest.osm.pbf");
    if hedmark.exists() && oppland.exists() {
        let poi = CombinedPoiIndex::load(&[oppland, hedmark])?;
        report.line(&format!("Combined POI records: {}", poi.total_len()));
        // Count water near a few known Rondane samples from prior report
        let probes = [
            (61.15537, 10.91746, "Aakersaetra"),
            (61.13526, 10.88475, "old-water-1"),
            (61.20561, 10.66224, "old-water-5"),
            (61.58578, 10.35365, "Jammerdalsbu"),
            (61.87875, 9.79634, "Rondvassbu"),
        ];
        for (lat, lon, label) in probes {
            let hits = poi.nearest(PoiCategory::Water, lat, lon, 5_000.0);
            report.line(&format!("  Water within 5 km of {label}: {}", hits.len()));
        }
        // Also count water along current hiking-like eco path if we build foot graph later — skip here.
        let _ = sample_route_points;
        let _ = nearest_node;
    } else {
        report.line("POI PBFs missing — skip water diagnostic");
    }

    let out = fixtures.join("route_geometry_audit_report.md");
    report.write(&out)?;
    println!("{}", report.to_string());
    println!("Wrote {}", out.display());
    Ok(())
}
