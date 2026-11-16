//! UniFFI surface for CoMaps routing (Phase 3).
//!
//! Without the `comaps-native` feature on `driver-break-core`, calls return a
//! clear "unavailable" error so CI stays green.

use std::sync::Mutex;
use std::time::Instant;

use std::sync::Arc;

use driver_break_core::mwm::comaps_search::{self, ComapsSearch};
use driver_break_core::mwm::{
    mwm_client_enabled, suggest_snap, ComapsLatLon, ComapsProfile, ComapsRouter, ComapsRouterCode,
    ComapsRouterError, MergeMode, SearchBatch, SearchHit, SearchHitKind, DEFAULT_SNAP_RADIUS_M,
};
use driver_break_core::poi::PoiCategory;

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

/// Arm CoMaps `navi_comaps_cancel`. This is a pre-flight latch: the next
/// [`comaps_session_route`] call consumes it and returns `Cancelled` before routing; a
/// route that is already running is **not** interrupted. Callers that cancel and then
/// start a new plan must not use it (the new plan would be cancelled instead).
#[uniffi::export]
pub fn comaps_session_cancel() {
    if let Ok(guard) = SESSION.lock() {
        if let Some(r) = guard.as_ref() {
            r.cancel();
        }
    }
}

/// True when [`comaps_session_open`] currently holds a live router.
#[uniffi::export]
pub fn comaps_session_is_open() -> bool {
    SESSION.lock().ok().map(|g| g.is_some()).unwrap_or(false)
}

/// When true, [`crate::plan_car_route`] / hiking must not run (MWM production
/// path is active). Set from Kotlin around MWM plans; cleared afterwards.
static MWM_PRODUCTION_PLANNER: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

#[uniffi::export]
pub fn set_mwm_production_planner_active(active: bool) {
    MWM_PRODUCTION_PLANNER.store(active, std::sync::atomic::Ordering::SeqCst);
}

#[uniffi::export]
pub fn mwm_production_planner_active() -> bool {
    MWM_PRODUCTION_PLANNER.load(std::sync::atomic::Ordering::SeqCst)
}

/// Call from `.rkyv` planners: logs + returns true when the MWM production
/// path forbids them (runtime assertion for flag-ON end-to-end).
pub(crate) fn assert_rkyv_planner_allowed(who: &str) -> bool {
    if !mwm_production_planner_active() {
        return true;
    }
    log::error!(
        target: "NaviPlan",
        "ASSERT: {who} called while MWM production planner is active (flag ON path must not use .rkyv)"
    );
    false
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

// --- Nearest-road suggestion (A2) ---------------------------------------------

/// Proposed replacement endpoint after `StartNotOnRoad` / `EndNotOnRoad`.
/// Informational: the app shows it and only routes after the user confirms.
#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiSnapSuggestion {
    /// False when there is nothing to offer (other route code, flag off, no
    /// road inside the radius, or the engine is not available).
    pub found: bool,
    /// `true` = the start failed to match a road, `false` = the destination.
    pub is_start: bool,
    pub original_lat: f64,
    pub original_lon: f64,
    pub snap_lat: f64,
    pub snap_lon: f64,
    pub distance_m: f64,
    pub feature_id: u32,
    pub segment_idx: u32,
    pub mwm_name: Option<String>,
    pub message: String,
    pub error_message: Option<String>,
}

fn no_snap(lat: f64, lon: f64, error: Option<String>) -> FfiSnapSuggestion {
    FfiSnapSuggestion {
        found: false,
        is_start: false,
        original_lat: lat,
        original_lon: lon,
        snap_lat: 0.0,
        snap_lon: 0.0,
        distance_m: 0.0,
        feature_id: 0,
        segment_idx: 0,
        mwm_name: None,
        message: String::new(),
        error_message: error,
    }
}

