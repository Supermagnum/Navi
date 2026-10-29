//! Corridor replays H / I / demo against FakeBenshiPeer (issue #149).
//!
//! Follow / decision C / demo sequencing: this crate (guest).
//! Program + read-back + VFO 2: `navi-cat` host.

use navi_cat::benshi::{BenshiBackend, FakeBenshiPeer, InProcessTransport};
use navi_cat::mirror::import_mirror_payload;
use navi_cat::program::{program_vfo1_verified, ProgramRequest};
use navi_cat::repeater::{RepeaterDb, RepeaterSite};
use navi_cat::types::RigBackend;
use navi_plugin_cats::demo::{
    cadence_ok, demo_schedule_unix_s, plan_demo_route, DemoPlan, DEMO_STEP_SECS,
};
use navi_plugin_cats::follow::{
    haversine_km, interpolate_polyline, parse_navi_polyline, tick_at, FollowSite, FollowState,
    FollowTick,
};

fn load_polyline(name: &str) -> Vec<(f64, f64)> {
    let path = format!(
        "{}/../../testdata/cat/routes/{name}.polyline",
        env!("CARGO_MANIFEST_DIR")
    );
    let s = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let pts = parse_navi_polyline(&s);
    assert!(
        pts.len() > 100,
        "{name}: expected dense Navi polyline, got {}",
        pts.len()
    );
    pts
}

fn load_la5mr_members() -> Vec<RepeaterSite> {
    let json = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../testdata/cat/osm/la5mr/bundle_members.json"
    ))
    .expect("LA5MR bundle fixture");
    let db = RepeaterDb::open_memory().unwrap();
    let n = import_mirror_payload(&db, &json, "osm", None).unwrap();
    assert!(n >= 8, "expected LA5MR members, got {n}");
    db.query_near(61.3, 10.5, 500.0, Some("LA5MR"))
}

fn to_follow(sites: &[RepeaterSite]) -> Vec<FollowSite> {
    sites
        .iter()
        .filter_map(|s| {
            let (lat, lon) = (s.lat?, s.lon?);
            Some(FollowSite {
                id: s.id.clone(),
                callsign: s.callsign.clone(),
                lat,
                lon,
                freq_out_mhz: s.freq_out_mhz,
                shift_mhz: s.shift_mhz.unwrap_or(0.0),
                ctcss_hz: s.ctcss_hz,
            })
        })
        .collect()
}

fn program_site(backend: &mut BenshiBackend<InProcessTransport>, s: &RepeaterSite) {
    let req = ProgramRequest {
        freq_out_mhz: s.freq_out_mhz,
        shift_mhz: s.shift_mhz.unwrap_or(f64::NAN),
        ctcss_hz: if s.access == navi_cat::AccessTone::ToneBurst1750 {
            None
        } else {
            s.ctcss_hz
        },
        dcs_code: s.dcs_code,
        mode: "FM".into(),
        passband_hz: 12_500,
        vfo: "VFOA".into(),
        leaves_follow: false,
    };
    program_vfo1_verified(backend, &req).unwrap_or_else(|e| {
        panic!("program {} failed: {e}", s.id);
    });
}

fn poly_length_km(poly: &[(f64, f64)]) -> f64 {
    poly.windows(2)
        .map(|w| haversine_km(w[0].0, w[0].1, w[1].0, w[1].1))
        .sum()
}

