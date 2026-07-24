//! US FMCSA Hours of Service parameters (property-carrying CMVs).
//!
//! Figures from FMCSA summary / 49 CFR 395.3 (property-carrying):
//! <https://www.fmcsa.dot.gov/regulations/hours-service/summary-hours-service-regulations>

use serde::{Deserialize, Serialize};

/// Property-carrying FMCSA HOS defaults used by Navi truck planning.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FmcsaHosParams {
    /// Max driving hours after 10 h off duty (11 h).
    pub max_driving_hours: f64,
    /// Consecutive on-duty window after which driving is forbidden (14 h).
    pub on_duty_window_hours: f64,
    /// Minimum consecutive off-duty hours before a new shift (10 h).
    pub min_off_duty_hours: f64,
    /// Driving hours before a 30-minute break is required (8 h).
    pub break_after_driving_hours: f64,
    pub break_minutes: u32,
    /// Rolling on-duty limit hours (70 when using 8-day cycle).
    pub cycle_on_duty_hours: f64,
    /// Rolling cycle length in days (8).
    pub cycle_days: u32,
    /// Optional restart length that resets the cycle (34 h).
    pub restart_hours: f64,
}

impl Default for FmcsaHosParams {
    fn default() -> Self {
        Self {
            max_driving_hours: 11.0,
            on_duty_window_hours: 14.0,
            min_off_duty_hours: 10.0,
            break_after_driving_hours: 8.0,
            break_minutes: 30,
            cycle_on_duty_hours: 70.0,
            cycle_days: 8,
            restart_hours: 34.0,
        }
    }
}
