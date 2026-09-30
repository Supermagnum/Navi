//! Host port for the camping engine (filled by native embedder or HostApi adapter).

use crate::safety_view::OvernightSafety;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TravelMode {
    NonMotorised,
    Motorised,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

/// Protected-area query from the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtectedAreaStatus {
    /// Layer not ready / unknown — Iceland declines; other Tier A packs note it.
    Unknown,
    /// Layer ready and point is not inside a protected area.
    Clear,
    /// Layer ready and point is inside a protected area.
    Inside,
}

/// What the camping engine needs from the host. Fail-safe: unavailable → decline.
pub trait CampingHost {
    fn safety_config(&self) -> Option<OvernightSafety>;
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
    /// Prefer [`Self::protected_area_status`]. Default: layer not ready.
    fn protected_area_layer_ready(&self) -> bool {
        false
    }
    fn protected_area_status(&self, _lat: f64, _lon: f64) -> ProtectedAreaStatus {
        if self.protected_area_layer_ready() {
            ProtectedAreaStatus::Clear
        } else {
            ProtectedAreaStatus::Unknown
        }
    }
    fn landcover_layer_ready(&self) -> bool {
        false
    }
    /// Loch Lomond & Trossachs CMZ polygon layer available.
    fn cmz_layer_ready(&self) -> bool {
        false
    }
    /// When CMZ layer is ready: is the point inside a management zone?
    fn cmz_contains(&self, _lat: f64, _lon: f64) -> bool {
        false
    }
}
