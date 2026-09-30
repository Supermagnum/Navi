//! Capability surface exposed to sandboxed plugins.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Declared plugin capabilities (must match the manifest before load).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    PositionRead,
    PoiQuery,
    PoiWrite,
    Log,
    WeatherRead,
    RouteRead,
    SafetyConfigRead,
    AdminRegionRead,
    ClockRead,
    PluginKv,
    ProtectedAreaQuery,
    LandTenureQuery,
    LandcoverQuery,
    TravelModeRead,
    VehicleProfileRead,
    TravellerProfileRead,
    RouteDestinationRead,
}

impl Capability {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PositionRead => "position_read",
            Self::PoiQuery => "poi_query",
            Self::PoiWrite => "poi_write",
            Self::Log => "log",
            Self::WeatherRead => "weather_read",
            Self::RouteRead => "route_read",
            Self::SafetyConfigRead => "safety_config_read",
            Self::AdminRegionRead => "admin_region_read",
            Self::ClockRead => "clock_read",
            Self::PluginKv => "plugin_kv",
            Self::ProtectedAreaQuery => "protected_area_query",
            Self::LandTenureQuery => "land_tenure_query",
            Self::LandcoverQuery => "landcover_query",
            Self::TravelModeRead => "travel_mode_read",
            Self::VehicleProfileRead => "vehicle_profile_read",
            Self::TravellerProfileRead => "traveller_profile_read",
            Self::RouteDestinationRead => "route_destination_read",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "position_read" | "position" => Some(Self::PositionRead),
            "poi_query" => Some(Self::PoiQuery),
            "poi_write" => Some(Self::PoiWrite),
            "log" => Some(Self::Log),
            "weather_read" => Some(Self::WeatherRead),
            "route_read" => Some(Self::RouteRead),
            "safety_config_read" => Some(Self::SafetyConfigRead),
            "admin_region_read" => Some(Self::AdminRegionRead),
            "clock_read" => Some(Self::ClockRead),
            "plugin_kv" | "storage" => Some(Self::PluginKv),
            "protected_area_query" => Some(Self::ProtectedAreaQuery),
            "land_tenure_query" => Some(Self::LandTenureQuery),
            "landcover_query" => Some(Self::LandcoverQuery),
            "travel_mode_read" => Some(Self::TravelModeRead),
            "vehicle_profile_read" => Some(Self::VehicleProfileRead),
            "traveller_profile_read" => Some(Self::TravellerProfileRead),
            "route_destination_read" => Some(Self::RouteDestinationRead),
            _ => None,
        }
    }

    /// Every capability the host may grant (policy universe).
    pub fn all() -> &'static [Capability] {
        &[
            Self::PositionRead,
            Self::PoiQuery,
            Self::PoiWrite,
            Self::Log,
            Self::WeatherRead,
            Self::RouteRead,
            Self::SafetyConfigRead,
            Self::AdminRegionRead,
            Self::ClockRead,
            Self::PluginKv,
            Self::ProtectedAreaQuery,
            Self::LandTenureQuery,
            Self::LandcoverQuery,
            Self::TravelModeRead,
            Self::VehicleProfileRead,
            Self::TravellerProfileRead,
            Self::RouteDestinationRead,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    pub lat: f64,
    pub lon: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PoiWrite {
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub kind: String,
    /// Host-evaluated opening-hours: `"true"` / `"false"` / `"unknown"`.
    pub open_now: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeatherSampleView {
    pub lat: f64,
    pub lon: f64,
    pub icon_slug: String,
    pub temp_c: Option<f64>,
    pub wind_ms: Option<f64>,
    pub precip_mm: Option<f64>,
    pub pressure_hpa: Option<f64>,
    pub provider: String,
    pub fetched_at_unix: i64,
    pub stale: bool,
    pub summary: String,
}

/// Whether a host spatial layer is available for hard filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LayerStatus {
    /// No authoritative data — guests must not treat absence as a pass.
    #[default]
    Unknown,
    /// Layer is loaded; empty hit lists mean "not inside".
    Ready,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SafetyConfigView {
    pub min_building_distance_m: f64,
    pub min_glacier_distance_m: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AdminRegionView {
    /// ISO 3166-1 alpha-2 lowercase, or `None` when unknown/ambiguous.
    pub country_iso: Option<String>,
    /// ISO 3166-2 when known; always `None` until a subdivision layer exists.
    pub subdivision_iso: Option<String>,
}

/// Local calendar clock for date-gated packs. Embedder supplies device-local date.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClockView {
    pub unix_secs: i64,
    pub year: i32,
    pub month: u32,
    pub day: u32,
    /// Always `"local"` when available — fire windows and night store are local-date rules.
    pub timezone: String,
}

/// Whether plugin-local KV is backed (required for max_nights hard filters).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginKvStatus {
    Available,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RouteView {
    /// Sampled corridor waypoints `[lat, lon]`.
    pub waypoints: Vec<[f64; 2]>,
    /// Optional road∩track seeds the host is willing to expose.
    #[serde(default)]
    pub junctions: Vec<RoadTrackJunction>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoadTrackJunction {
    pub lat: f64,
    pub lon: f64,
    /// `tertiary` | `unclassified` | `service`
    pub road_highway: String,
    pub track_continues: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RouteDestinationView {
    pub lat: Option<f64>,
    pub lon: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ProtectedAreaQueryView {
    pub status: LayerStatus,
    #[serde(default)]
    pub areas: Vec<ProtectedAreaHit>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProtectedAreaHit {
    pub name: Option<String>,
    pub kind: String,
    pub operator: Option<String>,
    pub zone: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LandTenureView {
    /// `"unknown"` when no authoritative layer covers the point.
    pub manager_type: String,
    pub manager_name: Option<String>,
    pub unit_id: Option<String>,
    pub unit_name: Option<String>,
    pub sub_unit: Option<String>,
    pub border_zone: Option<bool>,
}

impl Default for LandTenureView {
    fn default() -> Self {
        Self {
            manager_type: "unknown".into(),
            manager_name: None,
            unit_id: None,
            unit_name: None,
            sub_unit: None,
            border_zone: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct LandcoverQueryView {
    pub status: LayerStatus,
    /// Meaningful only when `status == Ready`.
    pub class: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TravelModeView {
    NonMotorised,
    Motorised,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct VehicleProfileView {
    /// `car` | `campervan_motorhome` | `hgv` | `unknown` (no caravan_combo).
    pub class: String,
    pub gross_weight_kg: Option<f64>,
    pub is_professional_driver_under_rest_rules: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TravellerProfileView {
    /// ISO 3166-1 lowercase, or `None` = unknown.
    pub residency_country: Option<String>,
}

/// Host-side callbacks the engine may invoke on behalf of a plugin.
///
/// New camping capabilities default to honest **unknown / empty** so embedders
/// only override what they can back with real data.
pub trait HostApi: Send + Sync {
    fn position(&self) -> Option<Position>;
    fn poi_query(&self, lat: f64, lon: f64, radius_m: f64) -> Vec<PoiWrite>;
    fn poi_write(&mut self, poi: PoiWrite) -> Result<(), String>;
    fn log(&mut self, message: &str);

    fn weather_read(&self, lat: f64, lon: f64, radius_m: f64) -> Vec<WeatherSampleView> {
        let _ = (lat, lon, radius_m);
        Vec::new()
    }

    fn route_read(&self) -> RouteView {
        RouteView::default()
    }

    fn route_destination_read(&self) -> RouteDestinationView {
        RouteDestinationView::default()
    }

    /// `None` = unavailable. Guests must decline wild camp (never treat as 0 m).
    fn safety_config_read(&self) -> Option<SafetyConfigView> {
        None
    }

    fn admin_region_read(&self, lat: f64, lon: f64) -> AdminRegionView {
        let _ = (lat, lon);
        AdminRegionView::default()
    }

    /// `None` = unavailable. Guests must use the cautious fire text, never the
    /// permissive outside-window wording.
    fn clock_read(&self) -> Option<ClockView> {
        None
    }

    /// Default: KV not backed. Packs with max_nights hard filters must decline.
    fn plugin_kv_status(&self) -> PluginKvStatus {
        PluginKvStatus::Unavailable
    }

    fn plugin_kv_get(&self, key: &str) -> Option<String> {
        let _ = key;
        None
    }

    fn plugin_kv_set(&mut self, key: &str, value: &str) -> Result<(), String> {
        let _ = (key, value);
        Err("plugin_kv unavailable".into())
    }

    fn protected_area_query(&self, lat: f64, lon: f64) -> ProtectedAreaQueryView {
        let _ = (lat, lon);
        ProtectedAreaQueryView {
            status: LayerStatus::Unknown,
            areas: Vec::new(),
        }
    }

    fn land_tenure_query(&self, lat: f64, lon: f64) -> LandTenureView {
        let _ = (lat, lon);
        LandTenureView::default()
    }

    fn landcover_query(&self, lat: f64, lon: f64) -> LandcoverQueryView {
        let _ = (lat, lon);
        LandcoverQueryView {
            status: LayerStatus::Unknown,
            class: None,
        }
    }

    fn travel_mode_read(&self) -> TravelModeView {
        TravelModeView::Unknown
    }

    fn vehicle_profile_read(&self) -> VehicleProfileView {
        VehicleProfileView {
            class: "unknown".into(),
            gross_weight_kg: None,
            is_professional_driver_under_rest_rules: false,
        }
    }

    fn traveller_profile_read(&self) -> TravellerProfileView {
        TravellerProfileView::default()
    }
}

/// In-memory KV used by tests and simple host embeds.
#[derive(Debug, Default)]
pub struct MemoryPluginKv {
    pub map: HashMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_camping_capabilities_including_storage_alias() {
        assert_eq!(Capability::parse("plugin_kv"), Some(Capability::PluginKv));
        assert_eq!(Capability::parse("storage"), Some(Capability::PluginKv));
        assert_eq!(
            Capability::parse("admin_region_read"),
            Some(Capability::AdminRegionRead)
        );
        assert_eq!(
            Capability::parse("land_tenure_query"),
            Some(Capability::LandTenureQuery)
        );
        assert!(Capability::parse("not_a_cap").is_none());
    }

    #[test]
    fn all_capabilities_round_trip_as_str() {
        for cap in Capability::all() {
            assert_eq!(Capability::parse(cap.as_str()), Some(*cap));
        }
    }
}
