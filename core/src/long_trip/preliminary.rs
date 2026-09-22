//! Unified preliminary corridor router: BRouter primary, ORS fallback.
//!
//! # Provider policy (exact)
//! 1. `allowed_countries` is `Some` (non-empty) → ORS only. No key →
//!    [`PreliminaryError::NoApiKey`] with a message that country-restricted
//!    trips need ORS. Never consult Phase 1 country rings for this decision.
//! 2. `allowed_countries` is `None` / empty → BRouter first.
//! 3. Fall back to ORS (when a key is set) if BRouter returns rate-limit/403,
//!    timeout, 5xx, invalid response, or a route with ferry segments. With no
//!    key, surface the BRouter failure — except ferries: return the route with
//!    a ferry warning.
//! 4. Result records provider + warnings for the UI status line.
//! 5. Disclosure covers third parties; BRouter logging noted in
//!    [`PRELIMINARY_ROUTE_DISCLOSURE`].

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use super::brouter::{request_brouter_route, BrouterConfig, BrouterError, BrouterRoute};
use super::ors::{request_directions, OrsConfig, OrsError, OrsRoute};

/// Combined privacy disclosure (settings / HUD).
pub const PRELIMINARY_ROUTE_DISCLOSURE: &str = "Long-trip mode sends your origin, via points and destination coordinates to third-party routing services (BRouter and/or OpenRouteService) to estimate which map regions you need. The BRouter operator logs IP address, User-Agent and route coordinates for about two weeks (see brouter.de privacy policy). An OpenRouteService API key, when used, stays on your device settings and is never logged.";

/// Warning attached when a BRouter route includes ferry segments and ORS was
/// unavailable as a fallback.
pub const FERRY_WARNING: &str =
    "Preliminary route includes ferry segments; install ferry timetables or refine vias before relying on the corridor.";

// Network pacing lives in `super::pace_preliminary_network` (live HTTP only).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteProviderId {
    Brouter,
    Ors,
}

impl RouteProviderId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Brouter => "brouter",
            Self::Ors => "ors",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreliminaryRoute {
    pub lat_lon: Vec<(f64, f64)>,
    pub distance_m: f64,
    pub duration_s: Option<f64>,
    pub provider: RouteProviderId,
    pub warnings: Vec<String>,
    pub ferry_segments: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreliminaryError {
    /// Country-restricted trip without an ORS key, or ORS auth failure.
    NoApiKey {
        message: String,
    },
    RateLimited,
    Timeout,
    ServerError {
        status: u16,
    },
    InvalidResponse(String),
    NoRoute,
    Offline,
    RequestTooLarge {
        reason: String,
    },
}

impl std::fmt::Display for PreliminaryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoApiKey { message } => write!(f, "{message}"),
            Self::RateLimited => write!(f, "preliminary router rate limited"),
            Self::Timeout => write!(f, "preliminary router timed out"),
            Self::ServerError { status } => write!(f, "preliminary router HTTP {status}"),
            Self::InvalidResponse(d) => write!(f, "preliminary router invalid response: {d}"),
            Self::NoRoute => write!(f, "preliminary router found no route"),
            Self::Offline => write!(f, "preliminary router unreachable"),
            Self::RequestTooLarge { reason } => {
                write!(f, "preliminary request too large: {reason}")
            }
        }
    }
}

impl std::error::Error for PreliminaryError {}

impl From<BrouterError> for PreliminaryError {
    fn from(e: BrouterError) -> Self {
        match e {
            BrouterError::RateLimited => Self::RateLimited,
            BrouterError::Timeout => Self::Timeout,
            BrouterError::ServerError { status } => Self::ServerError { status },
            BrouterError::InvalidResponse(d) => Self::InvalidResponse(d),
            BrouterError::NoRoute => Self::NoRoute,
            BrouterError::Offline => Self::Offline,
        }
    }
}

impl From<OrsError> for PreliminaryError {
    fn from(e: OrsError) -> Self {
        match e {
            OrsError::NoApiKey => Self::NoApiKey {
                message: "OpenRouteService API key not set".into(),
            },
            OrsError::Offline => Self::Offline,
            OrsError::RateLimited => Self::RateLimited,
            OrsError::NoRoute => Self::NoRoute,
            OrsError::InvalidResponse(d) => Self::InvalidResponse(d),
            OrsError::RequestTooLarge { reason } => Self::RequestTooLarge { reason },
        }
    }
}

