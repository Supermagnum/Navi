//! Sensor thread placeholders. Android GPS/IMU integration is out of scope for this pass.

/// Position sample published by the T0 sensor thread (reference: Android fused location).
#[derive(Debug, Clone, Copy, Default)]
pub struct PositionSample {
    pub lat: f64,
    pub lon: f64,
    pub speed_m_s: f64,
    pub course_deg: f64,
    pub timestamp_unix: u64,
}
