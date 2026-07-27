//! Battery-electric car pack capacity and route range estimates.
//!
//! Climbing-capability (torque / wheel) is intentionally **not** modeled for cars
//! in this module — that scenario is e-bike-specific. Here we only persist pack
//! size and estimate route draw as a share of capacity (with eco regen credit).

use serde::{Deserialize, Serialize};

use super::defaults::{DEFAULT_EV_CAR_BATTERY_KWH, DEFAULT_EV_CAR_MOTOR_EFFICIENCY};
use super::{range_estimate, EbikeRangeEstimate};

/// Persisted Electric Car vehicle energy specs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvCarConfig {
    /// Usable battery pack capacity in kilowatt-hours.
    pub battery_capacity_kwh: Option<f64>,
}

impl Default for EvCarConfig {
    fn default() -> Self {
        Self {
            battery_capacity_kwh: Some(DEFAULT_EV_CAR_BATTERY_KWH),
        }
    }
}

impl EvCarConfig {
    pub fn battery_kwh_or_default(&self) -> f64 {
        self.battery_capacity_kwh
            .filter(|v| v.is_finite() && *v > 0.0)
            .unwrap_or(DEFAULT_EV_CAR_BATTERY_KWH)
    }

    pub fn battery_wh_or_default(&self) -> f64 {
        self.battery_kwh_or_default() * 1000.0
    }
}

pub fn default_ev_car_motor_efficiency() -> f64 {
    DEFAULT_EV_CAR_MOTOR_EFFICIENCY
}

/// Route range estimate for an EV car (same math as e-bike, capacity in Wh).
pub fn ev_car_range_estimate(
    mechanical_energy_j: f64,
    config: &EvCarConfig,
) -> EbikeRangeEstimate {
    range_estimate(
        mechanical_energy_j,
        config.battery_wh_or_default(),
        default_ev_car_motor_efficiency(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_spec_example() {
        let d = EvCarConfig::default();
        assert_eq!(d.battery_kwh_or_default(), 60.0);
        assert_eq!(d.battery_wh_or_default(), 60_000.0);
    }

    #[test]
    fn pct_scales_with_capacity() {
        let j = 21_600_000.0; // 6 kWh mechanical at η=1
        let a = ev_car_range_estimate(
            j,
            &EvCarConfig {
                battery_capacity_kwh: Some(60.0),
            },
        );
        // draw Wh = 6e6 / 0.85 / 3600 at default η
        let draw_wh = (j / 0.85) / 3600.0;
        assert!((a.battery_draw_wh - draw_wh).abs() < 1e-6);
        assert!((a.pct_of_capacity - 100.0 * draw_wh / 60_000.0).abs() < 1e-6);
    }
}
