//! Hamar → Rendalsportalen: car should prefer Elverum / Rv3 asphalt over gravel shortcuts.

use driver_break_core::routing::graph::{
    apply_surface_preference, MotorSoftCostProfile, RouteGraph, RouteOptions, RoutingProfile,
    SurfaceQuality, SurfaceRoutingMode,
};
use std::collections::HashMap;
use std::path::PathBuf;

const HAMAR: (f64, f64) = (60.7947205, 11.0680555);
const RENDALSPORTALEN: (f64, f64) = (61.8351511, 10.8862441);

fn pbf() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/integration-fixtures/ostlandet-latest.osm.pbf")
}

fn summarize(label: &str, graph: &RouteGraph, edges: &[usize], cost: f64) -> f64 {
    let mut total = 0.0;
    let mut nongood = 0.0;
    let mut by_ref: HashMap<String, f64> = HashMap::new();
    for &idx in edges {
        let e = &graph.edges[idx];
        total += e.length_m;
        if e.surface_quality != SurfaceQuality::Good {
            nongood += e.length_m;
        }
        let r = e.road_ref.clone().unwrap_or_else(|| "(none)".into());
        *by_ref.entry(r).or_default() += e.length_m;
    }
    let path_km = total / 1000.0;
    let nongood_pct = 100.0 * (nongood / 1000.0) / path_km.max(1e-9);
    let mut refs: Vec<_> = by_ref.into_iter().collect();
    refs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    eprintln!(
        "{label}: path_km={path_km:.1} nongood_pct={nongood_pct:.1}% cost={cost:.0} refs={:?}",
        refs.iter()
            .take(8)
            .map(|(k, v)| format!("{k}={:.1}km", v / 1000.0))
            .collect::<Vec<_>>()
    );
    nongood_pct
}

#[test]
#[ignore = "needs ostlandet-latest.osm.pbf under target/integration-fixtures"]
fn hamar_rendalsportalen_prefers_asphalt_over_gravel() {
    let pbf = pbf();
    assert!(pbf.is_file(), "missing {}", pbf.display());

    let pad = 0.45;
    let bbox = [
        HAMAR.0.min(RENDALSPORTALEN.0) - pad,
        HAMAR.1.min(RENDALSPORTALEN.1) - pad,
        HAMAR.0.max(RENDALSPORTALEN.0) + pad,
        HAMAR.1.max(RENDALSPORTALEN.1) + pad,
    ];
    let mut graph =
        RouteGraph::build_from_pbf_bbox(&pbf, RoutingProfile::Car, bbox).expect("bbox build");
    let (s, _) = graph
        .nearest_routable(HAMAR.0, HAMAR.1)
        .expect("snap Hamar");
    let (g, _) = graph
        .nearest_routable(RENDALSPORTALEN.0, RENDALSPORTALEN.1)
        .expect("snap Rendalsportalen");

    // Soft costs with real surface classes.
    graph.surface_routing_mode = SurfaceRoutingMode::Car;
    apply_surface_preference(
        &mut graph,
        SurfaceRoutingMode::Car,
        MotorSoftCostProfile::Car,
    );
    let (_p1, e1, c1) = graph
        .shortest_path_with_options(s, g, false, &RouteOptions::default())
        .expect("route with soft");
    let nongood = summarize("WITH_SOFT", &graph, &e1, c1);
    assert!(
        nongood < 8.0,
        "expected asphalt via Elverum/Rv3; nongood_pct={nongood:.1}%"
    );

    // All-Good pack regression: highway-class soft cost must still avoid gravel
    // tertiary shortcuts even when surface_quality is missing.
    for e in &mut graph.edges {
        e.surface_quality = SurfaceQuality::Good;
        e.base_weight = e.length_m;
        if let Some(ref mut eco) = e.eco_weight {
            *eco = e.length_m;
        }
    }
    apply_surface_preference(
        &mut graph,
        SurfaceRoutingMode::Car,
        MotorSoftCostProfile::Car,
    );
    let (_p2, e2, c2) = graph
        .shortest_path_with_options(s, g, false, &RouteOptions::default())
        .expect("route all-good");
    let _ = summarize("ALL_GOOD_PACK", &graph, &e2, c2);
    let uses_rv3 = e2.iter().any(|&idx| {
        graph.edges[idx]
            .road_ref
            .as_deref()
            .is_some_and(|r| r == "3" || r.starts_with("3;") || r.ends_with(";3"))
    });
    let tertiary_km: f64 = e2
        .iter()
        .filter(|&&idx| graph.edges[idx].highway.as_deref() == Some("tertiary"))
        .map(|&idx| graph.edges[idx].length_m)
        .sum::<f64>()
        / 1000.0;
    assert!(
        uses_rv3 && tertiary_km < 20.0,
        "all-Good pack must still prefer Rv3 over tertiary shortcut; uses_rv3={uses_rv3} tertiary_km={tertiary_km:.1}"
    );
}
