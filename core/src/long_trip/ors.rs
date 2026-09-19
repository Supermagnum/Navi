//! OpenRouteService directions client (preliminary long-trip corridor).
//!
//! Docs verified 2026-09-19:
//! - POST `{base}/v2/directions/driving-car/geojson`
//! - Coordinates: `[lon, lat]` pairs; min 2, max 50 waypoints
//! - `options.avoid_features`: `ferries` (also highways/tollways/fords/steps)
//! - `options.avoid_countries`: integer ORS country ids (not ISO strings)
//! - Public API max driving distance: 6000 km; avoid-areas: 150 km
//! - Standard plan quota: 2000 directions/day, 40/min
//! - Prefer base `https://api.heigit.org/openrouteservice` (api.openrouteservice.org deprecated)

use std::time::Duration;

use serde_json::{json, Value};

use super::neighbours::avoid_country_ids_for_allowed;

/// Default HeiGIT ORS host (no trailing slash).
pub const DEFAULT_ORS_BASE_URL: &str = "https://api.heigit.org/openrouteservice";

/// UI disclosure: origin, vias and destination are sent to a third party.
pub const ORS_DISCLOSURE: &str = "Long-trip mode sends your origin, via points and destination coordinates to OpenRouteService (a third party) to estimate which map regions you need. The API key stays on your device settings and is never logged.";

/// Public API driving-car maximum route distance (metres).
pub const ORS_MAX_DISTANCE_M: f64 = 6_000_000.0;

/// Maximum waypoints per directions request (public API).
pub const ORS_MAX_WAYPOINTS: usize = 50;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
pub struct OrsConfig {
    /// User setting — never log this value.
    pub api_key: String,
    pub base_url: String,
}

