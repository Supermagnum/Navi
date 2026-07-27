//! Electric-cycle (e-bike / pedelec) vehicle specs and capability estimates.
//!
//! Specs persist via [`crate::storage::ConfigStore`]. Legal assist caps (EU 250 W /
//! 25 km/h; US Class 1–3 up to 750 W / 20–28 mph) are **not** enforced — values are
//! the rider's real bike for planning only.
//!
//! Climbing model is a simplification: mid-drive rated torque is treated as if
//! applied at a representative final stage (torque / wheel radius). Real mid-drives
//! have variable derailleur reduction; this is not a full drivetrain simulation.

use serde::{Deserialize, Serialize};

use super::defaults::{
    DEFAULT_EBIKE_BATTERY_WH, DEFAULT_EBIKE_MOTOR_EFFICIENCY, DEFAULT_EBIKE_TORQUE_NM,
    DEFAULT_EBIKE_WHEEL_DIAMETER_IN, GRAVITY_M_S2,
};
use super::EcoConfig;

/// Persisted e-bike vehicle specs (Electric Cycle profile).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EbikeConfig {
    /// Battery capacity in watt-hours.
    pub battery_capacity_wh: Option<f64>,
    /// Motor peak / continuous rated torque (Nm).
    pub motor_torque_nm: Option<f64>,
    /// Wheel diameter in inches (canonical store; UI may offer 20/26/27.5/29 + custom).
    pub wheel_diameter_in: Option<f64>,
}

impl Default for EbikeConfig {
    fn default() -> Self {
        Self {
            battery_capacity_wh: Some(DEFAULT_EBIKE_BATTERY_WH),
            motor_torque_nm: Some(DEFAULT_EBIKE_TORQUE_NM),
            wheel_diameter_in: Some(DEFAULT_EBIKE_WHEEL_DIAMETER_IN),
        }
    }
}

impl EbikeConfig {
    pub fn battery_wh_or_default(&self) -> f64 {
        self.battery_capacity_wh
            .filter(|v| v.is_finite() && *v > 0.0)
            .unwrap_or(DEFAULT_EBIKE_BATTERY_WH)
    }

    pub fn torque_nm_or_default(&self) -> f64 {
        self.motor_torque_nm
            .filter(|v| v.is_finite() && *v > 0.0)
            .unwrap_or(DEFAULT_EBIKE_TORQUE_NM)
    }

    pub fn wheel_diameter_in_or_default(&self) -> f64 {
        self.wheel_diameter_in
            .filter(|v| v.is_finite() && *v > 0.0)
            .unwrap_or(DEFAULT_EBIKE_WHEEL_DIAMETER_IN)
    }

    pub fn wheel_radius_m(&self) -> f64 {
        // 1 inch = 0.0254 m; radius = diameter / 2.
        self.wheel_diameter_in_or_default() * 0.0254 / 2.0
    }

    /// Ground tractive force (N) under the simplified mid-drive model:
    /// `F = torque_Nm / wheel_radius_m`.
    pub fn tractive_force_n(&self) -> f64 {
        let r = self.wheel_radius_m().max(1e-6);
        self.torque_nm_or_default() / r
    }
}

/// Result of comparing route mechanical energy to battery capacity.
#[derive(Debug, Clone, PartialEq)]
pub struct EbikeRangeEstimate {
    pub mechanical_energy_j: f64,
    pub battery_draw_wh: f64,
    pub battery_capacity_wh: f64,
    /// Estimated share of battery used (may exceed 100).
    pub pct_of_capacity: f64,
    pub motor_efficiency: f64,
}

/// Climbing capability from torque + wheel + mass + rolling resistance.
#[derive(Debug, Clone, PartialEq)]
pub struct EbikeClimbCapability {
    pub tractive_force_n: f64,
    /// Maximum sustained grade as a fraction (0.10 = 10%), not percent.
    pub max_grade_fraction: f64,
    /// Same as [`Self::max_grade_fraction`] × 100.
    pub max_grade_pct: f64,
}

/// Battery Wh drawn for a route's mechanical energy at motor efficiency η:
/// `Wh = J / η / 3600`.
pub fn battery_draw_wh(mechanical_energy_j: f64, motor_efficiency: f64) -> f64 {
    let eta = motor_efficiency.clamp(0.05, 1.0);
    (mechanical_energy_j.max(0.0) / eta) / 3600.0
}

pub fn range_estimate(
    mechanical_energy_j: f64,
    battery_capacity_wh: f64,
    motor_efficiency: f64,
) -> EbikeRangeEstimate {
    let cap = battery_capacity_wh.max(1e-6);
    let draw = battery_draw_wh(mechanical_energy_j, motor_efficiency);
    EbikeRangeEstimate {
        mechanical_energy_j,
        battery_draw_wh: draw,
        battery_capacity_wh: cap,
        pct_of_capacity: 100.0 * draw / cap,
        motor_efficiency: motor_efficiency.clamp(0.05, 1.0),
    }
}

