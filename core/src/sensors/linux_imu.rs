//! Linux IMU helpers for SBC / USB-I2C mounts.
//!
//! gpsd does not provide IMU data. This module assumes a board-level sensor
//! (e.g. BMI160 / BNO055) whose fused or raw readings are published as
//! [`ImuSample`]. Full chip drivers are deployment-specific; here we provide
//! the publish path and a simple software complementary filter for accel+gyro
//! when a higher-level AHRS crate is not wired yet.
//!
//! Compass / Direction-of-travel map modes consume [`ImuSample::heading_deg`]
//! (or gpsd course for travel mode) via [`SensorBus`] — the same bus Android
//! uses conceptually for fused rotation / GPS bearing.

use std::time::{SystemTime, UNIX_EPOCH};

use super::{ImuSample, SensorBus};

/// Publish a fused heading/attitude sample onto the sensor bus.
pub fn publish_imu(bus: &SensorBus, heading_deg: f64, pitch_deg: f64, roll_deg: f64) {
    let timestamp_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    bus.publish_imu(ImuSample {
        heading_deg,
        pitch_deg,
        roll_deg,
        timestamp_unix,
    });
}

/// Very light complementary filter for demo / bring-up (not a full AHRS).
/// Prefer uf-ahrs / chip fusion for production SBC deployments.
#[derive(Debug, Clone)]
pub struct SimpleImuFusion {
    pub heading_deg: f64,
    pub pitch_deg: f64,
    pub roll_deg: f64,
    alpha: f64,
}

impl Default for SimpleImuFusion {
    fn default() -> Self {
        Self {
            heading_deg: 0.0,
            pitch_deg: 0.0,
            roll_deg: 0.0,
            alpha: 0.98,
        }
    }
}

impl SimpleImuFusion {
    pub fn new(alpha: f64) -> Self {
        Self {
            alpha: alpha.clamp(0.0, 1.0),
            ..Self::default()
        }
    }

    /// `gyro_*` in deg/s; `accel_*` in m/s²; `dt_s` seconds; optional mag heading.
    pub fn update(
        &mut self,
        gyro_x: f64,
        gyro_y: f64,
        gyro_z: f64,
        accel_x: f64,
        accel_y: f64,
        accel_z: f64,
        mag_heading_deg: Option<f64>,
        dt_s: f64,
    ) {
        self.pitch_deg += gyro_x * dt_s;
        self.roll_deg += gyro_y * dt_s;
        self.heading_deg = (self.heading_deg + gyro_z * dt_s).rem_euclid(360.0);

        let accel_pitch = (accel_x / accel_z.max(1e-6)).atan().to_degrees();
        let accel_roll = (accel_y / accel_z.max(1e-6)).atan().to_degrees();
        self.pitch_deg = self.alpha * self.pitch_deg + (1.0 - self.alpha) * accel_pitch;
        self.roll_deg = self.alpha * self.roll_deg + (1.0 - self.alpha) * accel_roll;

        if let Some(mag) = mag_heading_deg {
            self.heading_deg =
                self.alpha * self.heading_deg + (1.0 - self.alpha) * mag.rem_euclid(360.0);
        }
    }

    pub fn sample(&self) -> ImuSample {
        ImuSample {
            heading_deg: self.heading_deg,
            pitch_deg: self.pitch_deg,
            roll_deg: self.roll_deg,
            timestamp_unix: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publish_reaches_bus() {
        let bus = SensorBus::new();
        publish_imu(&bus, 180.0, 1.0, -2.0);
        let s = bus.latest_imu().expect("imu");
        assert!((s.heading_deg - 180.0).abs() < 1e-9);
    }
}
