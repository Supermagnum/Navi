//! Per-travel-profile POI search radii (pause / overnight / hut matching).

use serde::{Deserialize, Serialize};

use super::defaults::*;
use super::{Profile, SafetyConfig};

/// POI search radii and road-link policy for one travel profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfilePoiRadii {
    /// Crow-flies search radius for pause / overnight POI matching (metres).
    pub search_radius_m: f64,
    /// Cabin / wilderness-hut / overnight-facility radius (metres).
    pub cabin_radius_m: f64,
    /// Network-hut (DNT/STF/…) search radius (metres).
    pub network_hut_radius_m: f64,
    /// Prefer network huts within this radius before open cabins (metres).
    pub network_hut_preference_radius_m: f64,
    /// When true, only accept POIs linked to the road/path network (or planned
    /// corridor within [`crate::routing::graph::RoadNodeIndex::MAX_LINK_M`]).
    /// Motor profiles default to true (car / truck / mobile home / motorcycle).
    pub require_road_link: bool,
}

impl ProfilePoiRadii {
    fn hike_cycle_default() -> Self {
        // Align with Drive slider floors (hiking 10.5–20 km; cycling up to 28 km).
        Self {
            search_radius_m: 10_500.0,
            cabin_radius_m: 10_500.0,
            network_hut_radius_m: POI_RADIUS_NETWORK_HUT_M.min(28_000.0),
            network_hut_preference_radius_m: 10_500.0,
            require_road_link: false,
        }
    }

    fn motor_hours_default() -> Self {
        // Drive slider default mid-band: 3 h × 80 km/h.
        let search_m = 3.0 * 80.0 * 1000.0;
        Self {
            search_radius_m: search_m,
            cabin_radius_m: POI_RADIUS_CABIN_M,
            network_hut_radius_m: POI_RADIUS_NETWORK_HUT_M,
            network_hut_preference_radius_m: POI_NETWORK_HUT_PREFERENCE_RADIUS_M,
            require_road_link: true,
        }
    }

    /// Clamp obviously broken UI values to a usable band.
    pub fn sanitized(mut self) -> Self {
        self.search_radius_m = self.search_radius_m.clamp(500.0, 100_000.0);
        self.cabin_radius_m = self.cabin_radius_m.clamp(500.0, 100_000.0);
        self.network_hut_radius_m = self.network_hut_radius_m.clamp(500.0, 100_000.0);
        self.network_hut_preference_radius_m =
            self.network_hut_preference_radius_m.clamp(500.0, 100_000.0);
        self
    }

    /// Overlay hut / general radii onto a [`SafetyConfig`] for overnight planners.
    pub fn apply_to_safety(&self, safety: &mut SafetyConfig) {
        safety.poi_radius_general_m = self.search_radius_m;
        safety.poi_radius_cabin_m = self.cabin_radius_m;
        safety.poi_radius_network_hut_m = self.network_hut_radius_m;
        safety.network_hut_preference_radius_m = self.network_hut_preference_radius_m;
    }
}

/// Persisted table of POI radii keyed by menu travel profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfilePoiRadiiTable {
    pub car: ProfilePoiRadii,
    pub motorcycle: ProfilePoiRadii,
    pub truck: ProfilePoiRadii,
    pub mobile_home: ProfilePoiRadii,
    pub cycling: ProfilePoiRadii,
    pub cycling_electric: ProfilePoiRadii,
    pub hiking: ProfilePoiRadii,
}

impl Default for ProfilePoiRadiiTable {
    fn default() -> Self {
        Self {
            car: ProfilePoiRadii::motor_hours_default(),
            motorcycle: ProfilePoiRadii::motor_hours_default(),
            truck: ProfilePoiRadii::motor_hours_default(),
            mobile_home: ProfilePoiRadii::motor_hours_default(),
            cycling: ProfilePoiRadii::hike_cycle_default(),
            cycling_electric: ProfilePoiRadii::hike_cycle_default(),
            hiking: ProfilePoiRadii::hike_cycle_default(),
        }
    }
}

impl ProfilePoiRadiiTable {
    /// Resolve radii for a routing profile (electric variants share the base chip).
    pub fn for_profile(&self, profile: Profile) -> &ProfilePoiRadii {
        match profile {
            Profile::Car | Profile::CarElectric => &self.car,
            Profile::Motorcycle | Profile::MotorcycleElectric => &self.motorcycle,
            Profile::Truck | Profile::TruckElectric => &self.truck,
            Profile::MobileHome => &self.mobile_home,
            Profile::Cycling => &self.cycling,
            Profile::CyclingElectric => &self.cycling_electric,
            Profile::Hiking => &self.hiking,
        }
    }

    pub fn for_profile_mut(&mut self, profile: Profile) -> &mut ProfilePoiRadii {
        match profile {
            Profile::Car | Profile::CarElectric => &mut self.car,
            Profile::Motorcycle | Profile::MotorcycleElectric => &mut self.motorcycle,
            Profile::Truck | Profile::TruckElectric => &mut self.truck,
            Profile::MobileHome => &mut self.mobile_home,
            Profile::Cycling => &mut self.cycling,
            Profile::CyclingElectric => &mut self.cycling_electric,
            Profile::Hiking => &mut self.hiking,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn motor_requires_road_link_by_default() {
        let t = ProfilePoiRadiiTable::default();
        assert!(t.for_profile(Profile::Car).require_road_link);
        assert!(t.for_profile(Profile::Motorcycle).require_road_link);
        assert!(t.for_profile(Profile::Truck).require_road_link);
        assert!(t.for_profile(Profile::MobileHome).require_road_link);
        assert!(!t.for_profile(Profile::Hiking).require_road_link);
        assert!(!t.for_profile(Profile::Cycling).require_road_link);
        assert!(!t.for_profile(Profile::CyclingElectric).require_road_link);
    }

    #[test]
    fn sanitize_clamps_extreme_values() {
        let r = ProfilePoiRadii {
            search_radius_m: 1.0,
            cabin_radius_m: 1_000_000.0,
            network_hut_radius_m: 11_000.0,
            network_hut_preference_radius_m: 11_000.0,
            require_road_link: true,
        }
        .sanitized();
        assert_eq!(r.search_radius_m, 500.0);
        assert_eq!(r.cabin_radius_m, 100_000.0);
    }

    #[test]
    fn hike_cycle_default_matches_slider_floor() {
        let t = ProfilePoiRadiiTable::default();
        assert_eq!(t.for_profile(Profile::Hiking).cabin_radius_m, 10_500.0);
        assert_eq!(t.for_profile(Profile::Cycling).search_radius_m, 10_500.0);
        assert_eq!(
            t.for_profile(Profile::Car).search_radius_m,
            3.0 * 80.0 * 1000.0
        );
    }
}
