//! Provider condition codes → Meteocons semantic slugs.

use serde_json::Value;
use std::collections::HashSet;
use std::path::Path;

use crate::weather::icons::FALLBACK_WEATHER_ICON_SLUG;

#[derive(Debug, Clone, PartialEq)]
pub struct WeatherCondition {
    pub icon_slug: String,
    pub summary: String,
    pub is_alert: bool,
}

/// Slug whitelist loaded from `manifest.json` (skip null placeholders).
#[derive(Debug, Clone, Default)]
pub struct ManifestSlugSet {
    slugs: HashSet<String>,
}

impl ManifestSlugSet {
    pub fn from_manifest_path(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let raw = std::fs::read_to_string(path)?;
        Self::from_manifest_json(&raw)
    }

    pub fn from_manifest_json(raw: &str) -> anyhow::Result<Self> {
        let v: Value = serde_json::from_str(raw)?;
        let mut slugs = HashSet::new();
        if let Some(cats) = v.get("categories").and_then(|c| c.as_array()) {
            for cat in cats {
                if let Some(icons) = cat.get("icons").and_then(|i| i.as_array()) {
                    for icon in icons {
                        if icon.is_null() {
                            continue;
                        }
                        if let Some(slug) = icon.get("slug").and_then(|s| s.as_str()) {
                            slugs.insert(slug.to_string());
                        }
                    }
                }
            }
        }
        Ok(Self { slugs })
    }

    pub fn contains(&self, slug: &str) -> bool {
        self.slugs.contains(slug)
    }

    pub fn resolve(&self, candidate: &str) -> String {
        if self.slugs.is_empty() {
            return candidate.to_string();
        }
        if self.slugs.contains(candidate) {
            candidate.to_string()
        } else {
            FALLBACK_WEATHER_ICON_SLUG.to_string()
        }
    }
}

/// Map MET Norway `symbol_code` (e.g. `partlycloudy_day`) to a Meteocons slug.
pub fn map_met_norway_symbol(symbol: &str) -> WeatherCondition {
    let s = symbol.trim().to_ascii_lowercase();
    let is_alert = s.contains("thunder");
    let slug = match s.as_str() {
        "clearsky_day" | "fair_day" => "clear-day",
        "clearsky_night" | "fair_night" => "clear-night",
        "clearsky_polartwilight" | "fair_polartwilight" => "clear-day",
        "partlycloudy_day" => "partly-cloudy-day",
        "partlycloudy_night" => "partly-cloudy-night",
        "partlycloudy_polartwilight" => "partly-cloudy-day",
        "cloudy" => "cloudy",
        "fog" => "fog",
        "lightrain"
        | "lightrainshowers_day"
        | "lightrainshowers_night"
        | "lightrainshowers_polartwilight" => "drizzle",
        "rain" | "rainshowers_day" | "rainshowers_night" | "rainshowers_polartwilight" => "rain",
        "heavyrain"
        | "heavyrainshowers_day"
        | "heavyrainshowers_night"
        | "heavyrainshowers_polartwilight" => "extreme-rain",
        "lightsleet"
        | "lightsleetshowers_day"
        | "lightsleetshowers_night"
        | "sleet"
        | "sleetshowers_day"
        | "sleetshowers_night" => "sleet",
        "heavysleet" | "heavysleetshowers_day" | "heavysleetshowers_night" => "extreme-sleet",
        "lightsnow"
        | "lightsnowshowers_day"
        | "lightsnowshowers_night"
        | "snow"
        | "snowshowers_day"
        | "snowshowers_night" => "snow",
        "heavysnow" | "heavysnowshowers_day" | "heavysnowshowers_night" => "extreme-snow",
        "lightrainandthunder"
        | "rainandthunder"
        | "heavyrainandthunder"
        | "lightrainshowersandthunder_day"
        | "rainshowersandthunder_day"
        | "heavyrainshowersandthunder_day"
        | "lightrainshowersandthunder_night"
        | "rainshowersandthunder_night"
        | "thunderstorm" => "thunderstorms",
        "lightsleetandthunder" | "sleetandthunder" | "heavysleetandthunder" => {
            "thunderstorms-day-sleet"
        }
        "lightsnowandthunder" | "snowandthunder" | "heavysnowandthunder" => {
            "thunderstorms-day-snow"
        }
        _ if s.contains("thunder") => "thunderstorms",
        _ if s.contains("snow") => "snow",
        _ if s.contains("sleet") => "sleet",
        _ if s.contains("rain") => "rain",
        _ if s.contains("fog") => "fog",
        _ if s.contains("cloud") => "cloudy",
        _ => FALLBACK_WEATHER_ICON_SLUG,
    };
    let alert_slug = if is_alert {
        if s.contains("extreme") || s.contains("heavy") {
            "thunderstorms-extreme"
        } else {
            slug
        }
    } else {
        slug
    };
    WeatherCondition {
        icon_slug: alert_slug.to_string(),
        summary: s,
        is_alert,
    }
}

