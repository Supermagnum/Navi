//! Eco-mode investigation: Falletvegen (Stange) -> Atnbrufossen (Atnbrua).
//!
//! Compares eco-on vs eco-off routes chosen by the router alone (no forced vias).
//! Observes whether the eco path naturally passes near Atnosen.
//!
//! Run: `cargo test --test falletvegen_atnbrufossen_eco -- --nocapture --ignored`

mod helpers;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use driver_break_core::config::EcoConfig;
use driver_break_core::routing::elevation::{
    bbox_to_tiles, DownloadControl, ElevationCache, ElevationDownloader, ElevationService,
};
use driver_break_core::routing::graph::{RouteGraph, RoutingProfile};
use driver_break_core::storage::{ElevationJobStore, JobStatus, Storage};
use helpers::{haversine_m, path_edge_indices, route_metrics, TestReport};
use tokio::runtime::Runtime;

/// Falletvegen / Espa area, Stange 2338. Exact coords used by the known-good
/// corridor test (on Falletvegen approach); Nominatim Falletvegen centroid can
/// snap to a disconnected spur even with routable filtering.
const START_LAT: f64 = 60.562_191_4;
const START_LON: f64 = 11.256_123_9;
/// Atnbrufossen bus stop, Atnbrua.
const END_LAT: f64 = 61.851_250_0;
const END_LON: f64 = 10.233_842_0;
/// Observation only — not used as a routing via.
const ATNOSEN_LAT: f64 = 61.729_384_8;
const ATNOSEN_LON: f64 = 10.817_000_0;

const J_PER_KWH: f64 = 3_600_000.0;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("integration-fixtures")
}

fn passat_eco_config() -> EcoConfig {
    EcoConfig {
        drag_coefficient: 0.28,
        frontal_area_m2: 2.2,
        mass_kg: 1500.0,
        ..EcoConfig::default()
    }
}

fn ensure_dem_tiles(data_dir: &Path, storage: &Storage) -> anyhow::Result<()> {
    let bbox = [60.35, 9.95, 62.05, 11.65];
    let cache = ElevationCache::new(data_dir);
    let tiles = bbox_to_tiles(bbox);
    if tiles.iter().all(|t| cache.tile_exists(*t)) {
        return Ok(());
    }
    let downloader = ElevationDownloader::new(storage.clone(), cache.clone());
    let job = downloader.queue_region(bbox)?;
    let rt = Runtime::new()?;
    let record = rt.block_on(downloader.run_job(job.id, &DownloadControl::default()))?;
    anyhow::ensure!(
        matches!(record.status, JobStatus::Completed),
        "DEM job failed: {:?}",
        record.status
    );
    let store = ElevationJobStore::new(storage);
    let (done, total) = store.progress(job.id)?;
    anyhow::ensure!(done == total && done > 0, "DEM incomplete {done}/{total}");
    Ok(())
}

fn min_dist_to_atnosen(graph: &RouteGraph, path: &[osm4routing::NodeId]) -> f64 {
    path.iter()
        .filter_map(|id| graph.nodes.get(id))
        .map(|n| haversine_m(ATNOSEN_LAT, ATNOSEN_LON, n.coord.y, n.coord.x))
        .fold(f64::INFINITY, f64::min)
}

fn nearest_routable(
    graph: &RouteGraph,
    lat: f64,
    lon: f64,
    require_outgoing: bool,
) -> anyhow::Result<(osm4routing::NodeId, f64, f64, f64)> {
    use rayon::prelude::*;
    use std::collections::HashSet;
    let allowed: HashSet<osm4routing::NodeId> = if require_outgoing {
        graph.edges.iter().map(|e| e.source).collect()
    } else {
        let mut s = HashSet::new();
        for e in &graph.edges {
            s.insert(e.source);
            s.insert(e.target);
        }
        s
    };
    let best = allowed
        .par_iter()
        .filter_map(|id| graph.nodes.get(id).map(|n| (n, id)))
        .map(|(node, _)| {
            let d = haversine_m(lat, lon, node.coord.y, node.coord.x);
            (node.id, node.coord.y, node.coord.x, d)
        })
        .min_by(|a, b| a.3.partial_cmp(&b.3).unwrap_or(std::cmp::Ordering::Equal))
        .ok_or_else(|| anyhow::anyhow!("no routable nodes in graph"))?;
    Ok(best)
}

fn mean_gradient_pct(climb_m: f64, descent_m: f64, distance_m: f64) -> f64 {
    if distance_m < 1.0 {
        return 0.0;
    }
    (climb_m + descent_m) / distance_m * 100.0
}

fn avg_power_kw(energy_j: f64, distance_m: f64, speed_kmh: f64) -> f64 {
    let hours = (distance_m / 1000.0) / speed_kmh;
    if hours <= 0.0 {
        return 0.0;
    }
    // J / s / 1000 = kW; duration_s = hours * 3600
    energy_j / (hours * 3600.0) / 1000.0
}

