//! Host dry run of the Espa -> Dombas follow at 12x and 20x route simulation.
//!
//! Drives the same path the app uses, with a fake real-time clock and no sleeping:
//! host `status_json_at` (follow clock) -> host `repeater_query_json` (rig and
//! band-plan gating) -> guest row parsing / `select_members` / `tick_at`
//! (dwell, gap, hysteresis). Tuned sites are then programmed through the host
//! `vfo_set_json` onto the VR-N76 lab peer.
//!
//! The fixture databases are real (`non_networked.json`, LA5MR bundle). Three
//! DMR-only RadioID rows are added test-locally on the route; they would win
//! the nearest-site contest if the host did not skip them. The LA5MR bundle
//! already carries the LA2TRR row with the 287.9375 MHz shift.

use navi_cat::benshi::{BenshiBackend, FakeBenshiPeer, InProcessTransport};
use navi_cat::importers::import_osm_json;
use navi_cat::mirror::import_mirror_payload;
use navi_cat::program::ProgramRequest;
use navi_cat::repeater::{AccessTone, RepeaterDb, RepeaterSite, RepeaterSource};
use navi_cat::CatService;
use navi_plugin_cats::follow::{
    haversine_km, interpolate_polyline, live_members_from_rows, select_members, tick_at,
    FollowState, FollowTick,
};

type Svc = CatService<BenshiBackend<InProcessTransport>>;

const ESPA: (f64, f64) = (60.563, 11.257);
const DOMBAS: (f64, f64) = (62.076, 9.128);
const SPEED_KMH: f64 = 80.0;
/// The app follow tick runs every 5 s of real time.
const APP_TICK_REAL_S: f64 = 5.0;
const REAL_T0: f64 = 1_800_000_000.0;

