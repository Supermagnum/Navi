//! Follow VFO programming with a bounded retry and a user-visible failure.
//!
//! The follow guest picks a site; the host programs it. A program can fail for a
//! transient reason (link hiccup, rig busy) or a lasting one (no radio connected,
//! rig refuses the value). The rule here:
//!
//! - at most [`VFO_PROGRAM_ATTEMPTS`] attempts in total (the first try plus two
//!   retries), with [`VFO_RETRY_PAUSE`] of real time between attempts;
//! - after the last failed attempt, stop and record a [`ProgramFailureNotice`]
//!   naming the repeater and the frequency, so the UI can tell the user;
//! - the outcome is reported to the guest ([`FollowProgramReport`]) so the follow
//!   state is never committed as "tuned to this site" on a failure.
//!
//! One attempt is one call to [`program_vfo1_verified`], which itself writes the
//! VFO twice when a read-back mismatches. Shift values that are not plausible
//! offsets are skipped (`cat_skip`), not retried and not reported to the user.
//!
//! Log lines (greppable):
//!
//! - `cat_vfo_retry attempt=2/3 site=LA5MR freq=145.6000 error="..."`
//! - `cat_vfo_ok site=LA5MR freq=145.6000 attempts=2`
//! - `cat_vfo_failed site=LA5MR freq=145.6000 attempts=3 notified=true error="..."`

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::program::{program_vfo1_verified, ProgramError, ProgramRequest, ReportedVfo};
use crate::repeater::{AccessTone, RepeaterSite};
use crate::types::RigBackend;
use crate::CatService;

/// Total program attempts per follow switch: the first try plus two retries.
pub const VFO_PROGRAM_ATTEMPTS: u32 = 3;
/// Real-time pause between two program attempts.
pub const VFO_RETRY_PAUSE: Duration = Duration::from_millis(1_500);

/// Attempt budget and pause. Tests use [`RetryPolicy::no_pause`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub pause: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: VFO_PROGRAM_ATTEMPTS,
            pause: VFO_RETRY_PAUSE,
        }
    }
}

impl RetryPolicy {
    /// Same attempt budget, no pause between attempts.
    pub fn no_pause() -> Self {
        Self {
            pause: Duration::ZERO,
            ..Self::default()
        }
    }
}

/// Result of [`program_with_retry`].
#[derive(Debug)]
pub enum RetryOutcome {
    Programmed {
        reported: ReportedVfo,
        attempts: u32,
    },
    Failed {
        attempts: u32,
        error: ProgramError,
        /// False for a skip (invalid shift): nothing the user can act on.
        notify: bool,
    },
}

/// Program `req`, retrying as described in the module docs. `label` is the
/// callsign used in log lines. `sleep` is called with the pause between attempts.
pub fn program_with_retry<B: RigBackend>(
    backend: &mut B,
    label: &str,
    req: &ProgramRequest,
    policy: &RetryPolicy,
    sleep: &mut dyn FnMut(Duration),
) -> RetryOutcome {
    let max = policy.max_attempts.max(1);
    let mut attempt = 1;
    loop {
        match program_vfo1_verified(backend, req) {
            Ok(reported) => {
                log::info!(
                    target: "NaviCat",
                    "cat_vfo_ok site={label} freq={:.4} attempts={attempt}",
                    req.freq_out_mhz,
                );
                return RetryOutcome::Programmed {
                    reported,
                    attempts: attempt,
                };
            }
            Err(error) => {
                let skip = matches!(error, ProgramError::InvalidShift { .. });
                if skip || attempt >= max {
                    log::warn!(
                        target: "NaviCat",
                        "cat_vfo_failed site={label} freq={:.4} attempts={attempt} notified={} error=\"{error}\"",
                        req.freq_out_mhz,
                        !skip,
                    );
                    return RetryOutcome::Failed {
                        attempts: attempt,
                        error,
                        notify: !skip,
                    };
                }
                attempt += 1;
                log::warn!(
                    target: "NaviCat",
                    "cat_vfo_retry attempt={attempt}/{max} site={label} freq={:.4} error=\"{error}\"",
                    req.freq_out_mhz,
                );
                sleep(policy.pause);
            }
        }
    }
}

/// User-facing record of the last failed follow program. Cleared by the next
/// successful program or a reconnect. `seq` grows with every new failure so a UI
/// can show each failure once.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProgramFailureNotice {
    pub seq: u64,
    pub site_id: String,
    pub callsign: String,
    pub freq_mhz: f64,
    pub attempts: u32,
    pub error: String,
    /// English text naming the repeater and the frequency. The app shows its own
    /// localized string built from `callsign`, `freq_mhz` and `attempts`.
    pub message: String,
}

