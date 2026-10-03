//! Pure ICE decode: ELM327 Mode 01 ASCII, J1939 SPNs, MegaSquirt injector math.
//!
//! No Bluetooth, serial, CAN, or WASM. Read-only PID/SPN numbers only
//! (no Mode 04 / burn). Optional sensors that are absent decode to `None`,
//! never a silent zero.

use super::ambient::altitude_m_from_baro_kpa;
use super::fuel::{
    derive_maf_fuel_rate_ex, detect_fuel_cut_ex, fuel_current_l, instant_l_per_100km,
    lambda_eq_max_from_pid4f_a, lambda_from_pid_raw, lambda_is_saturated, maf_to_fuel_rate_l_h,
    megasquirt_fuel_rate_l_h, stoich_afr, torque_bsfc_fuel_l_h, FuelCutInput, FuelRateQuality,
    FuelRateSource, IceFuelKind, DEFAULT_LAMBDA_EQ_MAX,
};
use super::LiveEnergySnapshot;

/// J1939 SPN 183 not-available sentinel (two-byte).
pub const J1939_SPN183_NA: u16 = 0xFFFF;
/// J1939 SPN 96 not-available sentinel (one-byte).
pub const J1939_SPN96_NA: u8 = 0xFF;
/// SPN 183 resolution (L/h per bit) — `mathematical-formulas.md`.
pub const SPN183_L_H_PER_BIT: f64 = 0.05;
/// SPN 96 resolution (% per bit).
pub const SPN96_PCT_PER_BIT: f64 = 0.4;

const ELM_NO_DATA: &[&str] = &[
    "NO DATA",
    "UNABLE TO CONNECT",
    "BUS INIT",
    "CAN ERROR",
    "STOPPED",
    "ERROR",
];

/// Decoded ICE quantities. Extra fields stay here; `LiveEnergySnapshot` is unchanged.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct IceDecode {
    pub rpm: Option<f64>,
    pub speed_kmh: Option<f64>,
    pub maf_g_s: Option<f64>,
    pub fuel_rate_l_h: Option<f64>,
    pub fuel_level_pct: Option<f64>,
    pub ethanol_pct: Option<f64>,
    pub calc_load_pct: Option<f64>,
    pub abs_load_pct: Option<f64>,
    pub baro_kpa: Option<f64>,
    pub map_kpa: Option<f64>,
    pub lambda: Option<f64>,
    pub lambda_raw: Option<u16>,
    pub lambda_eq_max: Option<f64>,
    pub lambda_saturated: bool,
    pub altitude_m: Option<f64>,
    pub coolant_c: Option<f64>,
    pub iat_c: Option<f64>,
    pub throttle_pct: Option<f64>,
    pub pedal_pct: Option<f64>,
    pub fuel_temp_c: Option<f64>,
    pub actual_torque_pct: Option<f64>,
    pub demand_torque_pct: Option<f64>,
    pub reference_torque_nm: Option<f64>,
    /// MAF fuel at the lambda cap; never copied into `fuel_rate_l_h`.
    pub fuel_rate_cap_l_h: Option<f64>,
    pub bsfc_g_kwh_override: Option<f64>,
    pub fuel_rate_source: FuelRateSource,
    pub fuel_rate_quality: FuelRateQuality,
}

impl IceDecode {
    pub fn to_live_snapshot(&self) -> LiveEnergySnapshot {
        LiveEnergySnapshot {
            fuel_rate_l_h: self.fuel_rate_l_h,
            state_of_charge_pct: None,
            power_kw: None,
        }
    }

    pub fn instant_l_per_100km(&self) -> Option<f64> {
        instant_l_per_100km(self.fuel_rate_l_h, self.speed_kmh)
    }

    pub fn fuel_current_l(&self, tank_capacity_l: Option<f64>) -> Option<f64> {
        fuel_current_l(self.fuel_level_pct, tank_capacity_l)
    }

