//! Optional FFI backend wrapping `navi-hamlib-sys`.
//!
//! Built when feature `ffi` is enabled (links libhamlib). Without that feature
//! [`FfiRigBackend::open`] returns an error.

use crate::gating::GateDecision;
#[cfg(feature = "ffi")]
use crate::gating::gate_from_dump_caps;
use crate::types::{RigBackend, RigError, VfoState};
#[cfg(feature = "ffi")]
use crate::types::ShiftDir;

pub struct FfiRigBackend {
    #[cfg(feature = "ffi")]
    rig: *mut navi_hamlib_sys::RIG,
    gate: GateDecision,
    model: i32,
    model_name: String,
}

#[cfg(feature = "ffi")]
unsafe impl Send for FfiRigBackend {}

impl FfiRigBackend {
    pub fn open(model: i32, allow_beta: bool) -> Result<Self, RigError> {
        #[cfg(feature = "ffi")]
        {
            unsafe {
                let rig = navi_hamlib_sys::rig_init(model);
                if rig.is_null() {
                    return Err(RigError::Protocol("rig_init failed".into()));
                }
                let rc = navi_hamlib_sys::rig_open(rig);
                if rc != navi_hamlib_sys::RIG_OK {
                    let _ = navi_hamlib_sys::rig_cleanup(rig);
                    return Err(RigError::Protocol(format!("rig_open failed: {rc}")));
                }
                let allow_dummy = cfg!(test) || cfg!(feature = "allow-dummy-rig");
                let dump = navi_hamlib_sys::caps_dump_text(rig).unwrap_or_default();
                let gate = gate_from_dump_caps(&dump, allow_beta, allow_dummy);
                let model_name = navi_hamlib_sys::caps_model_name(rig).unwrap_or_else(|| "unknown".into());
                Ok(Self {
                    rig,
                    gate,
                    model,
                    model_name,
                })
            }
        }
        #[cfg(not(feature = "ffi"))]
        {
            let _ = (model, allow_beta);
            Err(RigError::Unsupported(
                "ffi feature is not enabled (navi-hamlib-sys/link-hamlib)".into(),
            ))
        }
    }
}

#[cfg(feature = "ffi")]
impl Drop for FfiRigBackend {
    fn drop(&mut self) {
        unsafe {
            if !self.rig.is_null() {
                let _ = navi_hamlib_sys::rig_close(self.rig);
                let _ = navi_hamlib_sys::rig_cleanup(self.rig);
                self.rig = std::ptr::null_mut();
            }
        }
    }
}

impl RigBackend for FfiRigBackend {
    fn is_connected(&self) -> bool {
        #[cfg(feature = "ffi")]
        {
            !self.rig.is_null()
        }
        #[cfg(not(feature = "ffi"))]
        {
            false
        }
    }

    fn model_name(&self) -> String {
        self.model_name.clone()
    }

    fn model_number(&self) -> i32 {
        self.model
    }

    fn gate(&self) -> GateDecision {
        self.gate.clone()
    }

    fn get_ptt(&mut self) -> Result<bool, RigError> {
        #[cfg(feature = "ffi")]
        {
            unsafe {
                let mut ptt = navi_hamlib_sys::RIG_PTT_OFF;
                let rc = navi_hamlib_sys::rig_get_ptt(self.rig, navi_hamlib_sys::RIG_VFO_A, &mut ptt);
                if rc != navi_hamlib_sys::RIG_OK {
                    return Err(RigError::Protocol(format!("get_ptt {rc}")));
                }
                Ok(ptt != navi_hamlib_sys::RIG_PTT_OFF)
            }
        }
        #[cfg(not(feature = "ffi"))]
        {
            Err(RigError::Unsupported("ffi disabled".into()))
        }
    }

    fn get_dcd(&mut self) -> Result<bool, RigError> {
        #[cfg(feature = "ffi")]
        {
            unsafe {
                let mut dcd = navi_hamlib_sys::RIG_DCD_OFF;
                let rc = navi_hamlib_sys::rig_get_dcd(self.rig, navi_hamlib_sys::RIG_VFO_A, &mut dcd);
                if rc != navi_hamlib_sys::RIG_OK {
                    return Ok(false);
                }
                Ok(dcd == navi_hamlib_sys::RIG_DCD_ON)
            }
        }
        #[cfg(not(feature = "ffi"))]
        {
            Err(RigError::Unsupported("ffi disabled".into()))
        }
    }

