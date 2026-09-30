//! Host port for the camping engine (filled by native embedder or HostApi adapter).

use driver_break_core::config::SafetyConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TravelMode {
    NonMotorised,
    Motorised,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalDate {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

impl LocalDate {
    pub fn to_naive(self) -> Option<chrono::NaiveDate> {
        chrono::NaiveDate::from_ymd_opt(self.year, self.month, self.day)
    }
}

/// What the camping engine needs from the host. Fail-safe: unavailable → decline.
pub trait CampingHost {
    fn safety_config(&self) -> Option<SafetyConfig>;
    /// Device-local calendar date. `None` → cautious fire text only.
    fn clock_local(&self) -> Option<LocalDate>;
    fn plugin_kv_available(&self) -> bool;
    fn kv_get(&self, key: &str) -> Option<String>;
    fn kv_set(&mut self, key: &str, value: &str) -> Result<(), String>;
    fn admin_country_iso(&self, lat: f64, lon: f64) -> Option<String>;
    fn admin_subdivision_iso(&self, lat: f64, lon: f64) -> Option<String>;
    fn travel_mode(&self) -> TravelMode;
    /// Overnight building points for the building-distance hard filter.
    fn overnight_buildings(&self) -> &[(f64, f64)];
    fn overnight_glacier_rings(&self) -> &[Vec<[f64; 2]>];
    /// Protected-area layer ready?
    fn protected_area_layer_ready(&self) -> bool {
        false
    }
    fn landcover_layer_ready(&self) -> bool {
        false
    }
}