    /// Source order: 5E/J1939/MegaSquirt, MAF+unsaturated lambda, torque×BSFC,
    /// petrol MAF stoich estimate, else None.
    pub fn finish_fuel_rate(&mut self, kind: IceFuelKind) {
        self.altitude_m = altitude_m_from_baro_kpa(self.baro_kpa);
        self.refresh_lambda();
        if detect_fuel_cut_ex(FuelCutInput {
            kind,
            rpm: self.rpm,
            speed_kmh: self.speed_kmh,
            fuel_rate_direct: self.fuel_rate_l_h,
            lambda: self.lambda,
            lambda_saturated: self.lambda_saturated,
            coolant_c: self.coolant_c,
            throttle_pct: self.throttle_pct.or(self.pedal_pct),
            actual_torque_pct: self.actual_torque_pct,
        }) {
            let from_5e =
                self.fuel_rate_l_h == Some(0.0) && self.fuel_rate_source == FuelRateSource::Pid5E;
            self.fuel_rate_l_h = Some(0.0);
            if !from_5e && self.fuel_rate_source == FuelRateSource::None {
                self.fuel_rate_source = if self.actual_torque_pct.map(|t| t <= 0.0) == Some(true) {
                    FuelRateSource::TorqueBsfc
                } else {
                    FuelRateSource::Pid5E
                };
            }
            self.fuel_rate_quality = if from_5e {
                FuelRateQuality::Measured
            } else if self.actual_torque_pct.map(|t| t <= 0.0) == Some(true) {
                FuelRateQuality::TorqueEstimate
            } else {
                FuelRateQuality::Measured
            };
            return;
        }
        if self.fuel_rate_l_h.is_some() {
            self.fuel_rate_quality = FuelRateQuality::Measured;
            return;
        }
        if self.lambda_saturated {
            if let (Some(maf), Some(lam)) = (self.maf_g_s, self.lambda) {
                let afr = lam * stoich_afr(kind, self.ethanol_pct);
                let rho =
                    super::fuel::fuel_density_kg_l_at(kind, self.ethanol_pct, self.fuel_temp_c);
                self.fuel_rate_cap_l_h = maf_to_fuel_rate_l_h(maf, afr, rho);
            }
        }
        let maf_lambda = if self.lambda_saturated {
            None
        } else {
            self.lambda
        };
        if maf_lambda.is_some() {
            if let Some(derived) = derive_maf_fuel_rate_ex(
                kind,
                self.maf_g_s,
                maf_lambda,
                self.ethanol_pct,
                self.fuel_temp_c,
                self.coolant_c,
                self.calc_load_pct,
            ) {
                self.fuel_rate_l_h = Some(derived.fuel_l_h);
                self.fuel_rate_source = FuelRateSource::MafDerived;
                self.fuel_rate_quality = derived.quality;
                return;
            }
        }
        if let Some(rate) = torque_bsfc_fuel_l_h(
            kind,
            self.actual_torque_pct,
            self.reference_torque_nm,
            self.rpm,
            self.speed_kmh,
            self.ethanol_pct,
            self.fuel_temp_c,
            self.bsfc_g_kwh_override,
        ) {
            self.fuel_rate_l_h = Some(rate);
            self.fuel_rate_source = FuelRateSource::TorqueBsfc;
            self.fuel_rate_quality = FuelRateQuality::TorqueEstimate;
            return;
        }
        if kind == IceFuelKind::Petrol {
            if let Some(derived) = derive_maf_fuel_rate_ex(
                kind,
                self.maf_g_s,
                None,
                self.ethanol_pct,
                self.fuel_temp_c,
                self.coolant_c,
                self.calc_load_pct,
            ) {
                self.fuel_rate_l_h = Some(derived.fuel_l_h);
                self.fuel_rate_source = FuelRateSource::MafDerived;
                self.fuel_rate_quality = derived.quality;
            }
        }
    }

    fn refresh_lambda(&mut self) {
        let max = self.lambda_eq_max.unwrap_or(DEFAULT_LAMBDA_EQ_MAX);
        if let Some(raw) = self.lambda_raw {
            let lam = lambda_from_pid_raw(raw, max);
            self.lambda = Some(lam);
            self.lambda_saturated = lambda_is_saturated(lam, max);
        }
    }
}

/// True when the ELM adapter reported a bus/session failure (not a zero reading).
pub fn elm327_is_no_data(text: &str) -> bool {
    let u = text.to_ascii_uppercase();
    if u.trim() == "?" {
        return true;
    }
    ELM_NO_DATA.iter().any(|p| u.contains(p))
}

fn hex_nibble(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'A'..=b'F' => Some(c - b'A' + 10),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    }
}

