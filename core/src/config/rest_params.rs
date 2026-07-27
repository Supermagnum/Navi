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

/// Car rest parameters (also used for motorcycle and mobilehome soft guidance).
///
/// `max_hours` drives multi-day overnight splitting when a trip's driving time
/// exceeds the daily budget. Soft wellbeing guidance — not legal EC 561.
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
            soft_limit_hours: Some(CAR_SOFT_LIMIT_HOURS),
            max_hours: Some(CAR_MAX_DAILY_HOURS),
            break_interval_min_hours: CAR_BREAK_INTERVAL_MIN_HOURS,
            break_interval_max_hours: CAR_BREAK_INTERVAL_MAX_HOURS,
            break_duration_min_minutes: CAR_BREAK_DURATION_MIN_MINUTES,
            break_duration_max_minutes: CAR_BREAK_DURATION_MAX_MINUTES,
            eco_mode_enabled: false,
        }
    }
}

/// Truck rest parameters for EU Regulation EC 561/2006.
///
/// See [`docs/ec-561-truck-rest.md`](../../../docs/ec-561-truck-rest.md) for which
/// fields are enforced in single-day planning vs tracked/informational vs deferred.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TruckRestParams {
    pub mandatory_break_after_hours: f64,
    pub break_duration_minutes: u32,
    /// When true, treat the 45 min break as 15 + 30 instead of one continuous block.
    #[serde(default)]
    pub prefer_split_break: bool,
    pub max_daily_driving_hours: f64,
    /// Extended daily cap (typically 10 h); usable [`max_daily_extensions_per_week`] times.
    #[serde(default = "default_daily_extended")]
    pub max_daily_driving_extended_hours: f64,
    #[serde(default = "default_daily_extensions")]
    pub max_daily_extensions_per_week: u32,
    pub max_weekly_driving_hours: f64,
    #[serde(default = "default_fortnightly")]
    pub max_fortnightly_driving_hours: f64,
    #[serde(default = "default_daily_rest")]
    pub daily_rest_hours: f64,
    #[serde(default = "default_daily_rest_reduced")]
    pub daily_rest_reduced_hours: f64,
    #[serde(default = "default_max_reduced_daily_rests")]
    pub max_reduced_daily_rests: u32,
    #[serde(default = "default_split_daily_first")]
    pub split_daily_rest_first_hours: f64,
    #[serde(default = "default_split_daily_second")]
    pub split_daily_rest_second_hours: f64,
    /// When true, prefer 3 h + 9 h split daily rest over one continuous block.
    #[serde(default)]
    pub prefer_split_daily_rest: bool,
    #[serde(default = "default_weekly_rest")]
    pub weekly_rest_hours: f64,
    #[serde(default = "default_weekly_rest_reduced")]
    pub weekly_rest_reduced_hours: f64,
    #[serde(default = "default_max_consecutive_days")]
    pub max_consecutive_working_days: u32,
    /// Regular 45 h weekly rest must not be taken in the cab (tracked flag).
    #[serde(default = "default_true")]
    pub regular_weekly_rest_not_in_cab: bool,
    #[serde(default = "default_exceptional_hours")]
    pub exceptional_extension_hours: f64,
    /// When true, the next plan may use the +1 h exceptional extension (explicit opt-in).
    #[serde(default)]
    pub exceptional_extension_armed: bool,
    pub eco_mode_enabled: bool,
}

