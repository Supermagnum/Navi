//! Coverage score J: talk-coverage along Navi car-router corridors (issue #149).

use navi_cat::coverage::{
    interpolate_polyline, parse_navi_polyline, score_route_talk_coverage,
    talk_polygons_by_callsign, CoverageScore,
};
use navi_cat::mirror::import_mirror_payload;
use navi_cat::repeater::{RepeaterDb, RepeaterSite};
use navi_plugin_cats::follow::{haversine_km, tick_at, FollowSite, FollowState, FollowTick};

fn load_polyline(name: &str) -> Vec<(f64, f64)> {
    let path = format!(
        "{}/../../testdata/cat/routes/{name}.polyline",
        env!("CARGO_MANIFEST_DIR")
    );
    let s = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let pts = parse_navi_polyline(&s);
    assert!(pts.len() > 100, "{name}: expected dense Navi polyline");
    pts
}

fn load_members() -> Vec<RepeaterSite> {
    let json = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../testdata/cat/osm/la5mr/bundle_members.json"
    ))
    .unwrap();
    let db = RepeaterDb::open_memory().unwrap();
    import_mirror_payload(&db, &json, "osm", None).unwrap();
    db.query_near(61.3, 10.5, 500.0, Some("LA5MR"))
}

fn to_follow(sites: &[RepeaterSite]) -> Vec<FollowSite> {
    sites
        .iter()
        .filter_map(|s| {
            Some(FollowSite {
                id: s.id.clone(),
                callsign: s.callsign.clone(),
                lat: s.lat?,
                lon: s.lon?,
                freq_out_mhz: s.freq_out_mhz,
                shift_mhz: s.shift_mhz.unwrap_or(0.0),
                ctcss_hz: s.ctcss_hz,
            })
        })
        .collect()
}

fn poly_len_km(poly: &[(f64, f64)]) -> f64 {
    poly.windows(2)
        .map(|w| haversine_km(w[0].0, w[0].1, w[1].0, w[1].1))
        .sum()
}

fn score_corridor(poly: &[(f64, f64)], members: &[RepeaterSite], geojson: &str) -> CoverageScore {
    let talk = talk_polygons_by_callsign(geojson).expect("parse talk geojson");
    let follow_members = to_follow(members);
    let mut state = FollowState {
        network_id: Some("LA5MR".into()),
        ..Default::default()
    };
    let total = poly_len_km(poly);
    let sample_km = 1.0;
    let n = (total / sample_km).ceil() as usize;
    let mut samples = Vec::new();
    let mut selected: Vec<Option<String>> = Vec::new();
    for i in 0..=n {
        let frac = i as f64 / n as f64;
        let (lat, lon) = interpolate_polyline(poly, frac).unwrap();
        let now = (frac * total / 80.0 * 3600.0) as u64 + i as u64 * 60;
        match tick_at(&mut state, lat, lon, &follow_members, now) {
            FollowTick::Switch { site_id } => {
                state.current_site = Some(site_id);
            }
            FollowTick::LeaveNetwork { .. } => {}
            FollowTick::Hold => {}
        }
        samples.push((lat, lon));
        selected.push(state.current_site.clone());
    }
    let selected_refs: Vec<Option<&str>> = selected.iter().map(|s| s.as_deref()).collect();
    score_route_talk_coverage(&samples, &selected_refs, members, &talk)
}

fn reduced_geojson() -> String {
    std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../testdata/cat/splat/LA5MR_talk_corridor_reduced.geojson"
    ))
    .expect("reduced SPLAT fixture")
}

#[test]
fn j_espa_dombas_navi_polyline_talk_coverage_ge_85() {
    let poly = load_polyline("espa_dombas_e6");
    let members = load_members();
    let score = score_corridor(&poly, &members, &reduced_geojson());
    eprintln!(
        "J Espa→Dombås: {:.1}% switches={} any_talk={}",
        score.percent, score.switches, score.samples_with_any_talk
    );
    assert!(
        score.samples_with_any_talk >= 50,
        "expected substantial talk samples"
    );
    assert!(
        score.percent >= 85.0,
        "talk coverage {:.1}% < 85%",
        score.percent
    );
}

#[test]
fn j_oster_dalen_navi_polyline_talk_coverage_ge_85() {
    let poly = load_polyline("espa_litjfossen_rv3_skansen");
    let members = load_members();
    let score = score_corridor(&poly, &members, &reduced_geojson());
    eprintln!(
        "J Østerdalen (Rv 3 / Skansen): {:.1}% switches={} any_talk={}",
        score.percent, score.switches, score.samples_with_any_talk
    );
    assert!(
        score.samples_with_any_talk >= 50,
        "expected substantial talk samples on Østerdalen"
    );
    assert!(
        score.percent >= 85.0,
        "Østerdalen talk coverage {:.1}% < 85%",
        score.percent
    );
}

/// When `CAT_SPLAT_GEOJSON` points at the full extract, reduced fixture must
/// match within ±2.0 percentage points (and same switch count if both ≥50 samples).
#[test]
fn coverage_j_reduced_matches_full_within_tolerance() {
    let Ok(full_path) = std::env::var("CAT_SPLAT_GEOJSON") else {
        eprintln!("CAT_SPLAT_GEOJSON unset — skipping full-vs-reduced parity");
        return;
    };
    let full = std::fs::read_to_string(&full_path).expect("full SPLAT");
    let reduced = reduced_geojson();
    let members = load_members();
    for name in ["espa_dombas_e6", "espa_litjfossen_rv3_skansen"] {
        let poly = load_polyline(name);
        let s_full = score_corridor(&poly, &members, &full);
        let s_red = score_corridor(&poly, &members, &reduced);
        eprintln!(
            "parity {name}: full={:.2}%/{} red={:.2}%/{}",
            s_full.percent, s_full.switches, s_red.percent, s_red.switches
        );
        assert!(
            s_full.samples_with_any_talk >= 50 && s_red.samples_with_any_talk >= 50,
            "{name}: need ≥50 any-talk samples on both"
        );
        assert!(
            (s_full.percent - s_red.percent).abs() <= 2.0,
            "{name}: percent delta {:.2} exceeds ±2.0",
            (s_full.percent - s_red.percent).abs()
        );
        assert_eq!(
            s_full.switches, s_red.switches,
            "{name}: switch count must match when both runs are well populated"
        );
    }
}