/// Collect hex bytes. Space-separated odd-length tokens (e.g. CAN ID `7E8`) are skipped.
pub fn parse_hex_bytes(text: &str) -> Option<Vec<u8>> {
    let has_space = text.bytes().any(|b| b.is_ascii_whitespace());
    if has_space {
        let mut out = Vec::new();
        for tok in text.split_whitespace() {
            if !tok.bytes().all(|b| hex_nibble(b).is_some()) {
                continue;
            }
            if tok.len() % 2 != 0 {
                continue;
            }
            let mut i = 0;
            let bytes = tok.as_bytes();
            while i < bytes.len() {
                let hi = hex_nibble(bytes[i])?;
                let lo = hex_nibble(bytes[i + 1])?;
                out.push((hi << 4) | lo);
                i += 2;
            }
        }
        if out.is_empty() {
            None
        } else {
            Some(out)
        }
    } else {
        let mut nibbles = Vec::new();
        for b in text.bytes() {
            if let Some(n) = hex_nibble(b) {
                nibbles.push(n);
            }
        }
        if nibbles.is_empty() || nibbles.len() % 2 != 0 {
            return None;
        }
        Some(
            nibbles
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| (c[0] << 4) | c[1])
                .collect(),
        )
    }
}

/// Mode 01 positive response: `41 <pid> <data…>` (ISO-TP PCI nibble may precede).
pub fn parse_mode01_payload(text: &str) -> Option<(u8, Vec<u8>)> {
    if elm327_is_no_data(text) {
        return None;
    }
    let bytes = parse_hex_bytes(text)?;
    // Scan for 0x41 <pid>.
    for i in 0..bytes.len().saturating_sub(1) {
        if bytes[i] == 0x41 {
            let pid = bytes[i + 1];
            let data = bytes[i + 2..].to_vec();
            if !data.is_empty() || matches!(pid, 0x00 | 0x20 | 0x40 | 0x60 | 0x80 | 0xA0) {
                return Some((pid, data));
            }
        }
    }
    None
}

fn u16_ab(data: &[u8]) -> Option<u16> {
    if data.len() < 2 {
        None
    } else {
        Some(u16::from(data[0]) * 256 + u16::from(data[1]))
    }
}

/// Decode one Mode 01 PID into `out`. Unknown PIDs are ignored. ICE only (no 5B).
pub fn apply_mode01_pid(out: &mut IceDecode, pid: u8, data: &[u8]) {
    match pid {
        0x0C => {
            if let Some(raw) = u16_ab(data) {
                out.rpm = Some(f64::from(raw) / 4.0);
            }
        }
        0x0D => {
            if let Some(a) = data.first() {
                out.speed_kmh = Some(f64::from(*a));
            }
        }
        0x10 => {
            if let Some(raw) = u16_ab(data) {
                out.maf_g_s = Some(f64::from(raw) / 100.0);
            }
        }
        0x2F => {
            if let Some(a) = data.first() {
                out.fuel_level_pct = Some(f64::from(*a) * 100.0 / 255.0);
            }
        }
        0x5E => {
            if let Some(raw) = u16_ab(data) {
                out.fuel_rate_l_h = Some(f64::from(raw) / 20.0);
                out.fuel_rate_source = FuelRateSource::Pid5E;
                out.fuel_rate_quality = FuelRateQuality::Measured;
            }
        }
        0x52 => {
            if let Some(a) = data.first() {
                out.ethanol_pct = Some(f64::from(*a) * 100.0 / 255.0);
            }
        }
        0x04 => {
            if let Some(a) = data.first() {
                out.calc_load_pct = Some(f64::from(*a) * 100.0 / 255.0);
            }
        }
        0x43 => {
            if let Some(raw) = u16_ab(data) {
                out.abs_load_pct = Some(f64::from(raw) * 100.0 / 255.0);
            }
        }
        0x33 => {
            if let Some(a) = data.first() {
                out.baro_kpa = Some(f64::from(*a));
            }
        }
        0x0B => {
            if let Some(a) = data.first() {
                out.map_kpa = Some(f64::from(*a));
            }
        }
        0x05 => {
            if let Some(a) = data.first() {
                out.coolant_c = Some(f64::from(*a) - 40.0);
            }
        }
        0x0F => {
            if let Some(a) = data.first() {
                out.iat_c = Some(f64::from(*a) - 40.0);
            }
        }
        0x11 => {
            if let Some(a) = data.first() {
                out.throttle_pct = Some(f64::from(*a) * 100.0 / 255.0);
            }
        }
        0x49 | 0x5A => {
            if let Some(a) = data.first() {
                out.pedal_pct = Some(f64::from(*a) * 100.0 / 255.0);
            }
        }
        0x44 | 0x24 | 0x34 => {
            if let Some(raw) = u16_ab(data) {
                out.lambda_raw = Some(raw);
                let max = out.lambda_eq_max.unwrap_or(DEFAULT_LAMBDA_EQ_MAX);
                let lam = lambda_from_pid_raw(raw, max);
                out.lambda = Some(lam);
                out.lambda_saturated = lambda_is_saturated(lam, max);
            }
        }
        0x4F => {
            if let Some(a) = data.first() {
                out.lambda_eq_max = Some(lambda_eq_max_from_pid4f_a(*a));
                if let Some(raw) = out.lambda_raw {
                    let max = out.lambda_eq_max.unwrap_or(DEFAULT_LAMBDA_EQ_MAX);
                    let lam = lambda_from_pid_raw(raw, max);
                    out.lambda = Some(lam);
                    out.lambda_saturated = lambda_is_saturated(lam, max);
                }
            }
        }
        0x61 => {
            if let Some(a) = data.first() {
                out.demand_torque_pct = Some(f64::from(*a) - 125.0);
            }
        }
        0x62 => {
            if let Some(a) = data.first() {
                out.actual_torque_pct = Some(f64::from(*a) - 125.0);
            }
        }
        0x63 => {
            if let Some(raw) = u16_ab(data) {
                out.reference_torque_nm = Some(f64::from(raw));
            }
        }
        _ => {}
    }
}

