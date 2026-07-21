//! Moving-icon / APRS-style station tracking.
//!
//! Upsert by station id updates coordinates in place (no duplicate markers).
//! Timeout is hard-capped at [`STATION_TIMEOUT_MAX_S`]. Display range is clamped
//! to [`DISPLAY_RANGE_MIN_KM`]..=[`DISPLAY_RANGE_MAX_KM`].

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Hard maximum station display timeout (seconds). Values above this are rejected/clamped.
pub const STATION_TIMEOUT_MAX_S: u64 = 3600;
/// Minimum allowed display-range setting (km).
pub const DISPLAY_RANGE_MIN_KM: f64 = 50.0;
/// Maximum allowed display-range setting (km). Unlimited / global is forbidden.
pub const DISPLAY_RANGE_MAX_KM: f64 = 150.0;

const EARTH_RADIUS_KM: f64 = 6371.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackStation {
    /// Stable identity (callsign-SSID or test id). Key for in-place updates.
    pub id: String,
    pub lat: f64,
    pub lon: f64,
    /// APRS symbol table `/` or `\`.
    pub symbol_table: String,
    /// APRS symbol code (one character as string for FFI friendliness).
    pub symbol_code: String,
    /// Host icon asset key (e.g. `aprs_car`) resolved for MapLibre.
    pub symbol_key: String,
    pub last_heard_unix: u64,
    pub comment: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpsertOutcome {
    Created,
    Updated,
}

#[derive(Debug, Clone)]
pub struct TrackStore {
    stations: HashMap<String, TrackStation>,
    timeout_s: u64,
    range_km: f64,
}

impl Default for TrackStore {
    fn default() -> Self {
        Self::new(STATION_TIMEOUT_MAX_S, DISPLAY_RANGE_MAX_KM)
    }
}

impl TrackStore {
    pub fn new(timeout_s: u64, range_km: f64) -> Self {
        Self {
            stations: HashMap::new(),
            timeout_s: clamp_timeout(timeout_s),
            range_km: clamp_range(range_km),
        }
    }

    pub fn timeout_s(&self) -> u64 {
        self.timeout_s
    }

    pub fn range_km(&self) -> f64 {
        self.range_km
    }

    pub fn set_timeout_s(&mut self, timeout_s: u64) {
        self.timeout_s = clamp_timeout(timeout_s);
    }

    pub fn set_range_km(&mut self, range_km: f64) {
        self.range_km = clamp_range(range_km);
    }

    pub fn len(&self) -> usize {
        self.stations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.stations.is_empty()
    }

    /// Insert or update a station by id. Never creates a second entry for the same id.
    pub fn upsert(&mut self, station: TrackStation) -> UpsertOutcome {
        let id = station.id.clone();
        if self.stations.contains_key(&id) {
            self.stations.insert(id, station);
            UpsertOutcome::Updated
        } else {
            self.stations.insert(id, station);
            UpsertOutcome::Created
        }
    }

    pub fn get(&self, id: &str) -> Option<&TrackStation> {
        self.stations.get(id)
    }

    /// Remove stations not heard within `timeout_s` of `now_unix`.
    pub fn expire(&mut self, now_unix: u64) -> Vec<String> {
        let timeout = self.timeout_s;
        let mut removed = Vec::new();
        self.stations.retain(|id, st| {
            let age = now_unix.saturating_sub(st.last_heard_unix);
            if age > timeout {
                removed.push(id.clone());
                false
            } else {
                true
            }
        });
        removed
    }

    /// Stations within `range_km` of the centre (Haversine).
    pub fn visible(&self, center_lat: f64, center_lon: f64) -> Vec<&TrackStation> {
        let max_km = self.range_km;
        self.stations
            .values()
            .filter(|st| haversine_km(center_lat, center_lon, st.lat, st.lon) <= max_km)
            .collect()
    }

    pub fn all(&self) -> Vec<&TrackStation> {
        self.stations.values().collect()
    }
}

pub fn clamp_timeout(timeout_s: u64) -> u64 {
    timeout_s.clamp(1, STATION_TIMEOUT_MAX_S)
}

pub fn clamp_range(range_km: f64) -> f64 {
    range_km.clamp(DISPLAY_RANGE_MIN_KM, DISPLAY_RANGE_MAX_KM)
}

pub fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_KM * a.sqrt().asin()
}

