//! Barometric / MAP helpers for ICE decode (no live adapter).
//!
//! PID `33` baro is kPa. Altitude uses the ISA troposphere inversion of
//! `P = 101.325 * (1 - 2.25577e-5 * h)^5.25588`.

/// Mean sea-level pressure used by the ISA model (kPa).
pub const ISA_SEA_LEVEL_KPA: f64 = 101.325;
const ISA_LAPSE: f64 = 2.25577e-5;
const ISA_EXP: f64 = 5.25588;

/// Standard-atmosphere geometric altitude (m) from station pressure (kPa).
/// Missing / non-finite / non-positive pressure → `None`.
pub fn altitude_m_from_baro_kpa(baro_kpa: Option<f64>) -> Option<f64> {
    let p = baro_kpa?;
    if !p.is_finite() || p <= 0.0 {
        return None;
    }
    let ratio = (p / ISA_SEA_LEVEL_KPA).clamp(1e-6, 2.0);
    let h = (1.0 - ratio.powf(1.0 / ISA_EXP)) / ISA_LAPSE;
    if h.is_finite() {
        Some(h)
    } else {
        None
    }
}

/// Sea-level ISA pressure check: ~0 m.
pub fn isa_sea_level_altitude_m() -> f64 {
    altitude_m_from_baro_kpa(Some(ISA_SEA_LEVEL_KPA)).unwrap_or(f64::NAN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sea_level_near_zero() {
        let h = altitude_m_from_baro_kpa(Some(ISA_SEA_LEVEL_KPA)).unwrap();
        assert!(h.abs() < 0.5);
    }

    #[test]
    fn missing_baro_is_none() {
        assert_eq!(altitude_m_from_baro_kpa(None), None);
        assert_eq!(altitude_m_from_baro_kpa(Some(0.0)), None);
    }

    #[test]
    fn lower_pressure_is_higher() {
        let h = altitude_m_from_baro_kpa(Some(90.0)).unwrap();
        assert!(h > 800.0 && h < 1200.0);
    }
}
