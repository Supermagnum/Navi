//! Bråstein/Figgjo boardwalk carve-out through indexed wetland + foot packs.
//!
//! Confirms the original Rogaland regression (way 688185327 over reedbed
//! relation 18344742) still works when wetlands are loaded from
//! `{stem}.navi-wetland.rkyv` — Hedmark packs do **not** cover this site.
//!
//! Positive: Heia → west tip of way 688185327.
//! Negative: Møgedal → Bråstein (east of reedbed).
//!
//! Requires `core/target/integration-fixtures/brastein-boardwalk-corridor.osm.pbf`.

use std::fs;
use std::path::{Path, PathBuf};

use driver_break_core::routing::graph::{RouteGraph, RoutingProfile};
use driver_break_core::routing::indexed::{
    convert_region_packs, load_graph_pack_bbox, load_wetland_pack, ConvertOptions,
};
use driver_break_core::routing::wetland::WetlandIndex;
use navi::plan_hiking_route;

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../core/target/integration-fixtures")
}

fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6_371_000.0;
    let p1 = lat1.to_radians();
    let p2 = lat2.to_radians();
    let dp = (lat2 - lat1).to_radians();
    let dl = (lon2 - lon1).to_radians();
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

fn polyline_min_dist(polyline: &str, lat: f64, lon: f64) -> f64 {
    let mut best = f64::MAX;
    for part in polyline.split(';') {
        let mut it = part.split(',');
        let (Some(a), Some(b)) = (it.next(), it.next()) else {
            continue;
        };
        let Ok(plon) = a.parse::<f64>() else {
            continue;
        };
        let Ok(plat) = b.parse::<f64>() else {
            continue;
        };
        best = best.min(haversine_m(lat, lon, plat, plon));
    }
    best
}

fn wetland_flags(report: &str) -> (bool, usize, usize) {
    let mut pack_hit = false;
    let mut kept = 0usize;
    let mut hard = 0usize;
    for line in report.lines() {
        if line.contains("wetland_pack_hit=true") {
            pack_hit = true;
        }
        for tok in line.split([';', ' ']) {
            let t = tok.trim();
            if let Some(v) = t.strip_prefix("wetland_boardwalk_kept=") {
                kept = v.parse().unwrap_or(kept);
            }
            if let Some(v) = t.strip_prefix("wetland_hard_removed=") {
                hard = v.parse().unwrap_or(hard);
            }
        }
    }
    (pack_hit, kept, hard)
}

fn plan(
    pbf: &Path,
    elev: &Path,
    cache: &Path,
    start: (f64, f64, &str),
    end: (f64, f64, &str),
) -> navi::CorridorRouteResult {
    let _ = fs::create_dir_all(cache);
    let wps = format!(
        r#"[{{"name":"{}","lat":{},"lon":{}}},{{"name":"{}","lat":{},"lon":{}}}]"#,
        start.2, start.0, start.1, end.2, end.0, end.1
    );
    plan_hiking_route(
        pbf.display().to_string(),
        elev.display().to_string(),
        cache.display().to_string(),
        wps,
        false,
        false,
    )
}

