//! HTTPS weather providers: MET Norway Locationforecast + Open-Meteo fallback.

use anyhow::{anyhow, Context, Result};
use reqwest::header::{HeaderMap, HeaderValue, IF_MODIFIED_SINCE, USER_AGENT as UA_HEADER};
use reqwest::StatusCode;
use serde_json::Value;
use std::time::Duration;
use thiserror::Error;

use crate::weather::cache::WeatherSample;
use crate::weather::map::{map_met_norway_symbol, map_open_meteo_wmo};
use crate::weather::nordic::in_nordic_arctic;

/// Mandatory identifying User-Agent (app + contact URL). Never generic.
pub const USER_AGENT: &str = "Navi/0.1.0 https://github.com/Supermagnum/Navi";

const MET_NORWAY_URL: &str = "https://api.met.no/weatherapi/locationforecast/2.0/compact";
const OPEN_METEO_URL: &str = "https://api.open-meteo.com/v1/forecast";
const FETCH_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderId {
    /// Norwegian Meteorological Institute Locationforecast (never labelled "Yr" in UI).
    MetNorway,
    OpenMeteo,
}

impl ProviderId {
    pub fn as_diag_str(self) -> &'static str {
        match self {
            Self::MetNorway => "met_norway",
            Self::OpenMeteo => "open_meteo",
        }
    }
}

#[derive(Debug, Error)]
pub enum WeatherFetchError {
    #[error("http {0}")]
    Http(u16),
    #[error("timeout")]
    Timeout,
    #[error("{0}")]
    Other(String),
}

#[derive(Debug, Clone)]
pub struct FetchOutcome {
    pub provider: ProviderId,
    pub sample: WeatherSample,
    pub last_modified: Option<String>,
    pub expires_unix: Option<i64>,
}

/// Truncate lat/lon to 4 decimal places (MET Norway traffic rule).
pub fn truncate_coord(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}

/// Fetch with failover: MET Norway first inside Nordic/Arctic, else Open-Meteo
/// first; always fall back to Open-Meteo on MET Norway error/timeout/429.
pub fn fetch_weather(
    lat: f64,
    lon: f64,
    if_modified_since: Option<&str>,
) -> Result<FetchOutcome, WeatherFetchError> {
    let lat = truncate_coord(lat);
    let lon = truncate_coord(lon);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| WeatherFetchError::Other(e.to_string()))?;
    rt.block_on(fetch_weather_async(lat, lon, if_modified_since))
}

async fn fetch_weather_async(
    lat: f64,
    lon: f64,
    if_modified_since: Option<&str>,
) -> Result<FetchOutcome, WeatherFetchError> {
    let client = reqwest::Client::builder()
        .timeout(FETCH_TIMEOUT)
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| WeatherFetchError::Other(e.to_string()))?;

    let try_met_first = in_nordic_arctic(lat, lon);
    if try_met_first {
        match fetch_met_norway(&client, lat, lon, if_modified_since).await {
            Ok(o) => return Ok(o),
            Err(WeatherFetchError::Http(429)) | Err(WeatherFetchError::Timeout) => {}
            Err(WeatherFetchError::Http(code)) if code >= 500 => {}
            Err(e) => {
                log::warn!("weather: met_norway failed ({e}); falling back to open_meteo");
            }
        }
        return fetch_open_meteo(&client, lat, lon).await;
    }

    match fetch_open_meteo(&client, lat, lon).await {
        Ok(o) => Ok(o),
        Err(e) => {
            log::warn!("weather: open_meteo failed ({e}); trying met_norway");
            fetch_met_norway(&client, lat, lon, if_modified_since).await
        }
    }
}