/// English failure text. Names the repeater and the frequency in MHz.
pub fn failure_message(callsign: &str, freq_mhz: f64, attempts: u32) -> String {
    format!(
        "The radio did not accept the frequency {freq_mhz:.4} MHz for repeater {callsign} after {attempts} attempts."
    )
}

/// What the host tells the follow guest after it tried to program a site.
/// Stored in plugin KV under `cat_follow_result`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FollowProgramReport {
    pub site_id: String,
    pub ok: bool,
    pub attempts: u32,
    pub notified: bool,
    /// Follow clock time (`now_unix_s` of the tick) at which the program ended.
    pub at_unix_s: u64,
}

impl FollowProgramReport {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// Build the VFO1 request for a repeater row. A 1750 Hz burst site programs no
/// CTCSS/DCS encode; an unknown shift is left untouched (NaN).
pub fn follow_request(site: &RepeaterSite) -> ProgramRequest {
    let burst = site.access == AccessTone::ToneBurst1750;
    ProgramRequest {
        freq_out_mhz: site.freq_out_mhz,
        shift_mhz: site.shift_mhz.unwrap_or(f64::NAN),
        ctcss_hz: if burst { None } else { site.ctcss_hz },
        dcs_code: if burst { None } else { site.dcs_code },
        mode: "FM".into(),
        passband_hz: 12_500,
        vfo: "VFOA".into(),
        leaves_follow: false,
    }
}

impl<B: RigBackend> CatService<B> {
    /// Program the follow site `site_id` with the retry rule. Updates the
    /// user-visible failure notice and returns the report for the guest.
    pub fn program_follow_site(
        &mut self,
        site_id: &str,
        now_unix_s: u64,
        policy: &RetryPolicy,
        sleep: &mut dyn FnMut(Duration),
    ) -> FollowProgramReport {
        let site = self.db.list_sites().into_iter().find(|s| s.id == site_id);
        let Some(site) = site else {
            log::warn!(
                target: "NaviCat",
                "VFO1 skip: site_id={site_id} not in repeater db",
            );
            return FollowProgramReport {
                site_id: site_id.to_string(),
                ok: false,
                attempts: 0,
                notified: false,
                at_unix_s: now_unix_s,
            };
        };
        let req = follow_request(&site);
        let label = if site.callsign.is_empty() {
            site.id.as_str()
        } else {
            site.callsign.as_str()
        };
        match program_with_retry(&mut self.backend, label, &req, policy, sleep) {
            RetryOutcome::Programmed { reported, attempts } => {
                log::info!(
                    target: "NaviCat",
                    "VFO1 program {} {} freq={:.3} shift={:.3} ctcss={:?} dcs={:?} attempts={attempts} readback={reported:?}",
                    site.callsign,
                    site.id,
                    site.freq_out_mhz,
                    req.shift_mhz,
                    req.ctcss_hz,
                    req.dcs_code,
                );
                self.program_failure = None;
                FollowProgramReport {
                    site_id: site.id,
                    ok: true,
                    attempts,
                    notified: false,
                    at_unix_s: now_unix_s,
                }
            }
            RetryOutcome::Failed {
                attempts,
                error,
                notify,
            } => {
                if notify {
                    self.failure_seq += 1;
                    self.program_failure = Some(ProgramFailureNotice {
                        seq: self.failure_seq,
                        site_id: site.id.clone(),
                        callsign: label.to_string(),
                        freq_mhz: site.freq_out_mhz,
                        attempts,
                        error: error.to_string(),
                        message: failure_message(label, site.freq_out_mhz, attempts),
                    });
                }
                FollowProgramReport {
                    site_id: site.id,
                    ok: false,
                    attempts,
                    notified: notify,
                    at_unix_s: now_unix_s,
                }
            }
        }
    }

