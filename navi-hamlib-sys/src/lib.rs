//! Minimal Hamlib FFI for Navi CAT.
//!
//! Hand-written `extern "C"` bindings covering only the VFO program / read-back
//! surface. **Never** binds `rig_set_ptt`. Without the `link-hamlib` feature the
//! crate still compiles (types + stub implementations) so desktop CI can build
//! without `libhamlib.so`.

#![allow(non_camel_case_types)]
#![allow(dead_code)]
#![allow(clippy::missing_safety_doc)]

use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_uint, c_void};

pub type RIG = c_void;
pub type vfo_t = c_uint;
pub type freq_t = f64;
pub type rmode_t = u64;
pub type pbwidth_t = c_int;
pub type tone_t = c_uint;
pub type ptt_t = c_int;
pub type dcd_t = c_int;
pub type rptr_shift_t = c_int;
pub type rig_model_t = c_uint;

pub const RIG_VFO_A: vfo_t = 1 << 0;
pub const RIG_VFO_B: vfo_t = 1 << 1;
pub const RIG_MODE_FM: rmode_t = 1u64 << 5;
pub const RIG_MODE_FMN: rmode_t = 1u64 << 21;
pub const RIG_PTT_OFF: ptt_t = 0;
pub const RIG_PTT_ON: ptt_t = 1;
pub const RIG_DCD_OFF: dcd_t = 0;
pub const RIG_DCD_ON: dcd_t = 1;
pub const RIG_RPT_SHIFT_NONE: rptr_shift_t = 0;
pub const RIG_RPT_SHIFT_MINUS: rptr_shift_t = 1;
pub const RIG_RPT_SHIFT_PLUS: rptr_shift_t = 2;
pub const RIG_STATUS_ALPHA: c_int = 0;
pub const RIG_STATUS_UNTESTED: c_int = 1;
pub const RIG_STATUS_BETA: c_int = 2;
pub const RIG_STATUS_STABLE: c_int = 3;
pub const RIG_STATUS_BUGGY: c_int = 4;
pub const RIG_OK: c_int = 0;
pub const RIG_MODEL_DUMMY: rig_model_t = 1;

/// Truncated head of `struct rig_caps` (first fields only — ABI-stable prefix).
#[repr(C)]
pub struct RigCapsHead {
    pub rig_model: c_int,
    pub model_name: *const c_char,
    pub mfg_name: *const c_char,
    pub version: *const c_char,
    pub copyright: *const c_char,
    pub status: c_int,
}

/// Opaque RIG with caps pointer as first field (`struct s_rig`).
#[repr(C)]
pub struct RigHandle {
    pub caps: *const RigCapsHead,
}

// Hamlib `enum rig_function_e` ordinals (4.6.5 / 4.7.x)
const FN_SET_RPTR_SHIFT: c_int = 13;
const FN_SET_RPTR_OFFS: c_int = 15;
const FN_SET_CTCSS_TONE: c_int = 34;

