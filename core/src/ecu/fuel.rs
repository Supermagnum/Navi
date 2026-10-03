//! ICE fuel-rate derivation (no live adapter).
//!
//! Formulas: repository `docs/mathematical-formulas.md`. Petrol stoichiometric
//! AFR 14.7 is **not** used for diesel unless a measured lambda / PID `5E` /
//! J1939 rate is present.

use super::LiveEnergySnapshot;

/// Liquid fuel assumed for AFR / density when deriving from MAF.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IceFuelKind {
    Petrol,
    Diesel,
}

/// Where `fuel_rate_l_h` came from (diagnostics only; not a snapshot field).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FuelRateSource {
    #[default]
    None,
    Pid5E,
    J1939Spn183,
    J1939LfeIllustrative,
    MafDerived,
    MegaSquirt,
}

/// Petrol stoich AFR (mass). Diesel must not silently reuse this.
pub const PETROL_STOICH_AFR: f64 = 14.7;
/// Typical diesel stoich AFR when converting measured lambda to AFR.
pub const DIESEL_STOICH_AFR: f64 = 14.5;
/// E100 stoich AFR (mass), used to interpolate flex blends.
pub const ETHANOL_STOICH_AFR: f64 = 9.0;
/// Petrol density kg/L from `mathematical-formulas.md` (not ECU.md 0.74).
pub const PETROL_DENSITY_KG_L: f64 = 0.745;
pub const DIESEL_DENSITY_KG_L: f64 = 0.832;
pub const ETHANOL_DENSITY_KG_L: f64 = 0.789;

/// MegaSquirt injector formula denominator (`mathematical-formulas.md`).
pub const MEGASQUIRT_RATE_DENOM: f64 = 2_000_000.0;

pub fn clamp_ethanol_fraction(ethanol_pct: Option<f64>) -> Option<f64> {
    let pct = ethanol_pct?;
    if !pct.is_finite() {
        return None;
    }
    Some((pct / 100.0).clamp(0.0, 1.0))
}

/// Stoichiometric AFR for MAF derivation. Diesel without a blend still uses
/// diesel stoich, never petrol 14.7, when lambda is applied.
pub fn stoich_afr(kind: IceFuelKind, ethanol_pct: Option<f64>) -> f64 {
    match kind {
        IceFuelKind::Diesel => DIESEL_STOICH_AFR,
        IceFuelKind::Petrol => match clamp_ethanol_fraction(ethanol_pct) {
            Some(e) => PETROL_STOICH_AFR * (1.0 - e) + ETHANOL_STOICH_AFR * e,
            None => PETROL_STOICH_AFR,
        },
    }
}

pub fn fuel_density_kg_l(kind: IceFuelKind, ethanol_pct: Option<f64>) -> f64 {
    match kind {
        IceFuelKind::Diesel => DIESEL_DENSITY_KG_L,
        IceFuelKind::Petrol => match clamp_ethanol_fraction(ethanol_pct) {
            Some(e) => PETROL_DENSITY_KG_L * (1.0 - e) + ETHANOL_DENSITY_KG_L * e,
            None => PETROL_DENSITY_KG_L,
        },
    }
}

/// Effective AFR: measured lambda * stoich, else petrol default stoich.
/// Diesel without lambda returns `None` (never invent 14.7).
pub fn effective_afr(
    kind: IceFuelKind,
    lambda: Option<f64>,
    ethanol_pct: Option<f64>,
) -> Option<f64> {
    if let Some(lam) = lambda {
        if lam.is_finite() && lam > 0.0 {
            return Some(lam * stoich_afr(kind, ethanol_pct));
        }
        return None;
    }
    match kind {
        IceFuelKind::Petrol => Some(stoich_afr(kind, ethanol_pct)),
        IceFuelKind::Diesel => None,
    }
}

/// MAF → L/h. `mathematical-formulas.md`:
/// `fuel_l_h = maf_g_s * 3600 / (AFR * rho * 1000)`.
pub fn maf_to_fuel_rate_l_h(maf_g_s: f64, afr: f64, density_kg_l: f64) -> Option<f64> {
    if !maf_g_s.is_finite() || maf_g_s < 0.0 {
        return None;
    }
    if !afr.is_finite() || afr <= 0.0 || !density_kg_l.is_finite() || density_kg_l <= 0.0 {
        return None;
    }
    Some(maf_g_s * 3600.0 / (afr * density_kg_l * 1000.0))
}

pub fn derive_maf_fuel_rate(
    kind: IceFuelKind,
    maf_g_s: Option<f64>,
    lambda: Option<f64>,
    ethanol_pct: Option<f64>,
) -> Option<f64> {
    let maf = maf_g_s?;
    let afr = effective_afr(kind, lambda, ethanol_pct)?;
    maf_to_fuel_rate_l_h(maf, afr, fuel_density_kg_l(kind, ethanol_pct))
}

