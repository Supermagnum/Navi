//! Offline search over CoMaps MWMs (Phase 5).
//!
//! Wraps the `navi_comaps_search*` / `navi_comaps_reverse_geocode` C ABI
//! (`native/comaps_shim/navi_comaps.h`) and merges the results with Navi's own
//! POI catalogs (OSM POI index, CAT repeaters, ...) under a source label.
//! Navi storage is not touched: catalogs are read through [`NaviPoiSource`].
//!
//! Like [`super::comaps_router`], the native calls return
//! [`ComapsRouterError::Unavailable`] without the `comaps-native` feature so
//! host unit tests and the flag-off job stay green. With the MWM client flag
//! off the app keeps using the `place_index` FTS search in [`crate::search`].

use super::comaps_router::ComapsRouterError;
use crate::poi::{PoiCategory, PoiIndex};

/// Source label of every hit that comes from the CoMaps search engine.
pub const SOURCE_COMAPS: &str = "comaps";
/// Source label prefix for Navi-owned catalogs (`navi:poi`, `navi:cat_repeater`, ...).
pub const SOURCE_NAVI_PREFIX: &str = "navi:";

/// Two hits with the same normalized name closer than this are one place.
pub const DEDUPE_RADIUS_M: f64 = 100.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchHitKind {
    Feature,
    LatLon,
    Postcode,
}

/// One search result, from CoMaps or from a Navi catalog.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    pub name: String,
    pub address: String,
    /// Localized type label ("Fuel station"); empty when unknown.
    pub feature_type: String,
    pub lat: f64,
    pub lon: f64,
    /// Metres from the search position when one was set.
    pub distance_m: Option<f64>,
    pub kind: SearchHitKind,
    /// [`SOURCE_COMAPS`] or a `navi:*` label.
    pub source: String,
    /// Other sources that reported the same place (set by [`merge_hits`]).
    pub merged_sources: Vec<String>,
    /// CoMaps category key the hit was found through, when category search.
    pub category_key: Option<String>,
    pub feature_id: Option<u32>,
    pub mwm_name: Option<String>,
}

impl SearchHit {
    /// Navi-catalog hit with a source label.
    pub fn navi(source: &str, name: &str, lat: f64, lon: f64) -> Self {
        Self {
            name: name.to_string(),
            address: String::new(),
            feature_type: String::new(),
            lat,
            lon,
            distance_m: None,
            kind: SearchHitKind::Feature,
            source: format!("{SOURCE_NAVI_PREFIX}{source}"),
            merged_sources: Vec::new(),
            category_key: None,
            feature_id: None,
            mwm_name: None,
        }
    }
}

/// A streamed batch: only hits appended since the previous batch of the request.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchBatch {
    pub request_id: u64,
    pub hits: Vec<SearchHit>,
    pub is_final: bool,
    pub cancelled: bool,
}

/// Nearest address for a point (reverse geocode).
#[derive(Debug, Clone, PartialEq)]
pub struct ReverseAddress {
    pub house_number: String,
    pub street: String,
    /// "Street 12" or the street alone; never empty for a resolved address.
    pub formatted: String,
    pub country_id: String,
    pub distance_m: f64,
}

// --- Category mapping -------------------------------------------------------

/// How completely a CoMaps category covers a Navi [`PoiCategory`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CategoryCoverage {
    /// The CoMaps group contains the Navi category's POI types.
    Full,
    /// The group is a superset (e.g. hotels include huts and camp sites).
    Partial,
    /// No CoMaps group; only Navi's own POI catalog finds these.
    Gap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CategoryMapping {
    pub category: PoiCategory,
    /// CoMaps `@group` key usable with `navi_comaps_category_query`.
    pub comaps_key: Option<&'static str>,
    pub coverage: CategoryCoverage,
    pub note: &'static str,
}

/// Every [`PoiCategory`] variant (kept in sync by a unit test on `match`).
pub const ALL_POI_CATEGORIES: [PoiCategory; 11] = [
    PoiCategory::Water,
    PoiCategory::Cabin,
    PoiCategory::General,
    PoiCategory::NetworkHut,
    PoiCategory::Restroom,
    PoiCategory::OvernightFacility,
    PoiCategory::CraftBrewery,
    PoiCategory::TentSite,
    PoiCategory::Fishing,
    PoiCategory::RestArea,
    PoiCategory::Lodging,
];

