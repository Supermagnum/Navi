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
    /// Raw CoMaps turns JSON (enriched with turn/ferry fields).
    pub turns_json: String,
    /// Navi nb/en instruction JSON from [`super::comaps_instructions`].
    pub instructions_json: String,
    pub ascent_m: f64,
    pub descent_m: f64,
    /// CoMaps altitude samples JSON (`null` = invalid).
    pub elevation_json: String,
    /// Router mode diagnostic (`Joints` / `LeapsOnly` / …).
    pub router_mode: String,
    /// JSON int array, one entry per polyline segment (point `i` to `i + 1`): posted maxspeed
    /// in km/h from the MWM maxspeeds section, `-1` untagged / not applicable (foot, bike),
    /// `-2` explicitly unlimited.
    pub speed_limits_json: String,
}

/// Nearest routable-road projection for a point that is not on the profile's
/// road graph (`StartNotOnRoad` / `EndNotOnRoad`). Informational only: callers
/// must show it to the user and only route after an explicit confirmation.
#[derive(Debug, Clone, PartialEq)]
pub struct ComapsSnap {
    pub lat: f64,
    pub lon: f64,
    /// Great-circle metres from the query point to the projection.
    pub distance_m: f64,
    pub feature_id: u32,
    pub segment_idx: u32,
    /// MWM leaf holding the road (e.g. `Norway_Oppland`).
    pub mwm_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComapsRouterCode {
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

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ComapsRouterError {
    #[error("CoMaps native core not linked (build with comaps-native)")]
    Unavailable,
    #[error("CoMaps router: {0:?}")]
    Router(ComapsRouterCode),
    #[error("null pointer from CoMaps shim")]
    NullPointer,
}

/// Map Navi C ABI codes (`navi_comaps.h`). Unknown → RouteNotFound (never Internal).
pub fn map_navi_comaps_code(code: i32) -> ComapsRouterCode {
    match code {
        0 => ComapsRouterCode::Ok,
        1 => ComapsRouterCode::NotInit,
        2 => ComapsRouterCode::RouteNotFound,
        3 => ComapsRouterCode::NeedMoreMaps,
        4 => ComapsRouterCode::Cancelled,
        5 => ComapsRouterCode::Internal,
        6 => ComapsRouterCode::Unsupported,
        7 => ComapsRouterCode::StartNotOnRoad,
        8 => ComapsRouterCode::EndNotOnRoad,
        9 => ComapsRouterCode::RoadClosed,
        _ => ComapsRouterCode::RouteNotFound,
    }
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

    /// Pass vehicle dimensions into the car estimator (None clears).
    pub fn set_vehicle_limits(&self, limits: Option<&crate::config::VehicleLimits>) {
        #[cfg(feature = "comaps-native")]
        {
            ffi::set_vehicle_limits(limits);
        }
        #[cfg(not(feature = "comaps-native"))]
        {
            let _ = limits;
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

    /// Nearest routable edge for `profile` within `radius_m`. `Ok(None)` when no
    /// road lies inside the radius. Never called from [`Self::route`].
    pub fn snap_to_road(
        &self,
        profile: ComapsProfile,
        point: ComapsLatLon,
        radius_m: f64,
    ) -> Result<Option<ComapsSnap>, ComapsRouterError> {
        #[cfg(feature = "comaps-native")]
        {
            ffi::snap_to_road(profile, point, radius_m)
        }
        #[cfg(not(feature = "comaps-native"))]
        {
            let _ = (profile, point, radius_m);
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

/// While the drape Framework is alive it owns the Platform directories and the style; the lock makes
/// `ComapsRouter::init` rebuild only the router (re-register leaves) and leave Platform untouched.
pub fn set_platform_locked(locked: bool) {
    #[cfg(feature = "comaps-native")]
    {
        ffi::set_platform_locked(locked);
    }
    #[cfg(not(feature = "comaps-native"))]
    {
        let _ = locked;
    }
}

/// CoMaps leaf (country id) containing the point from packed_polygons / countries.txt.
/// `None` when the engine is not initialised; `Some("")` outside every country.
pub fn leaf_for_point(lat: f64, lon: f64) -> Option<String> {
    #[cfg(feature = "comaps-native")]
    {
        ffi::leaf_for_point(lat, lon)
    }
    #[cfg(not(feature = "comaps-native"))]
    {
        let _ = (lat, lon);
        None
    }
}

/// Leaf ids among `leaf_ids` whose loaded sidecar has no `vehicle_limits` section
/// (or whose sidecar is not loaded). Empty input ⇒ empty result. Blank ids skipped.
///
/// Without `comaps-native`, every non-blank id is reported missing (conservative).
pub fn leaves_missing_vehicle_limits(leaf_ids: &[String]) -> Vec<String> {
    #[cfg(feature = "comaps-native")]
    {
        ffi::leaves_missing_vehicle_limits(leaf_ids)
    }
    #[cfg(not(feature = "comaps-native"))]
    {
        leaf_ids
            .iter()
            .filter(|id| !id.is_empty())
            .cloned()
            .collect()
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
            fn_ptr: Option<unsafe extern "C" fn(*const c_char, u32, u32, c_int) -> u8>,
        );
        fn navi_comaps_set_platform_locked(locked: c_int);
        fn navi_comaps_leaf_for_point(lat: f64, lon: f64) -> *mut c_char;
        fn navi_comaps_free_string(s: *mut c_char);
        fn navi_comaps_set_elevation_lookup(
            fn_ptr: Option<unsafe extern "C" fn(*const c_char, u32, u32, *mut i32) -> c_int>,
        );
        fn navi_comaps_set_bike_wetland(enabled: c_int);
        fn navi_comaps_set_vehicle_limit_lookup(
            fn_ptr: Option<
                unsafe extern "C" fn(
                    *const c_char,
                    u32,
                    u32,
                    *mut u16,
                    *mut u16,
                    *mut u16,
                    *mut u16,
                    *mut u16,
                    *mut u16,
                ) -> c_int,
            >,
        );
        fn navi_comaps_set_vehicle(
            height_m: f64,
            width_m: f64,
            length_m: f64,
            total_weight_kg: f64,
            axle_weight_kg: f64,
            bogie_weight_kg: f64,
        );
        fn navi_comaps_clear_vehicle();
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
        fn navi_comaps_snap_to_road(
            profile: c_int,
            lat: f64,
            lon: f64,
            radius_m: f64,
            out: *mut NaviComapsSnapResult,
        ) -> c_int;
        fn navi_comaps_free_snap(result: *mut NaviComapsSnapResult);
        fn navi_comaps_shutdown();
    }

    #[repr(C)]
    struct NaviComapsSnapResult {
        code: c_int,
        proj_lat: f64,
        proj_lon: f64,
        distance_m: f64,
        feature_id: u32,
        segment_idx: u32,
        mwm_name: *mut c_char,
        error_message: *mut c_char,
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
        ascent_m: f64,
        descent_m: f64,
        elevation_json: *mut c_char,
        router_mode: *mut c_char,
        speed_limits_json: *mut c_char,
        vehicle_segments_json: *mut c_char,
    }

    /// Active vehicle dims for the post-route vehicle_limits guard (None = cleared).
    fn active_vehicle_limits() -> &'static Mutex<Option<crate::config::VehicleLimits>> {
        static LIMITS: OnceLock<Mutex<Option<crate::config::VehicleLimits>>> = OnceLock::new();
        LIMITS.get_or_init(|| Mutex::new(None))
    }

    pub fn set_platform_locked(locked: bool) {
        unsafe { navi_comaps_set_platform_locked(if locked { 1 } else { 0 }) };
    }

    /// Leaf ids that lack a loaded sidecar `vehicle_limits` section.
    pub fn leaves_missing_vehicle_limits(leaf_ids: &[String]) -> Vec<String> {
        let Ok(guard) = sidecars().lock() else {
            return leaf_ids
                .iter()
                .filter(|id| !id.is_empty())
                .cloned()
                .collect();
        };
        let mut missing = Vec::new();
        for id in leaf_ids {
            if id.is_empty() {
                continue;
            }
            match guard.iter().find(|(leaf, _)| leaf == id) {
                Some((_, reader)) if reader.has_vehicle_limits_section() => {}
                _ => missing.push(id.clone()),
            }
        }
        missing
    }

    /// CoMaps country (leaf) id containing the point (`Some("")` = outside every country).
    pub fn leaf_for_point(lat: f64, lon: f64) -> Option<String> {
        let p = unsafe { navi_comaps_leaf_for_point(lat, lon) };
        if p.is_null() {
            return None;
        }
        let s = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
        unsafe { navi_comaps_free_string(p) };
        Some(s)
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

    /// `(leaf_id, reader)` — leaf_id matches CoMaps country file name.
    static SIDECARS: OnceLock<Mutex<Vec<(String, SidecarReader)>>> = OnceLock::new();

    fn sidecars() -> &'static Mutex<Vec<(String, SidecarReader)>> {
        SIDECARS.get_or_init(|| Mutex::new(Vec::new()))
    }

    unsafe extern "C" fn wetland_lookup_cb(
        mwm_name: *const c_char,
        feature_id: u32,
        segment_idx: u32,
        forward: c_int,
    ) -> u8 {
        if mwm_name.is_null() {
            return 0;
        }
        let Ok(name) = CStr::from_ptr(mwm_name).to_str() else {
            return 0;
        };
        // Sidecar direction bit: 0 = forward, 1 = reverse (matches NVSC encoder).
        let direction: u8 = if forward != 0 { 0 } else { 1 };
        let seg = segment_idx.min(u16::MAX as u32) as u16;
        let Ok(guard) = sidecars().lock() else {
            return 0;
        };
        // Feature ids are per MWM: only the reader of the segment's own leaf may answer
        // (boardwalk exemption first).
        for (leaf, reader) in guard.iter() {
            if leaf != name {
                continue;
            }
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

    unsafe extern "C" fn elevation_lookup_cb(
        mwm_name: *const c_char,
        feature_id: u32,
        point_idx: u32,
        out_altitude_dm: *mut i32,
    ) -> c_int {
        if out_altitude_dm.is_null() || mwm_name.is_null() {
            return 0;
        }
        let Ok(name) = CStr::from_ptr(mwm_name).to_str() else {
            return 0;
        };
        let Ok(guard) = sidecars().lock() else {
            return 0;
        };
        let pt = point_idx.min(u16::MAX as u32) as u16;
        for (leaf, reader) in guard.iter() {
            if leaf != name {
                continue;
            }
            if let Ok(Some(dm)) = reader.elevation_dm(feature_id, pt) {
                *out_altitude_dm = dm;
                return 1;
            }
            return 0;
        }
        0
    }

    fn install_elevation_lookup() {
        unsafe { navi_comaps_set_elevation_lookup(Some(elevation_lookup_cb)) };
    }

    unsafe extern "C" fn vehicle_limit_lookup_cb(
        mwm_name: *const c_char,
        feature_id: u32,
        segment_idx: u32,
        max_height_cm: *mut u16,
        max_width_cm: *mut u16,
        max_length_cm: *mut u16,
        max_weight_10kg: *mut u16,
        max_axle_10kg: *mut u16,
        max_bogie_10kg: *mut u16,
    ) -> c_int {
        if mwm_name.is_null()
            || max_height_cm.is_null()
            || max_width_cm.is_null()
            || max_length_cm.is_null()
            || max_weight_10kg.is_null()
            || max_axle_10kg.is_null()
            || max_bogie_10kg.is_null()
        {
            return 0;
        }
        let Ok(name) = CStr::from_ptr(mwm_name).to_str() else {
            return 0;
        };
        let seg = segment_idx.min(u16::MAX as u32) as u16;
        let Ok(guard) = sidecars().lock() else {
            return 0;
        };
        for (leaf, reader) in guard.iter() {
            if leaf != name {
                continue;
            }
            if let Ok(Some(rec)) = reader.vehicle_limit(feature_id, seg) {
                *max_height_cm = rec.max_height_cm;
                *max_width_cm = rec.max_width_cm;
                *max_length_cm = rec.max_length_cm;
                *max_weight_10kg = rec.max_weight_10kg;
                *max_axle_10kg = rec.max_axleload_10kg;
                *max_bogie_10kg = rec.max_bogie_10kg;
                return 1;
            }
            return 0;
        }
        0
    }

    fn install_vehicle_limit_lookup() {
        unsafe { navi_comaps_set_vehicle_limit_lookup(Some(vehicle_limit_lookup_cb)) };
    }

    pub fn set_vehicle_limits(limits: Option<&crate::config::VehicleLimits>) {
        if let Ok(mut g) = active_vehicle_limits().lock() {
            *g = limits.cloned();
        }
        match limits {
            None => unsafe { navi_comaps_clear_vehicle() },
            Some(l) => {
                let or0 = |v: Option<f64>| v.filter(|x| x.is_finite() && *x > 0.0).unwrap_or(0.0);
                unsafe {
                    navi_comaps_set_vehicle(
                        or0(l.height_m),
                        or0(l.width_m),
                        or0(l.length_m),
                        or0(l.total_weight_kg),
                        or0(l.axle_weight_kg),
                        or0(l.bogie_weight_kg),
                    );
                }
            }
        }
    }

    fn map_code(code: c_int) -> ComapsRouterCode {
        super::map_navi_comaps_code(code)
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
            let leaf = name.trim_end_matches(".navi-sidecar.v1.zst").to_string();
            match SidecarReader::open_path(&p) {
                Ok(r) => loaded.push((leaf, r)),
                Err(_) => continue,
            }
        }
        let n = loaded.len();
        *sidecars()
            .lock()
            .map_err(|_| ComapsRouterError::NullPointer)? = loaded;
        // Elevation + vehicle limits are independent of the wetland cost hook.
        if n > 0 {
            install_elevation_lookup();
            install_vehicle_limit_lookup();
        }
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
            ascent_m: 0.0,
            descent_m: 0.0,
            elevation_json: ptr::null_mut(),
            router_mode: ptr::null_mut(),
            speed_limits_json: ptr::null_mut(),
            vehicle_segments_json: ptr::null_mut(),
        };
        unsafe {
            navi_comaps_route(profile_i, from.lat, from.lon, to.lat, to.lon, &mut out);
        }
        let mut code = map_code(out.code);
        let length_m = out.length_m;
        let time_s = out.time_s;
        let build_time_s = out.build_time_s;
        let ascent_m = out.ascent_m;
        let descent_m = out.descent_m;
        let mut points = Vec::new();
        if !out.points.is_null() && out.point_count > 0 {
            let slice = unsafe { std::slice::from_raw_parts(out.points, out.point_count) };
            points.extend(slice.iter().map(|p| ComapsLatLon {
                lat: p.lat,
                lon: p.lon,
            }));
        }
        let mut error_message = if out.error_message.is_null() {
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
        let elevation_json = if out.elevation_json.is_null() {
            "[]".to_string()
        } else {
            unsafe { CStr::from_ptr(out.elevation_json) }
                .to_string_lossy()
                .into_owned()
        };
        let router_mode = if out.router_mode.is_null() {
            "Unknown".to_string()
        } else {
            unsafe { CStr::from_ptr(out.router_mode) }
                .to_string_lossy()
                .into_owned()
        };
        let speed_limits_json = if out.speed_limits_json.is_null() {
            "[]".to_string()
        } else {
            unsafe { CStr::from_ptr(out.speed_limits_json) }
                .to_string_lossy()
                .into_owned()
        };
        let vehicle_segments_json = if out.vehicle_segments_json.is_null() {
            "[]".to_string()
        } else {
            unsafe { CStr::from_ptr(out.vehicle_segments_json) }
                .to_string_lossy()
                .into_owned()
        };
        unsafe { navi_comaps_free_route(&mut out) };

        // ×1e6 HardAvoid can still yield a finished route when the restricted
        // segment is the only corridor — reject it against vehicle_limits.
        if matches!(code, ComapsRouterCode::Ok) && matches!(profile, ComapsProfile::Car) {
            if let Ok(guard) = active_vehicle_limits().lock() {
                if let Some(ref limits) = *guard {
                    if let Ok(segs) = crate::mwm::vehicle_route_guard::parse_vehicle_segments_json(
                        &vehicle_segments_json,
                    ) {
                        let violation =
                            crate::mwm::vehicle_route_guard::find_route_vehicle_violation(
                                &segs,
                                limits,
                                |mwm, fid, seg| {
                                    let Ok(sc) = sidecars().lock() else {
                                        return None;
                                    };
                                    for (leaf, reader) in sc.iter() {
                                        if leaf != mwm {
                                            continue;
                                        }
                                        return reader.vehicle_limit(fid, seg).ok().flatten();
                                    }
                                    None
                                },
                            );
                        if let Some(v) = violation {
                            code = ComapsRouterCode::RouteNotFound;
                            error_message = Some(v.no_route_message());
                            points.clear();
                        }
                    }
                }
            }
        }

        let instructions_json = crate::mwm::comaps_instructions::enrich_turns_json(&turns_json)
            .unwrap_or_else(|_| "[]".to_string());
        Ok(ComapsRoute {
            code,
            length_m: if matches!(code, ComapsRouterCode::Ok) {
                length_m
            } else {
                0.0
            },
            time_s: if matches!(code, ComapsRouterCode::Ok) {
                time_s
            } else {
                0.0
            },
            build_time_s,
            points,
            error_message,
            turns_json,
            instructions_json,
            ascent_m,
            descent_m,
            elevation_json,
            router_mode,
            speed_limits_json,
        })
    }

    pub fn snap_to_road(
        profile: ComapsProfile,
        point: ComapsLatLon,
        radius_m: f64,
    ) -> Result<Option<ComapsSnap>, ComapsRouterError> {
        let profile_i = match profile {
            ComapsProfile::Car => 0,
            ComapsProfile::Pedestrian => 1,
            ComapsProfile::Bicycle => 2,
        };
        let mut out = NaviComapsSnapResult {
            code: 5,
            proj_lat: 0.0,
            proj_lon: 0.0,
            distance_m: 0.0,
            feature_id: 0,
            segment_idx: 0,
            mwm_name: ptr::null_mut(),
            error_message: ptr::null_mut(),
        };
        unsafe {
            navi_comaps_snap_to_road(profile_i, point.lat, point.lon, radius_m, &mut out);
        }
        let code = map_code(out.code);
        let result = match code {
            ComapsRouterCode::Ok => Ok(Some(ComapsSnap {
                lat: out.proj_lat,
                lon: out.proj_lon,
                distance_m: out.distance_m,
                feature_id: out.feature_id,
                segment_idx: out.segment_idx,
                mwm_name: if out.mwm_name.is_null() {
                    None
                } else {
                    Some(
                        unsafe { CStr::from_ptr(out.mwm_name) }
                            .to_string_lossy()
                            .into_owned(),
                    )
                },
            })),
            // No edge inside the radius is a normal outcome, not an error.
            ComapsRouterCode::RouteNotFound => Ok(None),
            other => Err(ComapsRouterError::Router(other)),
        };
        unsafe { navi_comaps_free_snap(&mut out) };
        result
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

    #[test]
    fn leaves_missing_vehicle_limits_without_native_is_conservative() {
        #[cfg(not(feature = "comaps-native"))]
        {
            let missing = leaves_missing_vehicle_limits(&[
                "Norway_Hedmark".into(),
                "".into(),
                "Norway_Oppland".into(),
            ]);
            assert_eq!(
                missing,
                vec!["Norway_Hedmark".to_string(), "Norway_Oppland".to_string()]
            );
        }
    }

    #[test]
    fn map_code_never_leaves_business_as_internal() {
        let cases = [
            (0, ComapsRouterCode::Ok),
            (1, ComapsRouterCode::NotInit),
            (2, ComapsRouterCode::RouteNotFound),
            (3, ComapsRouterCode::NeedMoreMaps),
            (4, ComapsRouterCode::Cancelled),
            (5, ComapsRouterCode::Internal),
            (6, ComapsRouterCode::Unsupported),
            (7, ComapsRouterCode::StartNotOnRoad),
            (8, ComapsRouterCode::EndNotOnRoad),
            (9, ComapsRouterCode::RoadClosed),
            (99, ComapsRouterCode::RouteNotFound),
        ];
        for (raw, expect) in cases {
            assert_eq!(map_navi_comaps_code(raw), expect);
        }
    }
}
