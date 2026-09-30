//! Host port for the camping engine (filled by native embedder or HostApi adapter).

use crate::packs::DesignatedLayer;
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
    Unknown,
    Clear,
    Inside,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LandTenureStatus {
    Unknown,
    Known,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TentSiteHit {
    pub lat: f64,
    pub lon: f64,
    pub name: Option<String>,
}

/// What the camping engine needs from the host. Fail-safe: unavailable → decline.
pub trait CampingHost {
    fn safety_config(&self) -> Option<OvernightSafety>;
    fn clock_local(&self) -> Option<LocalDate>;
    fn plugin_kv_available(&self) -> bool;
    fn kv_get(&self, key: &str) -> Option<String>;
    fn kv_set(&mut self, key: &str, value: &str) -> Result<(), String>;
    fn admin_country_iso(&self, lat: f64, lon: f64) -> Option<String>;
    fn admin_subdivision_iso(&self, lat: f64, lon: f64) -> Option<String>;
    fn travel_mode(&self) -> TravelMode;
    fn overnight_buildings(&self) -> &[(f64, f64)];
    fn overnight_glacier_rings(&self) -> &[Vec<[f64; 2]>];
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
    fn cmz_layer_ready(&self) -> bool {
        false
    }
    fn cmz_contains(&self, _lat: f64, _lon: f64) -> bool {
        false
    }
    /// Maintainer flags for Tier B / land-manager packs. Default: OFF.
    fn camping_pack_flag_enabled(&self, _flag_id: &str) -> bool {
        false
    }
    fn designated_layer_ready(&self, _layer: DesignatedLayer) -> bool {
        false
    }
    /// TentSite POIs near a point. Default: empty (do not fabricate).
    fn tent_sites_near(&self, _lat: f64, _lon: f64, _radius_m: f64) -> Vec<TentSiteHit> {
        Vec::new()
    }
    fn land_tenure_status(&self, _lat: f64, _lon: f64) -> LandTenureStatus {
        LandTenureStatus::Unknown
    }
    /// PAD-US / Crown manager token when known (`blm`, `usfs`, `nps`, `crown_on`, …).
    fn land_tenure_manager(&self, _lat: f64, _lon: f64) -> Option<String> {
        None
    }
    fn above_treeline(&self, _lat: f64, _lon: f64) -> Option<bool> {
        None
    }
    fn is_forest(&self, _lat: f64, _lon: f64) -> Option<bool> {
        None
    }
    fn is_residential_ground(&self, _lat: f64, _lon: f64) -> Option<bool> {
        None
    }
    /// Traveller residency ISO; `None` = unknown (Ontario → non-resident path).
    fn residency_country_iso(&self) -> Option<String> {
        None
    }

    // --- Phase 4 vehicle overnight (defaults are fail-safe) ---

    fn vehicle_overnight_profile(&self) -> crate::vehicle::VehicleProfile {
        crate::vehicle::VehicleProfile::default()
    }
    fn route_destination(&self) -> Option<(f64, f64)> {
        None
    }
    /// Positive NVDB rest-site classification. `None` → never invent 809/39.
    fn nvdb_rest_kind(&self, _lat: f64, _lon: f64) -> Option<crate::vehicle::NvdbRestKind> {
        None
    }
    fn vehicle_sites_near(
        &self,
        _lat: f64,
        _lon: f64,
        _radius_m: f64,
    ) -> Vec<crate::vehicle::VehicleSiteHit> {
        Vec::new()
    }
    /// France seashore / listed / catchment / EBC layers all available.
    fn france_vehicle_exclude_layers_ready(&self) -> bool {
        false
    }
    fn france_vehicle_exclude_clear(&self, _lat: f64, _lon: f64) -> bool {
        false
    }
    fn usfs_mvum_layer_ready(&self) -> bool {
        false
    }
}
