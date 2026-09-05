//! Host-owned weather fetch, SQLite cache, throttle, and icon path resolution.
//!
//! Product plugin guests call into the cache via the `weather_read` HostApi
//! capability; Android HUD uses the same cache through UniFFI. Guests never
//! open sockets or hold API keys.

mod cache;
mod icons;
mod map;
mod nordic;
mod places;
mod providers;
mod throttle;

pub use cache::{WeatherCache, WeatherSample};
pub use icons::{
    resolve_weather_icon_path, weather_icon_relative_path, DEFAULT_WEATHER_ICON_STYLE,
    FALLBACK_WEATHER_ICON_SLUG, WEATHER_ICON_STYLE_FILL,
};
pub use map::{
    map_met_norway_symbol, map_open_meteo_wmo, select_near, ManifestSlugSet, WeatherCondition,
};
pub use nordic::in_nordic_arctic;
pub use places::{
    apply_stale_flag, declutter_places_by_pixel_spacing, place_fetch_due, refresh_place_if_allowed,
    select_map_weather_places, PlaceRefreshResult, WeatherPlaceCache, WeatherPlaceSample,
    MAP_WEATHER_MAX_SYMBOLS, MAP_WEATHER_MIN_PIXEL_SPACING, MAP_WEATHER_PLACE_KIND,
    MAP_WEATHER_SYMBOLS_DEFAULT_ENABLED, MAP_WEATHER_ZOOM_MAX,
};
pub use providers::{
    fetch_weather, truncate_coord, FetchOutcome, ProviderId, WeatherFetchError, USER_AGENT,
};
pub use throttle::{
    decide_fetch, jittered_interval_secs, FetchDecision, ThrottleConfig, ThrottleState,
    DEFAULT_ACTIVE_INTERVAL_SECS, DEFAULT_MANUAL_MIN_INTERVAL_SECS,
};

/// Product requirement: weather plugin ships disabled until the user opts in.
/// Tests assert this stays `false`; flipping it without an explicit product
/// decision must fail the build.
pub const WEATHER_PLUGIN_DEFAULT_ENABLED: bool = false;

/// CC BY 4.0 attribution shown in settings / about (never the "Yr" brand).
pub const WEATHER_ATTRIBUTION: &str =
    "Weather data from the Norwegian Meteorological Institute and Open-Meteo (CC BY 4.0)";

/// Refresh when enabled + active, honouring throttle; otherwise return cache.
pub fn refresh_if_allowed(
    cache: &WeatherCache,
    lat: f64,
    lon: f64,
    enabled: bool,
    app_active: bool,
    manual: bool,
    now_unix: i64,
) -> RefreshResult {
    if !enabled {
        return RefreshResult {
            fetched: false,
            reason: "plugin_disabled".into(),
            sample: cache.nearest(lat, lon, 50_000.0),
            provider: None,
        };
    }
    if !app_active && !manual {
        return RefreshResult {
            fetched: false,
            reason: "app_backgrounded".into(),
            sample: cache.nearest(lat, lon, 50_000.0),
            provider: None,
        };
    }

    let meta = cache.meta().unwrap_or_default();
    let decision = decide_fetch(&meta.throttle, now_unix, manual, ThrottleConfig::default());
    match decision {
        FetchDecision::ServeCache { reason } => RefreshResult {
            fetched: false,
            reason,
            sample: cache.nearest(lat, lon, 50_000.0).map(|mut s| {
                s.stale = true;
                s
            }),
            provider: None,
        },
        FetchDecision::Fetch { scheduled_next } => {
            let if_mod = meta.if_modified_since.clone();
            match fetch_weather(lat, lon, if_mod.as_deref()) {
                Ok(outcome) => {
                    let mut sample = outcome.sample;
                    sample.fetched_at_unix = now_unix;
                    sample.stale = false;
                    let _ = cache.upsert_sample(&sample);
                    let mut throttle = meta.throttle;
                    throttle.last_fetch_unix = Some(now_unix);
                    if manual {
                        throttle.last_manual_unix = Some(now_unix);
                    }
                    throttle.next_scheduled_unix = Some(scheduled_next);
                    let _ = cache.save_meta(&cache::WeatherMeta {
                        throttle,
                        if_modified_since: outcome.last_modified,
                        expires_unix: outcome.expires_unix,
                        last_provider: Some(outcome.provider.as_diag_str().into()),
                    });
                    log::info!(
                        "weather: provider={} lat={:.4} lon={:.4} slug={}",
                        outcome.provider.as_diag_str(),
                        sample.lat,
                        sample.lon,
                        sample.icon_slug
                    );
                    RefreshResult {
                        fetched: true,
                        reason: "fetched".into(),
                        sample: Some(sample),
                        provider: Some(outcome.provider),
                    }
                }
                Err(e) => {
                    log::warn!("weather: fetch failed: {e}");
                    RefreshResult {
                        fetched: false,
                        reason: format!("fetch_error:{e}"),
                        sample: cache.nearest(lat, lon, 50_000.0).map(|mut s| {
                            s.stale = true;
                            s
                        }),
                        provider: None,
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct RefreshResult {
    pub fetched: bool,
    pub reason: String,
    pub sample: Option<WeatherSample>,
    pub provider: Option<ProviderId>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weather_plugin_default_enabled_is_false() {
        // Guard: product default must stay opt-in (plugins.md enable/disable).
        const _: () = assert!(!WEATHER_PLUGIN_DEFAULT_ENABLED);
    }

    #[test]
    fn attribution_never_mentions_yr_brand() {
        let lower = WEATHER_ATTRIBUTION.to_lowercase();
        assert!(!lower.contains("yr.no"));
        assert!(!lower.split_whitespace().any(|w| w == "yr"));
        // Avoid bare brand token "yr" as a word
        assert!(!WEATHER_ATTRIBUTION.contains(" Yr "));
        assert!(!WEATHER_ATTRIBUTION.starts_with("Yr"));
    }
}
