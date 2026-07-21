//! Documented default constants for rest intervals, POI radii, and safety distances.
//!
//! Values marked configurable are persisted via [`crate::storage`] and exposed to the host UI.

/// Hiking main rest interval (km), from Scandinavian rast tradition (~11.295 km).
pub const HIKING_MAIN_BREAK_DISTANCE_KM: f64 = 11.295;

/// Hiking alternative rest interval (km), quarter-mile fjerding scale (~2.275 km).
pub const HIKING_ALTERNATIVE_BREAK_DISTANCE_KM: f64 = 2.275;

/// Hiking suggested maximum daily distance (km).
pub const HIKING_MAX_DAILY_DISTANCE_KM: f64 = 40.0;

/// Cycling main rest interval (km), rast/vei scaled for cycling (~28.24 km).
pub const CYCLING_MAIN_BREAK_DISTANCE_KM: f64 = 28.24;

/// Cycling alternative rest interval (km) (~5.69 km).
pub const CYCLING_ALTERNATIVE_BREAK_DISTANCE_KM: f64 = 5.69;

/// Cycling suggested maximum daily distance (km).
pub const CYCLING_MAX_DAILY_DISTANCE_KM: f64 = 100.0;

/// Car break interval lower bound (hours).
pub const CAR_BREAK_INTERVAL_MIN_HOURS: f64 = 4.0;

/// Car break interval upper bound (hours).
pub const CAR_BREAK_INTERVAL_MAX_HOURS: f64 = 4.5;

/// Car break duration lower bound (minutes).
pub const CAR_BREAK_DURATION_MIN_MINUTES: u32 = 15;

/// Car break duration upper bound (minutes).
pub const CAR_BREAK_DURATION_MAX_MINUTES: u32 = 45;

/// Truck mandatory break after driving (hours), EU Regulation EC 561/2006.
pub const TRUCK_MANDATORY_BREAK_AFTER_HOURS: f64 = 4.5;

/// Truck mandatory break duration (minutes), EU Regulation EC 561/2006.
pub const TRUCK_BREAK_DURATION_MINUTES: u32 = 45;

/// Truck maximum daily driving hours, EU Regulation EC 561/2006.
pub const TRUCK_MAX_DAILY_DRIVING_HOURS: f64 = 9.0;

/// Truck weekly driving limit (hours), EU Regulation EC 561/2006.
pub const TRUCK_MAX_WEEKLY_DRIVING_HOURS: f64 = 56.0;

/// Default search radius for drinking water POIs (metres).
pub const POI_RADIUS_WATER_M: f64 = 2_000.0;

/// Default search radius for cabins/huts (metres).
pub const POI_RADIUS_CABIN_M: f64 = 5_000.0;

/// Default search radius for general car/truck amenities (metres).
pub const POI_RADIUS_GENERAL_M: f64 = 15_000.0;

/// Default search radius for network huts (DNT/STF/DAV/SAC/OeAV/Metsahallitus) (metres).
pub const POI_RADIUS_NETWORK_HUT_M: f64 = 25_000.0;

/// Default network-hut preference search radius (metres), ~10-12 km typical spacing.
pub const POI_NETWORK_HUT_PREFERENCE_RADIUS_M: f64 = 11_000.0;

/// Minimum overnight distance from buildings (metres), allemannsretten compliance.
pub const SAFETY_MIN_BUILDING_DISTANCE_M: f64 = 150.0;

/// Minimum overnight distance from glaciers (metres) unless at established facility.
pub const SAFETY_MIN_GLACIER_DISTANCE_M: f64 = 1_000.0;

/// HGT void / missing elevation sentinel.
pub const ELEVATION_VOID: i16 = -32_768;

/// Sea-level air density for drag calculations (kg/m^3).
pub const AIR_DENSITY_KG_M3: f64 = 1.225;

/// Standard gravity (m/s^2).
pub const GRAVITY_M_S2: f64 = 9.80665;

/// Default rolling resistance coefficient for eco routing.
pub const DEFAULT_ROLLING_RESISTANCE: f64 = 0.015;

/// Default drag coefficient Cd for eco routing.
pub const DEFAULT_DRAG_COEFFICIENT: f64 = 0.32;

/// Default frontal area (m^2) for eco routing.
pub const DEFAULT_FRONTAL_AREA_M2: f64 = 2.2;

/// Default total mass (kg) for eco routing.
pub const DEFAULT_VEHICLE_MASS_KG: f64 = 1_500.0;

/// Default cruise speed (m/s) used when estimating drag force along an edge.
pub const DEFAULT_CRUISE_SPEED_M_S: f64 = 25.0;
