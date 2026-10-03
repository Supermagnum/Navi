//! MegaSquirt / Speeduino realtime wideband helpers (no serial polling).
//!
//! Offsets are keyed by firmware family. They are Navi decode tables for tests,
//! not a TunerStudio `.ini` parser. A live plugin must match the signature.

use super::fuel::{
    blend_stoich_afr_mass, classify_lambda, LambdaState, LAMBDA_SATURATION_FRAC, PETROL_STOICH_AFR,
};

pub const MS_WB_AFR_MIN: f64 = 7.4;
pub const MS_WB_AFR_MAX: f64 = 22.4;
pub const MS_WB_WARMUP_S: f64 = 30.0;
pub const MS_WB_LINEAR_AFR_AT_0V: f64 = 7.35;
pub const MS_WB_LINEAR_AFR_AT_5V: f64 = 22.39;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MsFirmwareKind {
    Ms2Extra,
    Ms3,
    Speeduino,
}

#[derive(Debug, Clone, Copy)]
pub struct MsRealtimeLayout {
    pub seconds_off: usize,
    pub afr1_off: usize,
    pub afr2_off: Option<usize>,
    pub afr_target_off: usize,
    pub afr_scale: f64,
    pub min_len: usize,
}

impl MsFirmwareKind {
    pub fn from_signature(sig: &str) -> Option<Self> {
        let u = sig.to_ascii_uppercase();
        if u.contains("SPEEDUINO") {
            Some(Self::Speeduino)
        } else if u.contains("MS2") || u.contains("MSNS") {
            Some(Self::Ms2Extra)
        } else if u.contains("MS3") {
            Some(Self::Ms3)
        } else {
            None
        }
    }

