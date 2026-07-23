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
    /// Motorhome / camper — same physical clearance constraints as truck.
    MobileHome,
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
                | Profile::MobileHome
        )
    }

    /// Primary travel-mode chips in the UI. Truck / mobile home / electric
    /// variants stay in the enum for routing/rest logic but are not all chips.
    pub fn menu_focus(self) -> bool {
        matches!(
            self,
            Profile::Car
                | Profile::Cycling
                | Profile::Hiking
                | Profile::Motorcycle
                | Profile::Truck
                | Profile::MobileHome
        )
    }

    /// Motor profiles that expose avoid-toll / avoid-ferry toggles.
    pub fn supports_toll_ferry_avoid(self) -> bool {
        matches!(
            self,
            Profile::Car
                | Profile::CarElectric
                | Profile::Truck
                | Profile::TruckElectric
                | Profile::MobileHome
                | Profile::Motorcycle
                | Profile::MotorcycleElectric
        )
    }

    /// Profiles that apply full vehicle dimension/weight clearance filters.
    pub fn uses_vehicle_clearance_limits(self) -> bool {
        matches!(
            self,
            Profile::Truck | Profile::TruckElectric | Profile::MobileHome
        )
    }
}

/// Physical vehicle limits used to filter OSM tagged restrictions (EU 96/53/EC ranges as guidance).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VehicleLimits {
    pub axle_weight_kg: Option<f64>,
    /// Max bogie (axle-group) weight — EU multi-axle truck / mobile-home rules.
    #[serde(default)]
    pub bogie_weight_kg: Option<f64>,
    pub height_m: Option<f64>,
    pub width_m: Option<f64>,
    /// Overall vehicle length (maxlength OSM tag).
    #[serde(default)]
    pub length_m: Option<f64>,
    pub total_weight_kg: Option<f64>,
}

impl Default for VehicleLimits {
    fn default() -> Self {
        Self {
            axle_weight_kg: None,
            bogie_weight_kg: None,
            height_m: None,
            width_m: None,
            length_m: None,
            total_weight_kg: None,
        }
    }
}

/// Fuel tank / fill-up inputs for adaptive consumption learning (persisted).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuelConfig {
    /// Tank capacity in litres (canonical storage unit).
    pub tank_capacity_l: Option<f64>,
    /// Last fuel added in litres (feeds adaptive learning when live ECU is absent).
    pub fuel_added_l: Option<f64>,
    /// When false, UI should present gallons (value still stored as litres).
    pub prefer_liters: bool,
}

impl Default for FuelConfig {
    fn default() -> Self {
        Self {
            tank_capacity_l: None,
            fuel_added_l: None,
            prefer_liters: true,
        }
    }
}