    fn set_vfo_state(&mut self, want: &VfoState) -> Result<(), RigError> {
        #[cfg(feature = "ffi")]
        {
            unsafe {
                let vfo = navi_hamlib_sys::RIG_VFO_A;
                check(navi_hamlib_sys::rig_set_vfo(self.rig, vfo))?;
                check(navi_hamlib_sys::rig_set_freq(self.rig, vfo, want.freq_hz as f64))?;
                check(navi_hamlib_sys::rig_set_mode(
                    self.rig,
                    vfo,
                    navi_hamlib_sys::RIG_MODE_FM,
                    want.passband_hz as i32,
                ))?;
                let shift = match want.shift {
                    ShiftDir::None => navi_hamlib_sys::RIG_RPT_SHIFT_NONE,
                    ShiftDir::Minus => navi_hamlib_sys::RIG_RPT_SHIFT_MINUS,
                    ShiftDir::Plus => navi_hamlib_sys::RIG_RPT_SHIFT_PLUS,
                };
                check(navi_hamlib_sys::rig_set_rptr_shift(self.rig, vfo, shift))?;
                check(navi_hamlib_sys::rig_set_rptr_offs(
                    self.rig,
                    vfo,
                    want.offset_hz as i32,
                ))?;
                check(navi_hamlib_sys::rig_set_ctcss_tone(
                    self.rig,
                    vfo,
                    want.ctcss_tenths_hz.unwrap_or(0),
                ))?;
                check(navi_hamlib_sys::rig_set_dcs_code(
                    self.rig,
                    vfo,
                    want.dcs_code.unwrap_or(0),
                ))?;
            }
            Ok(())
        }
        #[cfg(not(feature = "ffi"))]
        {
            let _ = want;
            Err(RigError::Unsupported("ffi disabled".into()))
        }
    }

    fn read_vfo_state(&mut self) -> Result<VfoState, RigError> {
        #[cfg(feature = "ffi")]
        {
            unsafe {
                let vfo = navi_hamlib_sys::RIG_VFO_A;
                let mut freq = 0.0;
                check(navi_hamlib_sys::rig_get_freq(self.rig, vfo, &mut freq))?;
                let mut mode: navi_hamlib_sys::rmode_t = 0;
                let mut width = 0i32;
                check(navi_hamlib_sys::rig_get_mode(self.rig, vfo, &mut mode, &mut width))?;
                let mut shift = navi_hamlib_sys::RIG_RPT_SHIFT_NONE;
                check(navi_hamlib_sys::rig_get_rptr_shift(self.rig, vfo, &mut shift))?;
                let mut offs = 0i32;
                check(navi_hamlib_sys::rig_get_rptr_offs(self.rig, vfo, &mut offs))?;
                let mut ctcss = 0u32;
                check(navi_hamlib_sys::rig_get_ctcss_tone(self.rig, vfo, &mut ctcss))?;
                let mut dcs = 0u32;
                check(navi_hamlib_sys::rig_get_dcs_code(self.rig, vfo, &mut dcs))?;
                Ok(VfoState {
                    vfo: "VFOA".into(),
                    freq_hz: freq.round() as u64,
                    mode: "FM".into(),
                    passband_hz: width as u32,
                    shift: match shift {
                        x if x == navi_hamlib_sys::RIG_RPT_SHIFT_MINUS => ShiftDir::Minus,
                        x if x == navi_hamlib_sys::RIG_RPT_SHIFT_PLUS => ShiftDir::Plus,
                        _ => ShiftDir::None,
                    },
                    offset_hz: offs as u64,
                    ctcss_tenths_hz: (ctcss > 0).then_some(ctcss),
                    dcs_code: (dcs > 0).then_some(dcs),
                })
            }
        }
        #[cfg(not(feature = "ffi"))]
        {
            Err(RigError::Unsupported("ffi disabled".into()))
        }
    }
}

#[cfg(feature = "ffi")]
fn check(rc: i32) -> Result<(), RigError> {
    if rc == navi_hamlib_sys::RIG_OK {
        Ok(())
    } else {
        Err(RigError::Protocol(format!("hamlib error {rc}")))
    }
}