impl OrsConfig {
    pub fn from_parts(api_key: impl Into<String>, base_url: impl Into<String>) -> Self {
        let mut base = base_url.into();
        while base.ends_with('/') {
            base.pop();
        }
        Self {
            api_key: api_key.into(),
            base_url: if base.is_empty() {
                DEFAULT_ORS_BASE_URL.to_string()
            } else {
                base
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum OrsError {
    NoApiKey,
    Offline,
    RateLimited,
    NoRoute,
    InvalidResponse(String),
    /// Distance, waypoint count, or quota/size limit exceeded.
    RequestTooLarge {
        reason: String,
    },
}

impl std::fmt::Display for OrsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoApiKey => write!(f, "ORS API key not set"),
            Self::Offline => write!(f, "ORS unreachable (offline or network error)"),
            Self::RateLimited => write!(f, "ORS rate limited"),
            Self::NoRoute => write!(f, "ORS found no route"),
            Self::InvalidResponse(d) => write!(f, "ORS invalid response: {d}"),
            Self::RequestTooLarge { reason } => write!(f, "ORS request too large: {reason}"),
        }
    }
}

impl std::error::Error for OrsError {}

#[derive(Debug, Clone, PartialEq)]
pub struct OrsRoute {
    /// WGS84 samples as `(lat, lon)` for corridor densify / catalog PIP.
    pub lat_lon: Vec<(f64, f64)>,
    pub distance_m: f64,
}

/// Build the JSON body (no network). Coordinates must already be `[lon, lat]`.
pub fn build_directions_request_body(
    coordinates_lon_lat: &[[f64; 2]],
    allowed_countries: Option<&[String]>,
) -> Result<Value, OrsError> {
    if coordinates_lon_lat.len() < 2 {
        return Err(OrsError::InvalidResponse(
            "need at least origin and destination".into(),
        ));
    }
    if coordinates_lon_lat.len() > ORS_MAX_WAYPOINTS {
        return Err(OrsError::RequestTooLarge {
            reason: format!(
                "waypoints {} exceed ORS max {ORS_MAX_WAYPOINTS}",
                coordinates_lon_lat.len()
            ),
        });
    }
    let mut options = json!({
        "avoid_features": ["ferries"]
    });
    if let Some(allowed) = allowed_countries {
        if !allowed.is_empty() {
            let ids = avoid_country_ids_for_allowed(allowed);
            if !ids.is_empty() {
                options["avoid_countries"] = json!(ids);
            }
        }
    }
    Ok(json!({
        "coordinates": coordinates_lon_lat,
        "instructions": false,
        "geometry": true,
        "elevation": false,
        "options": options
    }))
}

/// Parse a GeoJSON FeatureCollection / Feature directions response.
pub fn parse_directions_geojson(body: &str) -> Result<OrsRoute, OrsError> {
    let v: Value =
        serde_json::from_str(body).map_err(|e| OrsError::InvalidResponse(format!("json: {e}")))?;
    if let Some(err) = v.get("error") {
        let code = err.get("code").and_then(|c| c.as_i64()).unwrap_or(0);
        let msg = err
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("error");
        return Err(classify_ors_error_message(code, msg));
    }

    let feature = if let Some(arr) = v.get("features").and_then(|f| f.as_array()) {
        arr.first().cloned().ok_or(OrsError::NoRoute)?
    } else if v.get("type").and_then(|t| t.as_str()) == Some("Feature") {
        v.clone()
    } else {
        return Err(OrsError::InvalidResponse("no features".into()));
    };

    let coords = feature
        .pointer("/geometry/coordinates")
        .and_then(|c| c.as_array())
        .ok_or_else(|| OrsError::InvalidResponse("missing geometry.coordinates".into()))?;

    let mut lat_lon = Vec::with_capacity(coords.len());
    for c in coords {
        let pair = c
            .as_array()
            .ok_or_else(|| OrsError::InvalidResponse("coord not array".into()))?;
        if pair.len() < 2 {
            return Err(OrsError::InvalidResponse("coord too short".into()));
        }
        let lon = pair[0]
            .as_f64()
            .ok_or_else(|| OrsError::InvalidResponse("lon".into()))?;
        let lat = pair[1]
            .as_f64()
            .ok_or_else(|| OrsError::InvalidResponse("lat".into()))?;
        lat_lon.push((lat, lon));
    }
    if lat_lon.len() < 2 {
        return Err(OrsError::NoRoute);
    }

    let distance_m = feature
        .pointer("/properties/summary/distance")
        .or_else(|| feature.pointer("/properties/segments/0/distance"))
        .and_then(|d| d.as_f64())
        .unwrap_or(0.0);

    if distance_m > ORS_MAX_DISTANCE_M + 1.0 {
        return Err(OrsError::RequestTooLarge {
            reason: format!(
                "route distance {distance_m:.0} m exceeds ORS max {ORS_MAX_DISTANCE_M:.0} m"
            ),
        });
    }

    Ok(OrsRoute {
        lat_lon,
        distance_m,
    })
}

fn classify_ors_error_message(code: i64, msg: &str) -> OrsError {
    let lower = msg.to_ascii_lowercase();
    if lower.contains("too large")
        || lower.contains("maximum distance")
        || lower.contains("max distance")
        || lower.contains("waypoints")
        || lower.contains("quota")
        || lower.contains("rate limit")
        || code == 2004
        || code == 2003
    {
        if lower.contains("rate") || code == 429 {
            return OrsError::RateLimited;
        }
        return OrsError::RequestTooLarge {
            reason: msg.to_string(),
        };
    }
    if lower.contains("not found") || lower.contains("could not find routable") || code == 2010 {
        return OrsError::NoRoute;
    }
    OrsError::InvalidResponse(msg.to_string())
}

/// POST directions. Never logs `api_key`.
pub fn request_directions(
    cfg: &OrsConfig,
    waypoints_lat_lon: &[(f64, f64)],
    allowed_countries: Option<&[String]>,
) -> Result<OrsRoute, OrsError> {
    if cfg.api_key.trim().is_empty() {
        return Err(OrsError::NoApiKey);
    }
    if waypoints_lat_lon.len() > ORS_MAX_WAYPOINTS {
        return Err(OrsError::RequestTooLarge {
            reason: format!(
                "waypoints {} exceed ORS max {ORS_MAX_WAYPOINTS}",
                waypoints_lat_lon.len()
            ),
        });
    }
    let coords: Vec<[f64; 2]> = waypoints_lat_lon
        .iter()
        .map(|&(lat, lon)| [lon, lat])
        .collect();
    let body = build_directions_request_body(&coords, allowed_countries)?;
    let url = format!("{}/v2/directions/driving-car/geojson", cfg.base_url);

    let client = crate::download::shared_http_client();
    let key = cfg.api_key.trim().to_string();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| OrsError::Offline)?;