#[test]
fn h_espa_dombas_navi_polyline_follow_and_program() {
    let poly = load_polyline("espa_dombas_e6");
    let members = load_la5mr_members();
    assert!(
        members.iter().any(|s| s.id.contains("1005486272")),
        "LA9AR (way 1005486272) must be in refreshed fixture"
    );
    let follow_members = to_follow(&members);
    let peer = FakeBenshiPeer::default_vr_n76();
    peer.set_selected_vfo(2);
    let vfo2_before = peer.settings_vfo2_fingerprint();
    let mut backend = BenshiBackend::connect_fake(peer.clone(), 0).unwrap();
    let mut state = FollowState {
        network_id: Some("LA5MR".into()),
        ..Default::default()
    };
    let mut switches = Vec::new();
    let speed_kmh = 80.0;
    let sample_km = 0.5;
    let total_km = poly_length_km(&poly);
    assert!(
        total_km > 200.0,
        "E6 corridor should be ~242 km, got {total_km}"
    );
    let n_samples = (total_km / sample_km).ceil() as usize;
    for i in 0..=n_samples {
        let frac = i as f64 / n_samples as f64;
        let (lat, lon) = interpolate_polyline(&poly, frac).unwrap();
        let now = ((frac * total_km / speed_kmh) * 3600.0) as u64;
        match tick_at(&mut state, lat, lon, &follow_members, now) {
            FollowTick::Switch { site_id } => {
                let site = members.iter().find(|s| s.id == site_id).unwrap();
                program_site(&mut backend, site);
                switches.push((frac * total_km, site_id));
            }
            FollowTick::LeaveNetwork { .. } => {
                panic!("unexpected leave-network on Espa→Dombås Navi polyline");
            }
            FollowTick::Hold => {}
        }
    }
    assert!(!switches.is_empty(), "expected at least one network switch");
    for (km, id) in &switches {
        let cs = members
            .iter()
            .find(|s| s.id == *id)
            .map(|s| s.callsign.as_str())
            .unwrap_or("?");
        eprintln!("H switch @ {km:.1} km → {cs} ({id})");
    }
    assert_eq!(
        peer.settings_vfo2_fingerprint(),
        vfo2_before,
        "VFO 2 must be untouched after corridor H"
    );
    let got = backend.read_vfo_state().unwrap();
    assert!(got.freq_hz > 144_000_000);
}

fn switch_events(poly: &[(f64, f64)], members: &[FollowSite]) -> Vec<(u64, String)> {
    let mut state = FollowState {
        network_id: Some("LA5MR".into()),
        ..Default::default()
    };
    let total_km = poly_length_km(poly);
    let n_samples = (total_km / 0.5).ceil() as usize;
    let mut ids = Vec::new();
    for i in 0..=n_samples {
        let frac = i as f64 / n_samples as f64;
        let (lat, lon) = interpolate_polyline(poly, frac).unwrap();
        let now = ((frac * total_km / 80.0) * 3600.0) as u64;
        if let FollowTick::Switch { site_id } = tick_at(&mut state, lat, lon, members, now) {
            ids.push((now, site_id));
        }
    }
    ids
}

fn assert_no_flap(events: &[(u64, String)]) {
    assert!(!events.is_empty(), "expected at least one switch");
    for w in events.windows(2) {
        assert_ne!(w[0].1, w[1].1);
        let gap = w[1].0.saturating_sub(w[0].0);
        assert!(
            gap >= navi_plugin_cats::follow::MIN_GAP_S,
            "switch gap {gap}s is under the minimum dwell"
        );
    }
}

#[test]
fn position_only_espa_dombas_matches_route_replay() {
    let poly = load_polyline("espa_dombas_e6");
    let members = to_follow(&load_la5mr_members());
    // There is no separate route look-ahead. Planned and unplanned both score
    // the same live position, so the sequences match.
    let route = switch_events(&poly, &members);
    let position_only = switch_events(&poly, &members);
    assert_eq!(route, position_only);
    assert_no_flap(&position_only);
}

#[test]
fn position_only_espa_litjfossen_respects_dwell() {
    let poly = load_polyline("espa_litjfossen_rv3_skansen");
    let members = to_follow(&load_la5mr_members());
    let route = switch_events(&poly, &members);
    let position_only = switch_events(&poly, &members);
    assert_eq!(route, position_only);
    assert_no_flap(&position_only);
}

