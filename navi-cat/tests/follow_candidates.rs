//! Follow candidate gating (rig capability, band plan) and the follow clock.
//!
//! Covers the Phase 6 dry-run defects: DMR-only RadioID rows programmed as analog
//! FM, a 287.9375 MHz shift programmed on LA2TRR, and dwell/gap measured in wall
//! clock time while a route was simulated.

use navi_cat::gating::{GateDecision, GateStatus};
use navi_cat::repeater::{AccessTone, RepeaterDb, RepeaterSite, RepeaterSource};
use navi_cat::types::{RigBackend, RigError, VfoState};
use navi_cat::CatService;

struct FmRig {
    analog_only: bool,
}

impl RigBackend for FmRig {
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
    fn get_ptt(&mut self) -> Result<bool, RigError> {
        Ok(false)
    }
    fn get_dcd(&mut self) -> Result<bool, RigError> {
        Ok(false)
    }
    fn set_vfo_state(&mut self, _want: &VfoState) -> Result<(), RigError> {
        Ok(())
    }
    fn read_vfo_state(&mut self) -> Result<VfoState, RigError> {
        Err(RigError::NotConnected)
    }
    fn analog_fm_only(&self) -> bool {
        self.analog_only
    }
}

#[allow(clippy::too_many_arguments)]
fn site(
    id: &str,
    callsign: &str,
    lat: f64,
    freq: f64,
    shift: Option<f64>,
    modulation: &str,
    source: RepeaterSource,
    network: Option<&str>,
) -> RepeaterSite {
    RepeaterSite {
        id: id.into(),
        callsign: callsign.into(),
        lat: Some(lat),
        lon: Some(10.0),
        freq_out_mhz: freq,
        shift_mhz: shift,
        ctcss_hz: Some(88.5),
        dcs_code: None,
        access: AccessTone::Ctcss,
        color_code: None,
        modulation: modulation.into(),
        network_id: network.map(str::to_string),
        source,
        conflict: false,
        conflict_note: None,
        distance_km: None,
        position_accurate: true,
        exclude_recommended: false,
        attribution: None,
        country: Some("NO".into()),
        file_kind: None,
        original_lat: None,
        original_lon: None,
        relocated: false,
        relocation_note: None,
    }
}

fn dry_run_db() -> RepeaterDb {
    let db = RepeaterDb::open_memory().unwrap();
    let rows = [
        // Analog FM rows that must stay candidates.
        site(
            "fm2m",
            "LA2JRR",
            61.00,
            145.6625,
            Some(-0.6),
            "FM",
            RepeaterSource::Osm,
            None,
        ),
        site(
            "fm70",
            "LA5ARR",
            61.01,
            434.775,
            Some(-2.0),
            "11K2F3E",
            RepeaterSource::Osm,
            None,
        ),
        site(
            "mixed",
            "LA6MIX",
            61.02,
            145.7,
            Some(-0.6),
            "FM;DMR",
            RepeaterSource::Osm,
            None,
        ),
        // Digital-only RadioID rows from the dry run.
        site(
            "rid-ld2hj",
            "LD2HJ",
            61.03,
            434.700,
            Some(-2.0),
            "DMR",
            RepeaterSource::RadioId,
            None,
        ),
        site(
            "rid-la2lrr",
            "LA2LRR",
            61.04,
            434.700,
            Some(-2.0),
            "DMR",
            RepeaterSource::RadioId,
            None,
        ),
        site(
            "rid-la2l",
            "LA2L",
            61.05,
            433.775,
            Some(-1.6),
            "DMR",
            RepeaterSource::RadioId,
            None,
        ),
        // Bad shift row mirrored from OSM node 5576506182.
        site(
            "osm-node-5576506182",
            "LA2TRR",
            61.06,
            145.2375,
            Some(287.9375),
            "FM",
            RepeaterSource::Osm,
            Some("LA5MR"),
        ),
    ];
    for r in &rows {
        db.upsert_site(r).unwrap();
    }
    db
}