    let (status, text) = rt.block_on(async {
        let resp = client
            .post(&url)
            .timeout(REQUEST_TIMEOUT)
            .header(reqwest::header::USER_AGENT, crate::pack_server::USER_AGENT)
            .header("Authorization", &key)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|_| OrsError::Offline)?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|_| OrsError::InvalidResponse("body".into()))?;
        Ok::<_, OrsError>((status, text))
    })?;

    match status.as_u16() {
        200 => parse_directions_geojson(&text),
        401 | 403 => Err(OrsError::NoApiKey),
        429 => Err(OrsError::RateLimited),
        404 => Err(OrsError::NoRoute),
        413 | 400 => {
            let lower = text.to_ascii_lowercase();
            if lower.contains("distance")
                || lower.contains("waypoint")
                || lower.contains("too large")
                || lower.contains("quota")
            {
                Err(OrsError::RequestTooLarge { reason: text })
            } else if lower.contains("routable") || lower.contains("not found") {
                Err(OrsError::NoRoute)
            } else {
                Err(parse_directions_geojson(&text)
                    .err()
                    .unwrap_or(OrsError::InvalidResponse(text)))
            }
        }
        _ => Err(OrsError::InvalidResponse(format!(
            "http {}: {}",
            status.as_u16(),
            text.chars().take(200).collect::<String>()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_uses_lon_lat_avoids_ferries_and_us_neighbours() {
        let body = build_directions_request_body(
            &[[-73.98, 40.74], [-118.40, 33.85]],
            Some(&["us".into()]),
        )
        .unwrap();
        let coords = body["coordinates"].as_array().unwrap();
        assert_eq!(coords[0][0], json!(-73.98));
        assert_eq!(coords[0][1], json!(40.74));
        let feats = body["options"]["avoid_features"].as_array().unwrap();
        assert!(feats.iter().any(|v| v == "ferries"));
        let avoid = body["options"]["avoid_countries"].as_array().unwrap();
        let ids: Vec<u64> = avoid.iter().filter_map(|v| v.as_u64()).collect();
        assert!(ids.contains(&35), "Canada id 35 in {ids:?}");
        assert!(ids.contains(&128), "Mexico id 128 in {ids:?}");
    }

    #[test]
    fn no_straight_line_fallback_in_module() {
        // Guard: this crate must not invent a chord when ORS fails.
        let src = include_str!("ors.rs");
        assert!(
            !src.contains("straight_line") && !src.contains("chord_fallback"),
            "ORS module must never fall back to a straight line"
        );
    }

    #[test]
    fn parse_geojson_feature_collection() {
        let body = r#"{
          "type":"FeatureCollection",
          "features":[{
            "type":"Feature",
            "properties":{"summary":{"distance":12345.0}},
            "geometry":{"type":"LineString","coordinates":[[10.0,53.5],[10.5,54.0],[11.0,55.0]]}
          }]
        }"#;
        let route = parse_directions_geojson(body).unwrap();
        assert_eq!(route.lat_lon.len(), 3);
        assert_eq!(route.lat_lon[0], (53.5, 10.0));
        assert!((route.distance_m - 12345.0).abs() < 0.1);
    }

    #[test]
    fn too_many_waypoints_is_request_too_large() {
        let pts: Vec<[f64; 2]> = (0..51).map(|i| [i as f64, 0.0]).collect();
        let err = build_directions_request_body(&pts, None).unwrap_err();
        assert!(matches!(err, OrsError::RequestTooLarge { .. }));
    }
}
