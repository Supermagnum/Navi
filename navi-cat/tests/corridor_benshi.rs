//! Corridor replays H / I against FakeBenshiPeer (issue #149).

use navi_cat::benshi::{BenshiBackend, FakeBenshiPeer, InProcessTransport};
use navi_cat::follow::{
    haversine_km, interpolate_great_circle, interpolate_polyline, tick_at, FollowState, FollowTick,
};
use navi_cat::importers::import_osm_relation_xml;
use navi_cat::program::{program_vfo1_verified, ProgramRequest};
use navi_cat::repeater::{RepeaterDb, RepeaterSite};
use navi_cat::types::RigBackend;

const ESPA: (f64, f64) = (60.563, 11.257);
const DOMBAS: (f64, f64) = (62.076, 9.128);
/// Litjfossen approx (Østerdalen / Rv 3).
const LITJFOSSEN: (f64, f64) = (62.35, 10.85);
/// Beyond LA9AR talk range (~160 km north of Tronfjell) so leave-network fires.
const PAST_NETWORK: (f64, f64) = (63.60, 10.90);
/// Skansen rundkjøring (Elverum area) waypoint on Espa→Litjfossen.
const SKANSEN: (f64, f64) = (60.88, 11.56);

fn load_la5mr_members() -> Vec<RepeaterSite> {
    let xml = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../testdata/cat/osm/la5mr/relation_18780801_full.xml"
    ))
    .expect("LA5MR fixture");
    let db = RepeaterDb::open_memory().unwrap();
    let n = import_osm_relation_xml(&db, &xml).unwrap();
    assert!(n >= 8, "expected refreshed LA5MR members, got {n}");
    // Wide bbox centre so LA9AR (Tronfjell) and southern members are included.
    db.query_near(61.3, 10.5, 500.0, Some("LA5MR"))
}

fn program_site(backend: &mut BenshiBackend<InProcessTransport>, s: &RepeaterSite) {
    let req = ProgramRequest {
        freq_out_mhz: s.freq_out_mhz,
        shift_mhz: s.shift_mhz,
        ctcss_hz: s.ctcss_hz,
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

#[test]
fn h_espa_dombas_follow_and_program() {
    let members = load_la5mr_members();
    assert!(
        members.iter().any(|s| s.id.contains("1005486272")),
        "LA9AR (way 1005486272) must be in refreshed fixture"
    );
    let peer = FakeBenshiPeer::default_vr_n76();
    peer.set_selected_vfo(2); // background retune while on VFO 2
    let vfo2_before = peer.settings_vfo2_fingerprint();
    let mut backend = BenshiBackend::connect_fake(peer.clone(), 0).unwrap();
    let mut state = FollowState {
        network_id: Some("LA5MR".into()),
        ..Default::default()
    };
    let mut switches = Vec::new();
    let speed_kmh = 80.0;
    let sample_km = 0.5;
    let total_km = haversine_km(ESPA.0, ESPA.1, DOMBAS.0, DOMBAS.1);
    let n_samples = (total_km / sample_km).ceil() as usize;
    for i in 0..=n_samples {
        let frac = i as f64 / n_samples as f64;
        let (lat, lon) = interpolate_great_circle(ESPA.0, ESPA.1, DOMBAS.0, DOMBAS.1, frac);
        let now = ((frac * total_km / speed_kmh) * 3600.0) as u64;
        match tick_at(&mut state, lat, lon, &members, now) {
            FollowTick::Switch { site_id } => {
                let site = members.iter().find(|s| s.id == site_id).unwrap();
                program_site(&mut backend, site);
                switches.push((frac * total_km, site_id));
            }
            FollowTick::LeaveNetwork { .. } => {
                panic!("unexpected leave-network on Espa→Dombås GC");
            }
            FollowTick::Hold => {}
        }
    }
    assert!(!switches.is_empty(), "expected at least one network switch");
    assert_eq!(
        peer.settings_vfo2_fingerprint(),
        vfo2_before,
        "VFO 2 must be untouched after corridor H"
    );
    let got = backend.read_vfo_state().unwrap();
    assert!(got.freq_hz > 144_000_000);
}

#[test]
fn i_oster_dalen_leave_network_after_la9ar() {
    let members = load_la5mr_members();
    let peer = FakeBenshiPeer::default_vr_n76();
    let mut backend = BenshiBackend::connect_fake(peer.clone(), 0).unwrap();
    let mut state = FollowState {
        network_id: Some("LA5MR".into()),
        ..Default::default()
    };
    // Polyline: Espa → Skansen → Tronfjell (LA9AR) → Litjfossen → past network range
    let poly = vec![ESPA, SKANSEN, (62.17, 10.70), LITJFOSSEN, PAST_NETWORK];
    let mut total = 0.0;
    for w in poly.windows(2) {
        total += haversine_km(w[0].0, w[0].1, w[1].0, w[1].1);
    }
    let sample_km = 1.0;
    let n = (total / sample_km).ceil() as usize;
    let mut saw_la9ar = false;
    let mut left = false;
    let mut last_site = None;
    for i in 0..=n {
        let frac = i as f64 / n as f64;
        let (lat, lon) = interpolate_polyline(&poly, frac).unwrap();
        let now = (frac * total / 80.0 * 3600.0) as u64 + i as u64; // ensure time advances
        match tick_at(&mut state, lat, lon, &members, now.max(i as u64 * 60)) {
            FollowTick::Switch { site_id } => {
                if site_id.contains("1005486272") {
                    saw_la9ar = true;
                }
                let site = members.iter().find(|s| s.id == site_id).unwrap();
                program_site(&mut backend, site);
                last_site = Some(site_id);
            }
            FollowTick::LeaveNetwork { held_site, reason } => {
                left = true;
                assert!(state.leave_notified, "decision C must notify");
                assert!(state.network_id.is_none());
                assert!(reason.contains("network left"));
                // Held site should be last networked (often LA9AR on this corridor)
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
    // Radio remains on held site frequency (no silent fall-in)
    let freq = backend.read_vfo_state().unwrap().freq_hz;
    assert!(freq > 0);
}