/// CoMaps `DisplayedCategories` keys at the pinned CoMaps release
/// (`libs/search/displayed_categories.cpp`).
pub const COMAPS_DISPLAYED_CATEGORY_KEYS: [&str; 23] = [
    "category_eat",
    "category_hotel",
    "category_food",
    "category_tourism",
    "category_wifi",
    "category_transport",
    "category_fuel",
    "category_parking",
    "category_shopping",
    "category_secondhand",
    "category_atm",
    "category_nightlife",
    "category_children",
    "category_bank",
    "category_entertainment",
    "category_water",
    "category_hospital",
    "category_pharmacy",
    "category_recycling",
    "category_rv",
    "category_police",
    "category_toilet",
    "category_post",
];

/// Non-displayed `@group` keys in `categories.txt` that Navi also uses.
pub const COMAPS_EXTRA_CATEGORY_KEYS: [&str; 1] = ["charging_station"];

/// Map a Navi POI category to a CoMaps category key.
///
/// Verified against `data/categories.txt` at the pin: `tourism-alpine_hut`,
/// `tourism-wilderness_hut` and `tourism-camp_site` belong to
/// `@category_hotel`; springs and drinking water to `@category_water`;
/// toilets to `@category_toilet`.
pub fn map_poi_category(category: PoiCategory) -> CategoryMapping {
    use CategoryCoverage::{Full, Gap, Partial};
    let (comaps_key, coverage, note) = match category {
        PoiCategory::Water => (Some("category_water"), Full, "springs and drinking water"),
        PoiCategory::Cabin => (
            Some("category_hotel"),
            Partial,
            "alpine/wilderness huts share the hotel group with hotels and camp sites",
        ),
        PoiCategory::General => (None, Gap, "Navi general POI set has no CoMaps group"),
        PoiCategory::NetworkHut => (
            None,
            Gap,
            "DNT/network membership is not in MWM data; use Navi catalog",
        ),
        PoiCategory::Restroom => (Some("category_toilet"), Full, "amenity=toilets"),
        PoiCategory::OvernightFacility => (
            Some("category_hotel"),
            Partial,
            "huts, camp sites and hotels in one group",
        ),
        PoiCategory::CraftBrewery => (
            None,
            Gap,
            "craft=brewery has no group; free-text query only",
        ),
        PoiCategory::TentSite => (
            Some("category_hotel"),
            Partial,
            "camp_site is in the hotel group; caravan sites are category_rv",
        ),
        PoiCategory::Fishing => (None, Gap, "leisure=fishing is not indexed as a group"),
        PoiCategory::RestArea => (
            None,
            Gap,
            "highway=rest_area/services has no group; category_parking is only a partial stand-in",
        ),
        PoiCategory::Lodging => (Some("category_hotel"), Full, "hotel/motel/guest house"),
    };
    CategoryMapping {
        category,
        comaps_key,
        coverage,
        note,
    }
}

/// All mappings, for documentation and tests.
pub fn all_category_mappings() -> Vec<CategoryMapping> {
    ALL_POI_CATEGORIES
        .iter()
        .map(|c| map_poi_category(*c))
        .collect()
}

/// Navi quick-search chips that are not a [`PoiCategory`] (fuel, EV, ...).
pub fn navi_quick_category_key(name: &str) -> Option<&'static str> {
    match name.trim().to_ascii_lowercase().as_str() {
        "fuel" | "gas" | "drivstoff" => Some("category_fuel"),
        "ev" | "charging" | "charging_station" => Some("charging_station"),
        "water" | "vann" => Some("category_water"),
        "toilet" | "restroom" | "toalett" => Some("category_toilet"),
        "parking" | "parkering" => Some("category_parking"),
        "hotel" | "lodging" => Some("category_hotel"),
        "rv" | "caravan" | "bobil" => Some("category_rv"),
        "food" | "eat" => Some("category_eat"),
        "pharmacy" | "apotek" => Some("category_pharmacy"),
        "hospital" | "sykehus" => Some("category_hospital"),
        _ => None,
    }
}

/// True if `key` is a group CoMaps knows at the pin.
pub fn is_known_category_key(key: &str) -> bool {
    COMAPS_DISPLAYED_CATEGORY_KEYS.contains(&key) || COMAPS_EXTRA_CATEGORY_KEYS.contains(&key)
}

// --- Navi catalog sources and merge -----------------------------------------

/// What a Navi catalog is asked for.
#[derive(Debug, Clone)]
pub struct NaviSearchRequest<'a> {
    pub query: &'a str,
    pub category: Option<PoiCategory>,
    pub near: Option<(f64, f64)>,
    pub radius_m: f64,
    pub limit: usize,
}

