use rusqlite::{params, Connection, Result as SqlResult};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::{
    EbikeConfig, EcoConfig, EvCarConfig, FuelConfig, RestConfig, SafetyConfig, TruckDrivingHistory,
    VehicleLimits,
};
use crate::storage::Storage;

const REST_CONFIG_KEY: &str = "rest_config";
const SAFETY_CONFIG_KEY: &str = "safety_config";
const ECO_CONFIG_KEY: &str = "eco_config";
const VEHICLE_LIMITS_KEY: &str = "vehicle_limits";
const FUEL_CONFIG_KEY: &str = "fuel_config";
const EBIKE_CONFIG_KEY: &str = "ebike_config";
const EV_CAR_CONFIG_KEY: &str = "ev_car_config";
const PREFER_OFFICIAL_NETWORKS_KEY: &str = "prefer_official_networks";
const TRUCK_DRIVING_HISTORY_KEY: &str = "truck_driving_history";

pub struct ConfigStore<'a> {
    storage: &'a Storage,
}

impl<'a> ConfigStore<'a> {
    pub fn new(storage: &'a Storage) -> Self {
        Self { storage }
    }

    pub fn load_rest_config(&self) -> SqlResult<RestConfig> {
        self.load_json(REST_CONFIG_KEY, RestConfig::default)
    }

    pub fn save_rest_config(&self, config: &RestConfig) -> SqlResult<()> {
        self.save_json(REST_CONFIG_KEY, config)
    }

    pub fn load_safety_config(&self) -> SqlResult<SafetyConfig> {
        self.load_json(SAFETY_CONFIG_KEY, SafetyConfig::default)
    }

    pub fn save_safety_config(&self, config: &SafetyConfig) -> SqlResult<()> {
        self.save_json(SAFETY_CONFIG_KEY, config)
    }

    pub fn load_eco_config(&self) -> SqlResult<EcoConfig> {
        self.load_json(ECO_CONFIG_KEY, EcoConfig::default)
    }

    pub fn save_eco_config(&self, config: &EcoConfig) -> SqlResult<()> {
        self.save_json(ECO_CONFIG_KEY, config)
    }

    pub fn load_vehicle_limits(&self) -> SqlResult<VehicleLimits> {
        self.load_json(VEHICLE_LIMITS_KEY, VehicleLimits::default)
    }

    pub fn save_vehicle_limits(&self, limits: &VehicleLimits) -> SqlResult<()> {
        self.save_json(VEHICLE_LIMITS_KEY, limits)
    }

    pub fn load_fuel_config(&self) -> SqlResult<FuelConfig> {
        self.load_json(FUEL_CONFIG_KEY, FuelConfig::default)
    }

    pub fn save_fuel_config(&self, config: &FuelConfig) -> SqlResult<()> {
        self.save_json(FUEL_CONFIG_KEY, config)
    }

    pub fn load_ebike_config(&self) -> SqlResult<EbikeConfig> {
        self.load_json(EBIKE_CONFIG_KEY, EbikeConfig::default)
    }

    pub fn save_ebike_config(&self, config: &EbikeConfig) -> SqlResult<()> {
        self.save_json(EBIKE_CONFIG_KEY, config)
    }

    pub fn load_ev_car_config(&self) -> SqlResult<EvCarConfig> {
        self.load_json(EV_CAR_CONFIG_KEY, EvCarConfig::default)
    }

    pub fn save_ev_car_config(&self, config: &EvCarConfig) -> SqlResult<()> {
        self.save_json(EV_CAR_CONFIG_KEY, config)
    }

    /// Soft preference for official hiking/cycling networks (off by default).
    pub fn load_prefer_official_networks(&self) -> SqlResult<bool> {
        self.load_json(PREFER_OFFICIAL_NETWORKS_KEY, || false)
    }

    pub fn save_prefer_official_networks(&self, prefer: bool) -> SqlResult<()> {
        self.save_json(PREFER_OFFICIAL_NETWORKS_KEY, &prefer)
    }

    /// Rolling truck duty history for EC 561 weekly / fortnightly caps.
    pub fn load_truck_driving_history(&self) -> SqlResult<TruckDrivingHistory> {
        self.load_json(TRUCK_DRIVING_HISTORY_KEY, TruckDrivingHistory::default)
    }

    pub fn save_truck_driving_history(&self, history: &TruckDrivingHistory) -> SqlResult<()> {
        self.save_json(TRUCK_DRIVING_HISTORY_KEY, history)
    }

    fn load_json<T>(&self, key: &str, default: fn() -> T) -> SqlResult<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        self.storage
            .with_conn(|conn| load_json_conn(conn, key, default))
    }

    fn save_json<T>(&self, key: &str, value: &T) -> SqlResult<()>
    where
        T: Serialize,
    {
        self.storage
            .with_conn(|conn| save_json_conn(conn, key, value))
    }
}

fn load_json_conn<T>(conn: &Connection, key: &str, default: fn() -> T) -> SqlResult<T>
where
    T: for<'de> Deserialize<'de>,
{
    let mut stmt = conn.prepare("SELECT value_json FROM app_config WHERE key = ?1")?;
    let result = stmt.query_row(params![key], |row| row.get::<_, String>(0));
    match result {
        Ok(json) => serde_json::from_str(&json).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        }),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(default()),
        Err(e) => Err(e),
    }
}

fn save_json_conn<T>(conn: &Connection, key: &str, value: &T) -> SqlResult<()>
where
    T: Serialize,
{
    let json = serde_json::to_string(value)
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
    let now = now_unix();
    conn.execute(
        "INSERT INTO app_config (key, value_json, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at",
        params![key, json, now],
    )?;
    Ok(())
}

fn now_unix() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Storage;

    #[test]
    fn round_trip_rest_config() {
        let storage = Storage::open_in_memory().unwrap();
        let store = ConfigStore::new(&storage);
        let config = RestConfig::default();
        store.save_rest_config(&config).unwrap();
        let loaded = store.load_rest_config().unwrap();
        assert_eq!(
            loaded.hiking.main_break_distance_km,
            config.hiking.main_break_distance_km
        );
    }

    #[test]
    fn round_trip_ebike_config() {
        let storage = Storage::open_in_memory().unwrap();
        let store = ConfigStore::new(&storage);
        let config = EbikeConfig {
            battery_capacity_wh: Some(600.0),
            motor_torque_nm: Some(75.0),
            wheel_diameter_in: Some(29.0),
        };
        store.save_ebike_config(&config).unwrap();
        let loaded = store.load_ebike_config().unwrap();
        assert_eq!(loaded.battery_capacity_wh, Some(600.0));
        assert_eq!(loaded.motor_torque_nm, Some(75.0));
        assert_eq!(loaded.wheel_diameter_in, Some(29.0));
    }

    #[test]
    fn round_trip_ev_car_config() {
        let storage = Storage::open_in_memory().unwrap();
        let store = ConfigStore::new(&storage);
        let config = EvCarConfig {
            battery_capacity_kwh: Some(75.0),
        };
        store.save_ev_car_config(&config).unwrap();
        let loaded = store.load_ev_car_config().unwrap();
        assert_eq!(loaded.battery_capacity_kwh, Some(75.0));
    }
}
