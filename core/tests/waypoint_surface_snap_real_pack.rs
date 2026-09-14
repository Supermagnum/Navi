//! Real ostlandet-pack verification for Espedalsvegen 656 and Søndre Grøtting.
//!
//! Device tile checksums match `target/espa-dombas-e2e` (verified 2026-09-14 on
//! SM-P613). These tests load those indexed car tiles and assert destination
//! snaps stay near the pin, not on a paved road hundreds of metres away.

use driver_break_core::config::CAR_MAX_WAYPOINT_SNAP_M;
use driver_break_core::routing::graph::{
    worst_incident_surface, RouteGraph, RouteOptions, RoutingProfile, SurfaceQuality,
    SurfaceRoutingMode,
};
use driver_break_core::routing::indexed::{load_graph_pack_bbox, merge_tile_graphs};
use osm4routing::NodeId;
use std::path::{Path, PathBuf};

const ESPEDALSVEGEN: (f64, f64) = (61.3636391, 9.6735332);
const SONDRE_GROTTING: (f64, f64) = (61.865580, 10.898674);

const ESPEDALSVEGEN_OLD_BUG_M: f64 = 125.0;
const SONDRE_GROTTING_OLD_BUG_M: f64 = 260.0;
const ENDPOINT_MAX_M: f64 = 50.0;

fn data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/espa-dombas-e2e")
}

fn bbox_around(lat: f64, lon: f64, pad_deg: f64) -> [f64; 4] {
    [lat - pad_deg, lon - pad_deg, lat + pad_deg, lon + pad_deg]
}

fn haversine_m(lat: f64, lon: f64, nlat: f64, nlon: f64) -> f64 {
    let dlat = (nlat - lat).to_radians();
    let dlon = (nlon - lon).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat.to_radians().cos() * nlat.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    2.0 * 6_378_100.0 * a.sqrt().asin()
}

fn load_car_pack(dir: &Path, bbox: [f64; 4]) -> RouteGraph {
    let mut graphs = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let n = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            n.starts_with("ostlandet-latest.navi-graph-car.") && n.ends_with(".rkyv")
        })
        .collect();
    entries.sort();
    for p in entries {
        if let Ok(g) = load_graph_pack_bbox(&p, RoutingProfile::Car, Some(bbox)) {
            if !g.edges.is_empty() {
                graphs.push(g);
            }
        }
    }
    assert!(
        !graphs.is_empty(),
        "no car tiles covering bbox in {}",
        dir.display()
    );
    let mut graph = merge_tile_graphs(graphs, RoutingProfile::Car);
    graph.surface_routing_mode = SurfaceRoutingMode::Car;
    graph
}

/// Pre-fix behaviour: best surface anywhere inside the full car snap budget.
fn legacy_full_budget_surface_snap(
    graph: &RouteGraph,
    lat: f64,
    lon: f64,
) -> (NodeId, f64, SurfaceQuality) {
    let mut best: Option<(NodeId, f64, SurfaceQuality)> = None;
    for n in graph.nodes.values() {
        if !graph.is_linked(n.id) {
            continue;
        }
        let dist = haversine_m(lat, lon, n.coord.y, n.coord.x);
        if dist > CAR_MAX_WAYPOINT_SNAP_M {
            continue;
        }
        let sq = worst_incident_surface(graph, n.id);
        let replace = match best {
            None => true,
            Some((_, prev_d, prev_sq)) => sq < prev_sq || (sq == prev_sq && dist < prev_d),
        };
        if replace {
            best = Some((n.id, dist, sq));
        }
    }
    best.expect("at least one linked node in snap budget")
}

fn incident_summary(graph: &RouteGraph, id: NodeId) -> String {
    let mut parts = Vec::new();
    for e in &graph.edges {
        if e.source != id && e.target != id {
            continue;
        }
        parts.push(format!(
            "{}:{:?}:{}",
            e.highway.as_deref().unwrap_or("?"),
            e.surface_quality,
            e.name.as_deref().unwrap_or("-")
        ));
        if parts.len() >= 8 {
            break;
        }
    }
    parts.join(" | ")
}

fn assert_real_destination_snap(label: &str, query: (f64, f64), old_bug_m: f64) {
    let dir = data_dir();
    assert!(
        dir.is_dir(),
        "missing {} — place ostlandet car tiles there",
        dir.display()
    );
    // Match plan-time pad scale so farm tracks stay connected into the giant
    // component (0.03° can clip the driveway into an island).
    let graph = load_car_pack(&dir, bbox_around(query.0, query.1, 0.05));
    let opts = RouteOptions::default();

    let (end_id, end_dist) = graph
        .nearest_routable_with_options(query.0, query.1, &opts, false)
        .unwrap_or_else(|e| panic!("{label}: endpoint snap failed: {e:?}"));
    let end_sq = worst_incident_surface(&graph, end_id);
    let end_node = graph.nodes.get(&end_id).expect("snapped node");

    let (via_id, via_dist) = graph
        .nearest_routable_with_options(query.0, query.1, &opts, true)
        .expect("via-style snap");
    let via_sq = worst_incident_surface(&graph, via_id);

    let (legacy_id, legacy_dist, legacy_sq) =
        legacy_full_budget_surface_snap(&graph, query.0, query.1);

    eprintln!(
        "{label} query=({:.6},{:.6})\n  endpoint: id={} dist={:.1}m surface={:?} at=({:.6},{:.6}) edges=[{}]\n  via:      id={} dist={:.1}m surface={:?}\n  legacy:   id={} dist={:.1}m surface={:?} (full-budget surface pick)",
        query.0,
        query.1,
        end_id.0,
        end_dist,
        end_sq,
        end_node.coord.y,
        end_node.coord.x,
        incident_summary(&graph, end_id),
        via_id.0,
        via_dist,
        via_sq,
        legacy_id.0,
        legacy_dist,
        legacy_sq,
    );

    assert!(
        end_dist <= ENDPOINT_MAX_M,
        "{label}: endpoint snap {end_dist:.1} m exceeds {ENDPOINT_MAX_M} m \
         (old paved bug was ~{old_bug_m} m); surface={end_sq:?}"
    );
    assert!(
        end_dist + 40.0 < old_bug_m,
        "{label}: endpoint snap {end_dist:.1} m is not clearly nearer than old bug ~{old_bug_m} m"
    );
    if legacy_sq < end_sq {
        assert!(
            legacy_dist > end_dist + 30.0,
            "{label}: expected legacy better-surface pick farther than driveway \
             (legacy={legacy_dist:.1}m {:?}, endpoint={end_dist:.1}m {:?})",
            legacy_sq,
            end_sq
        );
    }
}

#[test]
#[ignore = "needs ostlandet car tiles under target/espa-dombas-e2e"]
fn espedalsvegen_656_real_pack_destination_snap() {
    assert_real_destination_snap("Espedalsvegen 656", ESPEDALSVEGEN, ESPEDALSVEGEN_OLD_BUG_M);
}

#[test]
#[ignore = "needs ostlandet car tiles under target/espa-dombas-e2e"]
fn sondre_grotting_real_pack_destination_snap() {
    assert_real_destination_snap(
        "Søndre Grøtting",
        SONDRE_GROTTING,
        SONDRE_GROTTING_OLD_BUG_M,
    );
}