fn to_router_code(c: FfiComapsRouteCode) -> ComapsRouterCode {
    match c {
        FfiComapsRouteCode::Ok => ComapsRouterCode::Ok,
        FfiComapsRouteCode::NotInit => ComapsRouterCode::NotInit,
        FfiComapsRouteCode::RouteNotFound => ComapsRouterCode::RouteNotFound,
        FfiComapsRouteCode::NeedMoreMaps => ComapsRouterCode::NeedMoreMaps,
        FfiComapsRouteCode::Cancelled => ComapsRouterCode::Cancelled,
        FfiComapsRouteCode::Internal => ComapsRouterCode::Internal,
        FfiComapsRouteCode::Unsupported => ComapsRouterCode::Unsupported,
        FfiComapsRouteCode::Unavailable => ComapsRouterCode::Unavailable,
        FfiComapsRouteCode::StartNotOnRoad => ComapsRouterCode::StartNotOnRoad,
        FfiComapsRouteCode::EndNotOnRoad => ComapsRouterCode::EndNotOnRoad,
        FfiComapsRouteCode::RoadClosed => ComapsRouterCode::RoadClosed,
    }
}

/// Nearest drivable road for the endpoint that failed with `code`. Returns
/// `found = false` for every code except `StartNotOnRoad` / `EndNotOnRoad`.
/// Never called by [`comaps_session_route`]; the app calls it after a failure
/// and shows the result before routing anywhere.
#[uniffi::export]
pub fn comaps_session_snap_suggestion(
    profile: FfiComapsProfile,
    code: FfiComapsRouteCode,
    lat: f64,
    lon: f64,
    radius_m: f64,
) -> FfiSnapSuggestion {
    if !mwm_client_enabled() {
        return no_snap(lat, lon, Some("MWM client disabled".into()));
    }
    let Ok(guard) = SESSION.lock() else {
        return no_snap(lat, lon, Some("session mutex poisoned".into()));
    };
    let Some(router) = guard.as_ref() else {
        return no_snap(lat, lon, Some("call comaps_session_open first".into()));
    };
    let radius = if radius_m > 0.0 {
        radius_m
    } else {
        DEFAULT_SNAP_RADIUS_M
    };
    let point = ComapsLatLon { lat, lon };
    match suggest_snap(
        router,
        map_profile(profile),
        to_router_code(code),
        point,
        radius,
    ) {
        Ok(Some(s)) => FfiSnapSuggestion {
            found: true,
            is_start: s.role == driver_break_core::mwm::SnapRole::Start,
            original_lat: s.original.lat,
            original_lon: s.original.lon,
            snap_lat: s.snap.lat,
            snap_lon: s.snap.lon,
            distance_m: s.snap.distance_m,
            feature_id: s.snap.feature_id,
            segment_idx: s.snap.segment_idx,
            mwm_name: s.snap.mwm_name,
            message: s.message,
            error_message: None,
        },
        Ok(None) => no_snap(lat, lon, None),
        Err(e) => no_snap(lat, lon, Some(e.to_string())),
    }
}

// --- Offline search (Phase 5) -------------------------------------------------