/// A Navi-owned POI catalog searched alongside CoMaps. Implementations read
/// existing storage; they never write.
pub trait NaviPoiSource {
    /// Short label without the `navi:` prefix (`poi`, `cat_repeater`, ...).
    fn label(&self) -> &str;
    fn search(&self, request: &NaviSearchRequest<'_>) -> Vec<SearchHit>;
}

/// In-memory catalog (CAT repeaters, saved places, tests).
#[derive(Debug, Clone)]
pub struct StaticPoiSource {
    label: String,
    entries: Vec<SearchHit>,
}

impl StaticPoiSource {
    /// `entries` should be built with [`SearchHit::navi`] using the same label.
    pub fn new(label: &str, entries: Vec<SearchHit>) -> Self {
        Self {
            label: label.to_string(),
            entries,
        }
    }
}

impl NaviPoiSource for StaticPoiSource {
    fn label(&self) -> &str {
        &self.label
    }

    fn search(&self, request: &NaviSearchRequest<'_>) -> Vec<SearchHit> {
        let needle = normalize_name(request.query);
        let mut out: Vec<SearchHit> = self
            .entries
            .iter()
            .filter(|h| needle.is_empty() || normalize_name(&h.name).contains(&needle))
            .cloned()
            .map(|mut h| {
                h.distance_m = request
                    .near
                    .map(|(lat, lon)| haversine_m((lat, lon), (h.lat, h.lon)));
                h
            })
            .filter(|h| match (h.distance_m, request.near) {
                (Some(d), Some(_)) => d <= request.radius_m,
                _ => true,
            })
            .collect();
        sort_by_distance(&mut out);
        out.truncate(request.limit);
        out
    }
}

/// Adapter over the OSM POI index ([`PoiIndex`]); category and nearby queries.
pub struct PoiIndexSource<'a> {
    pub index: &'a PoiIndex,
}

impl NaviPoiSource for PoiIndexSource<'_> {
    fn label(&self) -> &str {
        "poi"
    }

    fn search(&self, request: &NaviSearchRequest<'_>) -> Vec<SearchHit> {
        let (Some(category), Some((lat, lon))) = (request.category, request.near) else {
            return Vec::new();
        };
        let mut out: Vec<SearchHit> = self
            .index
            .nearest(category, lat, lon, request.radius_m)
            .into_iter()
            .map(|r| {
                let mut h = SearchHit::navi(
                    "poi",
                    r.name.as_deref().unwrap_or(r.icon_key.as_str()),
                    r.lat,
                    r.lon,
                );
                h.feature_type = r.icon_key.clone();
                h.distance_m = Some(haversine_m((lat, lon), (r.lat, r.lon)));
                h
            })
            .collect();
        out.truncate(request.limit);
        out
    }
}

/// Query every Navi source and concatenate the hits (source order kept).
pub fn query_navi_sources(
    sources: &[&dyn NaviPoiSource],
    request: &NaviSearchRequest<'_>,
) -> Vec<SearchHit> {
    sources.iter().flat_map(|s| s.search(request)).collect()
}

/// How [`merge_hits`] orders the combined list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeMode {
    /// Free text: CoMaps ranking first; Navi-only exact-name matches are
    /// promoted to the front unless CoMaps already has an exact-name hit;
    /// other Navi-only hits follow by distance.
    Text,
    /// Category browse: everything ordered by distance when known.
    Category,
}

/// Merge CoMaps and Navi hits. A Navi hit with the same normalized name within
/// [`DEDUPE_RADIUS_M`] of an existing hit is folded into it (its source is
/// added to `merged_sources`); it is never dropped silently.
pub fn merge_hits(
    query: &str,
    mode: MergeMode,
    comaps: Vec<SearchHit>,
    navi: Vec<SearchHit>,
    limit: usize,
) -> Vec<SearchHit> {
    let mut primary = comaps;
    let mut navi_only: Vec<SearchHit> = Vec::new();
    for hit in navi {
        let key = normalize_name(&hit.name);
        let dup = primary.iter_mut().chain(navi_only.iter_mut()).find(|p| {
            normalize_name(&p.name) == key
                && haversine_m((p.lat, p.lon), (hit.lat, hit.lon)) <= DEDUPE_RADIUS_M
        });
        match dup {
            Some(existing) => {
                if existing.source != hit.source && !existing.merged_sources.contains(&hit.source) {
                    existing.merged_sources.push(hit.source.clone());
                }
                if existing.category_key.is_none() {
                    existing.category_key = hit.category_key.clone();
                }
            }
            None => navi_only.push(hit),
        }
    }

    let mut out = match mode {
        MergeMode::Text => {
            let q = normalize_name(query);
            // CoMaps already has an exact-name answer: keep its ranking first.
            let comaps_exact =
                !q.is_empty() && primary.iter().any(|h| normalize_name(&h.name) == q);
            let (mut exact, mut rest): (Vec<_>, Vec<_>) = navi_only
                .into_iter()
                .partition(|h| !comaps_exact && !q.is_empty() && normalize_name(&h.name) == q);
            sort_by_distance(&mut exact);
            sort_by_distance(&mut rest);
            let mut v = exact;
            v.extend(primary);
            v.extend(rest);
            v
        }
        MergeMode::Category => {
            let mut v = primary;
            v.extend(navi_only);
            sort_by_distance(&mut v);
            v
        }
    };
    out.truncate(limit);
    out
}

