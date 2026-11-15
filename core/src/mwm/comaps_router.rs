//! Thin Rust facade over the CoMaps C ABI (`native/comaps_shim/navi_comaps.h`).
//!
//! When built without the `comaps-native` feature the methods return
//! [`ComapsRouterError::Unavailable`] so host unit tests and the flag-OFF CI
//! job stay green without linking `liborganicmaps.so`.

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComapsProfile {
    Car,
    Pedestrian,
    Bicycle,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComapsLatLon {
    pub lat: f64,
    pub lon: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComapsRoute {
    pub code: ComapsRouterCode,
    pub length_m: f64,
    pub time_s: f64,
    pub build_time_s: f64,
    pub points: Vec<ComapsLatLon>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComapsRouterCode {
    Ok,
    NotInit,
    RouteNotFound,
    NeedMoreMaps,
    Cancelled,
    Internal,
    Unsupported,
    Unavailable,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ComapsRouterError {
    #[error("CoMaps native core not linked (build with comaps-native)")]
    Unavailable,
    #[error("CoMaps router: {0:?}")]
    Router(ComapsRouterCode),
    #[error("null pointer from CoMaps shim")]
    NullPointer,
}

/// Handle to an initialized CoMaps routing session.
#[derive(Debug)]
pub struct ComapsRouter {
    _private: (),
}

impl ComapsRouter {
    pub fn init(
        storage_path: &str,
        writable_path: &str,
        sidecar_dir: Option<&str>,
    ) -> Result<Self, ComapsRouterError> {
        #[cfg(feature = "comaps-native")]
        {
            ffi::init(storage_path, writable_path, sidecar_dir)?;
            Ok(Self { _private: () })
        }
        #[cfg(not(feature = "comaps-native"))]
        {
            let _ = (storage_path, writable_path, sidecar_dir);
            Err(ComapsRouterError::Unavailable)
        }
    }

    pub fn register_mwms(&self) -> Result<(), ComapsRouterError> {
        #[cfg(feature = "comaps-native")]
        {
            ffi::register_mwms()
        }
        #[cfg(not(feature = "comaps-native"))]
        {
            Err(ComapsRouterError::Unavailable)
        }
    }

    pub fn set_wetland_hook(&self, enabled: bool) {
        #[cfg(feature = "comaps-native")]
        {
            ffi::set_wetland_hook(enabled);
        }
        #[cfg(not(feature = "comaps-native"))]
        {
            let _ = enabled;
        }
    }

    pub fn route(
        &self,
        profile: ComapsProfile,
        from: ComapsLatLon,
        to: ComapsLatLon,
    ) -> Result<ComapsRoute, ComapsRouterError> {
        #[cfg(feature = "comaps-native")]
        {
            ffi::route(profile, from, to)
        }
        #[cfg(not(feature = "comaps-native"))]
        {
            let _ = (profile, from, to);
            Err(ComapsRouterError::Unavailable)
        }
    }

    pub fn cancel(&self) {
        #[cfg(feature = "comaps-native")]
        {
            ffi::cancel();
        }
    }
}

impl Drop for ComapsRouter {
    fn drop(&mut self) {
        #[cfg(feature = "comaps-native")]
        {
            ffi::shutdown();
        }
    }
}

#[cfg(feature = "comaps-native")]
mod ffi {
    use super::*;
    use std::ffi::{CStr, CString};
    use std::os::raw::{c_char, c_int};
    use std::ptr;

    #[repr(C)]
    struct NaviComapsLatLon {
        lat: f64,
        lon: f64,
    }

    #[repr(C)]
    struct NaviComapsRouteResult {
        code: c_int,
        length_m: f64,
        time_s: f64,
        build_time_s: f64,
        points: *mut NaviComapsLatLon,
        point_count: usize,
        error_message: *mut c_char,
    }

    extern "C" {
        fn navi_comaps_init(
            storage_path: *const c_char,
            writable_path: *const c_char,
            sidecar_dir: *const c_char,
        ) -> c_int;
        fn navi_comaps_register_mwms() -> c_int;
        fn navi_comaps_set_wetland_hook(enabled: c_int);
        fn navi_comaps_route(
            profile: c_int,
            from_lat: f64,
            from_lon: f64,
            to_lat: f64,
            to_lon: f64,
            out: *mut NaviComapsRouteResult,
        ) -> c_int;
        fn navi_comaps_cancel();
        fn navi_comaps_free_route(result: *mut NaviComapsRouteResult);
        fn navi_comaps_shutdown();
    }

    fn map_code(code: c_int) -> ComapsRouterCode {
        match code {
            0 => ComapsRouterCode::Ok,
            1 => ComapsRouterCode::NotInit,
            2 => ComapsRouterCode::RouteNotFound,
            3 => ComapsRouterCode::NeedMoreMaps,
            4 => ComapsRouterCode::Cancelled,
            6 => ComapsRouterCode::Unsupported,
            _ => ComapsRouterCode::Internal,
        }
    }

    pub fn init(
        storage_path: &str,
        writable_path: &str,
        sidecar_dir: Option<&str>,
    ) -> Result<(), ComapsRouterError> {
        let storage = CString::new(storage_path).map_err(|_| ComapsRouterError::NullPointer)?;
        let writable = CString::new(writable_path).map_err(|_| ComapsRouterError::NullPointer)?;
        let side = sidecar_dir
            .map(CString::new)
            .transpose()
            .map_err(|_| ComapsRouterError::NullPointer)?;
        let side_ptr = side.as_ref().map(|s| s.as_ptr()).unwrap_or(ptr::null());
        let code = unsafe { navi_comaps_init(storage.as_ptr(), writable.as_ptr(), side_ptr) };
        let mapped = map_code(code);
        if mapped == ComapsRouterCode::Ok {
            Ok(())
        } else {
            Err(ComapsRouterError::Router(mapped))
        }
    }

    pub fn register_mwms() -> Result<(), ComapsRouterError> {
        let code = unsafe { navi_comaps_register_mwms() };
        let mapped = map_code(code);
        if mapped == ComapsRouterCode::Ok {
            Ok(())
        } else {
            Err(ComapsRouterError::Router(mapped))
        }
    }

    pub fn set_wetland_hook(enabled: bool) {
        unsafe { navi_comaps_set_wetland_hook(if enabled { 1 } else { 0 }) }
    }

    pub fn route(
        profile: ComapsProfile,
        from: ComapsLatLon,
        to: ComapsLatLon,
    ) -> Result<ComapsRoute, ComapsRouterError> {
        let profile_i = match profile {
            ComapsProfile::Car => 0,
            ComapsProfile::Pedestrian => 1,
            ComapsProfile::Bicycle => 2,
        };
        let mut out = NaviComapsRouteResult {
            code: 5,
            length_m: 0.0,
            time_s: 0.0,
            build_time_s: 0.0,
            points: ptr::null_mut(),
            point_count: 0,
            error_message: ptr::null_mut(),
        };
        unsafe {
            navi_comaps_route(profile_i, from.lat, from.lon, to.lat, to.lon, &mut out);
        }
        let code = map_code(out.code);
        let mut points = Vec::new();
        if !out.points.is_null() && out.point_count > 0 {
            let slice = unsafe { std::slice::from_raw_parts(out.points, out.point_count) };
            points.extend(slice.iter().map(|p| ComapsLatLon {
                lat: p.lat,
                lon: p.lon,
            }));
        }
        let error_message = if out.error_message.is_null() {
            None
        } else {
            Some(
                unsafe { CStr::from_ptr(out.error_message) }
                    .to_string_lossy()
                    .into_owned(),
            )
        };
        unsafe { navi_comaps_free_route(&mut out) };
        Ok(ComapsRoute {
            code,
            length_m: out.length_m,
            time_s: out.time_s,
            build_time_s: out.build_time_s,
            points,
            error_message,
        })
    }

    pub fn cancel() {
        unsafe { navi_comaps_cancel() }
    }

    pub fn shutdown() {
        unsafe { navi_comaps_shutdown() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_without_native_feature() {
        #[cfg(not(feature = "comaps-native"))]
        {
            let err = ComapsRouter::init("/tmp", "/tmp", None).unwrap_err();
            assert!(matches!(err, ComapsRouterError::Unavailable));
        }
    }
}