#[derive(uniffi::Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum FfiSearchHitKind {
    Feature,
    LatLon,
    Postcode,
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiSearchHit {
    pub name: String,
    pub address: String,
    pub feature_type: String,
    pub lat: f64,
    pub lon: f64,
    /// Negative when no search position was set.
    pub distance_m: f64,
    pub kind: FfiSearchHitKind,
    /// `comaps` or a `navi:*` label.
    pub source: String,
    pub merged_sources: Vec<String>,
    pub category_key: Option<String>,
    pub feature_id: Option<u32>,
    pub mwm_name: Option<String>,
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiSearchBatch {
    pub request_id: u64,
    /// Hits appended since the previous batch of this request.
    pub hits: Vec<FfiSearchHit>,
    pub is_final: bool,
    pub cancelled: bool,
}

/// Receives streamed search batches. Called from a search worker thread;
/// implementations must not call back into the search functions synchronously.
#[uniffi::export(with_foreign)]
pub trait FfiSearchListener: Send + Sync {
    fn on_batch(&self, batch: FfiSearchBatch);
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiReverseAddress {
    pub found: bool,
    pub house_number: String,
    pub street: String,
    pub formatted: String,
    pub country_id: String,
    pub distance_m: f64,
    pub error_message: Option<String>,
}

#[derive(uniffi::Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum FfiCategoryCoverage {
    Full,
    Partial,
    Gap,
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiCategoryMapping {
    /// `PoiCategory` variant name (`Water`, `Cabin`, ...).
    pub navi_category: String,
    pub comaps_key: Option<String>,
    pub coverage: FfiCategoryCoverage,
    pub note: String,
}

fn kind_to_ffi(k: SearchHitKind) -> FfiSearchHitKind {
    match k {
        SearchHitKind::Feature => FfiSearchHitKind::Feature,
        SearchHitKind::LatLon => FfiSearchHitKind::LatLon,
        SearchHitKind::Postcode => FfiSearchHitKind::Postcode,
    }
}

fn kind_from_ffi(k: FfiSearchHitKind) -> SearchHitKind {
    match k {
        FfiSearchHitKind::Feature => SearchHitKind::Feature,
        FfiSearchHitKind::LatLon => SearchHitKind::LatLon,
        FfiSearchHitKind::Postcode => SearchHitKind::Postcode,
    }
}

fn hit_to_ffi(h: SearchHit) -> FfiSearchHit {
    FfiSearchHit {
        name: h.name,
        address: h.address,
        feature_type: h.feature_type,
        lat: h.lat,
        lon: h.lon,
        distance_m: h.distance_m.unwrap_or(-1.0),
        kind: kind_to_ffi(h.kind),
        source: h.source,
        merged_sources: h.merged_sources,
        category_key: h.category_key,
        feature_id: h.feature_id,
        mwm_name: h.mwm_name,
    }
}

fn hit_from_ffi(h: FfiSearchHit) -> SearchHit {
    SearchHit {
        name: h.name,
        address: h.address,
        feature_type: h.feature_type,
        lat: h.lat,
        lon: h.lon,
        distance_m: (h.distance_m >= 0.0).then_some(h.distance_m),
        kind: kind_from_ffi(h.kind),
        source: h.source,
        merged_sources: h.merged_sources,
        category_key: h.category_key,
        feature_id: h.feature_id,
        mwm_name: h.mwm_name,
    }
}

fn batch_to_ffi(b: SearchBatch) -> FfiSearchBatch {
    FfiSearchBatch {
        request_id: b.request_id,
        hits: b.hits.into_iter().map(hit_to_ffi).collect(),
        is_final: b.is_final,
        cancelled: b.cancelled,
    }
}

/// True when the app should use CoMaps search: the MWM client flag is on and
/// the native core is linked. Otherwise keep the `place_index` FTS search.
#[uniffi::export]
pub fn comaps_search_enabled() -> bool {
    cfg!(feature = "comaps-native") && mwm_client_enabled()
}

#[uniffi::export]
pub fn comaps_search_set_locale(locale: String) {
    if comaps_search_enabled() {
        ComapsSearch::set_locale(&locale);
    }
}

#[uniffi::export]
pub fn comaps_search_set_viewport(min_lat: f64, min_lon: f64, max_lat: f64, max_lon: f64) {
    if comaps_search_enabled() {
        ComapsSearch::set_viewport(min_lat, min_lon, max_lat, max_lon);
    }
}

#[uniffi::export]
pub fn comaps_search_clear_viewport() {
    if comaps_search_enabled() {
        ComapsSearch::clear_viewport();
    }
}

#[uniffi::export]
pub fn comaps_search_set_position(lat: f64, lon: f64, has_position: bool) {
    if comaps_search_enabled() {
        ComapsSearch::set_position(has_position.then_some((lat, lon)));
    }
}

/// Start an async, cancellable search; batches stream to `listener`. Returns a
/// request id, or 0 when search is unavailable (flag off, native core missing
/// or no session). `is_category` takes the string from [`comaps_category_query`].
#[uniffi::export]
pub fn comaps_search_start(
    query: String,
    is_category: bool,
    max_results: u32,
    listener: Arc<dyn FfiSearchListener>,
) -> u64 {
    if !comaps_search_enabled() {
        return 0;
    }
    ComapsSearch::start(
        &query,
        is_category,
        max_results as usize,
        Box::new(move |batch| listener.on_batch(batch_to_ffi(batch))),
    )
    .unwrap_or(0)
}

/// Cancel one request, or every in-flight request when `request_id` is 0.
#[uniffi::export]
pub fn comaps_search_cancel(request_id: u64) {
    if comaps_search_enabled() {
        ComapsSearch::cancel(request_id);
    }
}

/// CoMaps `DisplayedCategories` keys from the native engine (empty if unavailable).
#[uniffi::export]
pub fn comaps_category_keys() -> Vec<String> {
    if !comaps_search_enabled() {
        return Vec::new();
    }
    ComapsSearch::category_keys().unwrap_or_default()
}

/// Localized query string for a category key, falling back to English.
#[uniffi::export]
pub fn comaps_category_query(key: String, locale: String) -> Option<String> {
    if !comaps_search_enabled() {
        return None;
    }
    ComapsSearch::category_query(&key, &locale).ok().flatten()
}

/// Quick-chip name (`fuel`, `ev`, ...) to a CoMaps category key; pure logic.
#[uniffi::export]
pub fn comaps_quick_category_key(name: String) -> Option<String> {
    comaps_search::navi_quick_category_key(&name).map(str::to_string)
}

fn poi_category_name(c: PoiCategory) -> String {
    format!("{c:?}")
}

/// Navi `PoiCategory` to CoMaps category mapping with coverage; pure logic.
#[uniffi::export]
pub fn comaps_poi_category_mappings() -> Vec<FfiCategoryMapping> {
    comaps_search::all_category_mappings()
        .into_iter()
        .map(|m| FfiCategoryMapping {
            navi_category: poi_category_name(m.category),
            comaps_key: m.comaps_key.map(str::to_string),
            coverage: match m.coverage {
                comaps_search::CategoryCoverage::Full => FfiCategoryCoverage::Full,
                comaps_search::CategoryCoverage::Partial => FfiCategoryCoverage::Partial,
                comaps_search::CategoryCoverage::Gap => FfiCategoryCoverage::Gap,
            },
            note: m.note.to_string(),
        })
        .collect()
}

/// Merge CoMaps hits with Navi catalog hits (labelled `navi:*`) without
/// changing either storage. `category_mode` orders by distance instead of
/// CoMaps rank.
#[uniffi::export]
pub fn comaps_search_merge(
    query: String,
    category_mode: bool,
    comaps_hits: Vec<FfiSearchHit>,
    navi_hits: Vec<FfiSearchHit>,
    limit: u32,
) -> Vec<FfiSearchHit> {
    let mode = if category_mode {
        MergeMode::Category
    } else {
        MergeMode::Text
    };
    comaps_search::merge_hits(
        &query,
        mode,
        comaps_hits.into_iter().map(hit_from_ffi).collect(),
        navi_hits.into_iter().map(hit_from_ffi).collect(),
        limit as usize,
    )
    .into_iter()
    .map(hit_to_ffi)
    .collect()
}

/// Nearest address for a point from the registered MWMs.
#[uniffi::export]
pub fn comaps_reverse_geocode(lat: f64, lon: f64) -> FfiReverseAddress {
    let empty = |error: Option<String>| FfiReverseAddress {
        found: false,
        house_number: String::new(),
        street: String::new(),
        formatted: String::new(),
        country_id: String::new(),
        distance_m: -1.0,
        error_message: error,
    };
    if !comaps_search_enabled() {
        return empty(Some("MWM client disabled or native core missing".into()));
    }
    match ComapsSearch::reverse_geocode(lat, lon) {
        Ok(Some(a)) => FfiReverseAddress {
            found: true,
            house_number: a.house_number,
            street: a.street,
            formatted: a.formatted,
            country_id: a.country_id,
            distance_m: a.distance_m,
            error_message: None,
        },
        Ok(None) => empty(None),
        Err(e) => empty(Some(e.to_string())),
    }
}