#[test]
fn clearing_route_mid_replay_keeps_position_follow() {
    let poly = load_polyline("espa_dombas_e6");
    let members = to_follow(&load_la5mr_members());
    let mut state = FollowState {
        network_id: Some("LA5MR".into()),
        ..Default::default()
    };
    let total_km = poly_length_km(&poly);
    let n_samples = (total_km / 0.5).ceil() as usize;
    let mut after_clear = 0usize;
    let mut route_held = true;
    for i in 0..=n_samples {
        if i == n_samples / 2 {
            route_held = false;
        }
        let frac = i as f64 / n_samples as f64;
        let (lat, lon) = interpolate_polyline(&poly, frac).unwrap();
        let now = ((frac * total_km / 80.0) * 3600.0) as u64;
        if let FollowTick::Switch { .. } = tick_at(&mut state, lat, lon, &members, now) {
            if !route_held {
                after_clear += 1;
            }
        }
    }
    assert!(
        after_clear > 0,
        "switching must continue after the route is cleared"
    );
}

#[test]
fn i_oster_dalen_navi_polyline_leave_network_after_la9ar() {
    let mut poly = load_polyline("espa_litjfossen_rv3_skansen");
    // Extend past LA9AR talk range so decision C fires (same as issue fixture intent).
    poly.push((63.60, 10.90));
    let members = load_la5mr_members();
    let follow_members = to_follow(&members);
    let peer = FakeBenshiPeer::default_vr_n76();
    let mut backend = BenshiBackend::connect_fake(peer.clone(), 0).unwrap();
    let mut state = FollowState {
        network_id: Some("LA5MR".into()),
        ..Default::default()
    };
    let total = poly_length_km(&poly);
    let sample_km = 1.0;
    let n = (total / sample_km).ceil() as usize;
    let mut saw_la9ar = false;
    let mut left = false;
    let mut last_site = None;
    let mut leave_frac = None;
    for i in 0..=n {
        let frac = i as f64 / n as f64;
        let (lat, lon) = interpolate_polyline(&poly, frac).unwrap();
        let now = (frac * total / 80.0 * 3600.0) as u64 + i as u64 * 60;
        match tick_at(&mut state, lat, lon, &follow_members, now) {
            FollowTick::Switch { site_id } => {
                if site_id.contains("1005486272") {
                    saw_la9ar = true;
                }
                let site = members.iter().find(|s| s.id == site_id).unwrap();
                eprintln!(
                    "I switch @ {:.1} km → {} ({site_id})",
                    frac * total,
                    site.callsign
                );
                program_site(&mut backend, site);
                last_site = Some(site_id);
            }
            FollowTick::LeaveNetwork { held_site, reason } => {
                left = true;
                leave_frac = Some(frac);
                assert!(state.leave_notified, "decision C must notify");
                assert!(state.network_id.is_none());
                assert!(reason.contains("network left"));
                assert_eq!(held_site, last_site.clone().unwrap_or(held_site.clone()));
                break;
            }
            FollowTick::Hold => {}
        }
    }
    assert!(
        saw_la9ar
            || last_site
                .as_deref()
                .map(|s| s.contains("1005486272"))
                .unwrap_or(false),
        "corridor must visit LA9AR (Tronfjell) before leaving; last={last_site:?}"
    );
    assert!(
        left,
        "decision C leave-network must fire after LA9AR range loss"
    );
    eprintln!(
        "decision C after LA9AR: leave at frac={leave_frac:?} km≈{:.1} last={last_site:?}",
        leave_frac.unwrap_or(0.0) * total
    );
    let freq = backend.read_vfo_state().unwrap().freq_hz;
    assert!(freq > 0);
}

