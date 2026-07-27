//! Sensor samples for the highest-priority publish-only tier (docs/architecture.md).
//!
//! Android uses LocationManager / fused sensors. On Linux, optional `gpsd` and
//! `linux-imu` features feed the same [`PositionSample`] / [`ImuSample`] types.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

/// Position sample published by the T0 sensor thread.
#[derive(Debug, Clone, Copy, Default)]
pub struct PositionSample {
    pub lat: f64,
    pub lon: f64,
    /// WGS84 altitude in meters when the provider reports it.
    pub altitude_m: Option<f64>,
    pub speed_m_s: f64,
    /// Course over ground / track, degrees clockwise from north.
    pub course_deg: f64,
    pub timestamp_unix: u64,
    /// Horizontal accuracy in meters when known (gpsd eph / similar).
    pub horizontal_accuracy_m: Option<f64>,
    /// Satellite count from SKY when known.
    pub satellites_used: Option<u32>,
}

/// Attitude / heading from an IMU (or OS rotation vector on Android).
#[derive(Debug, Clone, Copy, Default)]
pub struct ImuSample {
    /// Magnetic or fused heading, degrees clockwise from north.
    pub heading_deg: f64,
    pub pitch_deg: f64,
    pub roll_deg: f64,
    pub timestamp_unix: u64,
}

/// Non-blocking bus: producers publish; consumers poll without waiting.
#[derive(Clone, Default)]
pub struct SensorBus {
    inner: Arc<Mutex<SensorBusInner>>,
}

#[derive(Default)]
struct SensorBusInner {
    position: Option<PositionSample>,
    imu: Option<ImuSample>,
}

impl SensorBus {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn publish_position(&self, sample: PositionSample) {
        if let Ok(mut g) = self.inner.lock() {
            g.position = Some(sample);
        }
    }

    pub fn publish_imu(&self, sample: ImuSample) {
        if let Ok(mut g) = self.inner.lock() {
            g.imu = Some(sample);
        }
    }

    pub fn latest_position(&self) -> Option<PositionSample> {
        self.inner.lock().ok().and_then(|g| g.position)
    }

    pub fn latest_imu(&self) -> Option<ImuSample> {
        self.inner.lock().ok().and_then(|g| g.imu)
    }
}

/// Channel helper for a dedicated sensor thread that never blocks publishers.
pub fn sensor_channel<T>() -> (Sender<T>, Receiver<T>) {
    mpsc::channel()
}

pub fn try_recv_latest<T>(rx: &Receiver<T>) -> Option<T> {
    let mut last = None;
    while let Ok(v) = rx.try_recv() {
        last = Some(v);
    }
    last
}

#[cfg(feature = "gpsd")]
pub mod gpsd;

#[cfg(feature = "linux-imu")]
pub mod linux_imu;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bus_publish_poll() {
        let bus = SensorBus::new();
        assert!(bus.latest_position().is_none());
        bus.publish_position(PositionSample {
            lat: 60.0,
            lon: 10.0,
            altitude_m: Some(200.0),
            speed_m_s: 12.0,
            course_deg: 90.0,
            timestamp_unix: 1,
            horizontal_accuracy_m: Some(5.0),
            satellites_used: Some(8),
        });
        let p = bus.latest_position().expect("pos");
        assert!((p.lat - 60.0).abs() < 1e-9);
        assert!((p.course_deg - 90.0).abs() < 1e-9);
    }
}
