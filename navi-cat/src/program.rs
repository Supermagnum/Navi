//! Program VFO 1 with read-back verification.

use serde::{Deserialize, Serialize};

use crate::types::{RigBackend, RigError, ShiftDir, VfoState};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgramRequest {
    pub freq_out_mhz: f64,
    pub shift_mhz: f64,
    pub ctcss_hz: Option<f64>,
    pub dcs_code: Option<u32>,
    #[serde(default = "default_mode")]
    pub mode: String,
    #[serde(default = "default_passband")]
    pub passband_hz: u32,
    #[serde(default = "default_vfo")]
    pub vfo: String,
    #[serde(default)]
    pub leaves_follow: bool,
}

fn default_mode() -> String {
    "FM".into()
}
fn default_passband() -> u32 {
    12_500
}
fn default_vfo() -> String {
    "VFOA".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportedVfo {
    pub vfo: String,
    pub freq_hz: u64,
    pub mode: String,
    pub passband_hz: u32,
    pub shift: ShiftDir,
    pub offset_hz: u64,
    pub ctcss_tenths_hz: Option<u32>,
    pub dcs_code: Option<u32>,
}

impl From<VfoState> for ReportedVfo {
    fn from(s: VfoState) -> Self {
        Self {
            vfo: s.vfo,
            freq_hz: s.freq_hz,
            mode: s.mode,
            passband_hz: s.passband_hz,
            shift: s.shift,
            offset_hz: s.offset_hz,
            ctcss_tenths_hz: s.ctcss_tenths_hz,
            dcs_code: s.dcs_code,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProgramError {
    #[error("backend not gated: {0}")]
    NotGated(String),
    #[error("PTT active — refuse to program")]
    PttActive,
    #[error("DCD active — refuse follow retune")]
    DcdActive,
    #[error("rig: {0}")]
    Rig(#[from] RigError),
    #[error("read-back mismatch on {field}: requested={requested} reported={reported}")]
    ReadbackMismatch {
        field: String,
        requested: String,
        reported: String,
    },
    #[error("field unverified (cannot read back): {0}")]
    Unverified(String),
}

impl ProgramError {
    pub fn field(&self) -> Option<&str> {
        match self {
            Self::ReadbackMismatch { field, .. } => Some(field),
            Self::Unverified(f) => Some(f),
            _ => None,
        }
    }
}

fn want_from_request(req: &ProgramRequest) -> VfoState {
    let freq_hz = (req.freq_out_mhz * 1_000_000.0).round() as u64;
    let offset_hz = (req.shift_mhz.abs() * 1_000_000.0).round() as u64;
    let shift = if req.shift_mhz < 0.0 {
        ShiftDir::Minus
    } else if req.shift_mhz > 0.0 {
        ShiftDir::Plus
    } else {
        ShiftDir::None
    };
    let ctcss_tenths_hz = req.ctcss_hz.map(|hz| (hz * 10.0).round() as u32);
    VfoState {
        vfo: req.vfo.clone(),
        freq_hz,
        mode: req.mode.clone(),
        passband_hz: req.passband_hz,
        shift,
        offset_hz,
        ctcss_tenths_hz,
        dcs_code: req.dcs_code,
    }
}

fn freq_matches(want: u64, got: u64, step: u64) -> bool {
    if want == got {
        return true;
    }
    if step == 0 {
        return false;
    }
    want.abs_diff(got) <= step
}

fn compare(want: &VfoState, got: &VfoState, step: u64) -> Result<(), ProgramError> {
    if want.vfo != got.vfo {
        return Err(ProgramError::ReadbackMismatch {
            field: "vfo".into(),
            requested: want.vfo.clone(),
            reported: got.vfo.clone(),
        });
    }
    if !freq_matches(want.freq_hz, got.freq_hz, step) {
        return Err(ProgramError::ReadbackMismatch {
            field: "frequency".into(),
            requested: want.freq_hz.to_string(),
            reported: got.freq_hz.to_string(),
        });
    }
    if want.mode != got.mode {
        return Err(ProgramError::ReadbackMismatch {
            field: "mode".into(),
            requested: want.mode.clone(),
            reported: got.mode.clone(),
        });
    }
    if want.passband_hz != got.passband_hz {
        return Err(ProgramError::ReadbackMismatch {
            field: "passband".into(),
            requested: want.passband_hz.to_string(),
            reported: got.passband_hz.to_string(),
        });
    }
    if want.shift != got.shift {
        return Err(ProgramError::ReadbackMismatch {
            field: "shift".into(),
            requested: format!("{:?}", want.shift),
            reported: format!("{:?}", got.shift),
        });
    }
    if want.offset_hz != got.offset_hz {
        return Err(ProgramError::ReadbackMismatch {
            field: "offset".into(),
            requested: want.offset_hz.to_string(),
            reported: got.offset_hz.to_string(),
        });
    }
    if want.ctcss_tenths_hz != got.ctcss_tenths_hz {
        return Err(ProgramError::ReadbackMismatch {
            field: "ctcss".into(),
            requested: format!("{:?}", want.ctcss_tenths_hz),
            reported: format!("{:?}", got.ctcss_tenths_hz),
        });
    }
    if want.dcs_code != got.dcs_code {
        return Err(ProgramError::ReadbackMismatch {
            field: "dcs".into(),
            requested: format!("{:?}", want.dcs_code),
            reported: format!("{:?}", got.dcs_code),
        });
    }
    Ok(())
}

/// Set sequence + read-back; retry once on mismatch. Returns **reported** state.
pub fn program_vfo1_verified(
    backend: &mut impl RigBackend,
    req: &ProgramRequest,
) -> Result<ReportedVfo, ProgramError> {
    let gate = backend.gate();
    if !gate.allowed {
        return Err(ProgramError::NotGated(gate.reason));
    }
    if backend.get_ptt()? {
        return Err(ProgramError::PttActive);
    }
    let want = want_from_request(req);
    let step = backend.freq_step_hz();

    let mut last_err = None;
    for _attempt in 0..2 {
        backend.set_vfo_state(&want)?;
        let got = match backend.read_vfo_state() {
            Ok(s) => s,
            Err(RigError::Unsupported(f)) => return Err(ProgramError::Unverified(f)),
            Err(e) => return Err(e.into()),
        };
        match compare(&want, &got, step) {
            Ok(()) => return Ok(ReportedVfo::from(got)),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| ProgramError::Unverified("unknown".into())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gating::{GateDecision, GateStatus};

    struct MockRig {
        state: VfoState,
        ptt: bool,
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

    fn base_req() -> ProgramRequest {
        ProgramRequest {
            freq_out_mhz: 145.725,
            shift_mhz: -0.6,
            ctcss_hz: Some(88.5),
            dcs_code: None,
            mode: "FM".into(),
            passband_hz: 12500,
            vfo: "VFOA".into(),
            leaves_follow: false,
        }
    }

    #[test]
    fn happy_path_returns_reported() {
        let mut rig = MockRig {
            state: want_from_request(&base_req()),
            ptt: false,
            fail_ctcss_once: false,
            calls: 0,
        };
        let r = program_vfo1_verified(&mut rig, &base_req()).unwrap();
        assert_eq!(r.freq_hz, 145_725_000);
        assert_eq!(r.ctcss_tenths_hz, Some(885));
        assert_eq!(r.shift, ShiftDir::Minus);
    }

    #[test]
    fn ptt_blocks() {
        let mut rig = MockRig {
            state: want_from_request(&base_req()),
            ptt: true,
            fail_ctcss_once: false,
            calls: 0,
        };
        assert!(matches!(
            program_vfo1_verified(&mut rig, &base_req()),
            Err(ProgramError::PttActive)
        ));
    }

    #[test]
    fn mismatch_retries_then_ok() {
        let mut rig = MockRig {
            state: want_from_request(&base_req()),
            ptt: false,
            fail_ctcss_once: true,
            calls: 0,
        };
        let r = program_vfo1_verified(&mut rig, &base_req()).unwrap();
        assert_eq!(r.ctcss_tenths_hz, Some(885));
        assert_eq!(rig.calls, 2);
    }
}
