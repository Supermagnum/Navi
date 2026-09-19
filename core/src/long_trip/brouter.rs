//! BRouter public-server client for preliminary long-trip corridors.
//!
//! GET `{base}/brouter?lonlats=lon,lat|…&profile=…&format=geojson&alternativeidx=0`
//! Default profile [`BROUTER_CAR_PROFILE`] = `car-eco` (see profile comparison in
//! tests: `car-fast` was killed by the public server watchdog on Klecken;
//! `car-eco` returned 4 ferry segments and completed).
//!
//! Ferry detection: count rows in GeoJSON `properties.messages` whose
//! `WayTags` column (index 9) contains `ferry` (case-insensitive), typically
//! `route=ferry`.

use std::time::Duration;

use serde_json::Value;

/// Default public BRouter host (no trailing slash).
pub const DEFAULT_BROUTER_BASE_URL: &str = "https://brouter.de";

/// Car profile chosen after Klecken comparison (see `long_trip` tests).
pub const BROUTER_CAR_PROFILE: &str = "car-eco";

/// Placeholder request timeout (policy).
pub const BROUTER_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone)]
pub struct BrouterConfig {
    pub base_url: String,
    pub profile: String,
}

impl BrouterConfig {
    pub fn from_parts(base_url: impl Into<String>, profile: impl Into<String>) -> Self {
        let mut base = base_url.into();
        while base.ends_with('/') {
            base.pop();
        }
        Self {
            base_url: if base.is_empty() {
                DEFAULT_BROUTER_BASE_URL.to_string()
            } else {
                base
            },
            profile: {
                let p = profile.into();
                if p.is_empty() {
                    BROUTER_CAR_PROFILE.to_string()
                } else {
                    p
                }
            },
        }
    }

    pub fn default_public() -> Self {
        Self::from_parts(DEFAULT_BROUTER_BASE_URL, BROUTER_CAR_PROFILE)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrouterError {
    RateLimited,
    Timeout,
    ServerError { status: u16 },
    InvalidResponse(String),
    NoRoute,
    Offline,
}

impl std::fmt::Display for BrouterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RateLimited => write!(f, "BRouter rate limited or asked to retry later"),
            Self::Timeout => write!(f, "BRouter request timed out"),
            Self::ServerError { status } => write!(f, "BRouter HTTP {status}"),
            Self::InvalidResponse(d) => write!(f, "BRouter invalid response: {d}"),
            Self::NoRoute => write!(f, "BRouter found no route"),
            Self::Offline => write!(f, "BRouter unreachable (offline or network error)"),
        }
    }
}

impl std::error::Error for BrouterError {}

#[derive(Debug, Clone, PartialEq)]
pub struct BrouterRoute {
    pub lat_lon: Vec<(f64, f64)>,
    pub distance_m: f64,
    pub duration_s: Option<f64>,
    pub ferry_segments: usize,
}

/// Build the request URL (no network).
pub fn build_brouter_url(
    cfg: &BrouterConfig,
    waypoints_lat_lon: &[(f64, f64)],
) -> Result<String, BrouterError> {
    if waypoints_lat_lon.len() < 2 {
        return Err(BrouterError::InvalidResponse(
            "need at least origin and destination".into(),
        ));
    }
    let lonlats: String = waypoints_lat_lon
        .iter()
        .map(|&(lat, lon)| format!("{lon},{lat}"))
        .collect::<Vec<_>>()
        .join("|");
    // Keep `|` unescaped in the path query the way the public API examples do;
    // percent-encoding both commas and pipes caused 400s on some probes.
    Ok(format!(
        "{}/brouter?lonlats={lonlats}&profile={}&alternativeidx=0&format=geojson",
        cfg.base_url, cfg.profile
    ))
}