/// Solve `F_tractive = m g sin(θ) + F_rolling` for grade fraction `g = tan(θ)`
/// approximated with `sin(θ) ≈ θ ≈ g` for moderate grades (standard road-grade model:
/// grade = rise/run = tan(θ); resistive = m g sin(θ) + Crr m g cos(θ)).
///
/// Using `F = m g (sin θ + Crr cos θ)` and `grade = tan θ`:
/// at equilibrium, `sin θ + Crr cos θ = F / (m g)`.
/// For numerical simplicity we use the common small-angle form
/// `F ≈ m g (grade + Crr)` ⇒ `grade ≈ F/(m g) − Crr` (clamped ≥ 0).
pub fn climb_capability(
    tractive_force_n: f64,
    mass_kg: f64,
    rolling_resistance: f64,
) -> EbikeClimbCapability {
    let m = mass_kg.max(1.0);
    let crr = rolling_resistance.max(0.0);
    let weight = m * GRAVITY_M_S2;
    let max_grade = ((tractive_force_n / weight) - crr).max(0.0);
    EbikeClimbCapability {
        tractive_force_n,
        max_grade_fraction: max_grade,
        max_grade_pct: max_grade * 100.0,
    }
}

pub fn climb_capability_for(config: &EbikeConfig, eco: &EcoConfig) -> EbikeClimbCapability {
    climb_capability(
        config.tractive_force_n(),
        eco.mass_kg.max(1.0),
        eco.rolling_resistance.max(0.0),
    )
}

pub fn default_motor_efficiency() -> f64 {
    DEFAULT_EBIKE_MOTOR_EFFICIENCY
}

/// Eco physics tuned for e-bike + rider (not car Passat baseline).
pub fn ebike_eco_config(regen: bool) -> EcoConfig {
    EcoConfig {
        drag_coefficient: 0.9,
        frontal_area_m2: 0.55,
        mass_kg: 100.0,
        rolling_resistance: 0.008,
        cruise_speed_m_s: 6.0,
        regen_efficiency: if regen {
            super::defaults::DEFAULT_EV_REGEN_EFFICIENCY
        } else {
            0.0
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tractive_force_hand_check_85nm_27_5in() {
        let cfg = EbikeConfig {
            battery_capacity_wh: Some(800.0),
            motor_torque_nm: Some(85.0),
            wheel_diameter_in: Some(27.5),
        };
        let r = cfg.wheel_radius_m();
        assert!((r - 0.34925).abs() < 1e-4, "radius={r}");
        let f = cfg.tractive_force_n();
        // 85 / 0.34925 ≈ 243.4 N
        assert!((f - 243.4).abs() < 1.0, "F_tractive={f}");
    }

    #[test]
    fn flat_only_max_grade_matches_formula() {
        let f = 244.0;
        let m = 100.0;
        let crr = 0.008;
        let cap = climb_capability(f, m, crr);
        let expected = f / (m * GRAVITY_M_S2) - crr;
        assert!((cap.max_grade_fraction - expected).abs() < 1e-9);
        assert!(cap.max_grade_pct > 20.0, "pct={}", cap.max_grade_pct);
    }

    #[test]
    fn zero_tractive_yields_zero_grade() {
        let cap = climb_capability(0.0, 100.0, 0.008);
        assert_eq!(cap.max_grade_fraction, 0.0);
    }

    #[test]
    fn range_pct_scales_with_capacity() {
        let j = 1_440_000.0; // 400 Wh mechanical at 100% η
        let a = range_estimate(j, 800.0, 1.0);
        let b = range_estimate(j, 400.0, 1.0);
        assert!((a.battery_draw_wh - 400.0).abs() < 1e-6);
        assert!((a.pct_of_capacity - 50.0).abs() < 1e-6);
        assert!((b.pct_of_capacity - 100.0).abs() < 1e-6);
    }

    #[test]
    fn motor_efficiency_increases_draw() {
        let j = 360_000.0; // 100 Wh at η=1
        let full = range_estimate(j, 800.0, 1.0);
        let lossy = range_estimate(j, 800.0, 0.8);
        assert!((full.battery_draw_wh - 100.0).abs() < 1e-6);
        assert!((lossy.battery_draw_wh - 125.0).abs() < 1e-6);
        assert!(lossy.pct_of_capacity > full.pct_of_capacity);
    }

    #[test]
    fn defaults_match_spec_examples() {
        let d = EbikeConfig::default();
        assert_eq!(d.battery_wh_or_default(), 800.0);
        assert_eq!(d.torque_nm_or_default(), 85.0);
        assert_eq!(d.wheel_diameter_in_or_default(), 27.5);
    }
}
