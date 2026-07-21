//! Named defaults and configurable rest/safety/eco parameters.

mod defaults;
mod eco;
mod rest_params;
mod safety;

pub use defaults::*;
pub use eco::EcoConfig;
pub use rest_params::{
    CarRestParams, CyclingRestParams, HikingRestParams, ProfileRestParams, RestConfig,
    TruckRestParams,
};
pub use safety::SafetyConfig;

use serde::{Deserialize, Serialize};

/// Travel / routing profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Profile {
    Car,
    /// Battery-electric car (enum present; not a primary menu chip).
    CarElectric,
    Truck,
    /// Battery-electric truck (enum present; not a primary menu chip).
    TruckElectric,
    Hiking,
    Cycling,
    Motorcycle,
    /// Electric motorcycle / scooter class (enum present; not a primary menu chip).
    MotorcycleElectric,
}

impl Default for Profile {
    fn default() -> Self {
        Self::Car
    }
}

impl Profile {
    pub fn eco_mode_default(self) -> bool {
        matches!(self, Profile::Hiking | Profile::Cycling)
    }

    /// Profiles that expose an eco-mode toggle in the UI (others lock eco on).
    pub fn eco_mode_user_toggle(self) -> bool {
        matches!(
            self,
            Profile::Car
                | Profile::CarElectric
                | Profile::Motorcycle
                | Profile::MotorcycleElectric
                | Profile::Truck
                | Profile::TruckElectric
        )
    }

    /// Primary travel-mode chips in the UI. Truck and electric variants stay in
    /// the enum for routing/rest logic but are not menu-focus entries.
    pub fn menu_focus(self) -> bool {
        matches!(
            self,
            Profile::Car | Profile::Cycling | Profile::Hiking | Profile::Motorcycle
        )
    }
}

/// Physical vehicle limits used to filter OSM tagged restrictions (EU 96/53/EC ranges as guidance).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VehicleLimits {
    pub axle_weight_kg: Option<f64>,
    pub height_m: Option<f64>,
    pub width_m: Option<f64>,
    pub total_weight_kg: Option<f64>,
}

impl Default for VehicleLimits {
    fn default() -> Self {
        Self {
            axle_weight_kg: None,
            height_m: None,
            width_m: None,
            total_weight_kg: None,
        }
    }
}