fn default_daily_extended() -> f64 {
    TRUCK_MAX_DAILY_DRIVING_EXTENDED_HOURS
}
fn default_daily_extensions() -> u32 {
    TRUCK_MAX_DAILY_EXTENSIONS_PER_WEEK
}
fn default_fortnightly() -> f64 {
    TRUCK_MAX_FORTNIGHTLY_DRIVING_HOURS
}
fn default_daily_rest() -> f64 {
    TRUCK_DAILY_REST_HOURS
}
fn default_daily_rest_reduced() -> f64 {
    TRUCK_DAILY_REST_REDUCED_HOURS
}
fn default_max_reduced_daily_rests() -> u32 {
    TRUCK_MAX_REDUCED_DAILY_RESTS
}
fn default_split_daily_first() -> f64 {
    TRUCK_SPLIT_DAILY_REST_FIRST_HOURS
}
fn default_split_daily_second() -> f64 {
    TRUCK_SPLIT_DAILY_REST_SECOND_HOURS
}
fn default_weekly_rest() -> f64 {
    TRUCK_WEEKLY_REST_HOURS
}
fn default_weekly_rest_reduced() -> f64 {
    TRUCK_WEEKLY_REST_REDUCED_HOURS
}
fn default_max_consecutive_days() -> u32 {
    TRUCK_MAX_CONSECUTIVE_WORKING_DAYS
}
fn default_exceptional_hours() -> f64 {
    TRUCK_EXCEPTIONAL_EXTENSION_HOURS
}
fn default_true() -> bool {
    true
}

impl Default for TruckRestParams {
    fn default() -> Self {
        Self {
            mandatory_break_after_hours: TRUCK_MANDATORY_BREAK_AFTER_HOURS,
            break_duration_minutes: TRUCK_BREAK_DURATION_MINUTES,
            prefer_split_break: false,
            max_daily_driving_hours: TRUCK_MAX_DAILY_DRIVING_HOURS,
            max_daily_driving_extended_hours: TRUCK_MAX_DAILY_DRIVING_EXTENDED_HOURS,
            max_daily_extensions_per_week: TRUCK_MAX_DAILY_EXTENSIONS_PER_WEEK,
            max_weekly_driving_hours: TRUCK_MAX_WEEKLY_DRIVING_HOURS,
            max_fortnightly_driving_hours: TRUCK_MAX_FORTNIGHTLY_DRIVING_HOURS,
            daily_rest_hours: TRUCK_DAILY_REST_HOURS,
            daily_rest_reduced_hours: TRUCK_DAILY_REST_REDUCED_HOURS,
            max_reduced_daily_rests: TRUCK_MAX_REDUCED_DAILY_RESTS,
            split_daily_rest_first_hours: TRUCK_SPLIT_DAILY_REST_FIRST_HOURS,
            split_daily_rest_second_hours: TRUCK_SPLIT_DAILY_REST_SECOND_HOURS,
            prefer_split_daily_rest: false,
            weekly_rest_hours: TRUCK_WEEKLY_REST_HOURS,
            weekly_rest_reduced_hours: TRUCK_WEEKLY_REST_REDUCED_HOURS,
            max_consecutive_working_days: TRUCK_MAX_CONSECUTIVE_WORKING_DAYS,
            regular_weekly_rest_not_in_cab: true,
            exceptional_extension_hours: TRUCK_EXCEPTIONAL_EXTENSION_HOURS,
            exceptional_extension_armed: false,
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
            Profile::Truck | Profile::TruckElectric => {
                ProfileRestParams::Truck(&self.truck)
            }
            // MobileHome: private motorhome drivers are not under EC 561/2006.
            // Rest reminders use the same soft Car cadence (driver-chosen hours),
            // not commercial HGV legal tracking. Clearance limits still use the
            // truck routing profile elsewhere.
            Profile::MobileHome => ProfileRestParams::Car(&self.car),
            Profile::Hiking => ProfileRestParams::Hiking(&self.hiking),
            Profile::Cycling | Profile::CyclingElectric => {
                ProfileRestParams::Cycling(&self.cycling)
            }
        }
    }

    pub fn eco_mode_enabled(&self, profile: Profile) -> bool {
        match profile {
            Profile::Car
            | Profile::CarElectric
            | Profile::Motorcycle
            | Profile::MotorcycleElectric
            | Profile::MobileHome => self.car.eco_mode_enabled,
            Profile::Truck | Profile::TruckElectric => self.truck.eco_mode_enabled,
            Profile::Hiking | Profile::Cycling | Profile::CyclingElectric => true,
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
