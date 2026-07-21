//! Rest-stop interval helpers driven by persisted profile parameters.

use crate::config::{Profile, ProfileRestParams, RestConfig};

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
