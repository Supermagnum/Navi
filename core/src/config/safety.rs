use serde::{Deserialize, Serialize};

use super::defaults::*;

/// General safety and POI radius configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyConfig {
    pub poi_radius_water_m: f64,
    pub poi_radius_cabin_m: f64,
    pub poi_radius_general_m: f64,
    pub poi_radius_network_hut_m: f64,
    pub poi_radius_restroom_m: Option<f64>,
    pub network_hut_preference_radius_m: f64,
    pub rest_interval_range_km_min: f64,
    pub rest_interval_range_km_max: f64,
    pub min_building_distance_m: f64,
    pub min_glacier_distance_m: f64,
}

impl Default for SafetyConfig {
    fn default() -> Self {
        Self {
            poi_radius_water_m: POI_RADIUS_WATER_M,
            poi_radius_cabin_m: POI_RADIUS_CABIN_M,
            poi_radius_general_m: POI_RADIUS_GENERAL_M,
            poi_radius_network_hut_m: POI_RADIUS_NETWORK_HUT_M,
            poi_radius_restroom_m: None,
            network_hut_preference_radius_m: POI_NETWORK_HUT_PREFERENCE_RADIUS_M,
            rest_interval_range_km_min: HIKING_ALTERNATIVE_BREAK_DISTANCE_KM,
            rest_interval_range_km_max: CYCLING_MAIN_BREAK_DISTANCE_KM,
            min_building_distance_m: SAFETY_MIN_BUILDING_DISTANCE_M,
            min_glacier_distance_m: SAFETY_MIN_GLACIER_DISTANCE_M,
        }
    }
}

impl SafetyConfig {
    pub fn restroom_radius_m(&self) -> f64 {
        self.poi_radius_restroom_m
            .unwrap_or(self.poi_radius_general_m)
    }
}
