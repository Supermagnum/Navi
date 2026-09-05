//! SQLite cache for weather samples and throttle metadata.

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::weather::throttle::ThrottleState;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WeatherSample {
    pub lat: f64,
    pub lon: f64,
    pub icon_slug: String,
    pub temp_c: Option<f64>,
    pub wind_ms: Option<f64>,
    pub precip_mm: Option<f64>,
    pub pressure_hpa: Option<f64>,
    pub provider: String,
    pub fetched_at_unix: i64,
    pub observation_unix: Option<i64>,
    pub stale: bool,
    pub summary: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WeatherMeta {
    pub throttle: ThrottleState,
    pub if_modified_since: Option<String>,
    pub expires_unix: Option<i64>,
    pub last_provider: Option<String>,
}

pub struct WeatherCache {
    conn: Connection,
}

impl WeatherCache {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path).context("open weather sqlite")?;
        let cache = Self { conn };
        cache.migrate()?;
        Ok(cache)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let cache = Self { conn };
        cache.migrate()?;
        Ok(cache)
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS weather_samples (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                lat REAL NOT NULL,
                lon REAL NOT NULL,
                icon_slug TEXT NOT NULL,
                temp_c REAL,
                wind_ms REAL,
                precip_mm REAL,
                pressure_hpa REAL,
                provider TEXT NOT NULL,
                fetched_at_unix INTEGER NOT NULL,
                observation_unix INTEGER,
                summary TEXT NOT NULL DEFAULT '',
                payload_json TEXT NOT NULL DEFAULT '{}'
            );
            CREATE TABLE IF NOT EXISTS weather_meta (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                json TEXT NOT NULL
            );
            ",
        )?;
        Ok(())
    }

    pub fn upsert_sample(&self, sample: &WeatherSample) -> Result<()> {
        let payload = serde_json::to_string(sample)?;
        self.conn.execute(
            "INSERT INTO weather_samples (
                id, lat, lon, icon_slug, temp_c, wind_ms, precip_mm, pressure_hpa,
                provider, fetched_at_unix, observation_unix, summary, payload_json
             ) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(id) DO UPDATE SET
                lat=excluded.lat, lon=excluded.lon, icon_slug=excluded.icon_slug,
                temp_c=excluded.temp_c, wind_ms=excluded.wind_ms, precip_mm=excluded.precip_mm,
                pressure_hpa=excluded.pressure_hpa, provider=excluded.provider,
                fetched_at_unix=excluded.fetched_at_unix, observation_unix=excluded.observation_unix,
                summary=excluded.summary, payload_json=excluded.payload_json",
            params![
                sample.lat,
                sample.lon,
                sample.icon_slug,
                sample.temp_c,
                sample.wind_ms,
                sample.precip_mm,
                sample.pressure_hpa,
                sample.provider,
                sample.fetched_at_unix,
                sample.observation_unix,
                sample.summary,
                payload,
            ],
        )?;
        Ok(())
    }

    pub fn latest(&self) -> Option<WeatherSample> {
        self.conn
            .query_row(
                "SELECT payload_json FROM weather_samples WHERE id = 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .ok()
            .and_then(|j| serde_json::from_str(&j).ok())
    }

    /// Return the cached sample when within `radius_m` of the query point.
    pub fn nearest(&self, lat: f64, lon: f64, radius_m: f64) -> Option<WeatherSample> {
        let mut sample = self.latest()?;
        let dist = haversine_m(lat, lon, sample.lat, sample.lon);
        if dist > radius_m {
            return None;
        }
        // Mark stale when older than active interval.
        let age = {
            use std::time::{SystemTime, UNIX_EPOCH};
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(sample.fetched_at_unix);
            now.saturating_sub(sample.fetched_at_unix)
        };
        if age > crate::weather::DEFAULT_ACTIVE_INTERVAL_SECS {
            sample.stale = true;
        }
        Some(sample)
    }

    pub fn meta(&self) -> Result<WeatherMeta> {
        match self
            .conn
            .query_row("SELECT json FROM weather_meta WHERE id = 1", [], |row| {
                row.get::<_, String>(0)
            }) {
            Ok(j) => Ok(serde_json::from_str(&j).unwrap_or_default()),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(WeatherMeta::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save_meta(&self, meta: &WeatherMeta) -> Result<()> {
        let j = serde_json::to_string(meta)?;
        self.conn.execute(
            "INSERT INTO weather_meta (id, json) VALUES (1, ?1)
             ON CONFLICT(id) DO UPDATE SET json = excluded.json",
            params![j],
        )?;
        Ok(())
    }
}

fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const R: f64 = 6_371_000.0;
    let to_rad = |d: f64| d.to_radians();
    let dlat = to_rad(lat2 - lat1);
    let dlon = to_rad(lon2 - lon1);
    let a = (dlat / 2.0).sin().powi(2)
        + to_rad(lat1).cos() * to_rad(lat2).cos() * (dlon / 2.0).sin().powi(2);
    2.0 * R * a.sqrt().asin()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_sample() {
        let cache = WeatherCache::open_in_memory().unwrap();
        let s = WeatherSample {
            lat: 59.91,
            lon: 10.75,
            icon_slug: "clear-day".into(),
            temp_c: Some(12.0),
            wind_ms: Some(3.0),
            precip_mm: Some(0.0),
            pressure_hpa: Some(1013.0),
            provider: "met_norway".into(),
            fetched_at_unix: 1_700_000_000,
            observation_unix: Some(1_700_000_000),
            stale: false,
            summary: "clear".into(),
        };
        cache.upsert_sample(&s).unwrap();
        let got = cache.nearest(59.91, 10.75, 1000.0).unwrap();
        assert_eq!(got.icon_slug, "clear-day");
        assert!(cache.nearest(0.0, 0.0, 1000.0).is_none());
    }
}
