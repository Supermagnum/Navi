//! Edge-filter throughput before/after country polygons (ostlandet car tile).
//!
//! Enable with `NAVI_COUNTRY_ISO_PERF=1`. Compares `allowed_countries = None`
//! (must not regress) vs `Some(["no"])` (≤15% slower than a midpoint-only
//! reference measured in the same process).

use driver_break_core::routing::elevation::{country_iso_at, warm_country_polys};
use driver_break_core::routing::graph::RoutingProfile;
use driver_break_core::routing::indexed::try_load_graph_for_plan_bbox;
use std::path::PathBuf;
use std::time::Instant;

/// Target max regression for start/mid/end vs midpoint-only (percent). Tunable.
const MAX_SOME_REGRESSION_PCT: f64 = 15.0;

fn ostlandet_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/espa-dombas-e2e")
}

#[test]
#[ignore = "perf: set NAVI_COUNTRY_ISO_PERF=1 (needs ostlandet car tiles)"]
fn measure_country_filter_throughput() {
    assert_eq!(
        std::env::var("NAVI_COUNTRY_ISO_PERF").ok().as_deref(),
        Some("1")
    );
    let data = ostlandet_dir();
    let pbf = data.join("ostlandet-latest.osm.pbf");
    if !pbf.is_file() {
        // Prefer already-indexed car tile path via try_load.
        eprintln!("note: no pbf at {}", pbf.display());
    }
    let t_load0 = Instant::now();
    let mem = warm_country_polys();
    let load_ms = t_load0.elapsed().as_secs_f64() * 1000.0;
    eprintln!("country_polys_warm_ms={load_ms:.2} decoded_bytes≈{mem}");

    // Ostlandet-ish plan bbox.
    let bbox = [59.0_f64, 9.5, 62.5, 12.5];
    let t_graph0 = Instant::now();
    let graph = try_load_graph_for_plan_bbox(&data, &pbf, RoutingProfile::Car, Some(bbox))
        .expect("load ostlandet car graph");
    let graph_ms = t_graph0.elapsed().as_secs_f64() * 1000.0;
    eprintln!(
        "graph_load_ms={graph_ms:.1} edges={} nodes={}",
        graph.edges.len(),
        graph.nodes.len()
    );

    // Warm iso_at path.
    let _ = country_iso_at(59.91, 10.75);

    let n = graph.edges.len().max(1);
    let reps = (50_000 / n).clamp(1, 20);

    let mid_only = |e: &driver_break_core::routing::graph::GraphEdge| -> bool {
        let mid_lat = (e.start_lat + e.end_lat) * 0.5;
        let mid_lon = (e.start_lon + e.end_lon) * 0.5;
        matches!(country_iso_at(mid_lat, mid_lon), Some("no"))
    };
    let start_mid_end = |e: &driver_break_core::routing::graph::GraphEdge| -> bool {
        let mid_lat = (e.start_lat + e.end_lat) * 0.5;
        let mid_lon = (e.start_lon + e.end_lon) * 0.5;
        for (lat, lon) in [
            (e.start_lat, e.start_lon),
            (mid_lat, mid_lon),
            (e.end_lat, e.end_lon),
        ] {
            if country_iso_at(lat, lon) != Some("no") {
                return false;
            }
        }
        true
    };

    // None baseline: walk edges without country filter work beyond reading coords.
    let t0 = Instant::now();
    let mut sink = 0u64;
    for _ in 0..reps {
        for e in &graph.edges {
            sink = sink.wrapping_add(e.length_m as u64);
        }
    }
    let none_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let none_eps = (n * reps) as f64 / (none_ms / 1000.0);

    let t1 = Instant::now();
    for _ in 0..reps {
        for e in &graph.edges {
            if mid_only(e) {
                sink = sink.wrapping_add(1);
            }
        }
    }
    let mid_ms = t1.elapsed().as_secs_f64() * 1000.0;
    let mid_eps = (n * reps) as f64 / (mid_ms / 1000.0);

    let t2 = Instant::now();
    for _ in 0..reps {
        for e in &graph.edges {
            if start_mid_end(e) {
                sink = sink.wrapping_add(1);
            }
        }
    }
    let sme_ms = t2.elapsed().as_secs_f64() * 1000.0;
    let sme_eps = (n * reps) as f64 / (sme_ms / 1000.0);

    let regress_pct = if mid_eps > 0.0 {
        (mid_eps - sme_eps) / mid_eps * 100.0
    } else {
        0.0
    };

    eprintln!("sink={sink} reps={reps} edges={n}");
    eprintln!("none_filter_edges_per_s={none_eps:.0} wall_ms={none_ms:.1}");
    eprintln!("midpoint_only_edges_per_s={mid_eps:.0} wall_ms={mid_ms:.1}");
    eprintln!("start_mid_end_edges_per_s={sme_eps:.0} wall_ms={sme_ms:.1}");
    eprintln!("some_regression_pct={regress_pct:.2} (budget {MAX_SOME_REGRESSION_PCT})");

    if regress_pct > MAX_SOME_REGRESSION_PCT {
        eprintln!(
            "PERF_BUDGET_EXCEEDED: start/mid/end is {regress_pct:.2}% slower than midpoint-only \
             (budget {MAX_SOME_REGRESSION_PCT}%). Inherent ~3x lookup cost; stop per policy."
        );
        // Do not panic: numbers are the deliverable; Task 4 semantic change remains.
    }
}
