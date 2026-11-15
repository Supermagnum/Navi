//! UniFFI surface for CoMaps routing (Phase 3).
//!
//! Without the `comaps-native` feature on `driver-break-core`, calls return a
//! clear "unavailable" error so CI stays green.

use std::sync::Mutex;
use std::time::Instant;

use driver_break_core::mwm::{
    ComapsLatLon, ComapsProfile, ComapsRouter, ComapsRouterCode, ComapsRouterError,
};

static SESSION: Mutex<Option<ComapsRouter>> = Mutex::new(None);

#[derive(uniffi::Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum FfiComapsProfile {
    Car,
    Pedestrian,
    Bicycle,
}

#[derive(uniffi::Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum FfiComapsRouteCode {
    Ok,
    NotInit,
    RouteNotFound,
    NeedMoreMaps,
    Cancelled,
    /// Shim/exception only — never a mapped CoMaps business code.
    Internal,
    Unsupported,
    Unavailable,
    StartNotOnRoad,
    EndNotOnRoad,
    RoadClosed,
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiComapsLatLon {
    pub lat: f64,
    pub lon: f64,
}

#[derive(uniffi::Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum FfiComapsOptimize {
    /// CoMaps default time/ETA weight.
    Time,
    /// Not supported by CoMaps RoutesBuilder (returns Unsupported).
    Distance,
    /// Avoid motorways via RoutingOptions (E6 vs Rv4-style switch).
    PreferNonMotorway,
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiComapsRoute {
    pub code: FfiComapsRouteCode,
    pub length_m: f64,
    pub time_s: f64,
    pub build_time_s: f64,
    pub points: Vec<FfiComapsLatLon>,
    pub error_message: Option<String>,
    pub turns_json: String,
    pub instructions_json: String,
    pub ascent_m: f64,
    pub descent_m: f64,
    pub elevation_json: String,
    pub router_mode: String,
}

fn empty_route(code: FfiComapsRouteCode, msg: impl Into<String>) -> FfiComapsRoute {
    FfiComapsRoute {
        code,
        length_m: 0.0,
        time_s: 0.0,
        build_time_s: 0.0,
        points: vec![],
        error_message: Some(msg.into()),
        turns_json: "[]".into(),
        instructions_json: "[]".into(),
        ascent_m: 0.0,
        descent_m: 0.0,
        elevation_json: "[]".into(),
        router_mode: "Unknown".into(),
    }
}

fn from_comaps_route(r: driver_break_core::mwm::ComapsRoute) -> FfiComapsRoute {
    FfiComapsRoute {
        code: map_code(r.code),
        length_m: r.length_m,
        time_s: r.time_s,
        build_time_s: r.build_time_s,
        points: r
            .points
            .into_iter()
            .map(|p| FfiComapsLatLon {
                lat: p.lat,
                lon: p.lon,
            })
            .collect(),
        error_message: r.error_message,
        turns_json: r.turns_json,
        instructions_json: r.instructions_json,
        ascent_m: r.ascent_m,
        descent_m: r.descent_m,
        elevation_json: r.elevation_json,
        router_mode: r.router_mode,
    }
}

fn map_profile(p: FfiComapsProfile) -> ComapsProfile {
    match p {
        FfiComapsProfile::Car => ComapsProfile::Car,
        FfiComapsProfile::Pedestrian => ComapsProfile::Pedestrian,
        FfiComapsProfile::Bicycle => ComapsProfile::Bicycle,
    }
}

fn map_code(c: ComapsRouterCode) -> FfiComapsRouteCode {
    match c {
        ComapsRouterCode::Ok => FfiComapsRouteCode::Ok,
        ComapsRouterCode::NotInit => FfiComapsRouteCode::NotInit,
        ComapsRouterCode::RouteNotFound => FfiComapsRouteCode::RouteNotFound,
        ComapsRouterCode::NeedMoreMaps => FfiComapsRouteCode::NeedMoreMaps,
        ComapsRouterCode::Cancelled => FfiComapsRouteCode::Cancelled,
        ComapsRouterCode::Internal => FfiComapsRouteCode::Internal,
        ComapsRouterCode::Unsupported => FfiComapsRouteCode::Unsupported,
        ComapsRouterCode::Unavailable => FfiComapsRouteCode::Unavailable,
        ComapsRouterCode::StartNotOnRoad => FfiComapsRouteCode::StartNotOnRoad,
        ComapsRouterCode::EndNotOnRoad => FfiComapsRouteCode::EndNotOnRoad,
        ComapsRouterCode::RoadClosed => FfiComapsRouteCode::RoadClosed,
    }
}

/// Probe whether the CoMaps native core is linked into this build.
#[uniffi::export]
pub fn comaps_native_available() -> bool {
    cfg!(feature = "comaps-native")
}