/// Lowercase alphanumerics only, so "Nørdre  Hytta" and "nørdre hytta" match.
pub fn normalize_name(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn sort_by_distance(hits: &mut [SearchHit]) {
    // Stable: unknown distances keep their relative order after known ones.
    hits.sort_by(|a, b| match (a.distance_m, b.distance_m) {
        (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
}

fn haversine_m(a: (f64, f64), b: (f64, f64)) -> f64 {
    let dlat = (b.0 - a.0).to_radians();
    let dlon = (b.1 - a.1).to_radians();
    let h = (dlat / 2.0).sin().powi(2)
        + a.0.to_radians().cos() * b.0.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    2.0 * 6_371_000.0 * h.sqrt().asin()
}

// --- Native session ----------------------------------------------------------

/// Stateless handle to the shim's search engine. The engine is opened lazily on
/// the first search, over the MWMs registered for routing.
pub struct ComapsSearch;

type BatchFn = Box<dyn Fn(SearchBatch) + Send + Sync + 'static>;

impl ComapsSearch {
    pub fn set_locale(locale: &str) {
        #[cfg(feature = "comaps-native")]
        ffi::set_locale(locale);
        #[cfg(not(feature = "comaps-native"))]
        let _ = locale;
    }

    pub fn set_viewport(min_lat: f64, min_lon: f64, max_lat: f64, max_lon: f64) {
        #[cfg(feature = "comaps-native")]
        ffi::set_viewport(min_lat, min_lon, max_lat, max_lon);
        #[cfg(not(feature = "comaps-native"))]
        let _ = (min_lat, min_lon, max_lat, max_lon);
    }

    pub fn clear_viewport() {
        #[cfg(feature = "comaps-native")]
        ffi::clear_viewport();
    }

    pub fn set_position(position: Option<(f64, f64)>) {
        #[cfg(feature = "comaps-native")]
        ffi::set_position(position);
        #[cfg(not(feature = "comaps-native"))]
        let _ = position;
    }

    /// Start an async search. `on_batch` runs on a search worker thread, once per
    /// batch of new hits and once more with `is_final`. Returns the request id.
    pub fn start(
        query: &str,
        is_category: bool,
        max_results: usize,
        on_batch: BatchFn,
    ) -> Result<u64, ComapsRouterError> {
        #[cfg(feature = "comaps-native")]
        {
            ffi::start(query, is_category, max_results, on_batch)
        }
        #[cfg(not(feature = "comaps-native"))]
        {
            let _ = (query, is_category, max_results, on_batch);
            Err(ComapsRouterError::Unavailable)
        }
    }

    /// Cancel one request, or every in-flight request when `request_id == 0`.
    pub fn cancel(request_id: u64) {
        #[cfg(feature = "comaps-native")]
        ffi::cancel(request_id);
        #[cfg(not(feature = "comaps-native"))]
        let _ = request_id;
    }

    /// CoMaps displayed category keys from the native engine.
    pub fn category_keys() -> Result<Vec<String>, ComapsRouterError> {
        #[cfg(feature = "comaps-native")]
        {
            ffi::category_keys()
        }
        #[cfg(not(feature = "comaps-native"))]
        {
            Err(ComapsRouterError::Unavailable)
        }
    }

    /// Localized query string for a category key (falls back to English).
    pub fn category_query(key: &str, locale: &str) -> Result<Option<String>, ComapsRouterError> {
        #[cfg(feature = "comaps-native")]
        {
            Ok(ffi::category_query(key, locale))
        }
        #[cfg(not(feature = "comaps-native"))]
        {
            let _ = (key, locale);
            Err(ComapsRouterError::Unavailable)
        }
    }

    pub fn reverse_geocode(
        lat: f64,
        lon: f64,
    ) -> Result<Option<ReverseAddress>, ComapsRouterError> {
        #[cfg(feature = "comaps-native")]
        {
            ffi::reverse_geocode(lat, lon)
        }
        #[cfg(not(feature = "comaps-native"))]
        {
            let _ = (lat, lon);
            Err(ComapsRouterError::Unavailable)
        }
    }
}

#[cfg(feature = "comaps-native")]
mod ffi {
    use super::*;
    use crate::mwm::comaps_router::{map_navi_comaps_code, ComapsRouterCode};
    use std::ffi::{c_void, CStr, CString};
    use std::os::raw::{c_char, c_int};

    #[link(name = "organicmaps")]
    extern "C" {
        fn navi_comaps_set_search_locale(locale: *const c_char);
        fn navi_comaps_set_search_viewport(min_lat: f64, min_lon: f64, max_lat: f64, max_lon: f64);
        fn navi_comaps_clear_search_viewport();
        fn navi_comaps_set_search_position(lat: f64, lon: f64, has_position: c_int);
        fn navi_comaps_search(
            query: *const c_char,
            is_category: c_int,
            max_results: usize,
            callback: Option<unsafe extern "C" fn(*mut NaviComapsSearchBatch, *mut c_void)>,
            user_data: *mut c_void,
        ) -> u64;
        fn navi_comaps_search_cancel(request_id: u64);
        fn navi_comaps_free_search_result(batch: *mut NaviComapsSearchBatch);
        fn navi_comaps_category_keys() -> *mut c_char;
        fn navi_comaps_category_query(key: *const c_char, locale: *const c_char) -> *mut c_char;
        fn navi_comaps_free_string(s: *mut c_char);
        fn navi_comaps_reverse_geocode(lat: f64, lon: f64, out: *mut NaviComapsAddress) -> c_int;
        fn navi_comaps_free_address(address: *mut NaviComapsAddress);
    }

    #[repr(C)]
    struct NaviComapsSearchResult {
        kind: c_int,
        lat: f64,
        lon: f64,
        distance_m: f64,
        feature_id: u32,
        name: *mut c_char,
        address: *mut c_char,
        feature_type: *mut c_char,
        mwm_name: *mut c_char,
    }

    #[repr(C)]
    struct NaviComapsSearchBatch {
        request_id: u64,
        results: *mut NaviComapsSearchResult,
        count: usize,
        is_final: c_int,
        cancelled: c_int,
    }

    #[repr(C)]
    struct NaviComapsAddress {
        code: c_int,
        house_number: *mut c_char,
        street: *mut c_char,
        formatted: *mut c_char,
        country_id: *mut c_char,
        distance_m: f64,
    }

    unsafe fn cstr(p: *const c_char) -> String {
        if p.is_null() {
            String::new()
        } else {
            CStr::from_ptr(p).to_string_lossy().into_owned()
        }
    }

    pub fn set_locale(locale: &str) {
        if let Ok(c) = CString::new(locale) {
            unsafe { navi_comaps_set_search_locale(c.as_ptr()) };
        }
    }

    pub fn set_viewport(min_lat: f64, min_lon: f64, max_lat: f64, max_lon: f64) {
        unsafe { navi_comaps_set_search_viewport(min_lat, min_lon, max_lat, max_lon) };
    }

    pub fn clear_viewport() {
        unsafe { navi_comaps_clear_search_viewport() };
    }

    pub fn set_position(position: Option<(f64, f64)>) {
        let (lat, lon, has) = match position {
            Some((lat, lon)) => (lat, lon, 1),
            None => (0.0, 0.0, 0),
        };
        unsafe { navi_comaps_set_search_position(lat, lon, has) };
    }

    unsafe fn convert_batch(batch: &NaviComapsSearchBatch) -> SearchBatch {
        let mut hits = Vec::with_capacity(batch.count);
        if !batch.results.is_null() {
            for r in std::slice::from_raw_parts(batch.results, batch.count) {
                let mwm = cstr(r.mwm_name);
                hits.push(SearchHit {
                    name: cstr(r.name),
                    address: cstr(r.address),
                    feature_type: cstr(r.feature_type),
                    lat: r.lat,
                    lon: r.lon,
                    distance_m: (r.distance_m >= 0.0).then_some(r.distance_m),
                    kind: match r.kind {
                        1 => SearchHitKind::LatLon,
                        2 => SearchHitKind::Postcode,
                        _ => SearchHitKind::Feature,
                    },
                    source: SOURCE_COMAPS.to_string(),
                    merged_sources: Vec::new(),
                    category_key: None,
                    feature_id: (r.kind == 0).then_some(r.feature_id),
                    mwm_name: (!mwm.is_empty()).then_some(mwm),
                });
            }
        }
        SearchBatch {
            request_id: batch.request_id,
            hits,
            is_final: batch.is_final != 0,
            cancelled: batch.cancelled != 0,
        }
    }

    /// Heap-owned closure the C callback receives as `user_data`.
    struct Listener {
        on_batch: BatchFn,
    }

    unsafe extern "C" fn trampoline(batch: *mut NaviComapsSearchBatch, user: *mut c_void) {
        if batch.is_null() || user.is_null() {
            return;
        }
        let converted = convert_batch(&*batch);
        let is_final = converted.is_final;
        let listener = &*(user as *const Listener);
        // A panic must not unwind into C++.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            (listener.on_batch)(converted)
        }));
        navi_comaps_free_search_result(batch);
        if is_final {
            // The final batch is the last call for this request: reclaim the listener.
            drop(Box::from_raw(user as *mut Listener));
        }
    }

    pub fn start(
        query: &str,
        is_category: bool,
        max_results: usize,
        on_batch: BatchFn,
    ) -> Result<u64, ComapsRouterError> {
        let q = CString::new(query).map_err(|_| ComapsRouterError::NullPointer)?;
        let listener = Box::into_raw(Box::new(Listener { on_batch }));
        let id = unsafe {
            navi_comaps_search(
                q.as_ptr(),
                is_category as c_int,
                max_results,
                Some(trampoline),
                listener as *mut c_void,
            )
        };
        if id == 0 {
            // Never queued: the callback will not run, so free the listener here.
            drop(unsafe { Box::from_raw(listener) });
            return Err(ComapsRouterError::Router(ComapsRouterCode::NotInit));
        }
        Ok(id)
    }

    pub fn cancel(request_id: u64) {
        unsafe { navi_comaps_search_cancel(request_id) };
    }

    pub fn category_keys() -> Result<Vec<String>, ComapsRouterError> {
        let p = unsafe { navi_comaps_category_keys() };
        if p.is_null() {
            return Err(ComapsRouterError::NullPointer);
        }
        let json = unsafe { cstr(p) };
        unsafe { navi_comaps_free_string(p) };
        serde_json::from_str::<Vec<String>>(&json)
            .map_err(|_| ComapsRouterError::Router(ComapsRouterCode::Internal))
    }

    pub fn category_query(key: &str, locale: &str) -> Option<String> {
        let k = CString::new(key).ok()?;
        let l = CString::new(locale).ok()?;
        let p = unsafe { navi_comaps_category_query(k.as_ptr(), l.as_ptr()) };
        if p.is_null() {
            return None;
        }
        let s = unsafe { cstr(p) };
        unsafe { navi_comaps_free_string(p) };
        Some(s)
    }

    pub fn reverse_geocode(
        lat: f64,
        lon: f64,
    ) -> Result<Option<ReverseAddress>, ComapsRouterError> {
        let mut out = NaviComapsAddress {
            code: 5,
            house_number: std::ptr::null_mut(),
            street: std::ptr::null_mut(),
            formatted: std::ptr::null_mut(),
            country_id: std::ptr::null_mut(),
            distance_m: -1.0,
        };
        unsafe { navi_comaps_reverse_geocode(lat, lon, &mut out) };
        let code = map_navi_comaps_code(out.code);
        let result = match code {
            ComapsRouterCode::Ok => Ok(Some(ReverseAddress {
                house_number: unsafe { cstr(out.house_number) },
                street: unsafe { cstr(out.street) },
                formatted: unsafe { cstr(out.formatted) },
                country_id: unsafe { cstr(out.country_id) },
                distance_m: out.distance_m,
            })),
            ComapsRouterCode::RouteNotFound => Ok(None),
            other => Err(ComapsRouterError::Router(other)),
        };
        unsafe { navi_comaps_free_address(&mut out) };
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::poi::PoiRecord;
    use std::collections::HashMap;

    fn comaps_hit(name: &str, lat: f64, lon: f64) -> SearchHit {
        SearchHit {
            name: name.into(),
            address: String::new(),
            feature_type: String::new(),
            lat,
            lon,
            distance_m: None,
            kind: SearchHitKind::Feature,
            source: SOURCE_COMAPS.into(),
            merged_sources: Vec::new(),
            category_key: None,
            feature_id: Some(1),
            mwm_name: Some("Norway_Oppland".into()),
        }
    }

    #[test]
    fn all_categories_are_listed_and_mapped() {
        // Exhaustive match: adding a PoiCategory variant fails to compile here
        // until the mapping table and ALL_POI_CATEGORIES are extended.
        for c in ALL_POI_CATEGORIES {
            match c {
                PoiCategory::Water
                | PoiCategory::Cabin
                | PoiCategory::General
                | PoiCategory::NetworkHut
                | PoiCategory::Restroom
                | PoiCategory::OvernightFacility
                | PoiCategory::CraftBrewery
                | PoiCategory::TentSite
                | PoiCategory::Fishing
                | PoiCategory::RestArea
                | PoiCategory::Lodging => {}
            }
        }
        let mut seen = std::collections::HashSet::new();
        for c in ALL_POI_CATEGORIES {
            assert!(seen.insert(c), "duplicate {c:?}");
        }
        assert_eq!(all_category_mappings().len(), ALL_POI_CATEGORIES.len());
    }

    #[test]
    fn mapped_keys_exist_in_comaps() {
        for m in all_category_mappings() {
            match (m.comaps_key, m.coverage) {
                (Some(key), CategoryCoverage::Full | CategoryCoverage::Partial) => {
                    assert!(is_known_category_key(key), "{:?} -> {key}", m.category)
                }
                (None, CategoryCoverage::Gap) => {}
                other => panic!("inconsistent mapping for {:?}: {other:?}", m.category),
            }
        }
    }

    #[test]
    fn expected_mappings() {
        assert_eq!(
            map_poi_category(PoiCategory::Water).comaps_key,
            Some("category_water")
        );
        assert_eq!(
            map_poi_category(PoiCategory::Restroom).comaps_key,
            Some("category_toilet")
        );
        assert_eq!(
            map_poi_category(PoiCategory::Lodging).coverage,
            CategoryCoverage::Full
        );
        for c in [
            PoiCategory::Cabin,
            PoiCategory::OvernightFacility,
            PoiCategory::TentSite,
        ] {
            let m = map_poi_category(c);
            assert_eq!(m.comaps_key, Some("category_hotel"));
            assert_eq!(m.coverage, CategoryCoverage::Partial);
        }
        for c in [
            PoiCategory::General,
            PoiCategory::NetworkHut,
            PoiCategory::CraftBrewery,
            PoiCategory::Fishing,
            PoiCategory::RestArea,
        ] {
            assert_eq!(map_poi_category(c).coverage, CategoryCoverage::Gap, "{c:?}");
        }
    }

    #[test]
    fn quick_categories() {
        assert_eq!(navi_quick_category_key("Fuel"), Some("category_fuel"));
        assert_eq!(navi_quick_category_key("ev"), Some("charging_station"));
        assert_eq!(navi_quick_category_key("drivstoff"), Some("category_fuel"));
        assert_eq!(navi_quick_category_key("unknown"), None);
        for name in ["fuel", "ev", "water", "toilet", "parking", "hotel", "rv"] {
            assert!(is_known_category_key(
                navi_quick_category_key(name).unwrap()
            ));
        }
    }

    #[test]
    fn normalize_handles_norwegian_letters() {
        assert_eq!(
            normalize_name("Nørdre  Hytta"),
            normalize_name("nørdre hytta")
        );
        assert_eq!(normalize_name("Ålesund"), "ålesund");
    }

    #[test]
    fn merge_folds_duplicate_navi_hit_into_comaps_hit() {
        let comaps = vec![comaps_hit("Rondvassbu", 61.8, 9.8)];
        let navi = vec![SearchHit::navi("poi", "Rondvassbu", 61.8003, 9.8002)];
        let merged = merge_hits("rondvassbu", MergeMode::Text, comaps, navi, 10);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].source, SOURCE_COMAPS);
        assert_eq!(merged[0].merged_sources, vec!["navi:poi".to_string()]);
    }

    #[test]
    fn merge_keeps_far_same_name_as_separate_place() {
        let comaps = vec![comaps_hit("Sjusjøen", 61.19, 10.76)];
        let navi = vec![SearchHit::navi("poi", "Sjusjøen", 61.5, 10.0)];
        let merged = merge_hits("sjusjøen", MergeMode::Text, comaps, navi, 10);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].source, SOURCE_COMAPS);
        assert_eq!(merged[1].source, "navi:poi");
    }

    #[test]
    fn merge_text_promotes_exact_navi_only_match() {
        let comaps = vec![comaps_hit("LA1ABC Gjerdet", 60.0, 10.0)];
        let navi = vec![
            SearchHit::navi("cat_repeater", "LA1ABC", 60.1, 10.1),
            SearchHit::navi("cat_repeater", "LA1ABC Syd", 60.2, 10.2),
        ];
        let merged = merge_hits("la1abc", MergeMode::Text, comaps, navi, 10);
        let names: Vec<&str> = merged.iter().map(|h| h.name.as_str()).collect();
        assert_eq!(names, vec!["LA1ABC", "LA1ABC Gjerdet", "LA1ABC Syd"]);
        assert_eq!(merged[0].source, "navi:cat_repeater");
    }

    #[test]
    fn merge_category_orders_by_distance_with_source_labels() {
        let mut a = comaps_hit("Circle K", 60.0, 10.0);
        a.distance_m = Some(900.0);
        let mut b = SearchHit::navi("poi", "Uno-X", 60.01, 10.0);
        b.distance_m = Some(300.0);
        let mut c = comaps_hit("Shell", 60.02, 10.0);
        c.distance_m = None;
        let merged = merge_hits("", MergeMode::Category, vec![a, c], vec![b], 10);
        let names: Vec<&str> = merged.iter().map(|h| h.name.as_str()).collect();
        assert_eq!(names, vec!["Uno-X", "Circle K", "Shell"]);
        assert_eq!(merged[0].source, "navi:poi");
        assert_eq!(merged[1].source, SOURCE_COMAPS);
    }

    #[test]
    fn merge_respects_limit() {
        let comaps: Vec<_> = (0..5)
            .map(|i| comaps_hit(&format!("P{i}"), 60.0 + i as f64, 10.0))
            .collect();
        assert_eq!(merge_hits("p", MergeMode::Text, comaps, vec![], 3).len(), 3);
    }

    #[test]
    fn static_source_filters_by_name_and_radius() {
        let src = StaticPoiSource::new(
            "cat_repeater",
            vec![
                SearchHit::navi("cat_repeater", "LA1ABC", 60.0, 10.0),
                SearchHit::navi("cat_repeater", "LA9XYZ", 60.0, 10.0),
                SearchHit::navi("cat_repeater", "LA1ABD", 70.0, 20.0),
            ],
        );
        let req = NaviSearchRequest {
            query: "la1",
            category: None,
            near: Some((60.0, 10.0)),
            radius_m: 50_000.0,
            limit: 10,
        };
        let hits = src.search(&req);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "LA1ABC");
        assert_eq!(hits[0].distance_m, Some(0.0));
    }

    #[test]
    fn poi_index_source_returns_labelled_category_hits() {
        let mut index = PoiIndex::new();
        index.insert_record(PoiRecord {
            osm_id: 1,
            lat: 61.0,
            lon: 9.0,
            categories: vec![PoiCategory::Cabin],
            icon_key: "alpine_hut".into(),
            tags: HashMap::new(),
            name: Some("Skålen".into()),
        });
        index.insert_record(PoiRecord {
            osm_id: 2,
            lat: 61.0,
            lon: 9.0,
            categories: vec![PoiCategory::Water],
            icon_key: "spring".into(),
            tags: HashMap::new(),
            name: Some("Kilde".into()),
        });
        let src = PoiIndexSource { index: &index };
        let req = NaviSearchRequest {
            query: "",
            category: Some(PoiCategory::Cabin),
            near: Some((61.0, 9.0)),
            radius_m: 10_000.0,
            limit: 10,
        };
        let hits = src.search(&req);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "Skålen");
        assert_eq!(hits[0].source, "navi:poi");
        // Without a pivot the index cannot answer.
        let none = NaviSearchRequest { near: None, ..req };
        assert!(src.search(&none).is_empty());
    }

    #[cfg(not(feature = "comaps-native"))]
    #[test]
    fn native_calls_unavailable_without_feature() {
        assert_eq!(
            ComapsSearch::start("x", false, 5, Box::new(|_| {})),
            Err(ComapsRouterError::Unavailable)
        );
        assert_eq!(
            ComapsSearch::reverse_geocode(60.0, 10.0),
            Err(ComapsRouterError::Unavailable)
        );
        assert_eq!(
            ComapsSearch::category_keys(),
            Err(ComapsRouterError::Unavailable)
        );
    }
}