#[cfg(feature = "link-hamlib")]
#[link(name = "hamlib")]
extern "C" {
    pub fn rig_init(rig_model: c_int) -> *mut RIG;
    pub fn rig_open(rig: *mut RIG) -> c_int;
    pub fn rig_close(rig: *mut RIG) -> c_int;
    pub fn rig_cleanup(rig: *mut RIG) -> c_int;
    pub fn rig_set_vfo(rig: *mut RIG, vfo: vfo_t) -> c_int;
    pub fn rig_set_freq(rig: *mut RIG, vfo: vfo_t, freq: freq_t) -> c_int;
    pub fn rig_set_mode(rig: *mut RIG, vfo: vfo_t, mode: rmode_t, width: pbwidth_t) -> c_int;
    pub fn rig_set_rptr_shift(rig: *mut RIG, vfo: vfo_t, rptr_shift: rptr_shift_t) -> c_int;
    pub fn rig_set_rptr_offs(rig: *mut RIG, vfo: vfo_t, rptr_offs: c_int) -> c_int;
    pub fn rig_set_ctcss_tone(rig: *mut RIG, vfo: vfo_t, tone: tone_t) -> c_int;
    pub fn rig_set_dcs_code(rig: *mut RIG, vfo: vfo_t, code: tone_t) -> c_int;
    pub fn rig_get_vfo(rig: *mut RIG, vfo: *mut vfo_t) -> c_int;
    pub fn rig_get_freq(rig: *mut RIG, vfo: vfo_t, freq: *mut freq_t) -> c_int;
    pub fn rig_get_mode(
        rig: *mut RIG,
        vfo: vfo_t,
        mode: *mut rmode_t,
        width: *mut pbwidth_t,
    ) -> c_int;
    pub fn rig_get_rptr_shift(rig: *mut RIG, vfo: vfo_t, rptr_shift: *mut rptr_shift_t) -> c_int;
    pub fn rig_get_rptr_offs(rig: *mut RIG, vfo: vfo_t, rptr_offs: *mut c_int) -> c_int;
    pub fn rig_get_ctcss_tone(rig: *mut RIG, vfo: vfo_t, tone: *mut tone_t) -> c_int;
    pub fn rig_get_dcs_code(rig: *mut RIG, vfo: vfo_t, code: *mut tone_t) -> c_int;
    pub fn rig_get_ptt(rig: *mut RIG, vfo: vfo_t, ptt: *mut ptt_t) -> c_int;
    pub fn rig_get_dcd(rig: *mut RIG, vfo: vfo_t, dcd: *mut dcd_t) -> c_int;
    pub fn rig_get_info(rig: *mut RIG) -> *const c_char;
    pub fn rig_get_caps(rig_model: c_int) -> *const RigCapsHead;
    pub fn rig_get_function_ptr(rig_model: c_int, func: c_int) -> *mut c_void;
}

#[cfg(not(feature = "link-hamlib"))]
mod stubs {
    use super::*;

    pub unsafe fn rig_init(_rig_model: c_int) -> *mut RIG {
        std::ptr::null_mut()
    }
    pub unsafe fn rig_open(_rig: *mut RIG) -> c_int {
        -4
    }
    pub unsafe fn rig_close(_rig: *mut RIG) -> c_int {
        -4
    }
    pub unsafe fn rig_cleanup(_rig: *mut RIG) -> c_int {
        -4
    }
    pub unsafe fn rig_set_vfo(_rig: *mut RIG, _vfo: vfo_t) -> c_int {
        -4
    }
    pub unsafe fn rig_set_freq(_rig: *mut RIG, _vfo: vfo_t, _freq: freq_t) -> c_int {
        -4
    }
    pub unsafe fn rig_set_mode(
        _rig: *mut RIG,
        _vfo: vfo_t,
        _mode: rmode_t,
        _width: pbwidth_t,
    ) -> c_int {
        -4
    }
    pub unsafe fn rig_set_rptr_shift(_rig: *mut RIG, _vfo: vfo_t, _rptr_shift: rptr_shift_t) -> c_int {
        -4
    }
    pub unsafe fn rig_set_rptr_offs(_rig: *mut RIG, _vfo: vfo_t, _rptr_offs: c_int) -> c_int {
        -4
    }
    pub unsafe fn rig_set_ctcss_tone(_rig: *mut RIG, _vfo: vfo_t, _tone: tone_t) -> c_int {
        -4
    }
    pub unsafe fn rig_set_dcs_code(_rig: *mut RIG, _vfo: vfo_t, _code: tone_t) -> c_int {
        -4
    }
    pub unsafe fn rig_get_vfo(_rig: *mut RIG, _vfo: *mut vfo_t) -> c_int {
        -4
    }
    pub unsafe fn rig_get_freq(_rig: *mut RIG, _vfo: vfo_t, _freq: *mut freq_t) -> c_int {
        -4
    }
    pub unsafe fn rig_get_mode(
        _rig: *mut RIG,
        _vfo: vfo_t,
        _mode: *mut rmode_t,
        _width: *mut pbwidth_t,
    ) -> c_int {
        -4
    }
    pub unsafe fn rig_get_rptr_shift(
        _rig: *mut RIG,
        _vfo: vfo_t,
        _rptr_shift: *mut rptr_shift_t,
    ) -> c_int {
        -4
    }
    pub unsafe fn rig_get_rptr_offs(_rig: *mut RIG, _vfo: vfo_t, _rptr_offs: *mut c_int) -> c_int {
        -4
    }
    pub unsafe fn rig_get_ctcss_tone(_rig: *mut RIG, _vfo: vfo_t, _tone: *mut tone_t) -> c_int {
        -4
    }
    pub unsafe fn rig_get_dcs_code(_rig: *mut RIG, _vfo: vfo_t, _code: *mut tone_t) -> c_int {
        -4
    }
    pub unsafe fn rig_get_ptt(_rig: *mut RIG, _vfo: vfo_t, _ptt: *mut ptt_t) -> c_int {
        -4
    }
    pub unsafe fn rig_get_dcd(_rig: *mut RIG, _vfo: vfo_t, _dcd: *mut dcd_t) -> c_int {
        -4
    }
    pub unsafe fn rig_get_info(_rig: *mut RIG) -> *const c_char {
        std::ptr::null()
    }
    pub unsafe fn rig_get_caps(_rig_model: c_int) -> *const RigCapsHead {
        std::ptr::null()
    }
    pub unsafe fn rig_get_function_ptr(_rig_model: c_int, _func: c_int) -> *mut c_void {
        std::ptr::null_mut()
    }
}