/// Map Open-Meteo / WMO weather interpretation codes.
pub fn map_open_meteo_wmo(code: i64) -> WeatherCondition {
    let (slug, summary, is_alert) = match code {
        0 => ("clear-day", "clear", false),
        1 => ("clear-day", "mainly_clear", false),
        2 => ("partly-cloudy-day", "partly_cloudy", false),
        3 => ("overcast", "overcast", false),
        45 | 48 => ("fog", "fog", false),
        51 | 53 | 55 | 56 | 57 => ("drizzle", "drizzle", false),
        61 | 63 | 66 | 80 | 81 => ("rain", "rain", false),
        65 | 67 | 82 => ("extreme-rain", "heavy_rain", false),
        71 | 73 | 77 | 85 => ("snow", "snow", false),
        75 | 86 => ("extreme-snow", "heavy_snow", false),
        95 => ("thunderstorms", "thunderstorm", true),
        96 | 99 => ("thunderstorms-extreme", "thunderstorm_hail", true),
        _ => (FALLBACK_WEATHER_ICON_SLUG, "unknown", false),
    };
    WeatherCondition {
        icon_slug: slug.to_string(),
        summary: summary.to_string(),
        is_alert,
    }
}

/// Pick the sample / grid cell nearest to `lat`/`lon` from a list of candidates.
pub fn select_near<T, F>(items: &[T], lat: f64, lon: f64, coord: F) -> Option<&T>
where
    F: Fn(&T) -> (f64, f64),
{
    items.iter().min_by(|a, b| {
        let (alat, alon) = coord(a);
        let (blat, blon) = coord(b);
        let da = (alat - lat).hypot(alon - lon);
        let db = (blat - lat).hypot(blon - lon);
        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_clear_and_precip_and_alert() {
        assert_eq!(map_met_norway_symbol("clearsky_day").icon_slug, "clear-day");
        assert_eq!(map_met_norway_symbol("rain").icon_slug, "rain");
        assert!(map_met_norway_symbol("thunderstorm").is_alert);
        assert_eq!(map_open_meteo_wmo(0).icon_slug, "clear-day");
        assert_eq!(map_open_meteo_wmo(61).icon_slug, "rain");
        assert!(map_open_meteo_wmo(95).is_alert);
    }

    #[test]
    fn manifest_skips_nulls() {
        let raw = r#"{"categories":[{"icons":[null,{"slug":"clear-day"},{"slug":"rain"}]}]}"#;
        let set = ManifestSlugSet::from_manifest_json(raw).unwrap();
        assert!(set.contains("clear-day"));
        assert!(set.contains("rain"));
        assert_eq!(set.resolve("nope"), FALLBACK_WEATHER_ICON_SLUG);
    }

    #[test]
    fn select_near_picks_closest() {
        let pts = [(0.0_f64, 0.0_f64), (10.0, 10.0), (59.9, 10.7)];
        let got = select_near(&pts, 59.91, 10.75, |p| *p).unwrap();
        assert_eq!(*got, (59.9, 10.7));
    }
}
