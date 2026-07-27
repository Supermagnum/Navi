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
    /// Fraction of gravitational PE recovered on descent (0 = combustion / braking
    /// dissipates all descent energy; EV regen typically 0.2–0.6).
    #[serde(default)]
    pub regen_efficiency: f64,
}

impl Default for EcoConfig {
    fn default() -> Self {
        Self {
            drag_coefficient: DEFAULT_DRAG_COEFFICIENT,
            frontal_area_m2: DEFAULT_FRONTAL_AREA_M2,
            mass_kg: DEFAULT_VEHICLE_MASS_KG,
            rolling_resistance: DEFAULT_ROLLING_RESISTANCE,
            cruise_speed_m_s: DEFAULT_CRUISE_SPEED_M_S,
            regen_efficiency: DEFAULT_REGEN_EFFICIENCY,
        }
    }
}

impl EcoConfig {
    /// Profile-scoped defaults: combustion/ICE keep regen at 0; electric drivetrains
    /// get [`DEFAULT_EV_REGEN_EFFICIENCY`] so descent recovers a fraction of PE.
    pub fn for_profile(profile: crate::config::Profile) -> Self {
        let mut cfg = Self::default();
        if matches!(
            profile,
            crate::config::Profile::CarElectric
                | crate::config::Profile::TruckElectric
                | crate::config::Profile::MotorcycleElectric
                | crate::config::Profile::CyclingElectric
        ) {
            cfg.regen_efficiency = DEFAULT_EV_REGEN_EFFICIENCY;
        }
        cfg
    }

    /// Flat (rolling + aerodynamic) energy for a level segment, joules.
    pub fn flat_energy_joules(&self, distance_m: f64) -> f64 {
        let f_rolling = self.rolling_resistance * self.mass_kg * GRAVITY_M_S2;
        let f_drag = 0.5
            * AIR_DENSITY_KG_M3
            * self.drag_coefficient
            * self.frontal_area_m2
            * self.cruise_speed_m_s
            * self.cruise_speed_m_s;
        (f_rolling + f_drag) * distance_m
    }

    /// Segment energy cost in joules.
    ///
    /// Climb: charge full `m g Δh`. Descent: apply `regen_efficiency * m g Δh`
    /// (negative). Default regen is 0, so undulating terrain is not free — only
    /// climbs add PE cost. Floor at 1% of flat energy keeps weights in joules
    /// (never `length_m * 0.01`, which mixed metres into a joule cost).
    pub fn segment_energy_joules(&self, distance_m: f64, delta_h_m: f64) -> f64 {
        let flat = self.flat_energy_joules(distance_m);
        let pe = self.mass_kg * GRAVITY_M_S2 * delta_h_m;
        let pe_cost = if delta_h_m >= 0.0 {
            pe
        } else {
            self.regen_efficiency.clamp(0.0, 1.0) * pe
        };
        (flat + pe_cost).max(flat * 0.01)
    }
}

#[cfg(test)]
mod tests {
    use super::EcoConfig;

    #[test]
    fn climb_costs_pe_descent_does_not_refund_without_regen() {
        let eco = EcoConfig {
            regen_efficiency: 0.0,
            ..EcoConfig::default()
        };
        let d = 100.0;
        let flat = eco.flat_energy_joules(d);
        let up = eco.segment_energy_joules(d, 10.0);
        let down = eco.segment_energy_joules(d, -10.0);
        assert!(up > flat);
        assert!(
            (down - flat).abs() < 1e-6,
            "descent should equal flat when regen=0"
        );
        assert!(up > down);
    }

    #[test]
    fn floor_is_in_joules_not_metres() {
        let eco = EcoConfig::default();
        let e = eco.segment_energy_joules(50.0, -100.0);
        let flat = eco.flat_energy_joules(50.0);
        assert!(e >= flat * 0.01 - 1e-6);
        assert!(e >= 100.0, "must not collapse to ~length*0.01 metres");
    }

    #[test]
    fn electric_regen_makes_descent_cheaper_than_combustion() {
        use crate::config::Profile;
        let ice = EcoConfig::for_profile(Profile::Car);
        let ev = EcoConfig::for_profile(Profile::CarElectric);
        assert_eq!(ice.regen_efficiency, 0.0);
        assert!(ev.regen_efficiency > 0.0);
        let d = 200.0;
        let dh = -25.0;
        let ice_down = ice.segment_energy_joules(d, dh);
        let ev_down = ev.segment_energy_joules(d, dh);
        assert!(
            ev_down < ice_down,
            "EV regen descent ({ev_down}) must be cheaper than ICE ({ice_down})"
        );
        let ice_up = ice.segment_energy_joules(d, 25.0);
        let ev_up = ev.segment_energy_joules(d, 25.0);
        assert!(
            (ice_up - ev_up).abs() < 1e-6,
            "climb cost must match when only regen differs"
        );
    }

    #[test]
    fn non_electric_profiles_keep_zero_regen() {
        use crate::config::Profile;
        for p in [
            Profile::Car,
            Profile::Truck,
            Profile::MobileHome,
            Profile::Motorcycle,
            Profile::Hiking,
            Profile::Cycling,
        ] {
            assert_eq!(
                EcoConfig::for_profile(p).regen_efficiency,
                0.0,
                "{p:?} must not get EV regen by default"
            );
        }
    }
}
