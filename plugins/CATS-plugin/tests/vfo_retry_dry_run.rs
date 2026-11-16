//! Dry run of the follow VFO retry rule with a fake rig that fails program attempts.
//!
//! Same path as the app tick, without wasm and without sleeping: guest applies the
//! host report of the previous program (`apply_program_result_json`), host query
//! (`repeater_query_json`), guest `tick_at`, then host `program_follow_site` with
//! the retry rule, whose report goes back to the guest at the next tick.
//!
//! Rule under test: three program attempts in total (two retries) with a pause
//! between them, then a user-visible failure naming the repeater and frequency,
//! no follow state committed for the failed site, and no new attempt for that site
//! until the cool-down (`MIN_GAP_S`) is over.

use std::cell::Cell;
use std::time::Duration;

use navi_cat::gating::{GateDecision, GateStatus};
use navi_cat::repeater::{AccessTone, RepeaterDb, RepeaterSite, RepeaterSource};
use navi_cat::types::{RigBackend, RigError, VfoState};
use navi_cat::{CatService, RetryPolicy, VFO_PROGRAM_ATTEMPTS, VFO_RETRY_PAUSE};
use navi_plugin_cats::follow::{
    apply_program_result_json, live_members_from_rows, select_members, tick_at, FollowState,
    FollowTick, MIN_GAP_S,
};

const POS: (f64, f64) = (60.0, 10.0);
const T0: u64 = 1_000;
const TICK_S: u64 = 5;

/// Rig whose first `fail_first` program attempts fail. With `connected = false`
/// the gate refuses every program, like a session that never saw a radio.
struct FakeRig {
    state: Option<VfoState>,
    fail_first: u32,
    /// `set_vfo_state` calls (rig writes).
    writes: u32,
    /// `gate` calls: one per program attempt, also when the rig is not connected.
    gate_calls: Cell<u32>,
    connected: bool,
}

impl FakeRig {
    fn failing_first(n: u32) -> Self {
        Self {
            state: None,
            fail_first: n,
            writes: 0,
            gate_calls: Cell::new(0),
            connected: true,
        }
    }

    fn never_connected() -> Self {
        Self {
            connected: false,
            ..Self::failing_first(0)
        }
    }
}

impl RigBackend for FakeRig {
    fn is_connected(&self) -> bool {
        self.connected
    }
    fn model_name(&self) -> String {
        "fake".into()
    }
    fn model_number(&self) -> i32 {
        1
    }
    fn gate(&self) -> GateDecision {
        self.gate_calls.set(self.gate_calls.get() + 1);
        GateDecision {
            allowed: self.connected,
            status: GateStatus::Stable,
            can_set_rptr_shift: true,
            can_set_rptr_offs: true,
            can_set_ctcss: true,
            reason: if self.connected {
                "ok".into()
            } else {
                "no radio connected".into()
            },
            beta_override_warning: None,
        }
    }
    fn get_ptt(&mut self) -> Result<bool, RigError> {
        Ok(false)
    }
    fn get_dcd(&mut self) -> Result<bool, RigError> {
        Ok(false)
    }
    fn set_vfo_state(&mut self, want: &VfoState) -> Result<(), RigError> {
        self.writes += 1;
        if self.writes <= self.fail_first {
            return Err(RigError::Io("link dropped".into()));
        }
        self.state = Some(want.clone());
        Ok(())
    }
    fn read_vfo_state(&mut self) -> Result<VfoState, RigError> {
        self.state.clone().ok_or(RigError::NotConnected)
    }
}

fn site(id: &str, callsign: &str, freq: f64, lat: f64) -> RepeaterSite {
    RepeaterSite {
        id: id.into(),
        callsign: callsign.into(),
        lat: Some(lat),
        lon: Some(10.0),
        freq_out_mhz: freq,
        shift_mhz: Some(-0.6),
        ctcss_hz: Some(88.5),
        dcs_code: None,
        access: AccessTone::Ctcss,
        color_code: None,
        modulation: "FM".into(),
        network_id: None,
        source: RepeaterSource::Osm,
        conflict: false,
        conflict_note: None,
        distance_km: None,
        position_accurate: true,
        exclude_recommended: false,
        attribution: None,
        country: Some("NO".into()),
        file_kind: Some("osm".into()),
        original_lat: None,
        original_lon: None,
        relocated: false,
        relocation_note: None,
    }
}

/// One follow chain: host service, guest state and the KV report between them.
struct Drive {
    svc: CatService<FakeRig>,
    state: FollowState,
    /// Plugin KV `cat_follow_result`: host report waiting for the next guest tick.
    kv_result: String,
    /// Pauses the retry rule asked for, in order.
    pauses: Vec<Duration>,
    /// Follow ticks that led the host to program, with the follow time.
    programs: Vec<u64>,
}