/// Instant L/100 km. Missing speed or rate, or non-positive speed → `None`.
pub fn instant_l_per_100km(fuel_rate_l_h: Option<f64>, speed_kmh: Option<f64>) -> Option<f64> {
    let rate = fuel_rate_l_h?;
    let speed = speed_kmh?;
    if !rate.is_finite() || !speed.is_finite() || speed <= 0.0 {
        return None;
    }
    Some(rate * 100.0 / speed)
}

/// Tank volume from level %. Missing either input → `None` (not 0 L).
pub fn fuel_current_l(fuel_level_pct: Option<f64>, tank_capacity_l: Option<f64>) -> Option<f64> {
    let pct = fuel_level_pct?;
    let cap = tank_capacity_l?;
    if !pct.is_finite() || !cap.is_finite() || cap < 0.0 {
        return None;
    }
    Some((pct / 100.0) * cap)
}

/// Range km from current litres and L/100 km. Missing / non-positive consumption → `None`.
pub fn range_km(fuel_current_l: Option<f64>, consumption_l_per_100km: Option<f64>) -> Option<f64> {
    let litres = fuel_current_l?;
    let cons = consumption_l_per_100km?;
    if !litres.is_finite() || !cons.is_finite() || cons <= 0.0 {
        return None;
    }
    Some(litres * 100.0 / cons)
}

/// MegaSquirt injector L/h from `mathematical-formulas.md` (not ECU.md 1200-duty sketch).
/// Out-of-range RPM / pulse width / flow are skipped (`None`).
pub fn megasquirt_fuel_rate_l_h(pw_ms: f64, rpm: f64, n_cyl: u32, flow_cc_min: f64) -> Option<f64> {
    if n_cyl == 0 {
        return None;
    }
    if !pw_ms.is_finite() || !rpm.is_finite() || !flow_cc_min.is_finite() {
        return None;
    }
    if !(0.0..=50.0).contains(&pw_ms) || !(1.0..=20_000.0).contains(&rpm) || flow_cc_min <= 0.0 {
        return None;
    }
    Some(pw_ms * rpm * f64::from(n_cyl) * flow_cc_min / MEGASQUIRT_RATE_DENOM)
}

/// Map ICE rate onto the existing snapshot contract (SoC/power stay unset).
pub fn ice_snapshot(fuel_rate_l_h: Option<f64>) -> LiveEnergySnapshot {
    LiveEnergySnapshot {
        fuel_rate_l_h,
        state_of_charge_pct: None,
        power_kw: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn petrol_maf_uses_formulas_density_not_ecu_md_074() {
        let rate = maf_to_fuel_rate_l_h(2.0, PETROL_STOICH_AFR, PETROL_DENSITY_KG_L).unwrap();
        let expected = 2.0 * 3600.0 / (14.7 * 0.745 * 1000.0);
        assert!((rate - expected).abs() < 1e-12);
        let ecu_md_sketch = 2.0 / 14.7 * 3600.0 / 740.0;
        assert!((rate - ecu_md_sketch).abs() > 1e-4);
    }

    #[test]
    fn diesel_maf_without_lambda_is_none() {
        assert_eq!(
            derive_maf_fuel_rate(IceFuelKind::Diesel, Some(12.0), None, None),
            None
        );
    }

    #[test]
    fn diesel_maf_with_lambda_does_not_use_petrol_147() {
        let with = derive_maf_fuel_rate(IceFuelKind::Diesel, Some(12.0), Some(1.2), None).unwrap();
        let if_petrol_stoich =
            maf_to_fuel_rate_l_h(12.0, 1.2 * PETROL_STOICH_AFR, DIESEL_DENSITY_KG_L).unwrap();
        assert!((with - if_petrol_stoich).abs() > 1e-6);
        let expected =
            maf_to_fuel_rate_l_h(12.0, 1.2 * DIESEL_STOICH_AFR, DIESEL_DENSITY_KG_L).unwrap();
        assert!((with - expected).abs() < 1e-12);
    }

    #[test]
    fn missing_level_or_tank_is_none_not_zero() {
        assert_eq!(fuel_current_l(None, Some(60.0)), None);
        assert_eq!(fuel_current_l(Some(50.0), None), None);
        assert_eq!(fuel_current_l(Some(0.0), Some(60.0)), Some(0.0));
    }

    #[test]
    fn l100km_requires_positive_speed() {
        assert_eq!(instant_l_per_100km(Some(6.0), Some(0.0)), None);
        assert_eq!(instant_l_per_100km(Some(6.0), None), None);
        let v = instant_l_per_100km(Some(6.0), Some(100.0)).unwrap();
        assert!((v - 6.0).abs() < 1e-12);
    }

    #[test]
    fn megasquirt_formula_and_range_skip() {
        let ok = megasquirt_fuel_rate_l_h(4.0, 3000.0, 4, 250.0).unwrap();
        let expected = 4.0 * 3000.0 * 4.0 * 250.0 / 2_000_000.0;
        assert!((ok - expected).abs() < 1e-12);
        assert_eq!(megasquirt_fuel_rate_l_h(4.0, 0.0, 4, 250.0), None);
        assert_eq!(megasquirt_fuel_rate_l_h(60.0, 3000.0, 4, 250.0), None);
    }
}
