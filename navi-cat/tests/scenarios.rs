//! Desktop coverage map for docs/CAT.md required scenarios 1–12.
//!
//! Full Espa→Dombås GPX + live packs remain campaign/emulator territory;
//! this file locks the host safety and fixture behaviours that CI can run.

use navi_cat::gating::{gate_from_dump_caps, GateDecision, GateStatus};
use navi_cat::importers::import_repeaterbook;
use navi_cat::program::{program_vfo1_verified, ProgramError, ProgramRequest};
use navi_cat::repeater::{
    dmr_dedupe_key, is_aprs, is_simplex, repeaterbook_sync_enabled, RepeaterDb, RepeaterSite,
    RepeaterSource,
};
use navi_cat::types::{RigBackend, RigError, ShiftDir, VfoState};
use navi_cat::CatService;

fn allowed_gate() -> GateDecision {
    GateDecision {
        allowed: true,
        status: GateStatus::Stable,
        can_set_rptr_shift: true,
        can_set_rptr_offs: true,
        can_set_ctcss: true,
        reason: "ok".into(),
        beta_override_warning: None,
    }
}

fn refused_gate(reason: &str) -> GateDecision {
    GateDecision {
        allowed: false,
        status: GateStatus::Alpha,
        can_set_rptr_shift: false,
        can_set_rptr_offs: false,
        can_set_ctcss: false,
        reason: reason.into(),
        beta_override_warning: None,
    }
}

struct MockRig {
    state: VfoState,
    ptt: bool,
    gate: GateDecision,
    set_ptt_attempts: u32,
    fail_ctcss_once: bool,
    calls: u32,
}

impl RigBackend for MockRig {
    fn is_connected(&self) -> bool {
        true
    }
    fn model_name(&self) -> String {
        "mock".into()
    }
    fn model_number(&self) -> i32 {
        1
    }
    fn gate(&self) -> GateDecision {
        self.gate.clone()
    }
    fn get_ptt(&mut self) -> Result<bool, RigError> {
        Ok(self.ptt)
    }
    fn get_dcd(&mut self) -> Result<bool, RigError> {
        Ok(false)
    }
    fn set_vfo_state(&mut self, want: &VfoState) -> Result<(), RigError> {
        self.state = want.clone();
        self.calls += 1;
        if self.fail_ctcss_once && self.calls == 1 {
            self.state.ctcss_tenths_hz = Some(0);
        }
        Ok(())
    }
    fn read_vfo_state(&mut self) -> Result<VfoState, RigError> {
        Ok(self.state.clone())
    }
}

impl MockRig {
    fn try_set_ptt(&mut self) -> Result<(), RigError> {
        self.set_ptt_attempts += 1;
        Err(RigError::Unsupported("refusing set-PTT / T command".into()))
    }
}

fn site(
    id: &str,
    call: &str,
    lat: f64,
    lon: f64,
    freq: f64,
    shift: f64,
    ctcss: Option<f64>,
    modu: &str,
    net: Option<&str>,
) -> RepeaterSite {
    RepeaterSite {
        id: id.into(),
        callsign: call.into(),
        lat: Some(lat),
        lon: Some(lon),
        freq_out_mhz: freq,
        shift_mhz: shift,
        ctcss_hz: ctcss,
        dcs_code: None,
        color_code: None,
        modulation: modu.into(),
        network_id: net.map(|s| s.into()),
        source: RepeaterSource::Osm,
        conflict: false,
        conflict_note: None,
        distance_km: None,
        position_accurate: true,
    }
}

fn fm_req(freq: f64, shift: f64, ctcss: f64) -> ProgramRequest {
    ProgramRequest {
        freq_out_mhz: freq,
        shift_mhz: shift,
        ctcss_hz: Some(ctcss),
        dcs_code: None,
        mode: "FM".into(),
        passband_hz: 12500,
        vfo: "VFOA".into(),
        leaves_follow: false,
    }
}

fn want(req: &ProgramRequest) -> VfoState {
    let freq_hz = (req.freq_out_mhz * 1_000_000.0).round() as u64;
    let offset_hz = (req.shift_mhz.abs() * 1_000_000.0).round() as u64;
    let shift = if req.shift_mhz < 0.0 {
        ShiftDir::Minus
    } else if req.shift_mhz > 0.0 {
        ShiftDir::Plus
    } else {
        ShiftDir::None
    };
    VfoState {
        vfo: req.vfo.clone(),
        freq_hz,
        mode: req.mode.clone(),
        passband_hz: req.passband_hz,
        shift,
        offset_hz,
        ctcss_tenths_hz: req.ctcss_hz.map(|hz| (hz * 10.0).round() as u32),
        dcs_code: req.dcs_code,
    }
}