pub fn decode_elm327_mode01(text: &str, into: &mut IceDecode) -> bool {
    let Some((pid, data)) = parse_mode01_payload(text) else {
        return false;
    };
    apply_mode01_pid(into, pid, &data);
    true
}

/// SPN 183 engine fuel rate. `0xFFFF` → `None`.
pub fn decode_spn183_l_h(raw: u16) -> Option<f64> {
    if raw == J1939_SPN183_NA {
        None
    } else {
        Some(f64::from(raw) * SPN183_L_H_PER_BIT)
    }
}

/// SPN 96 fuel level %. `0xFF` → `None`.
pub fn decode_spn96_pct(raw: u8) -> Option<f64> {
    if raw == J1939_SPN96_NA {
        None
    } else {
        Some(f64::from(raw) * SPN96_PCT_PER_BIT)
    }
}

/// PGN 65266 (FEF2) Fuel Consumption — little-endian SPN 183 in bytes 0–1
/// (`mathematical-formulas.md`).
pub fn decode_pgn_65266_fuel_rate(payload: &[u8]) -> Option<f64> {
    if payload.len() < 2 {
        return None;
    }
    let raw = u16::from(payload[0]) | (u16::from(payload[1]) << 8);
    decode_spn183_l_h(raw)
}

/// ECU.md illustrative LFE (PGN 65257) layout: LE 16-bit * 0.05 L/h.
/// The Digital Annex places **SPN 183** on PGN 65266, not 65257 — see docs.
pub fn decode_pgn_65257_lfe_illustrative(payload: &[u8]) -> Option<f64> {
    if payload.len() < 2 {
        return None;
    }
    let raw = u16::from(payload[0]) | (u16::from(payload[1]) << 8);
    decode_spn183_l_h(raw)
}

pub fn apply_j1939_spn183(out: &mut IceDecode, raw: u16) {
    out.fuel_rate_l_h = decode_spn183_l_h(raw);
    if out.fuel_rate_l_h.is_some() {
        out.fuel_rate_source = FuelRateSource::J1939Spn183;
        out.fuel_rate_quality = FuelRateQuality::Measured;
    }
}

pub fn apply_j1939_lfe_illustrative(out: &mut IceDecode, payload: &[u8]) {
    out.fuel_rate_l_h = decode_pgn_65257_lfe_illustrative(payload);
    if out.fuel_rate_l_h.is_some() {
        out.fuel_rate_source = FuelRateSource::J1939LfeIllustrative;
        out.fuel_rate_quality = FuelRateQuality::Measured;
    }
}

pub fn apply_j1939_spn96(out: &mut IceDecode, raw: u8) {
    out.fuel_level_pct = decode_spn96_pct(raw);
}

/// SPN 174 Fuel Temperature 1, PGN 65262 Engine Temperature 1.
/// Digital Annex: 1 C/bit, offset −40; byte index 1 of the 8-byte PGN (after
/// SPN 110 coolant in byte 0). `0xFF` → `None`.
pub fn decode_spn174_c(raw: u8) -> Option<f64> {
    if raw == 0xFF {
        None
    } else {
        Some(f64::from(raw) - 40.0)
    }
}

