//! Rest-stop interval helpers driven by persisted profile parameters.

mod hiking_multi_day;
mod motor_multi_day;
mod truck_duty;
mod truck_multi_day;

use crate::config::{Profile, ProfileRestParams, RestConfig, TruckRestParams};
use crate::config::{
    TRUCK_SPLIT_BREAK_FIRST_MINUTES, TRUCK_SPLIT_BREAK_SECOND_MINUTES,
};

pub use hiking_multi_day::{
    choose_hiking_overnight, hiking_samples_from_coords, plan_hiking_multi_day,
    HikingDaySegment, HikingMultiDayPlan, HikingOvernightStop, HikingRouteSample,
    OVERNIGHT_NEAR_HUT_MAX_M,
};
pub use motor_multi_day::{
    car_style_daily_hours, cycling_daily_km, motor_daily_budget, plan_motor_multi_day,
    uses_motor_multi_day, MotorDailyBudget, MotorDaySegment, MotorMultiDayPlan,
    MotorOvernightCandidate, MotorOvernightKind, MotorOvernightStop,
};
pub use truck_duty::{commit_truck_trip, evaluate_truck_trip, TruckDutyEvaluation};
pub use truck_multi_day::{
    choose_daily_overnight_rest, commit_truck_multi_day_plan, plan_truck_multi_day,
    truck_day_cap_hours, TruckDaySegment, TruckMultiDayPlan, TruckOvernightKind,
    TruckOvernightRest, TruckRestCandidate, TruckRestFacility,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BreakKind {
    Main,
    Alternative,
}

/// Returns the next break threshold for distance-based profiles (hiking/cycling).
pub fn next_break_distance_km(
    config: &RestConfig,
    profile: Profile,
    kind: BreakKind,
) -> Option<f64> {
    match config.for_profile(profile) {
        ProfileRestParams::Hiking(p) => Some(match kind {
            BreakKind::Main => p.main_break_distance_km,
            BreakKind::Alternative => p.alternative_break_distance_km,
        }),
        ProfileRestParams::Cycling(p) => Some(match kind {
            BreakKind::Main => p.main_break_distance_km,
            BreakKind::Alternative => p.alternative_break_distance_km,
        }),
        _ => None,
    }
}

pub fn max_daily_distance_km(config: &RestConfig, profile: Profile) -> Option<f64> {
    match config.for_profile(profile) {
        ProfileRestParams::Hiking(p) => Some(p.max_daily_distance_km),
        ProfileRestParams::Cycling(p) => Some(p.max_daily_distance_km),
        _ => None,
    }
}

pub fn car_break_interval_hours(config: &RestConfig) -> (f64, f64) {
    let p = &config.car;
    (p.break_interval_min_hours, p.break_interval_max_hours)
}

pub fn truck_mandatory_break_after_hours(config: &RestConfig) -> f64 {
    config.truck.mandatory_break_after_hours
}

pub fn truck_break_duration_minutes(config: &RestConfig) -> u32 {
    config.truck.break_duration_minutes
}

pub fn truck_max_daily_driving_hours(config: &RestConfig) -> f64 {
    config.truck.max_daily_driving_hours
}

pub fn truck_max_weekly_driving_hours(config: &RestConfig) -> f64 {
    config.truck.max_weekly_driving_hours
}

pub fn truck_max_fortnightly_driving_hours(config: &RestConfig) -> f64 {
    config.truck.max_fortnightly_driving_hours
}

pub fn uses_truck_rest(profile: Profile) -> bool {
    matches!(profile, Profile::Truck | Profile::TruckElectric)
}

/// Effective continuous break minutes for HUD / labels (45, or 15+30 when split).
pub fn truck_effective_break_parts(truck: &TruckRestParams) -> Vec<u32> {
    if truck.prefer_split_break {
        vec![
            TRUCK_SPLIT_BREAK_FIRST_MINUTES,
            TRUCK_SPLIT_BREAK_SECOND_MINUTES,
        ]
    } else {
        vec![truck.break_duration_minutes]
    }
}

/// Distance between motor break stops for the given profile.
///
/// Truck-like profiles convert `mandatory_break_after_hours` into kilometres using
/// the planned trip's average speed (`dist_km / eta_hours`). Other motor profiles
/// keep the legacy km heuristic (capped mid-route spacing).
pub fn motor_break_interval_km(
    profile: Profile,
    rest: &RestConfig,
    dist_km: f64,
    eta_minutes: f64,
) -> f64 {
    if uses_truck_rest(profile) {
        truck_break_interval_km(&rest.truck, dist_km, eta_minutes)
    } else {
        40.0_f64.min((dist_km / 2.0).max(15.0))
    }
}

pub fn truck_break_interval_km(truck: &TruckRestParams, dist_km: f64, eta_minutes: f64) -> f64 {
    let eta_h = (eta_minutes / 60.0).max(1e-6);
    let speed_kmh = (dist_km / eta_h).max(1.0);
    (speed_kmh * truck.mandatory_break_after_hours).max(1.0)
}

/// How many mandatory breaks are required for a truck trip of `driving_hours`.
pub fn truck_required_breaks(truck: &TruckRestParams, driving_hours: f64) -> u32 {
    let interval = truck.mandatory_break_after_hours.max(1e-6);
    if driving_hours <= interval {
        0
    } else {
        (driving_hours / interval).floor() as u32
    }
}

/// Break sample distances (km from start) for a truck plan.
pub fn truck_break_distances_km(
    truck: &TruckRestParams,
    dist_km: f64,
    eta_minutes: f64,
) -> Vec<f64> {
    let interval = truck_break_interval_km(truck, dist_km, eta_minutes);
    let mut out = Vec::new();
    let mut next = interval;
    while next < dist_km - 0.5 {
        out.push(next);
        next += interval;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TruckRestParams;

    #[test]
    fn truck_break_spacing_follows_mandatory_hours_not_car_heuristic() {
        let mut truck = TruckRestParams::default();
        // 500 km in 6.25 h => 80 km/h; 4.5 h -> break at 360 km.
        let dist = 500.0;
        let eta_min = 6.25 * 60.0;
        let breaks = truck_break_distances_km(&truck, dist, eta_min);
        assert_eq!(breaks.len(), 1, "expected one break under default 4.5 h: {breaks:?}");
        assert!(
            (breaks[0] - 360.0).abs() < 1.0,
            "break at {} km, want ~360",
            breaks[0]
        );

        // Shorter interval must move the break earlier (route-level evidence).
        truck.mandatory_break_after_hours = 2.0;
        let tight = truck_break_distances_km(&truck, dist, eta_min);
        assert!(
            tight.len() >= 2,
            "2 h interval on 6.25 h drive needs >=2 breaks: {tight:?}"
        );
        assert!(
            tight[0] < breaks[0] - 50.0,
            "tighter interval must place first break earlier: {} vs {}",
            tight[0],
            breaks[0]
        );
    }

    #[test]
    fn motor_interval_uses_truck_params_for_truck_profile() {
        let mut rest = RestConfig::default();
        rest.truck.mandatory_break_after_hours = 3.0;
        let truck_km = motor_break_interval_km(Profile::Truck, &rest, 300.0, 300.0);
        let car_km = motor_break_interval_km(Profile::Car, &rest, 300.0, 300.0);
        // Truck @ 60 km/h * 3 h = 180 km; car heuristic = min(40, max(15, 150)) = 40.
        assert!((truck_km - 180.0).abs() < 0.1, "truck_km={truck_km}");
        assert!((car_km - 40.0).abs() < 0.1, "car_km={car_km}");
        assert!(truck_km > car_km * 2.0);
    }

    #[test]
    fn split_break_parts_match_regulation() {
        let mut t = TruckRestParams::default();
        assert_eq!(truck_effective_break_parts(&t), vec![45]);
        t.prefer_split_break = true;
        assert_eq!(truck_effective_break_parts(&t), vec![15, 30]);
    }
}
