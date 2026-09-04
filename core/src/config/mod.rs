//! Named defaults and configurable rest/safety/eco parameters.

mod defaults;
mod driving_hours_pack;
mod ebike;
mod eco;
mod ev_car;
mod fmcsa_params;
mod poi_radii;
mod rest_params;
mod safety;
mod truck_history;

pub use defaults::*;
pub use driving_hours_pack::JurisdictionDrivingHoursPack;
pub use ebike::{
    battery_draw_wh, climb_capability, climb_capability_for, default_motor_efficiency,
    ebike_eco_config, range_estimate, EbikeClimbCapability, EbikeConfig, EbikeRangeEstimate,
};
pub use eco::{motorcycle_eco_config, EcoConfig};
pub use ev_car::{default_ev_car_motor_efficiency, ev_car_range_estimate, EvCarConfig};
pub use fmcsa_params::FmcsaHosParams;
pub use poi_radii::{ProfilePoiRadii, ProfilePoiRadiiTable};
pub use rest_params::{
    CarRestParams, CyclingRestParams, HikingRestParams, ProfileRestParams, RestConfig,
    TruckRestParams,
};
pub use safety::SafetyConfig;
pub use truck_history::{
    civil_date_add_days, iso_week_monday, outstanding_weekly_rest_compensations,
    prune_truck_driving_history, record_reduced_weekly_compensation, record_truck_driving_hours,
    rolling_date_window, try_repay_weekly_rest_compensation, weekly_rest_compensation_deadline,
    TruckDrivingDay, TruckDrivingHistory, TruckRestKind, WeeklyRestCompensationDebt,
};

use serde::{Deserialize, Serialize};

/// Travel / routing profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum Profile {
    #[default]
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
    /// Battery-assisted cycle / pedelec (routing uses bicycle graph; battery + climb specs).
    CyclingElectric,
    Motorcycle,
    /// Electric motorcycle / scooter class (enum present; not a primary menu chip).
    MotorcycleElectric,
}

impl Profile {
    pub fn eco_mode_default(self) -> bool {
        matches!(
            self,
            Profile::Hiking | Profile::Cycling | Profile::CyclingElectric
        )
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
                | Profile::CyclingElectric
                | Profile::Hiking
                | Profile::Motorcycle
                | Profile::Truck
                | Profile::MobileHome
        )
    }

    /// Profiles that expose the avoid-toll toggle (includes bike/hike for mode-specific tolls).
    pub fn supports_toll_avoid(self) -> bool {
        matches!(
            self,
            Profile::Car
                | Profile::CarElectric
                | Profile::Truck
                | Profile::TruckElectric
                | Profile::MobileHome
                | Profile::Motorcycle
                | Profile::MotorcycleElectric
                | Profile::Hiking
                | Profile::Cycling
                | Profile::CyclingElectric
        )
    }

    /// Motor profiles that expose avoid-ferry (and historically shared with toll UI).
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

    /// Motor profiles that expose avoid-ferry.
    pub fn supports_ferry_avoid(self) -> bool {
        self.supports_toll_ferry_avoid()
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
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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
