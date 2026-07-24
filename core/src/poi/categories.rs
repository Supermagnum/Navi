use serde::{Deserialize, Serialize};

/// POI categories with default search radii defined in [`crate::config::defaults`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PoiCategory {
    Water,
    Cabin,
    General,
    NetworkHut,
    Restroom,
    OvernightFacility,
    /// Microbrewery / craft alcohol (OSM tag variants OR'd together).
    CraftBrewery,
    /// Peak / ridge were never pause labels. TentSite is camp_site / camp_pitch only.
    TentSite,
    /// Fishing spots (`leisure=fishing` and related).
    Fishing,
    /// Truck / HGV rest: `highway=rest_area`, `highway=services`, or HGV parking.
    RestArea,
    /// Motor overnight lodging: hotel / motel / guest house / etc.
    Lodging,
}

impl PoiCategory {
    pub fn default_radius_m(self, safety: &crate::config::SafetyConfig) -> f64 {
        match self {
            Self::Water => safety.poi_radius_water_m,
            Self::Cabin => safety.poi_radius_cabin_m,
            Self::General => safety.poi_radius_general_m,
            Self::NetworkHut => safety.poi_radius_network_hut_m,
            Self::Restroom => safety.restroom_radius_m(),
            Self::OvernightFacility => safety.poi_radius_cabin_m,
            // Same default reach as General (15 km) unless safety overrides general.
            Self::CraftBrewery => safety.poi_radius_general_m,
            Self::TentSite => safety.poi_radius_cabin_m,
            Self::Fishing => safety.poi_radius_general_m,
            // Slightly wider than amenity pauses — truck rest areas are sparser.
            Self::RestArea => safety.poi_radius_general_m.max(20_000.0),
            Self::Lodging => safety.poi_radius_general_m.max(20_000.0),
        }
    }
}