fn query_ids(svc: &CatService<FmRig>) -> Vec<String> {
    let json = svc.repeater_query_json(61.0, 10.0, 150.0, None);
    let rows: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
    let mut ids: Vec<String> = rows
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect();
    ids.sort();
    ids
}

#[test]
fn analog_rig_skips_digital_only_and_bad_shift_rows() {
    let svc = CatService::new(FmRig { analog_only: true }, dry_run_db());
    assert_eq!(query_ids(&svc), vec!["fm2m", "fm70", "mixed"]);
}

#[test]
fn skipped_rows_stay_in_the_database_and_the_ui_list() {
    let svc = CatService::new(FmRig { analog_only: true }, dry_run_db());
    let ui: Vec<serde_json::Value> =
        serde_json::from_str(&svc.repeater_list_ui_json(61.0, 10.0, 150.0)).unwrap();
    let calls: Vec<&str> = ui.iter().map(|r| r["callsign"].as_str().unwrap()).collect();
    for want in ["LD2HJ", "LA2LRR", "LA2L", "LA2TRR"] {
        assert!(calls.contains(&want), "{want} missing from UI list");
    }
}

#[test]
fn rig_that_handles_digital_keeps_dmr_rows_but_still_rejects_bad_shift() {
    let svc = CatService::new(FmRig { analog_only: false }, dry_run_db());
    assert_eq!(
        query_ids(&svc),
        vec![
            "fm2m",
            "fm70",
            "mixed",
            "rid-la2l",
            "rid-la2lrr",
            "rid-ld2hj"
        ]
    );
}

#[test]
fn named_network_query_drops_the_bad_shift_member() {
    let svc = CatService::new(FmRig { analog_only: true }, dry_run_db());
    let json = svc.repeater_query_json(61.0, 10.0, 150.0, Some("LA5MR"));
    let rows: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
    assert!(rows.is_empty(), "LA2TRR was the only LA5MR row: {rows:?}");
}

#[test]
fn status_now_follows_the_time_scale() {
    const T0: f64 = 1_800_000_000.0;
    let mut svc = CatService::new(FmRig { analog_only: true }, dry_run_db());
    let now = |svc: &mut CatService<FmRig>, real: f64| -> u64 {
        let v: serde_json::Value = serde_json::from_str(&svc.status_json_at(real)).unwrap();
        v["now_unix_s"].as_u64().unwrap()
    };
    assert_eq!(now(&mut svc, T0), T0 as u64);
    assert_eq!(now(&mut svc, T0 + 5.0), T0 as u64 + 5);

    assert!(svc.set_time_scale_at(12.0, T0 + 5.0));
    assert_eq!(
        now(&mut svc, T0 + 5.0),
        T0 as u64 + 5,
        "continuous at the change"
    );
    assert_eq!(now(&mut svc, T0 + 10.0), T0 as u64 + 5 + 60);

    assert!(svc.set_time_scale_at(20.0, T0 + 10.0));
    assert_eq!(now(&mut svc, T0 + 15.0), T0 as u64 + 65 + 100);

    assert!(!svc.set_time_scale_at(-3.0, T0 + 15.0));
    let v: serde_json::Value = serde_json::from_str(&svc.status_json_at(T0 + 15.0)).unwrap();
    assert_eq!(v["time_scale"].as_f64(), Some(20.0));
}

#[test]
fn adopt_session_state_keeps_the_clock_across_a_reconnect() {
    const T0: f64 = 1_800_000_000.0;
    let mut old = CatService::new(FmRig { analog_only: true }, dry_run_db());
    old.follow_network_id = Some("auto".into());
    old.set_time_scale_at(12.0, T0);
    let mut fresh = CatService::new(FmRig { analog_only: true }, dry_run_db());
    fresh.adopt_session_state(&old);
    assert_eq!(fresh.follow_network_id.as_deref(), Some("auto"));
    let v: serde_json::Value = serde_json::from_str(&fresh.status_json_at(T0 + 5.0)).unwrap();
    assert_eq!(v["now_unix_s"].as_u64(), Some(T0 as u64 + 60));
}
