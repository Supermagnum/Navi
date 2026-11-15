//! Thin Rust facade over the CoMaps C ABI (`native/comaps_shim/navi_comaps.h`).
//!
//! When built without the `comaps-native` feature the methods return
//! [`ComapsRouterError::Unavailable`] so host unit tests and the flag-OFF CI
//! job stay green without linking `liborganicmaps.so`.

use thiserror::Error;

#[cfg(feature = "comaps-native")]
use super::sidecar::{SidecarReader, CLASS_HARD, CLASS_SOFT};
#[cfg(feature = "comaps-native")]
use std::sync::Mutex;

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
    /// JSON array from CoMaps `GetTurnsForDisplay` (may be `"[]"`).
    pub turns_json: String,
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

    /// Bike Soft/Hard wetland (default off). Foot always applies when hook on.
    pub fn set_bike_wetland(&self, enabled: bool) {
        #[cfg(feature = "comaps-native")]
        {
            ffi::set_bike_wetland(enabled);
        }
        #[cfg(not(feature = "comaps-native"))]
        {
            let _ = enabled;
        }
    }

    pub fn set_avoid_options(
        &self,
        avoid_toll: bool,
        avoid_motorway: bool,
        avoid_ferry: bool,
        avoid_dirty: bool,
    ) {
        #[cfg(feature = "comaps-native")]
        {
            ffi::set_avoid_options(avoid_toll, avoid_motorway, avoid_ferry, avoid_dirty);
        }
        #[cfg(not(feature = "comaps-native"))]
        {
            let _ = (avoid_toll, avoid_motorway, avoid_ferry, avoid_dirty);
        }
    }

    /// 0 = time, 1 = distance (unsupported), 2 = prefer non-motorway.
    pub fn set_optimize(&self, mode: i32) {
        #[cfg(feature = "comaps-native")]
        {
            ffi::set_optimize(mode);
        }
        #[cfg(not(feature = "comaps-native"))]
        {
            let _ = mode;
        }
    }

    pub fn set_turns_locale(&self, locale: &str) {
        #[cfg(feature = "comaps-native")]
        {
            ffi::set_turns_locale(locale);
        }
        #[cfg(not(feature = "comaps-native"))]
        {
            let _ = locale;
        }
    }

    /// Load every `*.navi-sidecar.v1.zst` under `dir` for wetland lookups.
    pub fn load_sidecars_from_dir(&self, dir: &str) -> Result<usize, ComapsRouterError> {
        #[cfg(feature = "comaps-native")]
        {
            ffi::load_sidecars_from_dir(dir)
        }
        #[cfg(not(feature = "comaps-native"))]
        {
            let _ = dir;
            Err(ComapsRouterError::Unavailable)
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
    use std::path::Path;
    use std::ptr;
    use std::sync::OnceLock;

    // Keep a live reference so `libnavi.so` emits `DT_NEEDED` for
    // `liborganicmaps.so` (same pattern as hamlib).
    #[link(name = "organicmaps")]
    extern "C" {
        fn navi_comaps_init(
            storage_path: *const c_char,
            writable_path: *const c_char,
            sidecar_dir: *const c_char,
        ) -> c_int;
        fn navi_comaps_register_mwms() -> c_int;
        fn navi_comaps_set_wetland_hook(enabled: c_int);
        fn navi_comaps_set_wetland_lookup(
            fn_ptr: Option<unsafe extern "C" fn(u32, u32, c_int) -> u8>,
        );
        fn navi_comaps_set_bike_wetland(enabled: c_int);
        fn navi_comaps_set_avoid_options(
            avoid_toll: c_int,
            avoid_motorway: c_int,
            avoid_ferry: c_int,
            avoid_dirty: c_int,
        );
        fn navi_comaps_set_optimize(optimize: c_int);
        fn navi_comaps_set_turns_locale(locale: *const c_char);
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
        turns_json: *mut c_char,
    }

    pub fn set_bike_wetland(enabled: bool) {
        unsafe { navi_comaps_set_bike_wetland(if enabled { 1 } else { 0 }) };
    }

    pub fn set_avoid_options(
        avoid_toll: bool,
        avoid_motorway: bool,
        avoid_ferry: bool,
        avoid_dirty: bool,
    ) {
        unsafe {
            navi_comaps_set_avoid_options(
                avoid_toll as c_int,
                avoid_motorway as c_int,
                avoid_ferry as c_int,
                avoid_dirty as c_int,
            )
        };
    }

    pub fn set_optimize(mode: i32) {
        unsafe { navi_comaps_set_optimize(mode) };
    }

    pub fn set_turns_locale(locale: &str) {
        if let Ok(c) = CString::new(locale) {
            unsafe { navi_comaps_set_turns_locale(c.as_ptr()) };
        }
    }

    static SIDECARS: OnceLock<Mutex<Vec<SidecarReader>>> = OnceLock::new();

    fn sidecars() -> &'static Mutex<Vec<SidecarReader>> {
        SIDECARS.get_or_init(|| Mutex::new(Vec::new()))
    }

    unsafe extern "C" fn wetland_lookup_cb(
        feature_id: u32,
        segment_idx: u32,
        forward: c_int,
    ) -> u8 {
        // Sidecar direction bit: 0 = forward, 1 = reverse (matches NVSC encoder).
        let direction: u8 = if forward != 0 { 0 } else { 1 };
        let seg = segment_idx.min(u16::MAX as u32) as u16;
        let Ok(guard) = sidecars().lock() else {
            return 0;
        };
        for reader in guard.iter() {
            if reader
                .is_boardwalk(feature_id, seg, direction)
                .unwrap_or(false)
            {
                return 0;
            }
            if let Ok(Some(class)) = reader.wetland_class(feature_id, seg, direction) {
                match class {
                    CLASS_SOFT => return 1,
                    CLASS_HARD => return 2,
                    _ => {}
                }
            }
        }
        0
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
        if enabled {
            unsafe {
                navi_comaps_set_wetland_hook(1);
                navi_comaps_set_wetland_lookup(Some(wetland_lookup_cb));
            }
        } else {
            unsafe {
                navi_comaps_set_wetland_hook(0);
                navi_comaps_set_wetland_lookup(None);
            }
        }
    }

    pub fn load_sidecars_from_dir(dir: &str) -> Result<usize, ComapsRouterError> {
        let path = Path::new(dir);
        if !path.is_dir() {
            return Err(ComapsRouterError::Router(ComapsRouterCode::Internal));
        }
        let mut loaded = Vec::new();
        let entries = std::fs::read_dir(path).map_err(|_| ComapsRouterError::NullPointer)?;
        for ent in entries.flatten() {
            let p = ent.path();
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if !name.ends_with(".navi-sidecar.v1.zst") {
                continue;
            }
            match SidecarReader::open_path(&p) {
                Ok(r) => loaded.push(r),
                Err(_) => continue,
            }
        }
        let n = loaded.len();
        *sidecars()
            .lock()
            .map_err(|_| ComapsRouterError::NullPointer)? = loaded;
        Ok(n)
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
            turns_json: ptr::null_mut(),
        };
        unsafe {
            navi_comaps_route(profile_i, from.lat, from.lon, to.lat, to.lon, &mut out);
        }
        let code = map_code(out.code);
        let length_m = out.length_m;
        let time_s = out.time_s;
        let build_time_s = out.build_time_s;
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
        let turns_json = if out.turns_json.is_null() {
            "[]".to_string()
        } else {
            unsafe { CStr::from_ptr(out.turns_json) }
                .to_string_lossy()
                .into_owned()
        };
        unsafe { navi_comaps_free_route(&mut out) };
        Ok(ComapsRoute {
            code,
            length_m,
            time_s,
            build_time_s,
            points,
            error_message,
            turns_json,
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