fn testdata(rel: &str) -> String {
    let path = format!("{}/../../testdata/cat/{rel}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

fn dmr_row(id: &str, callsign: &str, freq: f64, shift: f64, along: f64) -> RepeaterSite {
    let (lat, lon) = interpolate_polyline(&[ESPA, DOMBAS], along).unwrap();
    RepeaterSite {
        id: id.into(),
        callsign: callsign.into(),
        lat: Some(lat),
        lon: Some(lon),
        freq_out_mhz: freq,
        shift_mhz: Some(shift),
        ctcss_hz: None,
        dcs_code: None,
        access: AccessTone::Unknown,
        color_code: Some(1),
        modulation: "DMR".into(),
        network_id: None,
        source: RepeaterSource::RadioId,
        conflict: false,
        conflict_note: None,
        distance_km: None,
        position_accurate: true,
        exclude_recommended: false,
        attribution: Some("Data courtesy of RadioID.net".into()),
        country: Some("NO".into()),
        file_kind: Some("radioid".into()),
        original_lat: None,
        original_lon: None,
        relocated: false,
        relocation_note: None,
    }
}

fn build_db() -> RepeaterDb {
    let db = RepeaterDb::open_memory().unwrap();
    import_osm_json(&db, &testdata("non_networked.json")).unwrap();
    import_mirror_payload(&db, &testdata("osm/la5mr/bundle_members.json"), "osm", None).unwrap();
    // Same RX/shift values as the dry run. Positions are on the route so each
    // one is the nearest site around its position when not skipped.
    for row in [
        dmr_row("radioid-ld2hj", "LD2HJ", 434.700, -2.0, 0.20),
        dmr_row("radioid-la2lrr", "LA2LRR", 434.700, -2.0, 0.55),
        dmr_row("radioid-la2l", "LA2L", 433.775, -1.6, 0.85),
    ] {
        db.upsert_site(&row).unwrap();
    }
    db
}

fn service_with(db: RepeaterDb) -> Svc {
    let backend = BenshiBackend::connect_fake(FakeBenshiPeer::default_vr_n76(), 0).unwrap();
    CatService::new(backend, db)
}

fn service() -> Svc {
    service_with(build_db())
}

fn route_km() -> f64 {
    haversine_km(ESPA.0, ESPA.1, DOMBAS.0, DOMBAS.1)
}

struct Switch {
    km: f64,
    sim_s: u64,
    id: String,
    callsign: String,
}

/// Replay the route. `playback` is how many times faster than real driving the
/// app plays it. `tell_host_scale` is whether the app passes that scale to the
/// follow clock (the fix) or leaves it at 1.0 (the old behaviour). `tick_real_s`
/// is the real time between follow ticks (the app uses 5 s).
fn replay(
    svc: &mut Svc,
    choice: &str,
    playback: f64,
    tell_host_scale: bool,
    tick_real_s: f64,
    program: bool,
) -> Vec<Switch> {
    svc.network_follow_json(&format!(r#"{{"enabled":true,"network_id":"{choice}"}}"#));
    assert!(svc.set_time_scale_at(if tell_host_scale { playback } else { 1.0 }, REAL_T0));
    // Named follow narrows the host query; auto / standalone query everything.
    let filter = match choice {
        "auto" | "standalone" | "off" => None,
        name => Some(name),
    };
    let total = route_km();
    let mut state = FollowState::default();
    let mut out = Vec::new();
    let mut k = 0u32;
    loop {
        let real = REAL_T0 + f64::from(k) * tick_real_s;
        let sim_elapsed_s = f64::from(k) * tick_real_s * playback;
        let km = SPEED_KMH * sim_elapsed_s / 3600.0;
        if km > total {
            break;
        }
        let (lat, lon) = interpolate_polyline(&[ESPA, DOMBAS], km / total).unwrap();
        let status: serde_json::Value = serde_json::from_str(&svc.status_json_at(real)).unwrap();
        let now = status["now_unix_s"].as_u64().unwrap();
        let rows: Vec<serde_json::Value> =
            serde_json::from_str(&svc.repeater_query_json(lat, lon, 150.0, filter)).unwrap();
        let live = live_members_from_rows(&rows);
        let members = select_members(choice, &live);
        state.network_id = Some(choice.to_string());
        if let FollowTick::Switch { site_id } = tick_at(&mut state, lat, lon, &members, now) {
            let m = members.iter().find(|m| m.id == site_id).unwrap();
            if program {
                let site = svc
                    .db
                    .list_sites()
                    .into_iter()
                    .find(|s| s.id == site_id)
                    .unwrap();
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
                let resp: serde_json::Value =
                    serde_json::from_str(&svc.vfo_set_json(&serde_json::to_string(&req).unwrap()))
                        .unwrap();
                assert_eq!(resp["ok"], true, "program {} failed: {resp}", site.callsign);
            }
            out.push(Switch {
                km,
                sim_s: now - REAL_T0 as u64,
                id: site_id,
                callsign: m.callsign.clone(),
            });
        }
        k += 1;
    }
    out
}

/// Fixture-derived switch km from docs/cat-test.md (80 km/h, GC path).
const RUN_A: [(&str, f64); 4] = [
    ("LA6GR", 0.0),
    ("LA5ARR", 54.0),
    ("LA7GR", 115.5),
    ("LA2JRR", 159.0),
];
const RUN_B: [(&str, f64); 2] = [("1473491257", 0.0), ("1194875539", 112.0)];

fn print_run(name: &str, playback: f64, sw: &[Switch]) {
    eprintln!("{name} @ {playback}x:");
    for s in sw {
        eprintln!(
            "  km={:6.1} sim_t={:5}s {} ({})",
            s.km, s.sim_s, s.callsign, s.id
        );
    }
}

fn assert_matches_docs(
    sw: &[Switch],
    expected: &[(&str, f64)],
    playback: f64,
    tick_real_s: f64,
    by_id: bool,
) {
    // One follow tick spans this many route km; a switch is seen at the first
    // tick after the crossing. The docs table is rounded from a 0.5 km grid.
    let tick_km = SPEED_KMH * tick_real_s * playback / 3600.0;
    let tol = tick_km + 1.0;
    assert_eq!(
        sw.len(),
        expected.len(),
        "switch count differs: got {:?}",
        sw.iter().map(|s| &s.id).collect::<Vec<_>>()
    );
    for (got, (want, want_km)) in sw.iter().zip(expected) {
        let key = if by_id { &got.id } else { &got.callsign };
        assert!(key.contains(want), "expected {want}, got {key}");
        assert!(
            got.km >= want_km - 0.5 && got.km <= want_km + tol,
            "{want}: switched at {:.1} km, docs {want_km} km (+{tol:.1} km tick tolerance)",
            got.km
        );
    }
}

fn assert_never_tuned_skipped_rows(sw: &[Switch]) {
    for s in sw {
        for bad in ["LD2HJ", "LA2LRR", "LA2L", "LA2TRR", "LA2DRR", "LA7KR"] {
            assert_ne!(s.callsign, bad, "{bad} must never be tuned");
        }
    }
}

#[test]
fn skipped_rows_exist_in_the_data_and_would_win_if_not_skipped() {
    let svc = service();
    let all = svc.db.list_sites();
    for call in ["LD2HJ", "LA2LRR", "LA2L", "LA2TRR"] {
        assert!(all.iter().any(|s| s.callsign == call), "{call} not in DB");
    }
    let tr = all.iter().find(|s| s.callsign == "LA2TRR").unwrap();
    assert_eq!(tr.shift_mhz, Some(287.9375));
    // Raw nearest (no host gating) is a DMR row somewhere along the route.
    let mut raw_dmr_nearest = 0;
    for i in 0..=100 {
        let (lat, lon) = interpolate_polyline(&[ESPA, DOMBAS], f64::from(i) / 100.0).unwrap();
        let raw = svc.db.query_near(lat, lon, 150.0, None);
        if raw
            .first()
            .is_some_and(|s| matches!(s.callsign.as_str(), "LD2HJ" | "LA2LRR" | "LA2L"))
        {
            raw_dmr_nearest += 1;
        }
        let gated: Vec<serde_json::Value> =
            serde_json::from_str(&svc.repeater_query_json(lat, lon, 150.0, None)).unwrap();
        for row in &gated {
            let c = row["callsign"].as_str().unwrap();
            assert!(
                !matches!(
                    c,
                    "LD2HJ" | "LA2LRR" | "LA2L" | "LA2TRR" | "LA2DRR" | "LA7KR"
                ),
                "{c} leaked into the follow candidate list"
            );
        }
    }
    assert!(
        raw_dmr_nearest >= 3,
        "test rows must be nearest without gating (got {raw_dmr_nearest})"
    );
}

#[test]
fn run_a_standalone_matches_fixtures_at_12x_and_20x() {
    for playback in [12.0, 20.0] {
        let mut svc = service();
        let sw = replay(
            &mut svc,
            "standalone",
            playback,
            true,
            APP_TICK_REAL_S,
            true,
        );
        print_run("Run A standalone", playback, &sw);
        assert_matches_docs(&sw, &RUN_A, playback, APP_TICK_REAL_S, false);
        assert_never_tuned_skipped_rows(&sw);
    }
}

#[test]
fn run_b_la5mr_network_matches_fixtures_at_12x_and_20x() {
    for playback in [12.0, 20.0] {
        let mut svc = service();
        let sw = replay(&mut svc, "LA5MR", playback, true, APP_TICK_REAL_S, true);
        print_run("Run B LA5MR", playback, &sw);
        assert_matches_docs(&sw, &RUN_B, playback, APP_TICK_REAL_S, true);
        assert_never_tuned_skipped_rows(&sw);
    }
}

#[test]
fn real_time_run_a_matches_docs() {
    let mut svc = service();
    let a = replay(&mut svc, "standalone", 1.0, true, APP_TICK_REAL_S, false);
    print_run("Run A standalone", 1.0, &a);
    assert_matches_docs(&a, &RUN_A, 1.0, APP_TICK_REAL_S, false);
}

/// Real driving (1x, 5 s tick) passes through Bagn (node 2641537344) for 3.4 km
/// before LA6JRR: Bagn clears the hysteresis margin first, then the 30 s dwell
/// and 60 s gap hold LA6JRR back. The docs table (0.5 km grid) lists only the
/// direct LA5MR -> LA6JRR handover, and so does a 12x or 20x run with a 5 s
/// real tick, whose 60 s / 100 s of driving per tick step over the Bagn window.
/// Ticking every 5 s of SIMULATED time (5/12 s and 5/20 s real) reproduces real
/// driving exactly. This test pins both facts.
#[test]
fn run_b_tick_granularity_decides_the_bagn_hop() {
    let mut svc = service();
    let real = replay(&mut svc, "LA5MR", 1.0, true, APP_TICK_REAL_S, false);
    print_run("Run B real driving", 1.0, &real);
    let ids = |v: &[Switch]| v.iter().map(|s| s.id.clone()).collect::<Vec<_>>();
    assert_eq!(
        ids(&real),
        [
            "osm-node-1473491257",
            "osm-node-2641537344",
            "osm-way-1194875539"
        ]
    );

    for playback in [12.0, 20.0] {
        let mut coarse = service();
        let c = replay(&mut coarse, "LA5MR", playback, true, APP_TICK_REAL_S, false);
        assert_eq!(c.len(), 2, "5 s real tick steps over the Bagn window");

        let mut fine = service();
        let f = replay(
            &mut fine,
            "LA5MR",
            playback,
            true,
            APP_TICK_REAL_S / playback,
            false,
        );
        print_run("Run B 5 s simulated tick", playback, &f);
        assert_eq!(ids(&f), ids(&real));
        for (a, b) in f.iter().zip(&real) {
            assert!(
                (a.km - b.km).abs() < 0.2 && a.sim_s.abs_diff(b.sim_s) <= 10,
                "{}: {:.1} km {}s vs real {:.1} km {}s",
                a.id,
                a.km,
                a.sim_s,
                b.km,
                b.sim_s
            );
        }
    }
}

/// Three FM sites 20 km apart along the route. Each handover is about 11 km
/// after the previous one, so the second handover falls inside the wall-clock
/// gap window at 20x: 12 ticks of 2.2 km each.
fn close_sites_db() -> RepeaterDb {
    let db = RepeaterDb::open_memory().unwrap();
    for (n, along_km) in [0.0, 20.0, 40.0].into_iter().enumerate() {
        let mut row = dmr_row(&format!("syn-{n}"), &format!("SYN{n}"), 145.6, -0.6, 0.0);
        let (lat, lon) = interpolate_polyline(&[ESPA, DOMBAS], along_km / route_km()).unwrap();
        row.lat = Some(lat + 0.01);
        row.lon = Some(lon);
        row.modulation = "FM".into();
        row.source = RepeaterSource::Osm;
        row.access = AccessTone::Ctcss;
        row.ctcss_hz = Some(88.5);
        db.upsert_site(&row).unwrap();
    }
    db
}

#[test]
fn time_scale_fixes_dwell_and_gap_when_handovers_are_close() {
    let mut fixed = service_with(close_sites_db());
    let scaled = replay(&mut fixed, "standalone", 20.0, true, APP_TICK_REAL_S, false);
    let mut old = service_with(close_sites_db());
    let unscaled = replay(&mut old, "standalone", 20.0, false, APP_TICK_REAL_S, false);
    let mut real = service_with(close_sites_db());
    let reference = replay(&mut real, "standalone", 1.0, true, APP_TICK_REAL_S, false);
    print_run("Synthetic close sites, scale passed", 20.0, &scaled);
    print_run("Synthetic close sites, scale NOT passed", 20.0, &unscaled);
    print_run("Synthetic close sites, real time", 1.0, &reference);

    let ids = |v: &[Switch]| v.iter().map(|s| s.id.clone()).collect::<Vec<_>>();
    assert_eq!(ids(&scaled), ids(&reference));
    let tick_km = SPEED_KMH * APP_TICK_REAL_S * 20.0 / 3600.0;
    for (s, r) in scaled.iter().zip(&reference) {
        assert!(
            s.km >= r.km - 0.01 && s.km <= r.km + tick_km,
            "{}: scaled {:.1} km vs real time {:.1} km",
            s.id,
            s.km,
            r.km
        );
    }
    // Old behaviour: the second handover is held back by the wall-clock gap.
    assert_eq!(unscaled.len(), scaled.len());
    assert!(
        unscaled[2].km > scaled[2].km + 5.0,
        "wall-clock gap should delay the third site: {:.1} vs {:.1}",
        unscaled[2].km,
        scaled[2].km
    );
}