/// Offset a WGS84 point by east/north metres (local flat approximation).
pub fn offset_lat_lon(lat: f64, lon: f64, east_m: f64, north_m: f64) -> (f64, f64) {
    let dlat = north_m / 111_320.0;
    let dlon = east_m / (111_320.0 * lat.to_radians().cos());
    (lat + dlat, lon + dlon)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(id: &str, lat: f64, lon: f64, key: &str, heard: u64) -> TrackStation {
        TrackStation {
            id: id.into(),
            lat,
            lon,
            symbol_table: "/".into(),
            symbol_code: ">".into(),
            symbol_key: key.into(),
            last_heard_unix: heard,
            comment: String::new(),
        }
    }

    #[test]
    fn upsert_updates_in_place_no_duplicate() {
        let mut store = TrackStore::default();
        assert_eq!(
            store.upsert(st("LA1ABC-9", 60.0, 10.0, "aprs_car", 100)),
            UpsertOutcome::Created
        );
        assert_eq!(store.len(), 1);
        assert_eq!(
            store.upsert(st("LA1ABC-9", 60.1, 10.1, "aprs_car", 200)),
            UpsertOutcome::Updated
        );
        assert_eq!(store.len(), 1);
        let s = store.get("LA1ABC-9").unwrap();
        assert!((s.lat - 60.1).abs() < 1e-9);
        assert!((s.lon - 10.1).abs() < 1e-9);
        assert_eq!(s.symbol_key, "aprs_car");
        assert_eq!(s.last_heard_unix, 200);
    }

    #[test]
    fn symbol_preserved_across_position_update() {
        let mut store = TrackStore::default();
        store.upsert(st("HIKER-7", 60.0, 10.0, "aprs_human", 1));
        let mut next = st("HIKER-7", 60.002, 10.002, "aprs_human", 2);
        next.comment = "moved".into();
        store.upsert(next);
        assert_eq!(store.get("HIKER-7").unwrap().symbol_key, "aprs_human");
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn timeout_clamped_to_3600() {
        let store = TrackStore::new(99999, 100.0);
        assert_eq!(store.timeout_s(), STATION_TIMEOUT_MAX_S);
        assert_eq!(clamp_timeout(0), 1);
    }

    #[test]
    fn expire_removes_stale_station() {
        let mut store = TrackStore::new(10, 150.0);
        store.upsert(st("OLD", 60.72, 10.61, "aprs_house", 100));
        store.upsert(st("NEW", 60.72, 10.61, "aprs_car", 200));
        let removed = store.expire(211); // OLD age=111 > 10; NEW age=11 > 10 both?
        // NEW last_heard 200, now 211 → age 11 > 10 → both removed
        assert!(removed.contains(&"OLD".to_string()));
        let mut store = TrackStore::new(10, 150.0);
        store.upsert(st("OLD", 60.72, 10.61, "aprs_house", 100));
        store.upsert(st("NEW", 60.72, 10.61, "aprs_car", 205));
        let removed = store.expire(211); // OLD age 111, NEW age 6
        assert_eq!(removed, vec!["OLD".to_string()]);
        assert!(store.get("NEW").is_some());
        assert!(store.get("OLD").is_none());
    }

    #[test]
    fn range_clamped_and_nearby_visible() {
        assert_eq!(clamp_range(10.0), DISPLAY_RANGE_MIN_KM);
        assert_eq!(clamp_range(200.0), DISPLAY_RANGE_MAX_KM);
        let mut store = TrackStore::new(3600, 150.0);
        let center = (60.722823, 10.613182);
        let (lat, lon) = offset_lat_lon(center.0, center.1, 100.0, 0.0);
        store.upsert(st("NEAR", lat, lon, "aprs_digi", 1));
        let far = offset_lat_lon(center.0, center.1, 200_000.0, 0.0); // ~200 km
        store.upsert(st("FAR", far.0, far.1, "aprs_car", 1));
        let vis = store.visible(center.0, center.1);
        assert_eq!(vis.len(), 1);
        assert_eq!(vis[0].id, "NEAR");
        // 300 m test radius is well inside 50–150 km window.
        assert!(haversine_km(center.0, center.1, lat, lon) < 0.3);
        assert!(haversine_km(center.0, center.1, lat, lon) < DISPLAY_RANGE_MIN_KM);
    }
}