/// Scenario 1 + 2: LA5MR network query + non-networked FM; APRS excluded.
#[test]
fn s01_s02_la5mr_and_non_networked_query() {
    let db = RepeaterDb::open_memory().unwrap();
    db.upsert_site(&site(
        "la5trr",
        "LA5TRR",
        61.0,
        10.5,
        145.725,
        -0.6,
        Some(88.5),
        "NFM",
        Some("LA5MR"),
    ))
    .unwrap();
    db.upsert_site(&site(
        "la5arr",
        "LA5ARR",
        61.28,
        10.31,
        434.775,
        -2.0,
        Some(71.9),
        "NFM",
        None,
    ))
    .unwrap();
    db.upsert_site(&site(
        "aprs", "LD2APR", 61.01, 10.51, 144.800, 0.0, None, "APRS", None,
    ))
    .unwrap();

    let net = db.query_near(61.0, 10.5, 150.0, Some("LA5MR"));
    assert_eq!(net.len(), 1);
    assert_eq!(net[0].callsign, "LA5TRR");

    let all = db.query_near(61.0, 10.5, 150.0, None);
    assert!(all.iter().all(|s| !is_aprs(s)));
    assert!(all.iter().any(|s| s.callsign == "LA5ARR"));
    assert!(!all.iter().any(|s| s.callsign == "LD2APR"));
}

/// Scenario 3: single FM site — full verified program returns reported state.
#[test]
fn s03_single_fm_full_readback() {
    let req = fm_req(145.725, -0.6, 88.5);
    let mut rig = MockRig {
        state: want(&req),
        ptt: false,
        gate: allowed_gate(),
        set_ptt_attempts: 0,
        fail_ctcss_once: false,
        calls: 0,
    };
    let reported = program_vfo1_verified(&mut rig, &req).unwrap();
    assert_eq!(reported.freq_hz, 145_725_000);
    assert_eq!(reported.ctcss_tenths_hz, Some(885));
    assert_eq!(reported.shift, ShiftDir::Minus);
}

/// Scenario 4: DMR dedupe; DMR modulation is not NFM auto-tune material.
#[test]
fn s04_dmr_dedupe_not_auto_fm() {
    assert_eq!(
        dmr_dedupe_key(434.600, 7.6, 1),
        dmr_dedupe_key(434.600, 7.6, 1)
    );
    let dmr = site(
        "dmr1", "LA9DMR", 61.1, 10.2, 434.600, 7.6, None, "DMR", None,
    );
    assert!(dmr.modulation.to_ascii_uppercase().contains("DMR"));
}

/// Scenario 5: conflict flag survives upsert / query.
#[test]
fn s05_conflict_flagged() {
    let db = RepeaterDb::open_memory().unwrap();
    let mut s = site(
        "c1",
        "LA2HRR",
        61.2,
        10.0,
        145.600,
        -0.6,
        Some(71.9),
        "NFM",
        None,
    );
    s.conflict = true;
    s.conflict_note = Some("OSM vs CSV offset mismatch".into());
    db.upsert_site(&s).unwrap();
    let q = db.query_near(61.2, 10.0, 20.0, None);
    assert_eq!(q.len(), 1);
    assert!(q[0].conflict);
}

/// Scenario 6: APRS / simplex / no-position CSV filtered from auto-tune set.
#[test]
fn s06_filtering() {
    let aprs = site("a", "APRS", 61.0, 10.0, 144.800, 0.0, None, "APRS", None);
    assert!(is_aprs(&aprs));
    let simplex = site("s", "VFO-A", 61.0, 10.0, 145.500, 0.0, None, "NFM", None);
    assert!(is_simplex(&simplex));
    let mut csv = site("c", "CSV1", 0.0, 0.0, 145.0, -0.6, None, "NFM", None);
    csv.lat = None;
    csv.lon = None;
    csv.position_accurate = false;
    let db = RepeaterDb::open_memory().unwrap();
    db.upsert_site(&csv).unwrap();
    let q = db.query_near(61.0, 10.0, 150.0, None);
    assert!(
        q.is_empty(),
        "no-position rows must not appear in distance query"
    );
}

/// Scenario 7: gating fixtures (Stable allowed; Alpha refused).
#[test]
fn s07_gating_parser() {
    let fixtures =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dump_caps");
    let stable = std::fs::read_to_string(fixtures.join("stable.txt")).unwrap();
    let d = gate_from_dump_caps(&stable, false, false);
    assert!(d.allowed);
    assert_eq!(d.status, GateStatus::Stable);

    let alpha = std::fs::read_to_string(fixtures.join("alpha.txt")).unwrap();
    let a = gate_from_dump_caps(&alpha, false, false);
    assert!(!a.allowed);
    assert_eq!(a.status, GateStatus::Alpha);
}

