use rusqlite::{params, Connection, Result as SqlResult};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::{EcoConfig, RestConfig, SafetyConfig, VehicleLimits};
use crate::storage::Storage;

const REST_CONFIG_KEY: &str = "rest_config";
const SAFETY_CONFIG_KEY: &str = "safety_config";
const ECO_CONFIG_KEY: &str = "eco_config";
const VEHICLE_LIMITS_KEY: &str = "vehicle_limits";

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

    fn load_json<T>(&self, key: &str, default: fn() -> T) -> SqlResult<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        self.storage.with_conn(|conn| load_json_conn(conn, key, default))
    }

    fn save_json<T>(&self, key: &str, value: &T) -> SqlResult<()>
    where
        T: Serialize,
    {
        self.storage.with_conn(|conn| save_json_conn(conn, key, value))
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
    let json = serde_json::to_string(value).map_err(|e| {
        rusqlite::Error::ToSqlConversionFailure(Box::new(e))
    })?;
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
        assert_eq!(loaded.hiking.main_break_distance_km, config.hiking.main_break_distance_km);
    }
}
