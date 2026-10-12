//! Vehicle-frame attitude from gravity / accelerometer and magnetic field.
//!
//! Frame conventions (vehicle frame, after the mount rotation):
//! +X right, +Y forward, +Z up. The "up" vector is the direction the specific
//! force points at rest (an Android accelerometer reads +g along up).
//!
//! * pitch: nose up positive (`up . Y = sin(pitch)`).
//! * roll: right side down positive (`up . X = -sin(roll)`).
//! * heading: magnetic degrees clockwise from north of the vehicle's +Y axis
//!   (no declination correction).

use crate::math::{cross, dot, mat_vec, normalize, Mat3, Vec3};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Attitude {
    pub pitch_deg: f64,
    pub roll_deg: f64,
    /// Angle between vehicle up and true up, degrees.
    pub tilt_deg: f64,
}

/// `mount` rotates device-frame vectors into the vehicle frame.
pub fn attitude(mount: &Mat3, up_device: Vec3) -> Option<Attitude> {
    let up = normalize(mat_vec(mount, up_device))?;
    let pitch = up[1].atan2((up[0] * up[0] + up[2] * up[2]).sqrt());
    let roll = (-up[0]).atan2(up[2]);
    Some(Attitude {
        pitch_deg: pitch.to_degrees(),
        roll_deg: roll.to_degrees(),
        tilt_deg: up[2].clamp(-1.0, 1.0).acos().to_degrees(),
    })
}

/// Tilt-compensated magnetic heading of the vehicle's forward axis, `[0, 360)`.
pub fn heading_deg(mount: &Mat3, up_device: Vec3, mag_device: Vec3) -> Option<f64> {
    let up = normalize(mat_vec(mount, up_device))?;
    let mag = mat_vec(mount, mag_device);
    let east = cross(mag, up);
    if dot(east, east).sqrt() < 1e-6 {
        return None;
    }
    let east = normalize(east)?;
    let north = cross(up, east);
    let h = east[1].atan2(north[1]).to_degrees();
    Some(h.rem_euclid(360.0))
}