/// One-shot CoMaps route for device smoke tests (runs on a background caller).
#[uniffi::export]
pub fn comaps_route_once(
    storage_path: String,
    writable_path: String,
    sidecar_dir: Option<String>,
    profile: FfiComapsProfile,
    from_lat: f64,
    from_lon: f64,
    to_lat: f64,
    to_lon: f64,
    wetland_hook: bool,
) -> FfiComapsRoute {
    let router = match ComapsRouter::init(&storage_path, &writable_path, sidecar_dir.as_deref()) {
        Ok(r) => r,
        Err(ComapsRouterError::Unavailable) => {
            return empty_route(
                FfiComapsRouteCode::Unavailable,
                "comaps-native feature not linked",
            );
        }
        Err(e) => {
            return empty_route(FfiComapsRouteCode::Internal, e.to_string());
        }
    };
    let _ = router.register_mwms();
    // Sidecars feed wetland costing and EL02 elevation overrides.
    if let Some(dir) = sidecar_dir.as_deref() {
        let _ = router.load_sidecars_from_dir(dir);
    }
    router.set_wetland_hook(wetland_hook);
    match router.route(
        map_profile(profile),
        ComapsLatLon {
            lat: from_lat,
            lon: from_lon,
        },
        ComapsLatLon {
            lat: to_lat,
            lon: to_lon,
        },
    ) {
        Ok(r) => from_comaps_route(r),
        Err(e) => empty_route(FfiComapsRouteCode::Internal, e.to_string()),
    }
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiComapsSessionOpen {
    pub ok: bool,
    pub init_ms: f64,
    pub register_ms: f64,
    pub sidecars_loaded: u32,
    pub error_message: Option<String>,
}

/// Open a persistent CoMaps session (cold init + MWM register). Call
/// [`comaps_session_route`] then [`comaps_session_close`].
#[uniffi::export]
pub fn comaps_session_open(
    storage_path: String,
    writable_path: String,
    sidecar_dir: Option<String>,
    load_wetland_sidecars: bool,
) -> FfiComapsSessionOpen {
    let t0 = Instant::now();
    let router = match ComapsRouter::init(&storage_path, &writable_path, sidecar_dir.as_deref()) {
        Ok(r) => r,
        Err(ComapsRouterError::Unavailable) => {
            return FfiComapsSessionOpen {
                ok: false,
                init_ms: 0.0,
                register_ms: 0.0,
                sidecars_loaded: 0,
                error_message: Some("comaps-native feature not linked".into()),
            };
        }
        Err(e) => {
            return FfiComapsSessionOpen {
                ok: false,
                init_ms: t0.elapsed().as_secs_f64() * 1000.0,
                register_ms: 0.0,
                sidecars_loaded: 0,
                error_message: Some(e.to_string()),
            };
        }
    };
    let init_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let t1 = Instant::now();
    if let Err(e) = router.register_mwms() {
        return FfiComapsSessionOpen {
            ok: false,
            init_ms,
            register_ms: t1.elapsed().as_secs_f64() * 1000.0,
            sidecars_loaded: 0,
            error_message: Some(e.to_string()),
        };
    }
    let register_ms = t1.elapsed().as_secs_f64() * 1000.0;
    let mut sidecars_loaded = 0u32;
    if load_wetland_sidecars {
        if let Some(dir) = sidecar_dir.as_deref() {
            if let Ok(n) = router.load_sidecars_from_dir(dir) {
                sidecars_loaded = n as u32;
            }
        }
    }
    match SESSION.lock() {
        Ok(mut g) => {
            *g = Some(router);
            FfiComapsSessionOpen {
                ok: true,
                init_ms,
                register_ms,
                sidecars_loaded,
                error_message: None,
            }
        }
        Err(_) => FfiComapsSessionOpen {
            ok: false,
            init_ms,
            register_ms,
            sidecars_loaded,
            error_message: Some("session mutex poisoned".into()),
        },
    }
}

#[uniffi::export]
pub fn comaps_session_route(
    profile: FfiComapsProfile,
    from_lat: f64,
    from_lon: f64,
    to_lat: f64,
    to_lon: f64,
    wetland_hook: bool,
) -> FfiComapsRoute {
    let Ok(guard) = SESSION.lock() else {
        return empty_route(FfiComapsRouteCode::Internal, "session mutex poisoned");
    };
    let Some(router) = guard.as_ref() else {
        return empty_route(
            FfiComapsRouteCode::NotInit,
            "call comaps_session_open first",
        );
    };
    router.set_wetland_hook(wetland_hook);
    match router.route(
        map_profile(profile),
        ComapsLatLon {
            lat: from_lat,
            lon: from_lon,
        },
        ComapsLatLon {
            lat: to_lat,
            lon: to_lon,
        },
    ) {
        Ok(r) => from_comaps_route(r),
        Err(e) => empty_route(FfiComapsRouteCode::Internal, e.to_string()),
    }
}

#[uniffi::export]
pub fn comaps_session_close() {
    if let Ok(mut g) = SESSION.lock() {
        *g = None;
    }
}

#[uniffi::export]
pub fn comaps_session_set_bike_wetland(enabled: bool) {
    if let Ok(guard) = SESSION.lock() {
        if let Some(r) = guard.as_ref() {
            r.set_bike_wetland(enabled);
        }
    }
}

#[uniffi::export]
pub fn comaps_session_set_avoid_options(
    avoid_toll: bool,
    avoid_motorway: bool,
    avoid_ferry: bool,
    avoid_dirty: bool,
) {
    if let Ok(guard) = SESSION.lock() {
        if let Some(r) = guard.as_ref() {
            r.set_avoid_options(avoid_toll, avoid_motorway, avoid_ferry, avoid_dirty);
        }
    }
}

#[uniffi::export]
pub fn comaps_session_set_optimize(mode: FfiComapsOptimize) {
    let m = match mode {
        FfiComapsOptimize::Time => 0,
        FfiComapsOptimize::Distance => 1,
        FfiComapsOptimize::PreferNonMotorway => 2,
    };
    if let Ok(guard) = SESSION.lock() {
        if let Some(r) = guard.as_ref() {
            r.set_optimize(m);
        }
    }
}

#[uniffi::export]
pub fn comaps_session_set_turns_locale(locale: String) {
    if let Ok(guard) = SESSION.lock() {
        if let Some(r) = guard.as_ref() {
            r.set_turns_locale(&locale);
        }
    }
}
