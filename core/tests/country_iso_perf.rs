//! Edge-filter throughput and load-time budget for country polygons.
//!
//! Enable with `NAVI_COUNTRY_ISO_PERF=1` and build `--release`.
//! Compares old coarse rings (commit before 69135575), midpoint-only polygons,
//! and start+mid+end. Budget: classifying every ostlandet edge with
//! `allowed_countries = Some` must add at most
//! [`MAX_LOAD_OVERHEAD_PCT`] of graph load wall time — FAIL (not warn) if exceeded.

use driver_break_core::routing::elevation::{
    cell_owner_coverage, country_iso_at, iso_lookup_stats, reset_iso_lookup_stats,
    warm_country_polys,
};
use driver_break_core::routing::graph::{GraphEdge, RoutingProfile};
use driver_break_core::routing::indexed::try_load_graph_for_plan_bbox;
use geo::{point, Contains, Coord, LineString, Polygon};
use rayon::prelude::*;
use std::path::PathBuf;
use std::time::Instant;

/// Max added wall time vs graph load when filtering all edges with Some(%).
const MAX_LOAD_OVERHEAD_PCT: f64 = 15.0;

type LonLat = (f64, f64);

fn poly(ring: &[LonLat]) -> Polygon {
    let mut coords: Vec<Coord> = ring
        .iter()
        .map(|(lon, lat)| Coord { x: *lon, y: *lat })
        .collect();
    if let (Some(first), Some(last)) = (coords.first().copied(), coords.last().copied()) {
        if first != last {
            coords.push(first);
        }
    }
    Polygon::new(LineString::new(coords), vec![])
}

/// Coarse rings from `country_polys.rs` at the parent of 69135575 (pre-NE asset).
fn old_box_iso_at(lat: f64, lon: f64) -> Option<&'static str> {
    // Subset sufficient for ostlandet / Scandinavia + US border perf comparison.
    const RINGS: &[(&str, &[LonLat])] = &[
        (
            "dk",
            &[(8.05, 54.55), (12.70, 54.55), (12.70, 57.80), (8.05, 57.80)],
        ),
        (
            "se",
            &[
                (11.00, 55.20),
                (24.20, 55.20),
                (24.20, 69.10),
                (11.00, 69.10),
            ],
        ),
        (
            "fi",
            &[
                (20.50, 59.70),
                (31.60, 59.70),
                (31.60, 70.10),
                (20.50, 70.10),
            ],
        ),
        (
            "no",
            &[(4.30, 57.80), (31.20, 57.80), (31.20, 71.40), (4.30, 71.40)],
        ),
        (
            "de",
            &[(5.80, 47.20), (15.10, 47.20), (15.10, 55.20), (5.80, 55.20)],
        ),
        (
            "us",
            &[(-125.0, 24.0), (-66.0, 24.0), (-66.0, 49.5), (-125.0, 49.5)],
        ),
        (
            "ca",
            &[(-141.0, 41.5), (-52.0, 41.5), (-52.0, 70.0), (-141.0, 70.0)],
        ),
        (
            "mx",
            &[(-118.5, 14.5), (-86.5, 14.5), (-86.5, 32.8), (-118.5, 32.8)],
        ),
    ];
    let p = point!(x: lon, y: lat);
    for (code, ring) in RINGS {
        if poly(ring).contains(&p) {
            return Some(*code);
        }
    }
    None
}

fn ostlandet_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/espa-dombas-e2e")
}

fn mid_only_poly(e: &GraphEdge) -> bool {
    let mid_lat = (e.start_lat + e.end_lat) * 0.5;
    let mid_lon = (e.start_lon + e.end_lon) * 0.5;
    matches!(country_iso_at(mid_lat, mid_lon), Some("no"))
}

fn sme_poly(e: &GraphEdge) -> bool {
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
}

fn sme_old_box(e: &GraphEdge) -> bool {
    let mid_lat = (e.start_lat + e.end_lat) * 0.5;
    let mid_lon = (e.start_lon + e.end_lon) * 0.5;
    for (lat, lon) in [
        (e.start_lat, e.start_lon),
        (mid_lat, mid_lon),
        (e.end_lat, e.end_lon),
    ] {
        if old_box_iso_at(lat, lon) != Some("no") {
            return false;
        }
    }
    true
}