pub fn apply_j1939_spn174(out: &mut IceDecode, raw: u8) {
    out.fuel_temp_c = decode_spn174_c(raw);
}

/// SPN 513 Actual Engine - Percent Torque (1 %/bit, offset −125).
pub fn decode_spn513_pct(raw: u8) -> f64 {
    f64::from(raw) - 125.0
}

pub fn apply_j1939_spn513(out: &mut IceDecode, raw: u8) {
    out.actual_torque_pct = Some(decode_spn513_pct(raw));
}

/// SPN 544 Engine Reference Torque (1 Nm/bit). `0xFFFF` → `None`.
pub fn decode_spn544_nm(raw: u16) -> Option<f64> {
    if raw == 0xFFFF {
        None
    } else {
        Some(f64::from(raw))
    }
}

pub fn apply_j1939_spn544(out: &mut IceDecode, raw: u16) {
    out.reference_torque_nm = decode_spn544_nm(raw);
}

/// Flex-fuel composition sensor: ethanol % from frequency (50 Hz = 0 %, 150 Hz = 100 %).
pub fn decode_flex_ethanol_pct_from_hz(frequency_hz: f64) -> Option<f64> {
    if !frequency_hz.is_finite() || !(45.0..=155.0).contains(&frequency_hz) {
        return None;
    }
    Some((frequency_hz - 50.0).clamp(0.0, 100.0))
}

/// Flex-fuel sensor pulse width → fuel temperature.
/// `fuel_temp_c = -40 + (pulse_ms - 1.0) * 41.25` (1 ms = −40 C, 5 ms = 125 C).
pub fn decode_flex_fuel_temp_c(pulse_ms: f64) -> Option<f64> {
    if !pulse_ms.is_finite() || !(1.0..=5.0).contains(&pulse_ms) {
        return None;
    }
    let t = -40.0 + (pulse_ms - 1.0) * 41.25;
    if !(-40.0..=125.0).contains(&t) {
        None
    } else {
        Some(t)
    }
}

pub fn apply_flex_sensor(out: &mut IceDecode, frequency_hz: f64, pulse_ms: f64) {
    out.ethanol_pct = decode_flex_ethanol_pct_from_hz(frequency_hz);
    out.fuel_temp_c = decode_flex_fuel_temp_c(pulse_ms);
}

/// Encode a Mode 01 positive response (no spaces) for tests / self-test.
pub fn encode_mode01(pid: u8, payload: &[u8]) -> String {
    let mut s = format!("41{pid:02X}");
    for b in payload {
        s.push_str(&format!("{b:02X}"));
    }
    s
}