async fn fetch_met_norway(
    client: &reqwest::Client,
    lat: f64,
    lon: f64,
    if_modified_since: Option<&str>,
) -> Result<FetchOutcome, WeatherFetchError> {
    let url = format!("{MET_NORWAY_URL}?lat={lat:.4}&lon={lon:.4}");
    let mut headers = HeaderMap::new();
    headers.insert(UA_HEADER, HeaderValue::from_static(USER_AGENT));
    if let Some(ims) = if_modified_since {
        if let Ok(v) = HeaderValue::from_str(ims) {
            headers.insert(IF_MODIFIED_SINCE, v);
        }
    }
    let resp = client
        .get(&url)
        .headers(headers)
        .send()
        .await
        .map_err(map_reqwest_err)?;
    let status = resp.status();
    if status == StatusCode::NOT_MODIFIED {
        return Err(WeatherFetchError::Other("not_modified".into()));
    }
    if status == StatusCode::TOO_MANY_REQUESTS {
        return Err(WeatherFetchError::Http(429));
    }
    if !status.is_success() {
        return Err(WeatherFetchError::Http(status.as_u16()));
    }
    let last_modified = resp
        .headers()
        .get(reqwest::header::LAST_MODIFIED)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let expires_unix = resp
        .headers()
        .get(reqwest::header::EXPIRES)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_http_date_unix);
    let body = resp
        .text()
        .await
        .map_err(|e| WeatherFetchError::Other(e.to_string()))?;
    let sample = parse_met_norway_json(&body, lat, lon)
        .map_err(|e| WeatherFetchError::Other(e.to_string()))?;
    Ok(FetchOutcome {
        provider: ProviderId::MetNorway,
        sample,
        last_modified,
        expires_unix,
    })
}

async fn fetch_open_meteo(
    client: &reqwest::Client,
    lat: f64,
    lon: f64,
) -> Result<FetchOutcome, WeatherFetchError> {
    let url = format!(
        "{OPEN_METEO_URL}?latitude={lat:.4}&longitude={lon:.4}\
         &current=temperature_2m,weather_code,wind_speed_10m,precipitation,pressure_msl\
         &wind_speed_unit=ms"
    );
    let resp = client
        .get(&url)
        .header(UA_HEADER, USER_AGENT)
        .send()
        .await
        .map_err(map_reqwest_err)?;
    let status = resp.status();
    if status == StatusCode::TOO_MANY_REQUESTS {
        return Err(WeatherFetchError::Http(429));
    }
    if !status.is_success() {
        return Err(WeatherFetchError::Http(status.as_u16()));
    }
    let body = resp
        .text()
        .await
        .map_err(|e| WeatherFetchError::Other(e.to_string()))?;
    let sample = parse_open_meteo_json(&body, lat, lon)
        .map_err(|e| WeatherFetchError::Other(e.to_string()))?;
    Ok(FetchOutcome {
        provider: ProviderId::OpenMeteo,
        sample,
        last_modified: None,
        expires_unix: None,
    })
}

fn map_reqwest_err(e: reqwest::Error) -> WeatherFetchError {
    if e.is_timeout() {
        WeatherFetchError::Timeout
    } else {
        WeatherFetchError::Other(e.to_string())
    }
}

fn parse_http_date_unix(s: &str) -> Option<i64> {
    // Best-effort: chrono can parse RFC 2822 via DateTime::parse_from_rfc2822
    chrono::DateTime::parse_from_rfc2822(s)
        .ok()
        .map(|dt| dt.timestamp())
}