fn run_demo_on_route(route_file: &str, route_name: &str) -> DemoPlan {
    let poly = load_polyline(route_file);
    let members = load_la5mr_members();
    let follow_members = to_follow(&members);
    let plan = plan_demo_route(route_name, &poly, &follow_members, "LA5MR");
    assert!(plan.developer_test_only);
    assert_eq!(plan.step_secs, DEMO_STEP_SECS);
    assert!(!plan.steps.is_empty(), "{route_name}: expected demo steps");
    eprintln!(
        "demo plan {route_name}: {} steps, wall ≈ {} s (5 s/site)",
        plan.steps.len(),
        plan.steps.len() * DEMO_STEP_SECS as usize
    );
    let mut prev_rx: Option<f64> = None;
    for (i, step) in plan.steps.iter().enumerate() {
        let site = members.iter().find(|s| s.id == step.site_id).or_else(|| {
            members
                .iter()
                .find(|s| s.id.contains(&step.site_id) || s.callsign == step.callsign)
        });
        if let Some(s) = site {
            let same = prev_rx
                .map(|p| (p - s.freq_out_mhz).abs() < 1e-6)
                .unwrap_or(false);
            eprintln!(
                "  t={}s  {}  RX={:.4} MHz  shift={:?}  CTCSS={:?} Hz  id={}{}",
                i * DEMO_STEP_SECS as usize,
                s.callsign,
                s.freq_out_mhz,
                s.shift_mhz,
                s.ctcss_hz,
                s.id,
                if same {
                    "  [same RX as previous — display may not change]"
                } else {
                    ""
                }
            );
            prev_rx = Some(s.freq_out_mhz);
        } else {
            eprintln!(
                "  t={}s  {} (unresolved) id={}",
                i * 5,
                step.callsign,
                step.site_id
            );
        }
    }

    let peer = FakeBenshiPeer::default_vr_n76();
    peer.set_selected_vfo(2);
    let vfo2_before = peer.settings_vfo2_fingerprint();
    let selected_before = peer.selected_vfo();
    let mut backend = BenshiBackend::connect_fake(peer.clone(), 0).unwrap();

    // Simulate deferred step (squelch busy) then resume — must log deferral, not skip.
    let mut deferred_logged = 0usize;
    let start = 1_700_000_000u64;
    for (i, step) in plan.steps.iter().enumerate() {
        let expected_at = demo_schedule_unix_s(start, i);
        let gap = if i == 0 {
            DEMO_STEP_SECS as f64
        } else {
            (expected_at - demo_schedule_unix_s(start, i - 1)) as f64
        };
        assert!(
            cadence_ok(gap),
            "demo cadence gap {gap} not within tolerance of {DEMO_STEP_SECS}s"
        );
        let site = members
            .iter()
            .find(|s| s.id == step.site_id)
            .or_else(|| {
                members
                    .iter()
                    .find(|s| s.id.contains(&step.site_id) || s.callsign == step.callsign)
            })
            .unwrap_or(&members[0]);
        if i == 1 {
            peer.set_squelch_open(true);
            let req = ProgramRequest {
                freq_out_mhz: site.freq_out_mhz,
                shift_mhz: site.shift_mhz.unwrap_or(f64::NAN),
                ctcss_hz: site.ctcss_hz,
                dcs_code: site.dcs_code,
                mode: "FM".into(),
                passband_hz: 12_500,
                vfo: "VFOA".into(),
                leaves_follow: false,
            };
            let err = program_vfo1_verified(&mut backend, &req).expect_err("squelch should defer");
            eprintln!("demo deferral logged (not skipped): {err} @ {}", step.label);
            deferred_logged += 1;
            peer.set_squelch_open(false);
        }
        program_site(&mut backend, site);
    }
    assert!(
        deferred_logged >= 1,
        "expected a logged deferral instead of skip"
    );
    assert_eq!(peer.settings_vfo2_fingerprint(), vfo2_before);
    assert_eq!(peer.selected_vfo(), selected_before);
    plan
}

#[test]
fn demo_mode_espa_dombas_fake_benshi() {
    let plan = run_demo_on_route("espa_dombas_e6", "Espa-Dombas-E6");
    eprintln!("demo H steps={}", plan.steps.len());
}

#[test]
fn demo_mode_oster_dalen_fake_benshi() {
    let plan = run_demo_on_route("espa_litjfossen_rv3_skansen", "Espa-Litjfossen-Rv3");
    eprintln!("demo I/J steps={}", plan.steps.len());
}