#[test]
fn brastein_boardwalk_survives_wetland_pack_path() {
    let root = fixture_root();
    let src_pbf = root.join("brastein-boardwalk-corridor.osm.pbf");
    if !src_pbf.is_file() {
        eprintln!("skip: missing {}", src_pbf.display());
        return;
    }

    // Hedmark (~60–62°N / 10–12°E) does not cover Bråstein (~58.80°N / 5.77°E).
    assert!(
        !(60.4..=62.5).contains(&58.797) || !(9.8..=12.5).contains(&5.7666),
        "sanity: Bråstein is outside Hedmark"
    );

    let work = root.join("brastein-indexed-packs");
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(&work).unwrap();
    let pbf = work.join("brastein-boardwalk-corridor.osm.pbf");
    fs::copy(&src_pbf, &pbf).unwrap();
    let elev = root.join("elevation-empty");
    let _ = fs::create_dir_all(&elev);

    let mut opts = ConvertOptions::new(&work, &pbf);
    opts.elev_dir = Some(elev.clone());
    opts.profiles = vec![RoutingProfile::Foot];
    let conv = convert_region_packs(&opts).expect("convert");
    assert!(conv.wetland_rings > 0);
    eprintln!(
        "convert ok wetland_rings={} file={}",
        conv.wetland_rings, conv.wetland_file
    );

    let bbox = [58.50_f64, 5.45, 59.10, 6.08];
    let from_pbf = WetlandIndex::load_from_pbf_bbox(&pbf, bbox).unwrap();
    let from_pack = load_wetland_pack(&work.join(&conv.wetland_file), Some(bbox)).unwrap();
    assert_eq!(from_pbf.ring_count(), from_pack.ring_count());

    let foot_name = conv
        .graph_files
        .iter()
        .find(|n| n.contains("foot"))
        .expect("foot graph pack");
    let mut g_pbf = RouteGraph::build_from_pbf_bbox(&pbf, RoutingProfile::Foot, bbox).unwrap();
    let mut g_pack =
        load_graph_pack_bbox(&work.join(foot_name), RoutingProfile::Foot, Some(bbox)).unwrap();
    assert!(
        g_pack.edges.iter().any(|e| e.is_boardwalk_crossing),
        "graph pack must keep is_boardwalk_crossing for way 688185327"
    );
    let s_pbf = g_pbf.apply_wetland_hazards(&from_pbf);
    let s_pack = g_pack.apply_wetland_hazards(&from_pack);
    assert_eq!(s_pbf.boardwalk_kept, s_pack.boardwalk_kept);
    assert_eq!(s_pbf.hard_removed, s_pack.hard_removed);
    assert!(
        s_pack.boardwalk_kept >= 2,
        "boardwalk_kept expected >=2, got {}",
        s_pack.boardwalk_kept
    );
    assert!(
        s_pack.hard_removed >= 1,
        "hard_removed expected >=1, got {}",
        s_pack.hard_removed
    );
    eprintln!(
        "apply identity: soft={} hard={} boardwalk_kept={}",
        s_pack.soft_penalized, s_pack.hard_removed, s_pack.boardwalk_kept
    );

    // Positive: Heia → boardwalk tip (original methodology).
    let tip = (58.797122_f64, 5.7659694_f64);
    let mid = (58.796988_f64, 5.766573_f64);
    let r_pos = plan(
        &pbf,
        &elev,
        &work.join("cache-pos"),
        (58.796440, 5.746265, "Heia"),
        (tip.0, tip.1, "Boardwalk tip (way 688185327)"),
    );
    assert!(
        !r_pos.report.contains("FAIL:"),
        "positive plan FAIL: {}",
        r_pos.report
    );
    let (pack_hit, kept, hard) = wetland_flags(&r_pos.report);
    assert!(pack_hit, "expected wetland_pack_hit=true: {}", r_pos.report);
    assert!(
        kept >= 2,
        "positive wetland_boardwalk_kept expected >=2, got {kept}; report={}",
        r_pos.report
    );
    assert!(
        hard >= 1,
        "positive wetland_hard_removed expected >=1, got {hard}"
    );
    let tip_m = polyline_min_dist(&r_pos.route_polyline, tip.0, tip.1);
    let mid_m = polyline_min_dist(&r_pos.route_polyline, mid.0, mid.1);
    assert!(
        tip_m < 25.0,
        "route must reach boardwalk tip; tip_min_m={tip_m:.1}"
    );
    eprintln!(
        "POSITIVE pack-path: kept={kept} hard={hard} tip_m={tip_m:.1} mid_m={mid_m:.1} dist_km={:.3}",
        r_pos.distance_km
    );

    // Negative: Møgedal → Bråstein (east of reedbed).
    let r_neg = plan(
        &pbf,
        &elev,
        &work.join("cache-neg"),
        (58.79338, 5.775218, "Møgedal"),
        (58.799763, 5.783018, "Bråstein"),
    );
    assert!(
        !r_neg.report.contains("FAIL:"),
        "negative plan FAIL: {}",
        r_neg.report
    );
    let (pack_hit_n, kept_n, hard_n) = wetland_flags(&r_neg.report);
    assert!(pack_hit_n, "negative expected wetland_pack_hit=true");
    // Global apply still sees the boardwalk edges in the trip graph.
    assert!(
        kept_n >= 2,
        "negative still reports boardwalk_kept>={kept_n}"
    );
    assert!(hard_n >= 1, "negative still reports hard_removed");
    let tip_m_n = polyline_min_dist(&r_neg.route_polyline, tip.0, tip.1);
    assert!(
        tip_m_n > 80.0,
        "negative control must not need boardwalk tip; tip_min_m={tip_m_n:.1}"
    );
    eprintln!(
        "NEGATIVE pack-path: kept={kept_n} hard={hard_n} tip_m={tip_m_n:.1} dist_km={:.3}",
        r_neg.distance_km
    );
}