/// Parse a BRouter GeoJSON FeatureCollection body (raw, not the probe wrapper).
pub fn parse_brouter_geojson(body: &str) -> Result<BrouterRoute, BrouterError> {
    let v: Value = serde_json::from_str(body)
        .map_err(|e| BrouterError::InvalidResponse(format!("json: {e}")))?;
    let feature = v
        .get("features")
        .and_then(|f| f.as_array())
        .and_then(|a| a.first())
        .cloned()
        .ok_or(BrouterError::NoRoute)?;

    let coords = feature
        .pointer("/geometry/coordinates")
        .and_then(|c| c.as_array())
        .ok_or_else(|| BrouterError::InvalidResponse("missing geometry.coordinates".into()))?;

    let mut lat_lon = Vec::with_capacity(coords.len());
    for c in coords {
        let pair = c
            .as_array()
            .ok_or_else(|| BrouterError::InvalidResponse("coord not array".into()))?;
        if pair.len() < 2 {
            return Err(BrouterError::InvalidResponse("coord too short".into()));
        }
        let lon = pair[0]
            .as_f64()
            .ok_or_else(|| BrouterError::InvalidResponse("lon".into()))?;
        let lat = pair[1]
            .as_f64()
            .ok_or_else(|| BrouterError::InvalidResponse("lat".into()))?;
        lat_lon.push((lat, lon));
    }
    if lat_lon.len() < 2 {
        return Err(BrouterError::NoRoute);
    }

    let props = feature.get("properties");
    let distance_m = props
        .and_then(|p| p.get("track-length"))
        .and_then(|x| {
            x.as_f64()
                .or_else(|| x.as_str().and_then(|s| s.parse().ok()))
        })
        .unwrap_or(0.0);
    let duration_s = props.and_then(|p| p.get("total-time")).and_then(|x| {
        x.as_f64()
            .or_else(|| x.as_str().and_then(|s| s.parse().ok()))
    });

    let ferry_segments = count_ferry_segments_in_messages(props);
    Ok(BrouterRoute {
        lat_lon,
        distance_m,
        duration_s,
        ferry_segments,
    })
}

/// Count ferry segments from `properties.messages` WayTags (column index 9).
pub fn count_ferry_segments_in_messages(props: Option<&Value>) -> usize {
    let Some(msgs) = props
        .and_then(|p| p.get("messages"))
        .and_then(|m| m.as_array())
    else {
        return 0;
    };
    let mut n = 0usize;
    for row in msgs.iter().skip(1) {
        let Some(arr) = row.as_array() else {
            continue;
        };
        let tags = arr
            .get(9)
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if tags.contains("ferry") {
            n += 1;
        }
    }
    n
}

fn classify_http(status: u16, body: &str) -> BrouterError {
    let lower = body.to_ascii_lowercase();
    if status == 429 || lower.contains("retry later") || lower.contains("rate") {
        return BrouterError::RateLimited;
    }
    if status == 403 {
        return BrouterError::RateLimited;
    }
    if (500..600).contains(&status) {
        return BrouterError::ServerError { status };
    }
    if status == 400 && lower.contains("watchdog") {
        return BrouterError::ServerError { status };
    }
    if lower.contains("no track") || lower.contains("no route") {
        return BrouterError::NoRoute;
    }
    BrouterError::InvalidResponse(format!(
        "http {status}: {}",
        body.chars().take(200).collect::<String>()
    ))
}

/// GET directions. Never invents geometry.
pub fn request_brouter_route(
    cfg: &BrouterConfig,
    waypoints_lat_lon: &[(f64, f64)],
) -> Result<BrouterRoute, BrouterError> {
    let url = build_brouter_url(cfg, waypoints_lat_lon)?;
    crate::long_trip::pace_preliminary_network();
    let client = crate::download::shared_http_client();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| BrouterError::Offline)?;

    let (status, text) = rt.block_on(async {
        let resp = client
            .get(&url)
            .timeout(BROUTER_TIMEOUT)
            .header(reqwest::header::USER_AGENT, crate::pack_server::USER_AGENT)
            .header(
                reqwest::header::ACCEPT,
                "application/geo+json, application/json",
            )
            .send()
            .await;
        match resp {
            Ok(r) => {
                let status = r.status().as_u16();
                let text = r
                    .text()
                    .await
                    .map_err(|_| BrouterError::InvalidResponse("body".into()))?;
                Ok::<_, BrouterError>((status, text))
            }
            Err(e) => {
                if e.is_timeout() {
                    Err(BrouterError::Timeout)
                } else {
                    Err(BrouterError::Offline)
                }
            }
        }
    })?;

    match status {
        200 => parse_brouter_geojson(&text),
        other => Err(classify_http(other, &text)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_uses_lon_lat_pipe_and_car_eco() {
        let cfg = BrouterConfig::default_public();
        let url = build_brouter_url(&cfg, &[(53.334, 10.045), (61.59, 10.33)]).unwrap();
        assert!(url.contains("lonlats=10.045,53.334|10.33,61.59"));
        assert!(url.contains("profile=car-eco"));
        assert!(url.contains("format=geojson"));
    }

    #[test]
    fn no_straight_line_in_brouter_module() {
        let src = include_str!("brouter.rs");
        let prod = src.split("#[cfg(test)]").next().unwrap_or(src);
        assert!(!prod.contains("straight_line") && !prod.contains("chord_fallback"));
    }
}
