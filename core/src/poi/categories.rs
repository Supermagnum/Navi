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
        }
    }
}