#[test]
#[ignore = "perf: set NAVI_COUNTRY_ISO_PERF=1 (needs ostlandet car tiles; run --release)"]
fn measure_country_filter_throughput() {
    assert_eq!(
        std::env::var("NAVI_COUNTRY_ISO_PERF").ok().as_deref(),
        Some("1")
    );
    eprintln!(
        "build_profile={}",
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    );
    if cfg!(debug_assertions) {
        panic!("country_iso_perf must be run with --release");
    }

    let data = ostlandet_dir();
    let pbf = data.join("ostlandet-latest.osm.pbf");
    let t_warm0 = Instant::now();
    let mem = warm_country_polys();
    let (owned, cells) = cell_owner_coverage();
    let warm_ms = t_warm0.elapsed().as_secs_f64() * 1000.0;
    eprintln!("country_polys_warm_ms={warm_ms:.2} decoded_bytes≈{mem} cell_owner={owned}/{cells}");

    let bbox = [59.0_f64, 9.5, 62.5, 12.5];
    let t_graph0 = Instant::now();
    let graph = try_load_graph_for_plan_bbox(&data, &pbf, RoutingProfile::Car, Some(bbox))
        .expect("load ostlandet car graph");
    let graph_ms = t_graph0.elapsed().as_secs_f64() * 1000.0;
    let graph_s = graph_ms / 1000.0;
    eprintln!(
        "graph_load_ms={graph_ms:.1} edges={} nodes={}",
        graph.edges.len(),
        graph.nodes.len()
    );

    let n = graph.edges.len().max(1);
    let reps = (50_000 / n).clamp(1, 20);

    // --- throughput variants ---
    let mut sink = 0u64;
    let t0 = Instant::now();
    for _ in 0..reps {
        for e in &graph.edges {
            if sme_old_box(e) {
                sink = sink.wrapping_add(1);
            }
        }
    }
    let old_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let old_eps = (n * reps) as f64 / (old_ms / 1000.0);

    let t1 = Instant::now();
    for _ in 0..reps {
        for e in &graph.edges {
            if mid_only_poly(e) {
                sink = sink.wrapping_add(1);
            }
        }
    }
    let mid_ms = t1.elapsed().as_secs_f64() * 1000.0;
    let mid_eps = (n * reps) as f64 / (mid_ms / 1000.0);

    let t2 = Instant::now();
    for _ in 0..reps {
        for e in &graph.edges {
            if sme_poly(e) {
                sink = sink.wrapping_add(1);
            }
        }
    }
    let sme_ms = t2.elapsed().as_secs_f64() * 1000.0;
    let sme_eps = (n * reps) as f64 / (sme_ms / 1000.0);

    eprintln!("sink={sink} reps={reps} edges={n}");
    eprintln!(
        "old_box_start_mid_end_edges_per_s={old_eps:.0} wall_ms={old_ms:.1} added_s={:.3}",
        old_ms / 1000.0
    );
    eprintln!(
        "poly_midpoint_only_edges_per_s={mid_eps:.0} wall_ms={mid_ms:.1} added_s={:.3}",
        mid_ms / 1000.0
    );
    eprintln!(
        "poly_start_mid_end_edges_per_s={sme_eps:.0} wall_ms={sme_ms:.1} added_s={:.3}",
        sme_ms / 1000.0
    );

    // --- load-time budget: parallel classify-all (cached with the graph) ---
    reset_iso_lookup_stats();
    let t_filter0 = Instant::now();
    let allowed = graph.edges.par_iter().filter(|e| sme_poly(e)).count();
    let filter_s = t_filter0.elapsed().as_secs_f64();
    let overhead_pct = if graph_s > 0.0 {
        filter_s / graph_s * 100.0
    } else {
        0.0
    };
    let st = iso_lookup_stats();
    eprintln!(
        "filter_all_edges_parallel_allowed={allowed}/{n} filter_s={filter_s:.3} \
         graph_load_s={graph_s:.3} overhead_pct={overhead_pct:.2} \
         (budget {MAX_LOAD_OVERHEAD_PCT})"
    );
    eprintln!(
        "lookup_stats total={} grid_shortcut={} exact={} vertices_tested={} \
         grid_share={:.1}%",
        st.total,
        st.grid_shortcut,
        st.exact,
        st.vertices_tested,
        if st.total > 0 {
            st.grid_shortcut as f64 / st.total as f64 * 100.0
        } else {
            0.0
        }
    );

    // Single-lookup profile (interior NO vs border).
    reset_iso_lookup_stats();
    let t_one = Instant::now();
    for _ in 0..10_000 {
        let _ = country_iso_at(61.0, 10.5);
    }
    let one_ms = t_one.elapsed().as_secs_f64() * 1000.0 / 10_000.0;
    let st1 = iso_lookup_stats();
    eprintln!(
        "single_lookup_interior_no_us={one_ms:.4} grid_hits={} exact={}",
        st1.grid_shortcut, st1.exact
    );

    assert!(
        overhead_pct <= MAX_LOAD_OVERHEAD_PCT,
        "PERF_BUDGET_EXCEEDED: filtering all edges with allowed_countries=Some added \
         {overhead_pct:.2}% of graph load time (budget {MAX_LOAD_OVERHEAD_PCT}%, \
         filter_s={filter_s:.3}, graph_load_s={graph_s:.3})"
    );
}