    /// Last failed follow program the user has not yet been shown a success for.
    pub fn program_failure(&self) -> Option<&ProgramFailureNotice> {
        self.program_failure.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gating::{GateDecision, GateStatus};
    use crate::types::{RigError, VfoState};

    /// Rig whose first `fail_first` program attempts fail with an io error.
    /// `connected = false` makes the gate refuse, like a session without a radio.
    struct FakeRig {
        state: Option<VfoState>,
        fail_first: u32,
        calls: u32,
        connected: bool,
    }

    impl FakeRig {
        fn new(fail_first: u32) -> Self {
            Self {
                state: None,
                fail_first,
                calls: 0,
                connected: true,
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
            self.calls += 1;
            if self.calls <= self.fail_first {
                return Err(RigError::Io("link dropped".into()));
            }
            self.state = Some(want.clone());
            Ok(())
        }
        fn read_vfo_state(&mut self) -> Result<VfoState, RigError> {
            self.state.clone().ok_or(RigError::NotConnected)
        }
    }

    fn req() -> ProgramRequest {
        ProgramRequest {
            freq_out_mhz: 145.6,
            shift_mhz: -0.6,
            ctcss_hz: Some(88.5),
            dcs_code: None,
            mode: "FM".into(),
            passband_hz: 12_500,
            vfo: "VFOA".into(),
            leaves_follow: false,
        }
    }

    fn run(rig: &mut FakeRig, req: &ProgramRequest) -> (RetryOutcome, Vec<Duration>) {
        let mut pauses = Vec::new();
        let out = program_with_retry(rig, "LA5MR", req, &RetryPolicy::default(), &mut |d| {
            pauses.push(d)
        });
        (out, pauses)
    }

    #[test]
    fn constants_are_three_attempts_and_one_and_a_half_seconds() {
        assert_eq!(VFO_PROGRAM_ATTEMPTS, 3);
        assert_eq!(VFO_RETRY_PAUSE, Duration::from_millis(1_500));
        assert_eq!(RetryPolicy::default().max_attempts, 3);
        assert_eq!(RetryPolicy::no_pause().pause, Duration::ZERO);
    }

    #[test]
    fn first_try_success_does_not_pause() {
        let mut rig = FakeRig::new(0);
        let (out, pauses) = run(&mut rig, &req());
        assert!(matches!(out, RetryOutcome::Programmed { attempts: 1, .. }));
        assert!(pauses.is_empty());
    }

    #[test]
    fn one_and_two_failures_succeed_on_attempt_two_and_three() {
        for (fail, want) in [(1, 2), (2, 3)] {
            let mut rig = FakeRig::new(fail);
            let (out, pauses) = run(&mut rig, &req());
            match out {
                RetryOutcome::Programmed { attempts, .. } => assert_eq!(attempts, want),
                other => panic!("expected success, got {other:?}"),
            }
            assert_eq!(pauses, vec![VFO_RETRY_PAUSE; fail as usize]);
        }
    }

    #[test]
    fn three_and_many_failures_stop_after_exactly_three_attempts() {
        for fail in [3, 99] {
            let mut rig = FakeRig::new(fail);
            let (out, pauses) = run(&mut rig, &req());
            match out {
                RetryOutcome::Failed {
                    attempts, notify, ..
                } => {
                    assert_eq!(attempts, 3);
                    assert!(notify);
                }
                other => panic!("expected failure, got {other:?}"),
            }
            assert_eq!(rig.calls, 3, "no fourth attempt");
            assert_eq!(pauses.len(), 2, "pause only between attempts");
        }
    }

    #[test]
    fn never_connected_rig_is_bounded_too() {
        let mut rig = FakeRig::new(0);
        rig.connected = false;
        let (out, pauses) = run(&mut rig, &req());
        match out {
            RetryOutcome::Failed {
                attempts,
                error,
                notify,
            } => {
                assert_eq!(attempts, 3);
                assert!(notify);
                assert!(error.to_string().contains("no radio connected"), "{error}");
            }
            other => panic!("expected failure, got {other:?}"),
        }
        assert_eq!(rig.calls, 0);
        assert_eq!(pauses.len(), 2);
    }

    #[test]
    fn invalid_shift_is_a_skip_not_a_retry() {
        let mut bad = req();
        bad.freq_out_mhz = 145.2375;
        bad.shift_mhz = 287.9375;
        let mut rig = FakeRig::new(0);
        let (out, pauses) = run(&mut rig, &bad);
        match out {
            RetryOutcome::Failed {
                attempts, notify, ..
            } => {
                assert_eq!(attempts, 1);
                assert!(!notify, "a skip is not a radio refusal");
            }
            other => panic!("expected skip, got {other:?}"),
        }
        assert_eq!(rig.calls, 0);
        assert!(pauses.is_empty());
    }

    #[test]
    fn message_names_the_repeater_and_the_frequency() {
        let m = failure_message("LA5MR", 145.6, 3);
        assert!(m.contains("LA5MR"), "{m}");
        assert!(m.contains("145.6000 MHz"), "{m}");
        assert!(m.contains("3 attempts"), "{m}");
    }
}