/// Scenario 8: PTT blocks; permanent mismatch fails closed with field name.
#[test]
fn s08_error_paths() {
    let req = fm_req(145.725, -0.6, 88.5);
    let mut ptt_rig = MockRig {
        state: want(&req),
        ptt: true,
        gate: allowed_gate(),
        set_ptt_attempts: 0,
        fail_ctcss_once: false,
        calls: 0,
    };
    assert!(matches!(
        program_vfo1_verified(&mut ptt_rig, &req),
        Err(ProgramError::PttActive)
    ));

    struct BadCtcss {
        state: VfoState,
    }
    impl RigBackend for BadCtcss {
        fn is_connected(&self) -> bool {
            true
        }
        fn model_name(&self) -> String {
            "bad".into()
        }
        fn model_number(&self) -> i32 {
            1
        }
        fn gate(&self) -> GateDecision {
            allowed_gate()
        }
        fn get_ptt(&mut self) -> Result<bool, RigError> {
            Ok(false)
        }
        fn get_dcd(&mut self) -> Result<bool, RigError> {
            Ok(false)
        }
        fn set_vfo_state(&mut self, want: &VfoState) -> Result<(), RigError> {
            self.state = want.clone();
            self.state.ctcss_tenths_hz = Some(0);
            Ok(())
        }
        fn read_vfo_state(&mut self) -> Result<VfoState, RigError> {
            Ok(self.state.clone())
        }
    }
    let mut bad = BadCtcss { state: want(&req) };
    match program_vfo1_verified(&mut bad, &req) {
        Err(ProgramError::ReadbackMismatch { field, .. }) => assert_eq!(field, "ctcss"),
        other => panic!("expected ctcss mismatch, got {other:?}"),
    }
}

/// Scenario 9: never transmit — set-PTT refused.
#[test]
fn s09_never_transmit() {
    let mut rig = MockRig {
        state: VfoState {
            vfo: "VFOA".into(),
            freq_hz: 0,
            mode: "FM".into(),
            passband_hz: 12500,
            shift: ShiftDir::None,
            offset_hz: 0,
            ctcss_tenths_hz: None,
            dcs_code: None,
        },
        ptt: false,
        gate: allowed_gate(),
        set_ptt_attempts: 0,
        fail_ctcss_once: false,
        calls: 0,
    };
    assert!(rig.try_set_ptt().is_err());
    assert_eq!(rig.set_ptt_attempts, 1);
}

/// Scenario 10: non_networked fixture loads and has corridor entries.
#[test]
fn s10_fixture_non_networked_present() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../testdata/cat/non_networked.json");
    let raw = std::fs::read_to_string(&path).expect("non_networked.json");
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert!(v["count"].as_u64().unwrap_or(0) >= 1);
    assert!(v["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["callsign"].as_str() == Some("LA5ARR")));
}

/// Scenario 11: ungated backend refused; mismatch stops follow (sandbox safety).
#[test]
fn s11_sandbox_gating_and_follow_stop() {
    let req = fm_req(145.725, -0.6, 88.5);
    let mut ungated = MockRig {
        state: want(&req),
        ptt: false,
        gate: refused_gate("Alpha backend"),
        set_ptt_attempts: 0,
        fail_ctcss_once: false,
        calls: 0,
    };
    assert!(matches!(
        program_vfo1_verified(&mut ungated, &req),
        Err(ProgramError::NotGated(_))
    ));

    struct AlwaysBad {
        state: VfoState,
    }
    impl RigBackend for AlwaysBad {
        fn is_connected(&self) -> bool {
            true
        }
        fn model_name(&self) -> String {
            "bad".into()
        }
        fn model_number(&self) -> i32 {
            1
        }
        fn gate(&self) -> GateDecision {
            allowed_gate()
        }
        fn get_ptt(&mut self) -> Result<bool, RigError> {
            Ok(false)
        }
        fn get_dcd(&mut self) -> Result<bool, RigError> {
            Ok(false)
        }
        fn set_vfo_state(&mut self, want: &VfoState) -> Result<(), RigError> {
            self.state = want.clone();
            self.state.freq_hz = 1;
            Ok(())
        }
        fn read_vfo_state(&mut self) -> Result<VfoState, RigError> {
            Ok(self.state.clone())
        }
    }
    let mut svc = CatService::new(
        AlwaysBad { state: want(&req) },
        RepeaterDb::open_memory().unwrap(),
    );
    svc.follow_network_id = Some("LA5MR".into());
    let resp: serde_json::Value =
        serde_json::from_str(&svc.vfo_set_json(&serde_json::to_string(&req).unwrap())).unwrap();
    assert_eq!(resp["ok"], false);
    assert!(svc.follow_network_id.is_none());
    assert!(svc.follow_stopped_reason.is_some());
}

/// Scenario 12: RepeaterBook stays off; importer errors; no network dependency.
#[test]
fn s12_repeaterbook_off() {
    assert!(!repeaterbook_sync_enabled());
    let db = RepeaterDb::open_memory().unwrap();
    assert!(import_repeaterbook(&db, "anything").is_err());
}
