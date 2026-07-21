use serde::{Deserialize, Serialize};

use super::defaults::*;

/// Configurable eco-mode physics inputs (Cd, frontal area, mass).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EcoConfig {
    pub drag_coefficient: f64,
    pub frontal_area_m2: f64,
    pub mass_kg: f64,
    pub rolling_resistance: f64,
    pub cruise_speed_m_s: f64,
}

impl Default for EcoConfig {
    fn default() -> Self {
        Self {
            drag_coefficient: DEFAULT_DRAG_COEFFICIENT,
            frontal_area_m2: DEFAULT_FRONTAL_AREA_M2,
            mass_kg: DEFAULT_VEHICLE_MASS_KG,
            rolling_resistance: DEFAULT_ROLLING_RESISTANCE,
            cruise_speed_m_s: DEFAULT_CRUISE_SPEED_M_S,
        }
    }
}

impl EcoConfig {
    /// Segment energy cost: E ≈ (F_rolling + F_drag) × d + m g Δh
    pub fn segment_energy_joules(&self, distance_m: f64, delta_h_m: f64) -> f64 {
        let f_rolling = self.rolling_resistance * self.mass_kg * GRAVITY_M_S2;
        let f_drag = 0.5 * AIR_DENSITY_KG_M3 * self.drag_coefficient * self.frontal_area_m2
            * self.cruise_speed_m_s * self.cruise_speed_m_s;
        (f_rolling + f_drag) * distance_m + self.mass_kg * GRAVITY_M_S2 * delta_h_m
    }
}
