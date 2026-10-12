//! Configurable tilt-danger alarm with hysteresis.
//!
//! The guest never plays sound: it reports the level and the host owns audio.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AlarmLevel {
    #[default]
    None,
    Warning,
    Danger,
}

/// User thresholds in degrees (persisted by the host under `imu_alarm`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AlarmConfig {
    pub enabled: bool,
    pub pitch_warn_deg: f64,
    pub pitch_danger_deg: f64,
    pub roll_warn_deg: f64,
    pub roll_danger_deg: f64,
    /// Level must fall this far below a threshold before the alarm clears.
    pub hysteresis_deg: f64,
}

impl Default for AlarmConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            pitch_warn_deg: 20.0,
            pitch_danger_deg: 30.0,
            roll_warn_deg: 15.0,
            roll_danger_deg: 25.0,
            hysteresis_deg: 2.0,
        }
    }
}

fn sane(value: f64, default: f64, min: f64, max: f64) -> f64 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        default
    }
}

impl AlarmConfig {
    /// Clamp to usable values: thresholds in `(0, 89]`, danger >= warn.
    pub fn sanitized(self) -> Self {
        let d = Self::default();
        let pitch_warn = sane(self.pitch_warn_deg, d.pitch_warn_deg, 1.0, 89.0);
        let roll_warn = sane(self.roll_warn_deg, d.roll_warn_deg, 1.0, 89.0);
        Self {
            enabled: self.enabled,
            pitch_warn_deg: pitch_warn,
            pitch_danger_deg: sane(self.pitch_danger_deg, d.pitch_danger_deg, 1.0, 89.0)
                .max(pitch_warn),
            roll_warn_deg: roll_warn,
            roll_danger_deg: sane(self.roll_danger_deg, d.roll_danger_deg, 1.0, 89.0)
                .max(roll_warn),
            hysteresis_deg: sane(self.hysteresis_deg, d.hysteresis_deg, 0.0, 10.0),
        }
    }

    pub fn evaluate(&self, previous: AlarmLevel, pitch_deg: f64, roll_deg: f64) -> AlarmLevel {
        if !self.enabled {
            return AlarmLevel::None;
        }
        let axis = |value: f64, warn: f64, danger: f64| {
            let a = value.abs();
            let h = self.hysteresis_deg;
            if a >= danger || (previous == AlarmLevel::Danger && a >= danger - h) {
                AlarmLevel::Danger
            } else if a >= warn || (previous >= AlarmLevel::Warning && a >= warn - h) {
                AlarmLevel::Warning
            } else {
                AlarmLevel::None
            }
        };
        axis(pitch_deg, self.pitch_warn_deg, self.pitch_danger_deg).max(axis(
            roll_deg,
            self.roll_warn_deg,
            self.roll_danger_deg,
        ))
    }
}
