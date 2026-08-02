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

/// Soft suggested maximum driving hours per day for car / motorcycle / mobilehome
/// multi-day overnight splitting (wellbeing guidance, not legal).
pub const CAR_MAX_DAILY_HOURS: f64 = 8.0;

/// Soft warning threshold (hours) before the daily max for car-style profiles.
pub const CAR_SOFT_LIMIT_HOURS: f64 = 7.0;

/// Truck mandatory break after driving (hours), EU Regulation EC 561/2006.
pub const TRUCK_MANDATORY_BREAK_AFTER_HOURS: f64 = 4.5;

/// Truck mandatory break duration (minutes), EU Regulation EC 561/2006.
pub const TRUCK_BREAK_DURATION_MINUTES: u32 = 45;

/// Split-break first part (minutes), EC 561/2006 alternative to continuous 45.
pub const TRUCK_SPLIT_BREAK_FIRST_MINUTES: u32 = 15;

/// Split-break second part (minutes), EC 561/2006 (must follow the 15).
pub const TRUCK_SPLIT_BREAK_SECOND_MINUTES: u32 = 30;

/// Truck maximum daily driving hours, EU Regulation EC 561/2006.
pub const TRUCK_MAX_DAILY_DRIVING_HOURS: f64 = 9.0;

/// Extended daily driving hours (allowed at most twice per week).
pub const TRUCK_MAX_DAILY_DRIVING_EXTENDED_HOURS: f64 = 10.0;

/// How many times per week the 10 h daily extension may be used.
pub const TRUCK_MAX_DAILY_EXTENSIONS_PER_WEEK: u32 = 2;

/// Truck weekly driving limit (hours), EU Regulation EC 561/2006.
pub const TRUCK_MAX_WEEKLY_DRIVING_HOURS: f64 = 56.0;

/// Fortnightly (any two consecutive weeks) driving limit (hours).
pub const TRUCK_MAX_FORTNIGHTLY_DRIVING_HOURS: f64 = 90.0;

/// Regular daily rest (hours).
pub const TRUCK_DAILY_REST_HOURS: f64 = 11.0;

/// Reduced daily rest (hours), at most three times between weekly rests.
pub const TRUCK_DAILY_REST_REDUCED_HOURS: f64 = 9.0;

/// Max reduced daily rests between weekly rests.
pub const TRUCK_MAX_REDUCED_DAILY_RESTS: u32 = 3;

/// Split daily rest — first uninterrupted block (hours).
pub const TRUCK_SPLIT_DAILY_REST_FIRST_HOURS: f64 = 3.0;

/// Split daily rest — second uninterrupted block (hours); total 12 h.
pub const TRUCK_SPLIT_DAILY_REST_SECOND_HOURS: f64 = 9.0;

/// Regular weekly rest (hours, continuous).
pub const TRUCK_WEEKLY_REST_HOURS: f64 = 45.0;

/// Reduced weekly rest (hours); compensation owed; every second week.
pub const TRUCK_WEEKLY_REST_REDUCED_HOURS: f64 = 24.0;

/// Max consecutive working days before a weekly rest is due.
pub const TRUCK_MAX_CONSECUTIVE_WORKING_DAYS: u32 = 6;

/// Exceptional extension to reach a suitable stop (hours); user opt-in only.
pub const TRUCK_EXCEPTIONAL_EXTENSION_HOURS: f64 = 1.0;

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

/// Corridor pre-filter margin (metres) when loading overnight building samples.
///
/// Buildings farther than this from the planned route path are dropped before
/// the exact [`SAFETY_MIN_BUILDING_DISTANCE_M`] check. Chosen in the 1–2 km
/// band so the real 150 m threshold cannot false-exclude, while still cutting
/// the rectangular-bbox candidate set on long corridors.
pub const OVERNIGHT_BUILDING_CORRIDOR_MARGIN_M: f64 = 1_500.0;

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

/// Illustrative mid-size motorcycle Cd (naked/unfaired bikes often higher Cd
/// than a car; absolute drag still lower due to frontal area). Starting default.
pub const MOTORCYCLE_DRAG_COEFFICIENT: f64 = 0.65;

/// Illustrative motorcycle + rider frontal area (m²); cars are ~2.2 m².
pub const MOTORCYCLE_FRONTAL_AREA_M2: f64 = 0.60;

/// Illustrative motorcycle + rider mass (kg); mid-size bike class ~150–250 kg.
pub const MOTORCYCLE_MASS_KG: f64 = 220.0;

/// Default regenerative-braking efficiency on descent (0 = no recovery; diesel/ICE default).
pub const DEFAULT_REGEN_EFFICIENCY: f64 = 0.0;

/// Default regen efficiency for battery-electric drivetrains (partial PE recovery on descent).
pub const DEFAULT_EV_REGEN_EFFICIENCY: f64 = 0.4;

/// Default e-bike battery capacity (Wh) — mid/high-end pack (typical market 400–800 Wh).
pub const DEFAULT_EBIKE_BATTERY_WH: f64 = 800.0;

/// Default mid-drive motor torque (Nm) — Bosch/Brose/Bafang class (often 65–90 Nm).
pub const DEFAULT_EBIKE_TORQUE_NM: f64 = 85.0;

/// Default e-bike wheel diameter (inches) — 27.5" / 650B common on e-MTB and hybrids.
pub const DEFAULT_EBIKE_WHEEL_DIAMETER_IN: f64 = 27.5;

/// Assumed motor + controller efficiency for battery draw estimates (not measured).
pub const DEFAULT_EBIKE_MOTOR_EFFICIENCY: f64 = 0.80;

/// Default mid-size EV usable pack (kWh) — example default, not a measured vehicle.
pub const DEFAULT_EV_CAR_BATTERY_KWH: f64 = 60.0;

/// Assumed EV car drivetrain efficiency for pack-draw estimates (not measured).
pub const DEFAULT_EV_CAR_MOTOR_EFFICIENCY: f64 = 0.85;

/// Max distance (m) from a hiking waypoint to the nearest linked foot-graph node.
///
/// Normal search-result snaps are typically tens of metres (corridor tests often
/// log ~5–45 m). The DNT hiking integration already treats >500 m as too far.
/// Bound must stay well below silent ocean/open-terrain substitutions.
pub const HIKING_MAX_WAYPOINT_SNAP_M: f64 = 500.0;

/// Max distance (m) from a cycling waypoint to the nearest linked bike-graph node.
pub const CYCLING_MAX_WAYPOINT_SNAP_M: f64 = 500.0;

/// Max distance (m) from a car/motorcycle waypoint to the nearest linked road node.
/// Slightly looser than foot: parking / driveway offset from the carriageway.
pub const CAR_MAX_WAYPOINT_SNAP_M: f64 = 750.0;

/// Max distance (m) from a truck/motorhome waypoint to the nearest linked road node.
/// Truck stops and industrial access can sit farther from the highway graph.
pub const TRUCK_MAX_WAYPOINT_SNAP_M: f64 = 1_000.0;
