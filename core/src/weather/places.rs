//! Per-settlement weather samples for low-zoom map symbols.
//!
//! Deliberately separate from [`super::cache::WeatherCache`]'s single-row HUD
//! table (`weather_samples` CHECK id = 1). Extending that table would risk
//! regressing HUD upsert/nearest/meta semantics; a new table keeps the chip
//! path unmodified.
//!
//! v1 settlement filter is `place:city` only (see `MAP_WEATHER_PLACE_KIND`).
//!
//! # Throttle path (v1)
//! City-only Ostlandet = 10 places. With the existing ~45 min jittered
//! per-place interval and ~16 h active use: 10 × (16×60/45) ≈ **213 req/day**,
//! well under Open-Meteo's 10k/day. Burst is paced to **at most one HTTPS
//! fetch per map UI tick** (~15 s), so MET's ~20 req/s is never approached.
//! Viewport-batching is deferred until town/village tiers are added.
//!
//! # Selection order (spacing + cap)
//! Candidates are sorted by **distance to viewport center** (nearest first),
//! then name, then osm_id. A single greedy pass then:
//! 1. skips a candidate if it lands within [`MAP_WEATHER_MIN_PIXEL_SPACING`]
//!    of an already-kept symbol, and
//! 2. stops once [`MAP_WEATHER_MAX_SYMBOLS`] have been kept.
//!
//! Cap therefore applies **after** spacing rejection within the same pass
//! (never "take first N then space" — that would waste slots on clusters).
//! The place index has no population/rank column; alphabetical SQL order alone
//! is **not** the product priority (that was the pre-fix accidental order).

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::weather::cache::WeatherSample;
use crate::weather::providers::{fetch_weather, ProviderId};
use crate::weather::throttle::{
    jittered_interval_secs, ThrottleConfig, DEFAULT_ACTIVE_INTERVAL_SECS,
};

/// OSM kind string for v1 map symbols — cities only; town/village are follow-ups.
pub const MAP_WEATHER_PLACE_KIND: &str = "place:city";

/// Show map weather symbols only when MapLibre zoom is at or below this.
///
/// Ostlandet `place:city` (10) at a typical SM-P613 ~2000×1200 viewport centered
/// on Oslo / mid-Ostlandet (Web Mercator pixel math, confirmed against place_index):
/// - z5: 10 visible, min pairwise ~6.5 px → declutter keeps ~4
/// - z6: 10 visible, min ~13 px → keeps ~5
/// - z7: 10 visible, min ~26 px → keeps ~8
/// - z8: ~3–9 visible depending on center, min ~52 px near Oslo → keeps up to 8
/// - z9: 1–2 visible (HUD chip territory) — hide map symbols above z8
pub const MAP_WEATHER_ZOOM_MAX: f64 = 8.0;

/// Skip a symbol if it would land within this many screen pixels of one already kept.
/// Slightly above z8 Oslo min-pair (~52 px) so dense clusters still drop extras.
pub const MAP_WEATHER_MIN_PIXEL_SPACING: f64 = 56.0;

/// Hard cap on symbols drawn in one viewport (city-only Ostlandet has 10).
pub const MAP_WEATHER_MAX_SYMBOLS: usize = 10;

/// Product default: map weather symbols OFF even when the main weather plugin is ON.
pub const MAP_WEATHER_SYMBOLS_DEFAULT_ENABLED: bool = false;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WeatherPlaceSample {
    pub place_osm_id: i64,
    pub name: String,
    pub kind: String,
    pub sample: WeatherSample,
    /// Earliest unix time another network fetch is allowed for this place.
    pub next_fetch_unix: i64,
}

pub struct WeatherPlaceCache {
    conn: Connection,
}

