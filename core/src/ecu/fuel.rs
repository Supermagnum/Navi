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

/// How trustworthy the MAF/stoich path is. Direct volume PIDs stay `Measured`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FuelRateQuality {
    #[default]
    Unknown,
    /// PID 5E / J1939 / MegaSquirt injector volume.
    Measured,
    /// MAF plus a measured or commanded lambda.
    LambdaMaf,
    /// Petrol stoich assumed, and/or cold/high-load enrichment estimate.
    Estimate,
}

/// Cubic expansion coefficients (1/C) for density(T).
pub const BETA_PETROL_PER_C: f64 = 0.00095;
pub const BETA_DIESEL_PER_C: f64 = 0.00083;
pub const BETA_ETHANOL_PER_C: f64 = 0.0011;

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

/// Stoichiometric AFR for MAF derivation.
/// Petrol/ethanol blends use **mass fraction** (docs/mathematical-formulas.md),
/// not linear volume interpolation.
pub fn stoich_afr(kind: IceFuelKind, ethanol_pct: Option<f64>) -> f64 {
    match kind {
        IceFuelKind::Diesel => DIESEL_STOICH_AFR,
        IceFuelKind::Petrol => match clamp_ethanol_fraction(ethanol_pct) {
            Some(v) => blend_stoich_afr_mass(v),
            None => PETROL_STOICH_AFR,
        },
    }
}

/// Blend density at 15 C (volume fraction `v` of ethanol).
pub fn blend_density_15c(v: f64) -> f64 {
    v * ETHANOL_DENSITY_KG_L + (1.0 - v) * PETROL_DENSITY_KG_L
}

/// Mass-fraction stoich AFR for a petrol/ethanol volume fraction `v`.
pub fn blend_stoich_afr_mass(v: f64) -> f64 {
    let v = v.clamp(0.0, 1.0);
    let rho = blend_density_15c(v);
    if rho <= 0.0 {
        return PETROL_STOICH_AFR;
    }
    let mass_e = v * ETHANOL_DENSITY_KG_L / rho;
    mass_e * ETHANOL_STOICH_AFR + (1.0 - mass_e) * PETROL_STOICH_AFR
}

pub fn fuel_density_kg_l(kind: IceFuelKind, ethanol_pct: Option<f64>) -> f64 {
    fuel_density_kg_l_at(kind, ethanol_pct, None)
}

/// Density at fuel temperature `T`. `None` temperature uses 15 C (no flag).
/// Applied only when converting a fuel **mass** to litres (MAF path).
pub fn fuel_density_kg_l_at(
    kind: IceFuelKind,
    ethanol_pct: Option<f64>,
    fuel_temp_c: Option<f64>,
) -> f64 {
    let (rho15, beta) = match kind {
        IceFuelKind::Diesel => (DIESEL_DENSITY_KG_L, BETA_DIESEL_PER_C),
        IceFuelKind::Petrol => match clamp_ethanol_fraction(ethanol_pct) {
            Some(v) => (
                blend_density_15c(v),
                v * BETA_ETHANOL_PER_C + (1.0 - v) * BETA_PETROL_PER_C,
            ),
            None => (PETROL_DENSITY_KG_L, BETA_PETROL_PER_C),
        },
    };
    match fuel_temp_c {
        Some(t) if t.is_finite() => rho15 * (1.0 - beta * (t - 15.0)),
        _ => rho15,
    }
}

/// Petrol, no lambda: stoich under-reports when cold or at high load.
/// Enrichment uses the band midpoints from docs/ECU.md (estimate quality).
pub fn petrol_estimate_afr(coolant_c: Option<f64>, calc_load_pct: Option<f64>) -> f64 {
    if let Some(c) = coolant_c {
        if c < 40.0 {
            return interpolate_cold_petrol_afr(c);
        }
    }
    if calc_load_pct.unwrap_or(0.0) >= 90.0 {
        return 12.8;
    }
    PETROL_STOICH_AFR
}

