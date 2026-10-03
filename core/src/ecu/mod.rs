//! Live vehicle telemetry extension point.
//!
//! No OBD-II, J1939, or MegaSquirt **polling** is implemented in this pass.
//! Pure ICE decode / fuel-rate helpers live in [`decode`], [`fuel`], and
//! [`ambient`] for a future plugin. EV PID `5B` / SoC is out of scope here.
//!
//! Wire formats and worked examples: repository `docs/ECU.md`.

pub mod ambient;
pub mod decode;
pub mod fuel;
pub mod self_test;

use crate::config::Profile;

/// Optional live fuel/energy data for eco-mode cost refinement.
#[derive(Debug, Clone, Copy, Default)]
pub struct LiveEnergySnapshot {
    pub fuel_rate_l_h: Option<f64>,
    pub state_of_charge_pct: Option<f64>,
    pub power_kw: Option<f64>,
}

/// Hook where live ECU/BMS data would feed into routing when a plugin is present.
pub trait LiveEnergyProvider: Send + Sync {
    fn latest(&self, profile: Profile) -> Option<LiveEnergySnapshot>;
}

/// Default no-op provider used when no ECU plugin is loaded.
#[derive(Debug, Default)]
pub struct NoLiveEnergy;

impl LiveEnergyProvider for NoLiveEnergy {
    fn latest(&self, _profile: Profile) -> Option<LiveEnergySnapshot> {
        None
    }
}

/// Blend predicted segment energy with live fuel rate when available.
pub fn refine_energy_cost(
    predicted_joules: f64,
    distance_m: f64,
    live: Option<&LiveEnergySnapshot>,
) -> f64 {
    let Some(snapshot) = live else {
        return predicted_joules;
    };
    if let Some(fuel_rate) = snapshot.fuel_rate_l_h {
        if distance_m > 0.0 {
            let hours = distance_m / crate::config::DEFAULT_CRUISE_SPEED_M_S / 3600.0;
            return fuel_rate * hours * 36_000_000.0;
        }
    }
    predicted_joules
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_live_energy_is_none() {
        assert!(NoLiveEnergy.latest(Profile::Car).is_none());
    }
}
