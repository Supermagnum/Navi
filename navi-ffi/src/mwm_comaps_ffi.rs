//! UniFFI surface for CoMaps routing (Phase 3).
//!
//! Without the `comaps-native` feature on `driver-break-core`, calls return a
//! clear "unavailable" error so CI stays green.

use driver_break_core::mwm::{
    ComapsLatLon, ComapsProfile, ComapsRouter, ComapsRouterCode, ComapsRouterError,
};

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
    Internal,
    Unsupported,
    Unavailable,
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiComapsLatLon {
    pub lat: f64,
    pub lon: f64,
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiComapsRoute {
    pub code: FfiComapsRouteCode,
    pub length_m: f64,
    pub time_s: f64,
    pub build_time_s: f64,
    pub points: Vec<FfiComapsLatLon>,
    pub error_message: Option<String>,
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
    }
}

/// Probe whether the CoMaps native core is linked into this build.
#[uniffi::export]
pub fn comaps_native_available() -> bool {
    !matches!(
        ComapsRouter::init("/tmp", "/tmp", None),
        Err(ComapsRouterError::Unavailable)
    )
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
            return FfiComapsRoute {
                code: FfiComapsRouteCode::Unavailable,
                length_m: 0.0,
                time_s: 0.0,
                build_time_s: 0.0,
                points: vec![],
                error_message: Some("comaps-native feature not linked".into()),
            };
        }
        Err(e) => {
            return FfiComapsRoute {
                code: FfiComapsRouteCode::Internal,
                length_m: 0.0,
                time_s: 0.0,
                build_time_s: 0.0,
                points: vec![],
                error_message: Some(e.to_string()),
            };
        }
    };
    let _ = router.register_mwms();
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
        Ok(r) => FfiComapsRoute {
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
        },
        Err(e) => FfiComapsRoute {
            code: FfiComapsRouteCode::Internal,
            length_m: 0.0,
            time_s: 0.0,
            build_time_s: 0.0,
            points: vec![],
            error_message: Some(e.to_string()),
        },
    }
}
