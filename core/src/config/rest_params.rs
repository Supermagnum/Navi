use serde::{Deserialize, Serialize};

use super::defaults::*;
use super::Profile;

/// Per-profile rest parameters persisted and editable via host UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestConfig {
    pub car: CarRestParams,
    pub truck: TruckRestParams,
    pub hiking: HikingRestParams,
    pub cycling: CyclingRestParams,
}

impl Default for RestConfig {
    fn default() -> Self {
        Self {
            car: CarRestParams::default(),
            truck: TruckRestParams::default(),
            hiking: HikingRestParams::default(),
            cycling: CyclingRestParams::default(),
        }
    }
}

/// Car rest parameters. Soft/max hours are configurable without hardcoded defaults
/// (product decision pending).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CarRestParams {
    pub soft_limit_hours: Option<f64>,
    pub max_hours: Option<f64>,
    pub break_interval_min_hours: f64,
    pub break_interval_max_hours: f64,
    pub break_duration_min_minutes: u32,
    pub break_duration_max_minutes: u32,
    pub eco_mode_enabled: bool,
}

impl Default for CarRestParams {
    fn default() -> Self {
        Self {
            soft_limit_hours: None,
            max_hours: None,
            break_interval_min_hours: CAR_BREAK_INTERVAL_MIN_HOURS,
            break_interval_max_hours: CAR_BREAK_INTERVAL_MAX_HOURS,
            break_duration_min_minutes: CAR_BREAK_DURATION_MIN_MINUTES,
            break_duration_max_minutes: CAR_BREAK_DURATION_MAX_MINUTES,
            eco_mode_enabled: false,
        }
    }
}

/// Truck rest parameters aligned with EU Regulation EC 561/2006.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TruckRestParams {
    pub mandatory_break_after_hours: f64,
    pub break_duration_minutes: u32,
    pub max_daily_driving_hours: f64,
    pub max_weekly_driving_hours: f64,
    pub eco_mode_enabled: bool,
}

impl Default for TruckRestParams {
    fn default() -> Self {
        Self {
            mandatory_break_after_hours: TRUCK_MANDATORY_BREAK_AFTER_HOURS,
            break_duration_minutes: TRUCK_BREAK_DURATION_MINUTES,
            max_daily_driving_hours: TRUCK_MAX_DAILY_DRIVING_HOURS,
            max_weekly_driving_hours: TRUCK_MAX_WEEKLY_DRIVING_HOURS,
            eco_mode_enabled: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HikingRestParams {
    pub main_break_distance_km: f64,
    pub alternative_break_distance_km: f64,
    pub max_daily_distance_km: f64,
}

impl Default for HikingRestParams {
    fn default() -> Self {
        Self {
            main_break_distance_km: HIKING_MAIN_BREAK_DISTANCE_KM,
            alternative_break_distance_km: HIKING_ALTERNATIVE_BREAK_DISTANCE_KM,
            max_daily_distance_km: HIKING_MAX_DAILY_DISTANCE_KM,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CyclingRestParams {
    pub main_break_distance_km: f64,
    pub alternative_break_distance_km: f64,
    pub max_daily_distance_km: f64,
}

impl Default for CyclingRestParams {
    fn default() -> Self {
        Self {
            main_break_distance_km: CYCLING_MAIN_BREAK_DISTANCE_KM,
            alternative_break_distance_km: CYCLING_ALTERNATIVE_BREAK_DISTANCE_KM,
            max_daily_distance_km: CYCLING_MAX_DAILY_DISTANCE_KM,
        }
    }
}

impl RestConfig {
    pub fn for_profile(&self, profile: Profile) -> ProfileRestParams<'_> {
        match profile {
            Profile::Car
            | Profile::CarElectric
            | Profile::Motorcycle
            | Profile::MotorcycleElectric => ProfileRestParams::Car(&self.car),
            Profile::Truck | Profile::TruckElectric | Profile::MobileHome => {
                ProfileRestParams::Truck(&self.truck)
            }
            Profile::Hiking => ProfileRestParams::Hiking(&self.hiking),
            Profile::Cycling => ProfileRestParams::Cycling(&self.cycling),
        }
    }

    pub fn eco_mode_enabled(&self, profile: Profile) -> bool {
        match profile {
            Profile::Car
            | Profile::CarElectric
            | Profile::Motorcycle
            | Profile::MotorcycleElectric => self.car.eco_mode_enabled,
            Profile::Truck | Profile::TruckElectric | Profile::MobileHome => {
                self.truck.eco_mode_enabled
            }
            Profile::Hiking | Profile::Cycling => true,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ProfileRestParams<'a> {
    Car(&'a CarRestParams),
    Truck(&'a TruckRestParams),
    Hiking(&'a HikingRestParams),
    Cycling(&'a CyclingRestParams),
}
