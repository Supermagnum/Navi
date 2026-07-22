//! Live vehicle telemetry extension point.
//!
//! No OBD-II, J1939, or MegaSquirt polling is implemented in this pass.
//! Future WASM or native plugins may supply snapshots through this trait.
//!
//! Wire formats and worked examples: repository `docs/ECU.md`.

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