fn country_restricted(allowed: Option<&[String]>) -> bool {
    allowed
        .map(|a| a.iter().any(|c| !c.trim().is_empty()))
        .unwrap_or(false)
}

fn ors_key_present(cfg: Option<&OrsConfig>) -> bool {
    cfg.map(|c| !c.api_key.trim().is_empty()).unwrap_or(false)
}

fn cache_key(waypoints: &[(f64, f64)], allowed: Option<&[String]>) -> String {
    let mut s = String::new();
    for &(lat, lon) in waypoints {
        s.push_str(&format!("{lat:.6},{lon:.6};"));
    }
    s.push('|');
    if let Some(a) = allowed {
        let mut v: Vec<&str> = a
            .iter()
            .map(|c| c.trim())
            .filter(|c| !c.is_empty())
            .collect();
        v.sort_unstable();
        for c in v {
            s.push_str(c);
            s.push(',');
        }
    } else {
        s.push_str("none");
    }
    s
}

/// In-memory cache for preliminary routes (keyed by waypoints + allowed countries).
#[derive(Debug, Default)]
pub struct PreliminaryCache {
    routes: HashMap<String, PreliminaryRoute>,
}

impl PreliminaryCache {
    pub fn new() -> Self {
        Self::default()
    }

    fn get(&self, key: &str) -> Option<PreliminaryRoute> {
        self.routes.get(key).cloned()
    }

    fn insert(&mut self, key: String, route: PreliminaryRoute) {
        self.routes.insert(key, route);
    }
}

/// Trait for injectable BRouter (tests use fakes; production uses HTTP).
pub trait BrouterFetcher {
    fn fetch(&mut self, waypoints_lat_lon: &[(f64, f64)]) -> Result<BrouterRoute, BrouterError>;
}

/// Trait for injectable ORS.
pub trait OrsFetcher {
    fn fetch(
        &mut self,
        waypoints_lat_lon: &[(f64, f64)],
        allowed_countries: Option<&[String]>,
    ) -> Result<OrsRoute, OrsError>;
}

pub struct LiveBrouter(pub BrouterConfig);
impl BrouterFetcher for LiveBrouter {
    fn fetch(&mut self, waypoints_lat_lon: &[(f64, f64)]) -> Result<BrouterRoute, BrouterError> {
        request_brouter_route(&self.0, waypoints_lat_lon)
    }
}

pub struct LiveOrs(pub OrsConfig);
impl OrsFetcher for LiveOrs {
    fn fetch(
        &mut self,
        waypoints_lat_lon: &[(f64, f64)],
        allowed_countries: Option<&[String]>,
    ) -> Result<OrsRoute, OrsError> {
        request_directions(&self.0, waypoints_lat_lon, allowed_countries)
    }
}

fn from_brouter(r: BrouterRoute, warnings: Vec<String>) -> PreliminaryRoute {
    PreliminaryRoute {
        lat_lon: r.lat_lon,
        distance_m: r.distance_m,
        duration_s: r.duration_s,
        provider: RouteProviderId::Brouter,
        warnings,
        ferry_segments: r.ferry_segments,
    }
}

fn from_ors(r: OrsRoute) -> PreliminaryRoute {
    PreliminaryRoute {
        lat_lon: r.lat_lon,
        distance_m: r.distance_m,
        duration_s: None,
        provider: RouteProviderId::Ors,
        warnings: Vec::new(),
        ferry_segments: 0,
    }
}

fn brouter_triggers_ors_fallback(err: &BrouterError) -> bool {
    matches!(
        err,
        BrouterError::RateLimited
            | BrouterError::Timeout
            | BrouterError::ServerError { .. }
            | BrouterError::InvalidResponse(_)
    )
}