impl Drive {
    fn new(rig: FakeRig) -> Self {
        let db = RepeaterDb::open_memory().unwrap();
        db.upsert_site(&site("la5mr-1", "LA5MR", 145.6, 60.0))
            .unwrap();
        let mut svc = CatService::new(rig, db);
        svc.network_follow_json(r#"{"enabled":true,"network_id":"standalone"}"#);
        Self {
            svc,
            state: FollowState::default(),
            kv_result: String::new(),
            pauses: Vec::new(),
            programs: Vec::new(),
        }
    }

    /// One app follow tick at follow time `now`.
    fn tick(&mut self, now: u64) -> FollowTick {
        // Guest: apply the host report of the previous program, then clear it.
        let report = std::mem::take(&mut self.kv_result);
        apply_program_result_json(&mut self.state, &report);

        let rows: Vec<serde_json::Value> =
            serde_json::from_str(&self.svc.repeater_query_json(POS.0, POS.1, 150.0, None)).unwrap();
        let members = select_members("standalone", &live_members_from_rows(&rows));
        self.state.network_id = Some("standalone".into());
        let tick = tick_at(&mut self.state, POS.0, POS.1, &members, now);

        // Host: program the requested site with the retry rule.
        if let FollowTick::Switch { site_id } = &tick {
            self.programs.push(now);
            let pauses = &mut self.pauses;
            let report =
                self.svc
                    .program_follow_site(site_id, now, &RetryPolicy::default(), &mut |d| {
                        pauses.push(d)
                    });
            self.kv_result = report.to_json();
        }
        tick
    }

    /// Tick every `TICK_S` from `from` to `to` inclusive; returns the switches.
    fn run(&mut self, from: u64, to: u64) -> usize {
        let mut switches = 0;
        let mut t = from;
        while t <= to {
            if matches!(self.tick(t), FollowTick::Switch { .. }) {
                switches += 1;
            }
            t += TICK_S;
        }
        switches
    }

    fn status(&mut self) -> serde_json::Value {
        serde_json::from_str(&self.svc.status_json_at(1_800_000_000.0)).unwrap()
    }
}

#[test]
fn contract_is_three_attempts_with_a_one_and_a_half_second_pause() {
    assert_eq!(VFO_PROGRAM_ATTEMPTS, 3);
    assert_eq!(VFO_RETRY_PAUSE, Duration::from_millis(1_500));
}

#[test]
fn one_or_two_failures_succeed_on_a_retry_and_commit_the_site() {
    for (fail, attempts_needed) in [(1u32, 2u32), (2, 3)] {
        let mut d = Drive::new(FakeRig::failing_first(fail));
        let switches = d.run(T0, T0 + 12 * TICK_S);
        assert_eq!(switches, 1, "fail={fail}: one switch, no repeat");

        assert!(
            d.kv_result.is_empty(),
            "report was consumed by the next tick"
        );
        assert_eq!(
            d.svc.backend.writes,
            fail + 1,
            "fail={fail}: failed writes plus the one that worked"
        );
        assert_eq!(
            d.pauses,
            vec![VFO_RETRY_PAUSE; fail as usize],
            "fail={fail}"
        );
        assert_eq!(d.state.current_site.as_deref(), Some("la5mr-1"));
        assert!(d.state.pending.is_none(), "committed, nothing pending");
        assert!(d.state.failed_site.is_none());
        assert!(d.svc.program_failure().is_none());
        assert!(d.status()["program_failure"].is_null());
        assert!(
            d.svc.backend.state.is_some(),
            "attempt {attempts_needed}/{VFO_PROGRAM_ATTEMPTS} reached the radio"
        );
    }
}

#[test]
fn three_or_more_failures_stop_notify_and_do_not_commit() {
    for fail in [3u32, 99] {
        let mut d = Drive::new(FakeRig::failing_first(fail));

        // Tick 0: guest switches, host fails all three attempts.
        assert!(matches!(d.tick(T0), FollowTick::Switch { .. }));
        assert_eq!(d.svc.backend.writes, 3, "fail={fail}: exactly 3 attempts");
        assert_eq!(
            d.pauses,
            vec![VFO_RETRY_PAUSE; 2],
            "pause between attempts only"
        );

        let n = d
            .svc
            .program_failure()
            .expect("user-visible failure")
            .clone();
        assert_eq!(n.callsign, "LA5MR");
        assert_eq!(n.attempts, 3);
        assert!(n.message.contains("LA5MR"), "{}", n.message);
        assert!(n.message.contains("145.6000"), "{}", n.message);
        let status = d.status();
        assert_eq!(status["program_failure"]["callsign"], "LA5MR");
        assert_eq!(status["program_failure"]["freq_mhz"], 145.6);
        assert_eq!(status["program_failure"]["attempts"], 3);
        assert_eq!(status["program_failure"]["seq"], 1);

        // The guest committed in the same tick; the next tick applies the host
        // report and the site is not "tuned".
        assert_eq!(d.tick(T0 + TICK_S), FollowTick::Hold);
        assert_eq!(
            d.state.current_site, None,
            "failed program is not committed"
        );
        assert_eq!(d.state.failed_site.as_deref(), Some("la5mr-1"));
        assert_eq!(d.state.failed_at_unix_s, T0);

        // Inside the cool-down: no fourth attempt, no new message.
        let switches = d.run(T0 + 2 * TICK_S, T0 + MIN_GAP_S - 1);
        assert_eq!(switches, 0, "fail={fail}: cool-down holds the site");
        assert_eq!(d.svc.backend.writes, 3, "fail={fail}: no 4th attempt");
        assert_eq!(d.svc.program_failure().unwrap().seq, 1);
    }
}

#[test]
fn after_the_cooldown_the_site_is_tried_again() {
    // Rig that failed three times and then recovers: the retry after the cool-down works.
    let mut d = Drive::new(FakeRig::failing_first(3));
    assert!(matches!(d.tick(T0), FollowTick::Switch { .. }));
    d.run(T0 + TICK_S, T0 + MIN_GAP_S - 1);
    assert_eq!(d.svc.backend.writes, 3);

    assert!(matches!(d.tick(T0 + MIN_GAP_S), FollowTick::Switch { .. }));
    assert_eq!(
        d.svc.backend.writes, 4,
        "first attempt of the new batch works"
    );
    assert!(
        d.svc.program_failure().is_none(),
        "success clears the notice"
    );
    d.run(T0 + MIN_GAP_S + TICK_S, T0 + MIN_GAP_S + 6 * TICK_S);
    assert_eq!(d.state.current_site.as_deref(), Some("la5mr-1"));
    assert!(d.state.failed_site.is_none());
    assert_eq!(d.programs, vec![T0, T0 + MIN_GAP_S]);
}

#[test]
fn a_rig_that_keeps_failing_gets_one_batch_and_one_message_per_cooldown() {
    let mut d = Drive::new(FakeRig::failing_first(99));
    let switches = d.run(T0, T0 + 3 * MIN_GAP_S - 1);
    assert_eq!(switches, 3, "one program batch per cool-down window");
    assert_eq!(
        d.svc.backend.writes, 9,
        "3 batches of 3 attempts, never more"
    );
    assert_eq!(d.svc.program_failure().unwrap().seq, 3);
    assert_eq!(d.programs, vec![T0, T0 + MIN_GAP_S, T0 + 2 * MIN_GAP_S]);
}

#[test]
fn never_connected_rig_is_bounded_in_the_same_way() {
    let mut d = Drive::new(FakeRig::never_connected());
    assert!(matches!(d.tick(T0), FollowTick::Switch { .. }));
    assert_eq!(d.svc.backend.gate_calls.get(), 3, "exactly 3 attempts");
    assert_eq!(d.svc.backend.writes, 0, "gate refuses before any write");
    assert_eq!(d.pauses, vec![VFO_RETRY_PAUSE; 2]);

    let n = d
        .svc
        .program_failure()
        .expect("user-visible failure")
        .clone();
    assert!(n.message.contains("LA5MR") && n.message.contains("145.6000"));
    assert!(n.error.contains("no radio connected"), "{}", n.error);

    // Not committed, and not retried inside the cool-down.
    assert_eq!(d.tick(T0 + TICK_S), FollowTick::Hold);
    assert_eq!(d.state.current_site, None);
    assert_eq!(d.run(T0 + 2 * TICK_S, T0 + MIN_GAP_S - 1), 0);
    assert_eq!(d.svc.backend.gate_calls.get(), 3, "no 4th attempt");

    // The radio connects meanwhile: the site is tried again after the cool-down.
    d.svc.backend.connected = true;
    assert!(matches!(d.tick(T0 + MIN_GAP_S), FollowTick::Switch { .. }));
    assert!(d.svc.program_failure().is_none());
    d.run(T0 + MIN_GAP_S + TICK_S, T0 + MIN_GAP_S + 3 * TICK_S);
    assert_eq!(d.state.current_site.as_deref(), Some("la5mr-1"));
    assert!(d.svc.backend.state.is_some());
}

#[test]
fn guest_and_host_agree_on_the_report_json() {
    let mut d = Drive::new(FakeRig::failing_first(99));
    assert!(matches!(d.tick(T0), FollowTick::Switch { .. }));
    let v: serde_json::Value = serde_json::from_str(&d.kv_result).unwrap();
    assert_eq!(v["site_id"], "la5mr-1");
    assert_eq!(v["ok"], false);
    assert_eq!(v["attempts"], 3);
    assert_eq!(v["notified"], true);
    assert_eq!(v["at_unix_s"], T0);
}