    pub fn layout(self) -> MsRealtimeLayout {
        match self {
            Self::Ms2Extra => MsRealtimeLayout {
                seconds_off: 0,
                afr1_off: 22,
                afr2_off: Some(23),
                afr_target_off: 19,
                afr_scale: 0.1,
                min_len: 24,
            },
            Self::Ms3 => MsRealtimeLayout {
                seconds_off: 0,
                afr1_off: 52,
                afr2_off: Some(53),
                afr_target_off: 54,
                afr_scale: 0.1,
                min_len: 56,
            },
            Self::Speeduino => MsRealtimeLayout {
                seconds_off: 0,
                afr1_off: 10,
                afr2_off: Some(11),
                afr_target_off: 12,
                afr_scale: 0.1,
                min_len: 14,
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MsWidebandSettings {
    pub ecu_stoich: f64,
    pub reports_lambda: bool,
    pub warmup_s: f64,
    pub firmware_sensor_warming: bool,
}

impl Default for MsWidebandSettings {
    fn default() -> Self {
        Self {
            ecu_stoich: PETROL_STOICH_AFR,
            reports_lambda: false,
            warmup_s: MS_WB_WARMUP_S,
            firmware_sensor_warming: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MsWidebandSample {
    pub afr_reported: Option<f64>,
    pub lambda: Option<f64>,
    pub afr_real: Option<f64>,
    pub afr_target_reported: Option<f64>,
    pub lambda_commanded: Option<f64>,
    pub state: LambdaState,
    pub probes_used: u8,
    pub seconds_running: Option<f64>,
}

impl Default for MsWidebandSample {
    fn default() -> Self {
        Self {
            afr_reported: None,
            lambda: None,
            afr_real: None,
            afr_target_reported: None,
            lambda_commanded: None,
            state: LambdaState::NotReady,
            probes_used: 0,
            seconds_running: None,
        }
    }
}

pub fn afr_from_controller_volts(volts: f64, afr_at_0v: f64, afr_at_5v: f64) -> Option<f64> {
    if !volts.is_finite() || !afr_at_0v.is_finite() || !afr_at_5v.is_finite() {
        return None;
    }
    if !(0.0..=5.0).contains(&volts) {
        return None;
    }
    Some(afr_at_0v + (afr_at_5v - afr_at_0v) * (volts / 5.0))
}

pub fn ms_afr_to_lambda(afr_reported: f64, ecu_stoich: f64) -> Option<f64> {
    if !afr_reported.is_finite() || !ecu_stoich.is_finite() || ecu_stoich <= 0.0 {
        return None;
    }
    Some(afr_reported / ecu_stoich)
}

pub fn lambda_to_blend_afr(lambda: f64, ethanol_pct: Option<f64>) -> f64 {
    let v = ethanol_pct
        .map(|p| (p / 100.0).clamp(0.0, 1.0))
        .unwrap_or(0.0);
    lambda * blend_stoich_afr_mass(v)
}

pub fn classify_ms_reported_afr(afr_reported: f64) -> LambdaState {
    classify_lambda(afr_reported, MS_WB_AFR_MIN, MS_WB_AFR_MAX)
}

fn probe_afr(raw: u8, scale: f64) -> Option<f64> {
    if raw == 0 || raw == 0xFF {
        None
    } else {
        Some(f64::from(raw) * scale)
    }
}

fn u16_le(block: &[u8], off: usize) -> Option<u16> {
    let b = block.get(off..off + 2)?;
    Some(u16::from(b[0]) | (u16::from(b[1]) << 8))
}

pub fn wideband_ready(
    seconds_running: Option<f64>,
    warmup_s: f64,
    firmware_warming: bool,
    at_rail: bool,
    stuck_at_rail: bool,
    rpm: Option<f64>,
    overrun: bool,
) -> bool {
    if firmware_warming {
        return false;
    }
    if let Some(s) = seconds_running {
        if s.is_finite() && s < warmup_s {
            return false;
        }
    }
    let running_normal = rpm.unwrap_or(0.0) >= 900.0 && !overrun;
    if stuck_at_rail && at_rail && running_normal {
        return false;
    }
    true
}

pub fn decode_wideband_from_values(
    afr_or_lambda: f64,
    settings: MsWidebandSettings,
    ethanol_pct: Option<f64>,
    seconds_running: Option<f64>,
    rpm: Option<f64>,
    overrun: bool,
    stuck_at_rail: bool,
) -> MsWidebandSample {
    let (afr_reported, lambda) = if settings.reports_lambda {
        (
            Some(afr_or_lambda * settings.ecu_stoich),
            Some(afr_or_lambda),
        )
    } else {
        (
            Some(afr_or_lambda),
            ms_afr_to_lambda(afr_or_lambda, settings.ecu_stoich),
        )
    };
    let afr_rep = afr_reported.unwrap_or(afr_or_lambda);
    let mut state = classify_ms_reported_afr(afr_rep);
    let at_rail = state.saturated();
    if !wideband_ready(
        seconds_running,
        settings.warmup_s,
        settings.firmware_sensor_warming,
        at_rail,
        stuck_at_rail,
        rpm,
        overrun,
    ) {
        state = LambdaState::NotReady;
    }
    let afr_real = lambda
        .filter(|_| state.usable())
        .map(|l| lambda_to_blend_afr(l, ethanol_pct));
    MsWidebandSample {
        afr_reported,
        lambda: lambda.filter(|_| state != LambdaState::NotReady),
        afr_real,
        afr_target_reported: None,
        lambda_commanded: None,
        state,
        probes_used: 1,
        seconds_running,
    }
}

pub fn decode_ms_realtime_wideband(
    signature: &str,
    block: &[u8],
    settings: MsWidebandSettings,
    ethanol_pct: Option<f64>,
    rpm: Option<f64>,
    overrun: bool,
    stuck_at_rail: bool,
) -> Option<MsWidebandSample> {
    let kind = MsFirmwareKind::from_signature(signature)?;
    let lay = kind.layout();
    if block.len() < lay.min_len {
        return None;
    }
    let seconds = u16_le(block, lay.seconds_off).map(f64::from);
    let mut vals = Vec::new();
    if let Some(a) = block
        .get(lay.afr1_off)
        .copied()
        .and_then(|b| probe_afr(b, lay.afr_scale))
    {
        vals.push(a);
    }
    if let Some(off) = lay.afr2_off {
        if let Some(a) = block
            .get(off)
            .copied()
            .and_then(|b| probe_afr(b, lay.afr_scale))
        {
            vals.push(a);
        }
    }
    let tgt = block
        .get(lay.afr_target_off)
        .copied()
        .and_then(|b| probe_afr(b, lay.afr_scale));
    if vals.is_empty() {
        return Some(MsWidebandSample {
            afr_target_reported: tgt,
            lambda_commanded: tgt.and_then(|a| ms_afr_to_lambda(a, settings.ecu_stoich)),
            seconds_running: seconds,
            ..MsWidebandSample::default()
        });
    }
    let mean = vals.iter().sum::<f64>() / vals.len() as f64;
    let mut sample = decode_wideband_from_values(
        mean,
        settings,
        ethanol_pct,
        seconds,
        rpm,
        overrun,
        stuck_at_rail,
    );
    sample.probes_used = vals.len() as u8;
    sample.afr_target_reported = tgt;
    sample.lambda_commanded = tgt.and_then(|a| ms_afr_to_lambda(a, settings.ecu_stoich));
    Some(sample)
}

pub fn encode_ms_realtime_test(
    kind: MsFirmwareKind,
    seconds: u16,
    afr1: Option<f64>,
    afr2: Option<f64>,
    afr_target: Option<f64>,
) -> Vec<u8> {
    let lay = kind.layout();
    let mut b = vec![0u8; lay.min_len];
    b[lay.seconds_off] = (seconds & 0xff) as u8;
    b[lay.seconds_off + 1] = (seconds >> 8) as u8;
    let pack = |a: Option<f64>| match a {
        None => 0xFF,
        Some(v) => ((v / lay.afr_scale).round() as u8).max(1),
    };
    b[lay.afr1_off] = pack(afr1);
    if let Some(off) = lay.afr2_off {
        b[off] = pack(afr2);
    }
    b[lay.afr_target_off] = pack(afr_target);
    b
}

pub fn ms_wb_sat_band() -> f64 {
    (MS_WB_AFR_MAX - MS_WB_AFR_MIN) * LAMBDA_SATURATION_FRAC
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn firmware_offsets_differ() {
        let a = MsFirmwareKind::Ms2Extra.layout().afr1_off;
        let b = MsFirmwareKind::Ms3.layout().afr1_off;
        let c = MsFirmwareKind::Speeduino.layout().afr1_off;
        assert_ne!(a, b);
        assert_ne!(b, c);
        assert_ne!(a, c);
        assert_eq!(
            MsFirmwareKind::from_signature("MS3 Format 0262.14"),
            Some(MsFirmwareKind::Ms3)
        );
        assert_eq!(
            MsFirmwareKind::from_signature("MS2Extra comms332"),
            Some(MsFirmwareKind::Ms2Extra)
        );
        assert_eq!(
            MsFirmwareKind::from_signature("speeduino 202401"),
            Some(MsFirmwareKind::Speeduino)
        );
        assert!(MsFirmwareKind::from_signature("unknown").is_none());
    }

    #[test]
    fn stoich_vectors() {
        let s = PETROL_STOICH_AFR;
        assert!((ms_afr_to_lambda(14.7, s).unwrap() - 1.000).abs() < 1e-9);
        assert!((ms_afr_to_lambda(12.5, s).unwrap() - 0.850).abs() < 0.002);
        let r = decode_wideband_from_values(
            7.4,
            MsWidebandSettings::default(),
            None,
            Some(120.0),
            Some(2500.0),
            false,
            false,
        );
        assert_eq!(r.state, LambdaState::SaturatedRich);
        assert!((r.lambda.unwrap() - 0.503).abs() < 0.002);
        let l = decode_wideband_from_values(
            22.4,
            MsWidebandSettings::default(),
            None,
            Some(120.0),
            Some(2500.0),
            false,
            false,
        );
        assert_eq!(l.state, LambdaState::SaturatedLean);
        assert!((l.lambda.unwrap() - 1.524).abs() < 0.002);
        let e85 = lambda_to_blend_afr(0.850, Some(85.0));
        assert!((e85 - 8.35).abs() < 0.05, "e85 afr={e85}");
    }

    #[test]
    fn linear_voltage_2_5() {
        let a =
            afr_from_controller_volts(2.5, MS_WB_LINEAR_AFR_AT_0V, MS_WB_LINEAR_AFR_AT_5V).unwrap();
        assert!((a - 14.87).abs() < 0.02, "afr={a}");
    }

    #[test]
    fn ecu_stoich_not_147() {
        let st = MsWidebandSettings {
            ecu_stoich: 14.1,
            ..Default::default()
        };
        let s = decode_wideband_from_values(14.1, st, None, Some(60.0), Some(2000.0), false, false);
        assert_eq!(s.state, LambdaState::Valid);
        assert!((s.lambda.unwrap() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn two_probes_ignore_fault() {
        let kind = MsFirmwareKind::Ms3;
        let block = encode_ms_realtime_test(kind, 90, Some(14.7), None, Some(14.7));
        let s = decode_ms_realtime_wideband(
            "MS3 Format 0262.14",
            &block,
            MsWidebandSettings::default(),
            None,
            Some(2000.0),
            false,
            false,
        )
        .unwrap();
        assert_eq!(s.probes_used, 1);
        assert!((s.afr_reported.unwrap() - 14.7).abs() < 0.05);
        let both = encode_ms_realtime_test(kind, 90, Some(14.5), Some(14.9), Some(14.7));
        let s2 = decode_ms_realtime_wideband(
            "MS3 Format 0262.14",
            &both,
            MsWidebandSettings::default(),
            None,
            Some(2000.0),
            false,
            false,
        )
        .unwrap();
        assert_eq!(s2.probes_used, 2);
        assert!((s2.afr_reported.unwrap() - 14.7).abs() < 0.05);
    }

    #[test]
    fn not_ready_warmup_and_stuck_rail() {
        let w = decode_wideband_from_values(
            14.7,
            MsWidebandSettings::default(),
            None,
            Some(10.0),
            Some(2000.0),
            false,
            false,
        );
        assert_eq!(w.state, LambdaState::NotReady);
        let stuck = decode_wideband_from_values(
            22.4,
            MsWidebandSettings::default(),
            None,
            Some(90.0),
            Some(2000.0),
            false,
            true,
        );
        assert_eq!(stuck.state, LambdaState::NotReady);
        let fw = MsWidebandSettings {
            firmware_sensor_warming: true,
            ..Default::default()
        };
        let f = decode_wideband_from_values(14.7, fw, None, Some(90.0), Some(2000.0), false, false);
        assert_eq!(f.state, LambdaState::NotReady);
    }

    #[test]
    fn reports_lambda_skips_division() {
        let st = MsWidebandSettings {
            reports_lambda: true,
            ..Default::default()
        };
        let s = decode_wideband_from_values(
            0.85,
            st,
            Some(85.0),
            Some(60.0),
            Some(2000.0),
            false,
            false,
        );
        assert_eq!(s.state, LambdaState::Valid);
        assert!((s.lambda.unwrap() - 0.85).abs() < 1e-9);
        assert!((s.afr_real.unwrap() - 8.35).abs() < 0.05);
    }
}