/// Core selection policy with injectable providers (unit-tested via fakes).
pub fn request_preliminary_with_fetchers<B: BrouterFetcher, O: OrsFetcher>(
    waypoints_lat_lon: &[(f64, f64)],
    allowed_countries: Option<&[String]>,
    brouter: &mut B,
    ors: &mut O,
    ors_key_set: bool,
    cache: &mut PreliminaryCache,
) -> Result<PreliminaryRoute, PreliminaryError> {
    let key = cache_key(waypoints_lat_lon, allowed_countries);
    if let Some(hit) = cache.get(&key) {
        return Ok(hit);
    }

    // 1. Country-restricted → ORS only (no ring geometry).
    if country_restricted(allowed_countries) {
        if !ors_key_set {
            return Err(PreliminaryError::NoApiKey {
                message: "Country-restricted trips need an OpenRouteService API key.".into(),
            });
        }
        let route = ors.fetch(waypoints_lat_lon, allowed_countries)?;
        let out = from_ors(route);
        cache.insert(key, out.clone());
        return Ok(out);
    }

    // 2. Unrestricted → BRouter first.
    match brouter.fetch(waypoints_lat_lon) {
        Ok(br) if br.ferry_segments == 0 => {
            let out = from_brouter(br, Vec::new());
            cache.insert(key, out.clone());
            Ok(out)
        }
        Ok(br) => {
            // Ferry segments → ORS if key, else warn and keep BRouter geometry.
            if ors_key_set {
                match ors.fetch(waypoints_lat_lon, None) {
                    Ok(or) => {
                        let out = from_ors(or);
                        cache.insert(key, out.clone());
                        Ok(out)
                    }
                    Err(_) => {
                        let out = from_brouter(br, vec![FERRY_WARNING.to_string()]);
                        cache.insert(key, out.clone());
                        Ok(out)
                    }
                }
            } else {
                let out = from_brouter(br, vec![FERRY_WARNING.to_string()]);
                cache.insert(key, out.clone());
                Ok(out)
            }
        }
        Err(e) if brouter_triggers_ors_fallback(&e) => {
            if ors_key_set {
                let route = ors.fetch(waypoints_lat_lon, None)?;
                let out = from_ors(route);
                cache.insert(key, out.clone());
                Ok(out)
            } else {
                Err(e.into())
            }
        }
        Err(e) => Err(e.into()),
    }
}

/// Production entry: live BRouter + ORS with a shared process cache.
fn global_cache() -> &'static Mutex<PreliminaryCache> {
    static CACHE: OnceLock<Mutex<PreliminaryCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(PreliminaryCache::new()))
}

/// Request a preliminary corridor. Never falls back to a straight line.
pub fn request_preliminary_route(
    waypoints_lat_lon: &[(f64, f64)],
    allowed_countries: Option<&[String]>,
    ors_cfg: Option<&OrsConfig>,
    brouter_cfg: &BrouterConfig,
) -> Result<PreliminaryRoute, PreliminaryError> {
    let mut brouter = LiveBrouter(brouter_cfg.clone());
    let key_set = ors_key_present(ors_cfg);
    let mut ors = match ors_cfg {
        Some(c) => LiveOrs(c.clone()),
        None => LiveOrs(OrsConfig::from_parts("", super::ors::DEFAULT_ORS_BASE_URL)),
    };
    let mut cache = global_cache().lock().unwrap_or_else(|e| e.into_inner());
    request_preliminary_with_fetchers(
        waypoints_lat_lon,
        allowed_countries,
        &mut brouter,
        &mut ors,
        key_set,
        &mut cache,
    )
}

/// Clear the process cache (tests).
pub fn clear_preliminary_cache() {
    if let Ok(mut c) = global_cache().lock() {
        *c = PreliminaryCache::new();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disclosure_mentions_third_parties_and_brouter_logging() {
        let d = PRELIMINARY_ROUTE_DISCLOSURE.to_ascii_lowercase();
        assert!(d.contains("third-party") || d.contains("third party"));
        assert!(d.contains("brouter"));
        assert!(d.contains("openrouteservice") || d.contains("open route"));
        assert!(d.contains("two weeks") || d.contains("2 weeks"));
    }

    #[test]
    fn no_straight_line_in_preliminary_module() {
        let src = include_str!("preliminary.rs");
        let prod = src.split("#[cfg(test)]").next().unwrap_or(src);
        assert!(!prod.contains("straight_line") && !prod.contains("chord_fallback"));
    }

    #[test]
    fn default_brouter_base_is_public() {
        assert_eq!(
            super::super::brouter::DEFAULT_BROUTER_BASE_URL,
            "https://brouter.de"
        );
    }
}