pub fn parse_met_norway_json(body: &str, lat: f64, lon: f64) -> Result<WeatherSample> {
    let v: Value = serde_json::from_str(body).context("met norway json")?;
    let timeseries = v
        .pointer("/properties/timeseries")
        .and_then(|t| t.as_array())
        .ok_or_else(|| anyhow!("missing timeseries"))?;
    let first = timeseries
        .first()
        .ok_or_else(|| anyhow!("empty timeseries"))?;
    let details = first.pointer("/data/instant/details");
    let symbol = first
        .pointer("/data/next_1_hours/summary/symbol_code")
        .or_else(|| first.pointer("/data/next_6_hours/summary/symbol_code"))
        .or_else(|| first.pointer("/data/next_12_hours/summary/symbol_code"))
        .and_then(|s| s.as_str())
        .unwrap_or("unknown");
    let cond = map_met_norway_symbol(symbol);
    let temp_c = details
        .and_then(|d| d.get("air_temperature"))
        .and_then(|x| x.as_f64());
    let wind_ms = details
        .and_then(|d| d.get("wind_speed"))
        .and_then(|x| x.as_f64());
    let pressure_hpa = details
        .and_then(|d| d.get("air_pressure_at_sea_level"))
        .and_then(|x| x.as_f64());
    let precip_mm = first
        .pointer("/data/next_1_hours/details/precipitation_amount")
        .and_then(|x| x.as_f64());
    Ok(WeatherSample {
        lat,
        lon,
        icon_slug: cond.icon_slug,
        temp_c,
        wind_ms,
        precip_mm,
        pressure_hpa,
        provider: ProviderId::MetNorway.as_diag_str().into(),
        fetched_at_unix: 0,
        observation_unix: None,
        stale: false,
        summary: cond.summary,
    })
}

pub fn parse_open_meteo_json(body: &str, lat: f64, lon: f64) -> Result<WeatherSample> {
    let v: Value = serde_json::from_str(body).context("open meteo json")?;
    let current = v.get("current").ok_or_else(|| anyhow!("missing current"))?;
    let code = current
        .get("weather_code")
        .and_then(|x| x.as_i64())
        .unwrap_or(-1);
    let cond = map_open_meteo_wmo(code);
    Ok(WeatherSample {
        lat,
        lon,
        icon_slug: cond.icon_slug,
        temp_c: current.get("temperature_2m").and_then(|x| x.as_f64()),
        wind_ms: current.get("wind_speed_10m").and_then(|x| x.as_f64()),
        precip_mm: current.get("precipitation").and_then(|x| x.as_f64()),
        pressure_hpa: current.get("pressure_msl").and_then(|x| x.as_f64()),
        provider: ProviderId::OpenMeteo.as_diag_str().into(),
        fetched_at_unix: 0,
        observation_unix: None,
        stale: false,
        summary: cond.summary,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncates_to_four_decimals() {
        assert!((truncate_coord(59.91391234) - 59.9139).abs() < 1e-9);
    }

    #[test]
    fn user_agent_identifies_app() {
        assert!(USER_AGENT.contains("Navi"));
        assert!(USER_AGENT.contains("github.com"));
        assert!(!USER_AGENT.to_lowercase().contains("okhttp"));
        assert!(!USER_AGENT.to_lowercase().contains("dalvik"));
    }

    #[test]
    fn parse_met_norway_fixture() {
        let body = r#"{
          "properties": {
            "timeseries": [{
              "data": {
                "instant": { "details": {
                  "air_temperature": 5.1,
                  "wind_speed": 2.2,
                  "air_pressure_at_sea_level": 1012.0
                }},
                "next_1_hours": {
                  "summary": { "symbol_code": "clearsky_day" },
                  "details": { "precipitation_amount": 0.0 }
                }
              }
            }]
          }
        }"#;
        let s = parse_met_norway_json(body, 59.91, 10.75).unwrap();
        assert_eq!(s.icon_slug, "clear-day");
        assert_eq!(s.provider, "met_norway");
        assert!(!s.provider.contains("yr"));
    }

    #[test]
    fn parse_open_meteo_fixture() {
        let body = r#"{
          "current": {
            "temperature_2m": 12.0,
            "weather_code": 61,
            "wind_speed_10m": 4.0,
            "precipitation": 1.2,
            "pressure_msl": 1008.0
          }
        }"#;
        let s = parse_open_meteo_json(body, 35.0, 139.0).unwrap();
        assert_eq!(s.icon_slug, "rain");
        assert_eq!(s.provider, "open_meteo");
    }

    #[test]
    fn provider_diag_never_says_yr() {
        assert!(!ProviderId::MetNorway.as_diag_str().contains("yr"));
    }
}