#[cfg(not(feature = "link-hamlib"))]
pub use stubs::*;

fn cstr_or_empty(p: *const c_char) -> String {
    if p.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(p) }
        .to_string_lossy()
        .into_owned()
}

fn status_name(status: c_int) -> &'static str {
    match status {
        RIG_STATUS_STABLE => "Stable",
        RIG_STATUS_BETA => "Beta",
        RIG_STATUS_ALPHA => "Alpha",
        RIG_STATUS_UNTESTED => "Untested",
        RIG_STATUS_BUGGY => "Buggy",
        _ => "Unknown",
    }
}

/// Caps access helper: model name from an opened RIG.
pub unsafe fn caps_model_name(rig: *mut RIG) -> Option<String> {
    if rig.is_null() {
        return None;
    }
    let head = &*(rig as *const RigHandle);
    if head.caps.is_null() {
        return None;
    }
    Some(cstr_or_empty((*head.caps).model_name))
}

/// Caps access helper: synthesize a minimal dump_caps-like text for gating.
pub unsafe fn caps_dump_text(rig: *mut RIG) -> Option<String> {
    if rig.is_null() {
        return None;
    }
    let head = &*(rig as *const RigHandle);
    if head.caps.is_null() {
        return None;
    }
    let caps = &*head.caps;
    let model = caps.rig_model;
    let has = |f: c_int| !rig_get_function_ptr(model, f).is_null();
    let yn = |b: bool| if b { "Y" } else { "N" };
    Some(format!(
        "Caps dump for model: {model}\n\
         Model name:\t{name}\n\
         Backend status:\t{status}\n\
         Can set Repeater Shift:\t{shift}\n\
         Can set Repeater Offset:\t{offs}\n\
         Can set CTCSS Tone:\t{ctcss}\n",
        name = cstr_or_empty(caps.model_name),
        status = status_name(caps.status),
        shift = yn(has(FN_SET_RPTR_SHIFT)),
        offs = yn(has(FN_SET_RPTR_OFFS)),
        ctcss = yn(has(FN_SET_CTCSS_TONE)),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stubs_do_not_link() {
        unsafe {
            assert!(rig_init(RIG_MODEL_DUMMY as i32).is_null());
        }
    }

    #[test]
    fn source_never_binds_set_ptt() {
        let src = include_str!("lib.rs");
        let forbidden = format!("fn rig_set_{}", "ptt");
        let lines: Vec<_> = src
            .lines()
            .filter(|l| !l.trim_start().starts_with("//") && !l.trim_start().starts_with("#!"))
            .filter(|l| l.contains(&forbidden) || l.contains("rig_set_ptt"))
            .collect();
        // Only doc/comment mentions are OK; executable/bind lines must not appear.
        for line in &lines {
            assert!(
                line.contains("Never") || line.contains("never") || line.contains("forbidden"),
                "unexpected binding line: {line}"
            );
        }
    }
}
