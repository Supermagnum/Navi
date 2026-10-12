//! Level calibration: derive the device-to-vehicle mount rotation from a
//! stationary window of gravity (or accelerometer) samples.
//!
//! The user parks on a level surface and taps Calibrate. The mean "up" vector in
//! the device frame is rotated onto vehicle up (+Z) by the smallest rotation.
//! Rotation about the vertical axis (yaw) cannot be observed from gravity alone
//! and is assumed zero, so the device's screen axes stay aligned with the vehicle
//! axes for pitch/roll. That leaves a small, second-order yaw leakage when the
//! mount is tilted about two axes at once (about 0.2 deg of roll per 5 deg of
//! pitch for a 15/10 deg mount).

use serde::{Deserialize, Serialize};

use crate::math::{angle_deg, normalize, rotation_between, Mat3, Vec3};

/// Standard gravity, m/s^2.
pub const GRAVITY: f64 = 9.806_65;
/// Samples retained for the stationary window.
pub const HIST_LEN: usize = 16;
pub const MIN_SAMPLES: usize = 8;
/// The window must span at least this long.
pub const MIN_SPAN_MS: i64 = 700;
/// Gyroscope magnitude above this is motion, rad/s.
pub const MAX_GYRO_RAD_S: f64 = 0.05;
/// Linear-acceleration magnitude above this is motion, m/s^2.
pub const MAX_LINEAR_ACCEL: f64 = 0.3;
/// Per-sample |a| may deviate from 1 g by at most this, m/s^2.
pub const MAX_NORM_DEVIATION: f64 = 0.6;
/// Mean specific force must be within this of 1 g, m/s^2.
pub const MAX_MEAN_GRAVITY_ERROR: f64 = 1.5;
/// Max angular spread of "up" inside the window, degrees.
pub const MAX_SPREAD_DEG: f64 = 1.0;

/// One stored tick: device-frame up vector (m/s^2) plus motion indicators.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HistEntry {
    pub t: i64,
    pub u: Vec3,
    /// Gyroscope magnitude, rad/s, when a gyroscope is present.
    pub w: Option<f64>,
    /// Linear-acceleration magnitude, m/s^2, when the sensor is present.
    pub l: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalibrationError {
    NotEnoughSamples,
    /// The unit (or vehicle) moved during the window.
    Moving,
    /// Mean specific force is not close to 1 g (free fall, bad sensor).
    ImplausibleGravity,
}

pub fn calibrate(window: &[HistEntry]) -> Result<Mat3, CalibrationError> {
    let (first, last) = match (window.first(), window.last()) {
        (Some(f), Some(l)) => (f, l),
        _ => return Err(CalibrationError::NotEnoughSamples),
    };
    if window.len() < MIN_SAMPLES || last.t - first.t < MIN_SPAN_MS {
        return Err(CalibrationError::NotEnoughSamples);
    }

    let mut sum = [0.0; 3];
    for e in window {
        if !e.u.iter().all(|v| v.is_finite()) {
            return Err(CalibrationError::ImplausibleGravity);
        }
        for (s, v) in sum.iter_mut().zip(e.u) {
            *s += v;
        }
    }
    let mean_norm = crate::math::norm(sum) / window.len() as f64;
    if (mean_norm - GRAVITY).abs() > MAX_MEAN_GRAVITY_ERROR {
        return Err(CalibrationError::ImplausibleGravity);
    }
    let mean = normalize(sum).ok_or(CalibrationError::ImplausibleGravity)?;
    for e in window {
        let u = normalize(e.u).ok_or(CalibrationError::ImplausibleGravity)?;
        if e.w.is_some_and(|w| w > MAX_GYRO_RAD_S)
            || e.l.is_some_and(|l| l > MAX_LINEAR_ACCEL)
            || (crate::math::norm(e.u) - GRAVITY).abs() > MAX_NORM_DEVIATION
            || angle_deg(u, mean) > MAX_SPREAD_DEG
        {
            return Err(CalibrationError::Moving);
        }
    }
    Ok(rotation_between(mean, [0.0, 0.0, 1.0]))
}