#[test]
#[ignore = "network integration: uses cached ostlandet PBF + DEM"]
fn falletvegen_atnbrufossen_eco_on_vs_off() {
    if let Err(e) = run() {
        panic!("{e:#}");
    }
}

fn run() -> anyhow::Result<()> {
    let fixtures = fixture_dir();
    let ostlandet = fixtures.join("ostlandet-latest.osm.pbf");
    anyhow::ensure!(
        ostlandet.exists(),
        "missing {}; run the car corridor integration once first",
        ostlandet.display()
    );
    let data_dir = fixtures.join("elevation");
    fs::create_dir_all(&data_dir)?;
    let storage = Storage::open(fixtures.join("integration.db"))?;
    ensure_dem_tiles(&data_dir, &storage)?;

    let mut report = TestReport::with_title("Falletvegen -> Atnbrufossen eco investigation");
    report.section("Setup");
    report.line(&format!(
        "Start Falletvegen/Espa, Stange 2338: ({START_LAT:.7}, {START_LON:.7})"
    ));
    report.line("Start is the known-good Falletvegen-corridor point (same as car corridor test).");
    report.line(&format!("End Atnbrufossen: ({END_LAT:.7}, {END_LON:.7})"));
    report.line(&format!(
        "Atnosen (observation only, not a via): ({ATNOSEN_LAT:.7}, {ATNOSEN_LON:.7})"
    ));
    report.line(
        "Vehicle profile: VW Passat B8 diesel physics (Cd=0.28, A=2.2 m2, mass=1500 kg) — NOT an EV.",
    );
    report.line(
        "Energy is computed in joules; kWh = J/3.6e6. Average kW = energy / duration at 90 km/h.",
    );
    report.line("Eco-on vs eco-off: router chooses freely (no forced vias).");

    let t0 = Instant::now();
    let elevation = ElevationService::new(ElevationCache::new(&data_dir));
    let eco = passat_eco_config();

    let graph_flat = RouteGraph::build_from_pbf(&ostlandet, RoutingProfile::Car)
        .map_err(|e| anyhow::anyhow!("graph build: {e}"))?;
    let start = nearest_routable(&graph_flat, START_LAT, START_LON, true)?;
    let goal = nearest_routable(&graph_flat, END_LAT, END_LON, false)?;
    report.line(&format!(
        "Graph: {} nodes, {} edges; start snap {:.0} m, goal snap {:.0} m",
        graph_flat.nodes.len(),
        graph_flat.edges.len(),
        start.3,
        goal.3
    ));
    report.line(&format!(
        "Start node {} @ ({:.6},{:.6}); goal node {} @ ({:.6},{:.6})",
        start.0 .0, start.1, start.2, goal.0 .0, goal.1, goal.2
    ));

    // --- Eco off (length weights) ---
    report.section("Eco OFF (base_weight = length)");
    let path_off = graph_flat
        .shortest_path(start.0, goal.0, false)
        .ok_or_else(|| anyhow::anyhow!("no route eco-off"))?;
    let edges_off = path_edge_indices(&graph_flat, &path_off.0);
    // Physics energy on the eco-off geometry (fair comparison).
    let m_off = route_metrics(&graph_flat, &edges_off, &elevation, &eco, true);
    let atn_off = min_dist_to_atnosen(&graph_flat, &path_off.0);
    let kwh_off = m_off.energy_j / J_PER_KWH;
    let kw_off = avg_power_kw(m_off.energy_j, m_off.distance_m, 90.0);
    let kwh_per_100_off = kwh_off * 100.0 / (m_off.distance_m / 1000.0);
    report.log_route_metrics("Eco-off path", &m_off, path_off.1);
    report.line(&format!(
        "  Mean |gradient| proxy: {:.2}% (climb+descent)/distance",
        mean_gradient_pct(m_off.total_climb_m, m_off.total_descent_m, m_off.distance_m)
    ));
    report.line(&format!(
        "  Energy: {kwh_off:.2} kWh total · {kwh_per_100_off:.2} kWh/100km · avg {kw_off:.2} kW @90km/h"
    ));
    report.line(&format!(
        "  Min distance to Atnosen along path: {:.1} km",
        atn_off / 1000.0
    ));

    // --- Eco on ---
    report.section("Eco ON (elevation reweight + A*)");
    let mut graph_eco = RouteGraph::build_from_pbf(&ostlandet, RoutingProfile::Car)
        .map_err(|e| anyhow::anyhow!("graph build eco: {e}"))?;
    graph_eco.apply_eco_reweighting(&elevation, &eco);
    let path_on = graph_eco
        .shortest_path(start.0, goal.0, true)
        .ok_or_else(|| anyhow::anyhow!("no route eco-on"))?;
    let edges_on = path_edge_indices(&graph_eco, &path_on.0);
    let eco_some = edges_on
        .iter()
        .filter(|&&i| graph_eco.edges[i].eco_weight.is_some())
        .count();
    let eco_share = eco_some as f64 / edges_on.len().max(1) as f64;
    let m_on = route_metrics(&graph_eco, &edges_on, &elevation, &eco, true);
    let atn_on = min_dist_to_atnosen(&graph_eco, &path_on.0);
    let kwh_on = m_on.energy_j / J_PER_KWH;
    let kw_on = avg_power_kw(m_on.energy_j, m_on.distance_m, 90.0);
    let kwh_per_100_on = kwh_on * 100.0 / (m_on.distance_m / 1000.0);
    report.line(&format!(
        "Route edges with eco_weight Some: {}/{:.0}%",
        eco_some,
        eco_share * 100.0
    ));
    report.log_route_metrics("Eco-on path", &m_on, path_on.1);
    report.line(&format!(
        "  Mean |gradient| proxy: {:.2}%",
        mean_gradient_pct(m_on.total_climb_m, m_on.total_descent_m, m_on.distance_m)
    ));
    report.line(&format!(
        "  Energy: {kwh_on:.2} kWh total · {kwh_per_100_on:.2} kWh/100km · avg {kw_on:.2} kW @90km/h"
    ));
    report.line(&format!(
        "  Min distance to Atnosen along path: {:.1} km",
        atn_on / 1000.0
    ));

    // Cross-score: physics energy of each geometry (already both with use_eco=true above).
    report.section("Comparison (physics energy on each chosen geometry)");
    let paths_differ = path_on.0 != path_off.0;
    report.line(&format!("paths_differ: {paths_differ}"));
    report.line(&format!("Paths identical: {}", !paths_differ));
    report.line(&format!(
        "Distance delta (eco-on - eco-off): {:.2} km",
        (m_on.distance_m - m_off.distance_m) / 1000.0
    ));
    report.line(&format!(
        "Climb delta (eco-on - eco-off): {:.0} m",
        m_on.total_climb_m - m_off.total_climb_m
    ));
    report.line(&format!(
        "Energy delta (eco-on - eco-off): {:.0} J ({:.2} kWh)",
        m_on.energy_j - m_off.energy_j,
        (m_on.energy_j - m_off.energy_j) / J_PER_KWH
    ));
    report.line(&format!(
        "Atnosen proximity: eco-on {:.1} km vs eco-off {:.1} km (observation only)",
        atn_on / 1000.0,
        atn_off / 1000.0
    ));

    let eco_picked_higher = m_on.energy_j > m_off.energy_j + 1.0;
    if eco_picked_higher {
        report.line(
            "RESULT: eco-on path has HIGHER physics energy than eco-off — cost-function / selection bug suspected.",
        );
    } else if m_on.energy_j + 1.0 < m_off.energy_j {
        report.line("RESULT: eco-on path has lower physics energy than eco-off (expected).");
    } else {
        report.line("RESULT: physics energy essentially equal on both paths.");
    }

    // --- Atnosen reachability + diagnostic energy of via-Atnosen geometry ---
    // Diagnostic only: does NOT force eco selection; checks graph filters and
    // whether the ~10 km longer detour would win on energy under Passat physics.
    report.section("Atnosen graph reachability (not a forced eco via)");
    let atn_node = nearest_routable(&graph_flat, ATNOSEN_LAT, ATNOSEN_LON, false)?;
    report.line(&format!(
        "Atnosen snap: node {} @ ({:.6},{:.6}) {:.0} m from hamlet",
        atn_node.0 .0, atn_node.1, atn_node.2, atn_node.3
    ));
    let to_atn = graph_flat.shortest_path(start.0, atn_node.0, false);
    let from_atn = graph_flat.shortest_path(atn_node.0, goal.0, false);
    match (&to_atn, &from_atn) {
        (Some(a), Some(b)) => {
            report.line("Atnosen IS reachable from start and to goal under Car profile (not filter-excluded).");
            let mut via_nodes = a.0.clone();
            if via_nodes.last() == b.0.first() {
                via_nodes.extend(b.0.iter().skip(1).copied());
            } else {
                via_nodes.extend(b.0.iter().copied());
            }
            let via_edges = path_edge_indices(&graph_flat, &via_nodes);
            let m_via = route_metrics(&graph_flat, &via_edges, &elevation, &eco, true);
            let kwh_via = m_via.energy_j / J_PER_KWH;
            report.log_route_metrics("Diagnostic via-Atnosen geometry", &m_via, a.1 + b.1);
            report.line(&format!(
                "  Energy: {kwh_via:.2} kWh total · climb {:.0} m · descent {:.0} m",
                m_via.total_climb_m, m_via.total_descent_m
            ));
            report.line(&format!(
                "  Distance vs eco-off direct: via {:.2} km vs direct {:.2} km (delta {:+.2} km)",
                m_via.distance_m / 1000.0,
                m_off.distance_m / 1000.0,
                (m_via.distance_m - m_off.distance_m) / 1000.0
            ));
            report.line(&format!(
                "  Energy vs eco-off direct: via {:.0} J vs direct {:.0} J (delta {:+.0} J)",
                m_via.energy_j,
                m_off.energy_j,
                m_via.energy_j - m_off.energy_j
            ));
            if m_via.energy_j + 1.0 < m_on.energy_j {
                report.line(
                    "NOTE: via-Atnosen has LOWER physics energy than eco-on choice — eco should have preferred it (selection gap).",
                );
            } else {
                report.line(
                    "NOTE: via-Atnosen has higher-or-equal physics energy than eco-on — skipping Atnosen is consistent with the cost model (extra distance costs more than climb saved).",
                );
            }
            // Sample highway tags on edges within 2 km of Atnosen.
            let mut near_hw: Vec<String> = via_edges
                .iter()
                .filter_map(|&i| {
                    let e = &graph_flat.edges[i];
                    let d0 = haversine_m(ATNOSEN_LAT, ATNOSEN_LON, e.start_lat, e.start_lon);
                    let d1 = haversine_m(ATNOSEN_LAT, ATNOSEN_LON, e.end_lat, e.end_lon);
                    if d0.min(d1) < 2_000.0 {
                        e.highway.clone()
                    } else {
                        None
                    }
                })
                .collect();
            near_hw.sort();
            near_hw.dedup();
            report.line(&format!(
                "Highway tags within 2 km of Atnosen on via-path: {near_hw:?}"
            ));
        }
        _ => {
            report.line(
                "Atnosen NOT fully reachable under Car profile — filter/connectivity issue (separate from cost function).",
            );
            report.line(&format!(
                "  start->Atnosen: {}; Atnosen->goal: {}",
                to_atn.is_some(),
                from_atn.is_some()
            ));
        }
    }

    report.section("Distance cross-check notes");
    report.line(
        "OSRM driving (same coords): direct ~189.5 km; via Atnosen ~204.7 km — aligns with Navi ~190.6 km, not the 206.4/216.4 km 'recommended' figures.",
    );
    report.line(
        "Navi A* uses length (eco-off) / energy (eco-on); it is a shortest/lowest-cost path, not a Google-style 'preferred road' route.",
    );

    // DEM coverage on both geometries
    report.section("DEM coverage on chosen edges");
    for (label, graph, edges) in [
        ("eco-off", &graph_flat, &edges_off),
        ("eco-on", &graph_eco, &edges_on),
    ] {
        let mut ok = 0usize;
        let mut miss = 0usize;
        for &i in edges.iter() {
            let e = &graph.edges[i];
            let a = elevation.get_elevation(e.start_lat, e.start_lon);
            let b = elevation.get_elevation(e.end_lat, e.end_lon);
            if a.is_some() && b.is_some() {
                ok += 1;
            } else {
                miss += 1;
            }
        }
        report.line(&format!(
            "{label}: DEM both endpoints {ok}/{}, missing {miss}",
            edges.len()
        ));
    }

    // Floor unit check sample
    report.section("Eco floor sanity (first 5 eco-on edges with descent)");
    let mut n = 0;
    for &i in &edges_on {
        let e = &graph_eco.edges[i];
        let (Some(h0), Some(h1)) = (
            elevation.get_elevation(e.start_lat, e.start_lon),
            elevation.get_elevation(e.end_lat, e.end_lon),
        ) else {
            continue;
        };
        let dh = h1 - h0;
        if dh >= 0.0 {
            continue;
        }
        let raw = eco.segment_energy_joules(e.length_m, dh);
        let flat = eco.flat_energy_joules(e.length_m);
        let stored = e.eco_weight.unwrap_or(f64::NAN);
        report.line(&format!(
            "  len={:.0}m dh={:.1}m flat={:.0}J energy={:.0}J stored={:.0}",
            e.length_m, dh, flat, raw, stored
        ));
        n += 1;
        if n >= 5 {
            break;
        }
    }

    report.line(&format!("Elapsed: {:.1}s", t0.elapsed().as_secs_f64()));
    let out = fixtures.join("falletvegen_atnbrufossen_eco_report.md");
    report.write(&out)?;
    println!("{}", report.to_string());
    println!("Wrote {}", out.display());

    anyhow::ensure!(
        eco_share >= 0.5,
        "eco DEM coverage on eco-on route too low ({:.0}%)",
        eco_share * 100.0
    );
    Ok(())
}