impl WeatherPlaceCache {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path).context("open weather sqlite for places")?;
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
        // Same DB file as HUD cache; create place table without touching weather_samples.
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS weather_place_samples (
                place_osm_id INTEGER PRIMARY KEY NOT NULL,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
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
                next_fetch_unix INTEGER NOT NULL,
                payload_json TEXT NOT NULL DEFAULT '{}'
            );
            CREATE INDEX IF NOT EXISTS idx_weather_place_latlon
                ON weather_place_samples(lat, lon);
            ",
        )?;
        Ok(())
    }

    pub fn get(&self, place_osm_id: i64) -> Option<WeatherPlaceSample> {
        self.conn
            .query_row(
                "SELECT payload_json FROM weather_place_samples WHERE place_osm_id = ?1",
                params![place_osm_id],
                |row| row.get::<_, String>(0),
            )
            .ok()
            .and_then(|j| serde_json::from_str(&j).ok())
    }

    pub fn upsert(&self, place: &WeatherPlaceSample) -> Result<()> {
        let payload = serde_json::to_string(place)?;
        let s = &place.sample;
        self.conn.execute(
            "INSERT INTO weather_place_samples (
                place_osm_id, name, kind, lat, lon, icon_slug, temp_c, wind_ms, precip_mm,
                pressure_hpa, provider, fetched_at_unix, observation_unix, summary,
                next_fetch_unix, payload_json
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)
             ON CONFLICT(place_osm_id) DO UPDATE SET
                name=excluded.name, kind=excluded.kind, lat=excluded.lat, lon=excluded.lon,
                icon_slug=excluded.icon_slug, temp_c=excluded.temp_c, wind_ms=excluded.wind_ms,
                precip_mm=excluded.precip_mm, pressure_hpa=excluded.pressure_hpa,
                provider=excluded.provider, fetched_at_unix=excluded.fetched_at_unix,
                observation_unix=excluded.observation_unix, summary=excluded.summary,
                next_fetch_unix=excluded.next_fetch_unix, payload_json=excluded.payload_json",
            params![
                place.place_osm_id,
                place.name,
                place.kind,
                s.lat,
                s.lon,
                s.icon_slug,
                s.temp_c,
                s.wind_ms,
                s.precip_mm,
                s.pressure_hpa,
                s.provider,
                s.fetched_at_unix,
                s.observation_unix,
                s.summary,
                place.next_fetch_unix,
                payload,
            ],
        )?;
        Ok(())
    }

    pub fn list_in_bbox(
        &self,
        min_lat: f64,
        min_lon: f64,
        max_lat: f64,
        max_lon: f64,
    ) -> Result<Vec<WeatherPlaceSample>> {
        let mut stmt = self.conn.prepare(
            "SELECT payload_json FROM weather_place_samples
             WHERE lat BETWEEN ?1 AND ?2 AND lon BETWEEN ?3 AND ?4",
        )?;
        let rows = stmt.query_map(params![min_lat, max_lat, min_lon, max_lon], |row| {
            row.get::<_, String>(0)
        })?;
        let mut out = Vec::new();
        for j in rows.flatten() {
            if let Ok(p) = serde_json::from_str(&j) {
                out.push(p);
            }
        }
        Ok(out)
    }
}

/// Whether a network fetch is due for this place under the per-place active interval.
pub fn place_fetch_due(existing: Option<&WeatherPlaceSample>, now_unix: i64) -> bool {
    match existing {
        None => true,
        Some(p) => now_unix >= p.next_fetch_unix,
    }
}

/// Mark sample stale when older than the active refresh cadence.
pub fn apply_stale_flag(sample: &mut WeatherSample, now_unix: i64) {
    let age = now_unix.saturating_sub(sample.fetched_at_unix);
    sample.stale = age > DEFAULT_ACTIVE_INTERVAL_SECS;
}