fn interpolate_cold_petrol_afr(coolant_c: f64) -> f64 {
    // Midpoints of cold-idle petrol bands: -30 / 0 / +20 / +40 C.
    let knots = [(-30.0, 10.0), (0.0, 12.0), (20.0, 13.0), (40.0, 14.1)];
    if coolant_c <= knots[0].0 {
        return knots[0].1;
    }
    for w in knots.windows(2) {
        let (x0, y0) = w[0];
        let (x1, y1) = w[1];
        if coolant_c <= x1 {
            let t = (coolant_c - x0) / (x1 - x0);
            return y0 + t * (y1 - y0);
        }
    }
    PETROL_STOICH_AFR
}

/// Effective AFR: measured lambda * stoich, else petrol estimate (possibly enriched).
/// Diesel without lambda returns `None` (never invent 14.7).
pub fn effective_afr(
    kind: IceFuelKind,
    lambda: Option<f64>,
    ethanol_pct: Option<f64>,
) -> Option<f64> {
    effective_afr_ex(kind, lambda, ethanol_pct, None, None)
}

pub fn effective_afr_ex(
    kind: IceFuelKind,
    lambda: Option<f64>,
    ethanol_pct: Option<f64>,
    coolant_c: Option<f64>,
    calc_load_pct: Option<f64>,
) -> Option<f64> {
    if let Some(lam) = lambda {
        if lam.is_finite() && lam >= 0.6 {
            return Some(lam * stoich_afr(kind, ethanol_pct));
        }
        return None;
    }
    match kind {
        IceFuelKind::Petrol => {
            let gasoline_eq = petrol_estimate_afr(coolant_c, calc_load_pct);
            let stoich = stoich_afr(kind, ethanol_pct);
            Some(gasoline_eq * stoich / PETROL_STOICH_AFR)
        }
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
    derive_maf_fuel_rate_ex(kind, maf_g_s, lambda, ethanol_pct, None, None, None)
        .map(|r| r.fuel_l_h)
}

/// MAF derivation with fuel-temp density and petrol enrichment estimate.
pub struct MafFuelRate {
    pub fuel_l_h: f64,
    pub afr: f64,
    pub density_kg_l: f64,
    pub quality: FuelRateQuality,
}

pub fn derive_maf_fuel_rate_ex(
    kind: IceFuelKind,
    maf_g_s: Option<f64>,
    lambda: Option<f64>,
    ethanol_pct: Option<f64>,
    fuel_temp_c: Option<f64>,
    coolant_c: Option<f64>,
    calc_load_pct: Option<f64>,
) -> Option<MafFuelRate> {
    let maf = maf_g_s?;
    if !maf.is_finite() || maf < 0.0 {
        return None;
    }
    let afr = effective_afr_ex(kind, lambda, ethanol_pct, coolant_c, calc_load_pct)?;
    let density = fuel_density_kg_l_at(kind, ethanol_pct, fuel_temp_c);
    let fuel_l_h = maf_to_fuel_rate_l_h(maf, afr, density)?;
    let quality = if lambda.is_some() {
        FuelRateQuality::LambdaMaf
    } else {
        FuelRateQuality::Estimate
    };
    Some(MafFuelRate {
        fuel_l_h,
        afr,
        density_kg_l: density,
        quality,
    })
}

/// Idle floor (rpm) used when classifying overrun fuel cut.
pub const FUEL_CUT_MIN_RPM: f64 = 900.0;
/// Petrol overrun fuel cut is typically disabled until the engine is warm.
pub const PETROL_FUEL_CUT_MIN_COOLANT_C: f64 = 50.0;
/// Wideband lean limit treated as fuel cut (lambda).
pub const PETROL_LEAN_CUT_LAMBDA: f64 = 1.45;
pub const DIESEL_LEAN_CUT_LAMBDA: f64 = 4.0;

/// Fuel cut: `Some(0.0)` not `None`. Needs lambda or PID 5E evidence.
/// Closed throttle / high rpm alone is not enough (cold petrol keeps injecting).
pub fn detect_fuel_cut(
    kind: IceFuelKind,
    rpm: Option<f64>,
    speed_kmh: Option<f64>,
    fuel_rate_direct: Option<f64>,
    lambda: Option<f64>,
    coolant_c: Option<f64>,
) -> bool {
    let rpm = rpm.unwrap_or(0.0);
    let speed = speed_kmh.unwrap_or(0.0);
    if rpm < FUEL_CUT_MIN_RPM || speed <= 0.0 {
        return false;
    }
    let five_e_zero = fuel_rate_direct == Some(0.0);
    let lean = match (kind, lambda) {
        (IceFuelKind::Petrol, Some(l)) => l.is_finite() && l >= PETROL_LEAN_CUT_LAMBDA,
        (IceFuelKind::Diesel, Some(l)) => l.is_finite() && l >= DIESEL_LEAN_CUT_LAMBDA,
        _ => false,
    };
    if !five_e_zero && !lean {
        return false;
    }
    if kind == IceFuelKind::Petrol {
        if let Some(c) = coolant_c {
            if c < PETROL_FUEL_CUT_MIN_COOLANT_C && !five_e_zero && !lean {
                return false;
            }
        }
    }
    true
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

    #[test]
    fn blend_stoich_is_mass_fraction() {
        assert!((blend_stoich_afr_mass(0.0) - 14.70).abs() < 0.05);
        assert!((blend_stoich_afr_mass(0.10) - 14.10).abs() < 0.05);
        assert!((blend_stoich_afr_mass(0.85) - 9.82).abs() < 0.05);
        assert!((blend_stoich_afr_mass(1.0) - 9.00).abs() < 0.05);
        let vol_linear = 14.7 * 0.15 + 9.0 * 0.85;
        assert!(
            (blend_stoich_afr_mass(0.85) - vol_linear).abs() > 0.01,
            "mass-fraction stoich must differ from volume mix"
        );
    }

    #[test]
    fn fuel_temp_changes_maf_litres_not_volume_sources() {
        let cold = fuel_density_kg_l_at(IceFuelKind::Petrol, None, Some(-30.0));
        let hot = fuel_density_kg_l_at(IceFuelKind::Petrol, None, Some(40.0));
        assert!((cold - 0.777).abs() < 0.005);
        assert!((hot - 0.727).abs() < 0.005);
        let d15 = fuel_density_kg_l_at(IceFuelKind::Diesel, None, Some(15.0));
        let d40 = fuel_density_kg_l_at(IceFuelKind::Diesel, None, Some(40.0));
        let dm30 = fuel_density_kg_l_at(IceFuelKind::Diesel, None, Some(-30.0));
        assert!((d15 - 0.832).abs() < 1e-9);
        assert!((d40 - 0.815).abs() < 0.005);
        assert!((dm30 - 0.863).abs() < 0.005);
    }

    #[test]
    fn petrol_cold_and_high_load_enrich_without_lambda() {
        let warm = petrol_estimate_afr(Some(90.0), Some(40.0));
        assert!((warm - 14.7).abs() < 1e-9);
        let cold = petrol_estimate_afr(Some(0.0), Some(20.0));
        assert!(cold < 13.0 && cold > 11.0);
        let wot = petrol_estimate_afr(Some(90.0), Some(95.0));
        assert!((wot - 12.8).abs() < 1e-9);
        let r_warm = derive_maf_fuel_rate_ex(
            IceFuelKind::Petrol,
            Some(17.3),
            None,
            None,
            Some(15.0),
            Some(90.0),
            Some(40.0),
        )
        .unwrap();
        let r_cold = derive_maf_fuel_rate_ex(
            IceFuelKind::Petrol,
            Some(17.3),
            None,
            None,
            Some(15.0),
            Some(0.0),
            Some(20.0),
        )
        .unwrap();
        assert_eq!(r_warm.quality, FuelRateQuality::Estimate);
        assert!(r_cold.fuel_l_h > r_warm.fuel_l_h);
    }

    #[test]
    fn fuel_cut_needs_rate_or_lambda_evidence() {
        assert!(!detect_fuel_cut(
            IceFuelKind::Petrol,
            Some(2500.0),
            Some(90.0),
            None,
            None,
            Some(90.0)
        ));
        assert!(detect_fuel_cut(
            IceFuelKind::Diesel,
            Some(2500.0),
            Some(90.0),
            Some(0.0),
            None,
            Some(-20.0)
        ));
        assert!(detect_fuel_cut(
            IceFuelKind::Petrol,
            Some(2500.0),
            Some(90.0),
            None,
            Some(1.5),
            Some(90.0)
        ));
    }
}