/// Apply MegaSquirt injector math. Does **not** multiply again by ethanol %
/// (flexed pulse width is already corrected in the ECU).
pub fn apply_megasquirt(
    out: &mut IceDecode,
    pw_ms: f64,
    rpm: f64,
    n_cyl: u32,
    flow_cc_min: f64,
    ethanol_pct: Option<f64>,
) {
    out.rpm = Some(rpm);
    out.ethanol_pct = ethanol_pct;
    out.fuel_rate_l_h = megasquirt_fuel_rate_l_h(pw_ms, rpm, n_cyl, flow_cc_min);
    if out.fuel_rate_l_h.is_some() {
        out.fuel_rate_source = FuelRateSource::MegaSquirt;
        out.fuel_rate_quality = FuelRateQuality::Measured;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elm_5e_example_five_l_h() {
        let mut d = IceDecode::default();
        assert!(decode_elm327_mode01("415E0064", &mut d));
        assert_eq!(d.fuel_rate_l_h, Some(5.0));
        assert_eq!(d.fuel_rate_source, FuelRateSource::Pid5E);
    }

    #[test]
    fn elm_spaces_and_headers() {
        let mut d = IceDecode::default();
        assert!(decode_elm327_mode01("7E8 04 41 5E 00 64", &mut d));
        assert_eq!(d.fuel_rate_l_h, Some(5.0));
    }

    #[test]
    fn no_data_is_none() {
        let mut d = IceDecode::default();
        assert!(!decode_elm327_mode01("NO DATA", &mut d));
        assert_eq!(d.fuel_rate_l_h, None);
        assert!(!decode_elm327_mode01("?", &mut d));
    }

    #[test]
    fn pid_5e_zero_is_some_zero() {
        let mut d = IceDecode::default();
        assert!(decode_elm327_mode01("415E0000", &mut d));
        assert_eq!(d.fuel_rate_l_h, Some(0.0));
    }

    #[test]
    fn j1939_example_five_l_h() {
        assert_eq!(decode_spn183_l_h(100), Some(5.0));
        assert_eq!(decode_spn183_l_h(0xFFFF), None);
        assert_eq!(decode_pgn_65266_fuel_rate(&[0x64, 0x00]), Some(5.0));
        assert_eq!(decode_pgn_65257_lfe_illustrative(&[0x64, 0x00]), Some(5.0));
    }

    #[test]
    fn spn96_na_is_none() {
        assert_eq!(decode_spn96_pct(0xFF), None);
        assert!((decode_spn96_pct(125).unwrap() - 50.0).abs() < 1e-9);
    }

    #[test]
    fn pid_temps_throttle_pedal() {
        let mut d = IceDecode::default();
        decode_elm327_mode01("41057A", &mut d); // 122-40 = 82 C
        decode_elm327_mode01("410F3C", &mut d); // 60-40 = 20 C
        decode_elm327_mode01("411180", &mut d);
        decode_elm327_mode01("414980", &mut d);
        assert!((d.coolant_c.unwrap() - 82.0).abs() < 1e-9);
        assert!((d.iat_c.unwrap() - 20.0).abs() < 1e-9);
        assert!((d.throttle_pct.unwrap() - 50.196).abs() < 0.02);
        assert!((d.pedal_pct.unwrap() - 50.196).abs() < 0.02);
    }

    #[test]
    fn flex_and_spn174_fuel_temp() {
        assert_eq!(decode_flex_ethanol_pct_from_hz(50.0), Some(0.0));
        assert_eq!(decode_flex_ethanol_pct_from_hz(135.0), Some(85.0));
        assert_eq!(decode_flex_ethanol_pct_from_hz(150.0), Some(100.0));
        assert_eq!(decode_flex_ethanol_pct_from_hz(40.0), None);
        assert!((decode_flex_fuel_temp_c(1.0).unwrap() + 40.0).abs() < 1e-9);
        assert!((decode_flex_fuel_temp_c(2.333).unwrap() - 15.0).abs() < 0.05);
        assert!((decode_flex_fuel_temp_c(5.0).unwrap() - 125.0).abs() < 1e-9);
        assert_eq!(decode_flex_fuel_temp_c(0.5), None);
        assert_eq!(decode_spn174_c(55), Some(15.0));
        assert_eq!(decode_spn174_c(0xFF), None);
    }

    #[test]
    fn pid4f_scales_lambda_and_no_data() {
        let mut d = IceDecode::default();
        decode_elm327_mode01("4124FFFF", &mut d);
        assert!((d.lambda.unwrap() - 1.99997).abs() < 0.001);
        assert!(d.lambda_saturated);
        decode_elm327_mode01("414F04000000", &mut d);
        assert_eq!(d.lambda_eq_max, Some(4.0));
        assert!((d.lambda.unwrap() - 3.9999).abs() < 0.01);
        assert!(d.lambda_saturated);
        let mut e = IceDecode::default();
        decode_elm327_mode01("41248000", &mut e);
        assert!((e.lambda.unwrap() - 1.0).abs() < 0.002);
        assert!(!e.lambda_saturated);
        decode_elm327_mode01("414F00000000", &mut e);
        assert_eq!(e.lambda_eq_max, Some(2.0));
        let mut n = IceDecode::default();
        assert!(!decode_elm327_mode01("NO DATA", &mut n));
        assert!(n.lambda_eq_max.is_none());
        assert!(!n.lambda_saturated);
    }

    #[test]
    fn pid_torque_and_diesel_rejects_saturated_maf() {
        let mut d = IceDecode::default();
        decode_elm327_mode01("410C1F40", &mut d); // 2000 rpm
        decode_elm327_mode01("410D5A", &mut d);
        decode_elm327_mode01("411003E8", &mut d); // 10 g/s
        decode_elm327_mode01("4124FFFF", &mut d);
        decode_elm327_mode01("41629B", &mut d); // 155-125 = 30 %
        decode_elm327_mode01("416300D2", &mut d); // 210 Nm
        d.finish_fuel_rate(IceFuelKind::Diesel);
        assert!(d.lambda_saturated);
        assert_eq!(d.fuel_rate_source, FuelRateSource::TorqueBsfc);
        assert!(d.fuel_rate_l_h.unwrap() > 3.0);
        assert!(d.fuel_rate_cap_l_h.is_some());
    }
}