/// Fetch one city if due; otherwise return cached (possibly stale).
///
/// When `allow_network` is false, never opens a socket (used to pace at most one
/// fetch per map UI tick while still painting cached symbols).
pub fn refresh_place_if_allowed(
    cache: &WeatherPlaceCache,
    place_osm_id: i64,
    name: &str,
    kind: &str,
    lat: f64,
    lon: f64,
    weather_plugin_enabled: bool,
    map_symbols_enabled: bool,
    app_active: bool,
    allow_network: bool,
    now_unix: i64,
) -> PlaceRefreshResult {
    if !weather_plugin_enabled || !map_symbols_enabled {
        return PlaceRefreshResult {
            fetched: false,
            reason: "map_weather_disabled".into(),
            place: None,
        };
    }

    let existing = cache.get(place_osm_id);
    let serve_cache = |reason: &str, mut place: Option<WeatherPlaceSample>| {
        if let Some(ref mut p) = place {
            apply_stale_flag(&mut p.sample, now_unix);
        }
        PlaceRefreshResult {
            fetched: false,
            reason: reason.into(),
            place,
        }
    };

    if !app_active {
        return serve_cache("app_backgrounded", existing);
    }
    if !allow_network {
        return serve_cache(
            if place_fetch_due(existing.as_ref(), now_unix) {
                "paced_wait"
            } else {
                "place_throttled"
            },
            existing,
        );
    }
    if !place_fetch_due(existing.as_ref(), now_unix) {
        return serve_cache("place_throttled", existing);
    }

    match fetch_weather(lat, lon, None) {
        Ok(outcome) => {
            let mut sample = outcome.sample;
            sample.fetched_at_unix = now_unix;
            sample.stale = false;
            let next = now_unix + jittered_interval_secs(ThrottleConfig::default());
            let place = WeatherPlaceSample {
                place_osm_id,
                name: name.to_string(),
                kind: kind.to_string(),
                sample,
                next_fetch_unix: next,
            };
            let _ = cache.upsert(&place);
            log::info!(
                "weather_map: provider={} place={} id={} slug={}",
                outcome.provider.as_diag_str(),
                name,
                place_osm_id,
                place.sample.icon_slug
            );
            PlaceRefreshResult {
                fetched: true,
                reason: "fetched".into(),
                place: Some(place),
            }
        }
        Err(e) => {
            log::warn!("weather_map: fetch failed for {name}: {e}");
            serve_cache(&format!("fetch_error:{e}"), existing)
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlaceRefreshResult {
    pub fetched: bool,
    pub reason: String,
    pub place: Option<WeatherPlaceSample>,
}

/// Declutter: keep up to `max` places with min screen-pixel spacing at `zoom`.
///
/// **Input must already be priority-ordered** (nearest-to-center first). This
/// function only applies the greedy spacing filter and hard cap — it does not
/// re-rank. Prefer [`select_map_weather_places`] at call sites.
pub fn declutter_places_by_pixel_spacing(
    places: &[(i64, String, f64, f64)],
    zoom: f64,
    min_pixel_spacing: f64,
    max: usize,
) -> Vec<(i64, String, f64, f64)> {
    let mut kept: Vec<(i64, String, f64, f64)> = Vec::new();
    for p in places {
        if kept.len() >= max {
            break;
        }
        let ok = kept
            .iter()
            .all(|k| pixel_distance(p.2, p.3, k.2, k.3, zoom) >= min_pixel_spacing);
        if ok {
            kept.push(p.clone());
        }
    }
    kept
}

/// Rank by distance to viewport center, then apply spacing + cap greedily.
///
/// See module docs for the intended priority. `center_*` is typically the
/// midpoint of the visible lat/lon bbox.
pub fn select_map_weather_places(
    places: &[(i64, String, f64, f64)],
    center_lat: f64,
    center_lon: f64,
    zoom: f64,
    min_pixel_spacing: f64,
    max: usize,
) -> Vec<(i64, String, f64, f64)> {
    let mut ranked = places.to_vec();
    ranked.sort_by(|a, b| {
        let da = crate::tracks::haversine_km(center_lat, center_lon, a.2, a.3);
        let db = crate::tracks::haversine_km(center_lat, center_lon, b.2, b.3);
        da.total_cmp(&db)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.0.cmp(&b.0))
    });
    declutter_places_by_pixel_spacing(&ranked, zoom, min_pixel_spacing, max)
}

fn pixel_distance(lat1: f64, lon1: f64, lat2: f64, lon2: f64, zoom: f64) -> f64 {
    // Web mercator, 512px tiles (MapLibre default).
    let scale = 512.0 * 2f64.powf(zoom);
    let (x1, y1) = mercator_norm(lat1, lon1);
    let (x2, y2) = mercator_norm(lat2, lon2);
    ((x1 - x2) * scale).hypot((y1 - y2) * scale)
}

fn mercator_norm(lat: f64, lon: f64) -> (f64, f64) {
    let x = (lon + 180.0) / 360.0;
    let siny = lat.to_radians().sin().clamp(-0.9999, 0.9999);
    let y = 0.5 - ((1.0 + siny) / (1.0 - siny)).ln() / (4.0 * std::f64::consts::PI);
    (x, y)
}

#[allow(dead_code)]
fn _provider_diag_ok(p: ProviderId) -> bool {
    !p.as_diag_str().contains("yr")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_weather_symbols_default_off() {
        // Guard: product default must stay opt-in (same class as WeatherPluginDefaultOffTest).
        const _: () = assert!(!MAP_WEATHER_SYMBOLS_DEFAULT_ENABLED);
    }

    #[test]
    fn place_kind_is_city_only() {
        assert_eq!(MAP_WEATHER_PLACE_KIND, "place:city");
        assert!(!MAP_WEATHER_PLACE_KIND.contains("town"));
        assert!(!MAP_WEATHER_PLACE_KIND.contains("village"));
    }

    #[test]
    fn place_cache_independent_of_hud_row() {
        let path = tempfile::NamedTempFile::new().unwrap();
        // HUD + place caches share file; HUD path uses WeatherCache.
        let hud = crate::weather::WeatherCache::open(path.path()).unwrap();
        let places = WeatherPlaceCache::open(path.path()).unwrap();
        let s = WeatherSample {
            lat: 59.91,
            lon: 10.75,
            icon_slug: "clear-day".into(),
            temp_c: Some(1.0),
            wind_ms: None,
            precip_mm: None,
            pressure_hpa: None,
            provider: "met_norway".into(),
            fetched_at_unix: 100,
            observation_unix: None,
            stale: false,
            summary: "clear".into(),
        };
        hud.upsert_sample(&s).unwrap();
        places
            .upsert(&WeatherPlaceSample {
                place_osm_id: 42,
                name: "Oslo".into(),
                kind: MAP_WEATHER_PLACE_KIND.into(),
                sample: WeatherSample {
                    icon_slug: "rain".into(),
                    ..s.clone()
                },
                next_fetch_unix: 200,
            })
            .unwrap();
        assert_eq!(hud.latest().unwrap().icon_slug, "clear-day");
        assert_eq!(places.get(42).unwrap().sample.icon_slug, "rain");
    }

    #[test]
    fn declutter_drops_close_neighbours() {
        let places = vec![
            (1, "A".into(), 59.14, 9.65),
            (2, "B".into(), 59.21, 9.61), // close to A at low zoom
            (3, "C".into(), 60.79, 11.07),
        ];
        let kept = declutter_places_by_pixel_spacing(&places, 5.0, 56.0, 10);
        assert!(kept.len() < places.len());
        assert!(kept.iter().any(|p| p.0 == 1) || kept.iter().any(|p| p.0 == 2));
        assert!(kept.iter().any(|p| p.0 == 3));
    }

    /// Cap-trimming with >10 well-spaced cities: nearest-to-center win.
    ///
    /// Approach (a): synthetic list of 15 cities (production cap stays 10).
    /// Places sit on a coarse grid so pairwise pixel distance at z=8 exceeds
    /// 56px — only the count cap fires, not spacing.
    #[test]
    fn cap_trimming_keeps_nearest_to_center() {
        // 15 cities on a ~0.8° grid around (60, 10); at z=8 spacing >> 56px.
        let mut places = Vec::new();
        let mut id = 1i64;
        for i in 0..5 {
            for j in 0..3 {
                let lat = 58.5 + f64::from(i) * 0.8;
                let lon = 8.5 + f64::from(j) * 0.8;
                // Names reverse-alphabetical so SQL/name order would differ from
                // distance order if we accidentally used name priority.
                let name = format!("City{:02}", 20 - id);
                places.push((id, name, lat, lon));
                id += 1;
            }
        }
        assert_eq!(places.len(), 15);

        let center_lat = 60.1;
        let center_lon = 9.3;
        let kept = select_map_weather_places(&places, center_lat, center_lon, 8.0, 56.0, 10);
        assert_eq!(
            kept.len(),
            10,
            "cap must trim 15 well-spaced cities down to 10"
        );

        // Expected: the 10 nearest by haversine to center.
        let mut by_dist = places.clone();
        by_dist.sort_by(|a, b| {
            let da = crate::tracks::haversine_km(center_lat, center_lon, a.2, a.3);
            let db = crate::tracks::haversine_km(center_lat, center_lon, b.2, b.3);
            da.total_cmp(&db)
                .then_with(|| a.1.cmp(&b.1))
                .then_with(|| a.0.cmp(&b.0))
        });
        let expected_ids: Vec<i64> = by_dist.iter().take(10).map(|p| p.0).collect();
        let kept_ids: Vec<i64> = kept.iter().map(|p| p.0).collect();
        assert_eq!(
            kept_ids, expected_ids,
            "survivors must be the 10 nearest to viewport center, not name/hash order"
        );

        // Far corner city farthest from center must be among the five dropped.
        let farthest_id = by_dist.last().unwrap().0;
        assert!(
            !kept_ids.contains(&farthest_id),
            "farthest city id={farthest_id} must be cap-trimmed"
        );
    }

    /// Spacing runs inside the same greedy pass as the cap: a near-center
    /// twin that fails spacing does not consume a cap slot, so a farther
    /// but spaced city can still be kept.
    #[test]
    fn spacing_before_cap_frees_slots_for_farther_places() {
        // Center at (60, 10). A is nearest; A_twin is ~same spot (spacing reject);
        // B..K are farther but well spaced — with cap=3 we should get A,B,C
        // (not A + only two of the far set after wasting a slot on A_twin).
        let places = vec![
            (100, "Z_far_c".into(), 61.5, 11.5),
            (101, "Y_far_b".into(), 61.0, 11.0),
            (102, "X_far_a".into(), 60.5, 10.5),
            (1, "A_near".into(), 60.00, 10.00),
            (2, "A_twin".into(), 60.001, 10.001), // within 56px of A at z=8
            (103, "W_far_d".into(), 62.0, 12.0),
        ];
        let kept = select_map_weather_places(&places, 60.0, 10.0, 8.0, 56.0, 3);
        let ids: Vec<i64> = kept.iter().map(|p| p.0).collect();
        assert_eq!(kept.len(), 3);
        assert_eq!(ids[0], 1, "nearest A must be first");
        assert!(
            !ids.contains(&2),
            "A_twin must be spacing-rejected, not kept under the cap"
        );
        assert_eq!(
            ids,
            vec![1, 102, 101],
            "after A, next nearest spaced cities fill remaining cap slots"
        );
    }

    #[test]
    fn place_fetch_due_respects_next_fetch() {
        let p = WeatherPlaceSample {
            place_osm_id: 1,
            name: "Hamar".into(),
            kind: MAP_WEATHER_PLACE_KIND.into(),
            sample: WeatherSample {
                lat: 60.8,
                lon: 11.0,
                icon_slug: "cloudy".into(),
                temp_c: None,
                wind_ms: None,
                precip_mm: None,
                pressure_hpa: None,
                provider: "met_norway".into(),
                fetched_at_unix: 1000,
                observation_unix: None,
                stale: false,
                summary: "cloudy".into(),
            },
            next_fetch_unix: 2000,
        };
        assert!(!place_fetch_due(Some(&p), 1500));
        assert!(place_fetch_due(Some(&p), 2000));
        assert!(place_fetch_due(None, 0));
    }
}
