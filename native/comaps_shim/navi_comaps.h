/* Thin C ABI over CoMaps RoutesBuilder for Navi UniFFI.
 * Linked into liborganicmaps.so (see patches/comaps).
 */
#pragma once

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef enum NaviComapsProfile {
  NAVI_COMAPS_PROFILE_CAR = 0,
  NAVI_COMAPS_PROFILE_PEDESTRIAN = 1,
  NAVI_COMAPS_PROFILE_BICYCLE = 2,
} NaviComapsProfile;

/** Time = CoMaps default (ETA/weight). Distance = not supported by RoutesBuilder
 *  (returns NAVI_COMAPS_UNSUPPORTED). PreferNonMotorway uses RoutingOptions::Motorway
 *  avoid as a practical E6-vs-Rv4 switch until a true distance weight exists. */
typedef enum NaviComapsOptimize {
  NAVI_COMAPS_OPTIMIZE_TIME = 0,
  NAVI_COMAPS_OPTIMIZE_DISTANCE = 1,
  NAVI_COMAPS_OPTIMIZE_PREFER_NON_MOTORWAY = 2,
} NaviComapsOptimize;

/**
 * Navi-facing route codes. CoMaps RouterResultCode values must never reach the
 * UI as a bare INTERNAL — map every business outcome here.
 */
typedef enum NaviComapsCode {
  NAVI_COMAPS_OK = 0,
  NAVI_COMAPS_NOT_INIT = 1,
  NAVI_COMAPS_ROUTE_NOT_FOUND = 2,
  NAVI_COMAPS_NEED_MORE_MAPS = 3,
  NAVI_COMAPS_CANCELLED = 4,
  /** Shim/exception only — never a mapped CoMaps business code. */
  NAVI_COMAPS_INTERNAL = 5,
  NAVI_COMAPS_UNSUPPORTED = 6,
  NAVI_COMAPS_START_NOT_ON_ROAD = 7,
  NAVI_COMAPS_END_NOT_ON_ROAD = 8,
  /** Seasonal / conditional / access closed (when distinguishable). */
  NAVI_COMAPS_ROAD_CLOSED = 9,
} NaviComapsCode;

typedef struct NaviComapsLatLon {
  double lat;
  double lon;
} NaviComapsLatLon;

typedef struct NaviComapsRouteResult {
  NaviComapsCode code;
  double length_m;
  double time_s;
  double build_time_s;
  /* Caller frees with navi_comaps_free_route. */
  NaviComapsLatLon * points;
  size_t point_count;
  char * error_message; /* UTF-8; may be NULL */
  /* JSON array of turn steps (enriched); may be NULL/"[]". */
  char * turns_json;
  /* Total ascent/descent metres (CoMaps altitudes; sidecar applied in Rust). */
  double ascent_m;
  double descent_m;
  /* JSON array of {lat,lon,ele_m,ele_source} along the polyline; may be NULL/"[]". */
  char * elevation_json;
  /* "Joints" | "Leaps" | "NoLeaps" | "Unknown" — diagnostic. */
  char * router_mode;
  /* JSON int array, one entry per polyline segment (i = segment between point i and i+1):
   * posted maxspeed km/h, -1 untagged / not applicable, -2 explicitly unlimited. May be NULL. */
  char * speed_limits_json;
  /* JSON array of {mwm,fid,seg,name,lat,lon} for vehicle_limits post-route guard. May be NULL. */
  char * vehicle_segments_json;
} NaviComapsRouteResult;

/* storage_path: directory containing CoMaps data/ + writable maps dir with .mwm.
 * sidecar_dir: directory of {Leaf}.navi-sidecar.v1.zst (sets NAVI_WETLAND_SIDECAR_DIR).
 * Returns NAVI_COMAPS_OK on success. */
NaviComapsCode navi_comaps_init(char const * storage_path, char const * writable_path,
                                char const * sidecar_dir);

/* While the drape Framework is alive it owns the Platform resource / writable / settings
 * directories and the map style. With the lock set, navi_comaps_init() only rebuilds the
 * RoutesBuilder (re-registers MWMs) and leaves Platform and style untouched, so a leaf
 * download can re-init the router without moving the resource dir under a live drape. */
void navi_comaps_set_platform_locked(int locked);

/* CoMaps country (leaf) id containing the point according to packed_polygons / countries.txt
 * (e.g. "Norway_Oppland"), or "" when the point is outside every country (sea). Caller frees
 * with navi_comaps_free_string. NULL on error / not initialised. */
char * navi_comaps_leaf_for_point(double lat, double lon);

/* Register/refresh MWMs under writable_path (Phase 1 layout). */
NaviComapsCode navi_comaps_register_mwms(void);

/* Enable/disable wetland Soft×5 / Hard≈forbid (env gate + loader). */
void navi_comaps_set_wetland_hook(int enabled);
/* Install Rust SidecarReader-backed lookup; pass NULL to clear. */
/** mwm_name: CoMaps leaf id of the segment (feature ids are per MWM). */
typedef uint8_t (*NaviComapsWetlandLookupFn)(char const * mwm_name, uint32_t feature_id, uint32_t segment_idx,
                                             int forward);
void navi_comaps_set_wetland_lookup(NaviComapsWetlandLookupFn fn);
/** EL02 elevation override for one MWM leaf (`mwm_name` e.g. "Norway_Oppland").
 *  Return 1 and write absolute altitude (dm) when present. */
typedef int (*NaviComapsElevLookupFn)(char const * mwm_name, uint32_t feature_id, uint32_t point_idx,
                                      int32_t * out_altitude_dm);
void navi_comaps_set_elevation_lookup(NaviComapsElevLookupFn fn);

/** Bike wetland Soft/Hard (default off). Foot/hike always apply when hook on. */
void navi_comaps_set_bike_wetland(int enabled);

/** Vehicle dimension caps from NVSC `vehicle_limits` (car estimator ×1e6 HardAvoid). */
typedef int (*NaviComapsVehicleLimitLookupFn)(char const * mwm_name, uint32_t feature_id,
                                              uint32_t segment_idx, uint16_t * max_height_cm,
                                              uint16_t * max_width_cm, uint16_t * max_length_cm,
                                              uint16_t * max_weight_10kg, uint16_t * max_axle_10kg,
                                              uint16_t * max_bogie_10kg);
void navi_comaps_set_vehicle_limit_lookup(NaviComapsVehicleLimitLookupFn fn);
/** Metres / kilograms. Pass 0 (or negative) to leave a dimension unused. */
void navi_comaps_set_vehicle(double height_m, double width_m, double length_m,
                             double total_weight_kg, double axle_weight_kg, double bogie_weight_kg);
void navi_comaps_clear_vehicle(void);

/** Avoid flags written into CoMaps RoutingOptions settings before each route. */
void navi_comaps_set_avoid_options(int avoid_toll, int avoid_motorway, int avoid_ferry,
                                   int avoid_dirty);

void navi_comaps_set_optimize(NaviComapsOptimize optimize);

/** Locale for GetTurnsForDisplay (e.g. "nb", "en"). Empty/NULL → "en". */
void navi_comaps_set_turns_locale(char const * locale);

/* Synchronous route. Runs off the caller's thread; cancel via navi_comaps_cancel. */
NaviComapsCode navi_comaps_route(NaviComapsProfile profile, double from_lat, double from_lon,
                                 double to_lat, double to_lon, NaviComapsRouteResult * out);

void navi_comaps_cancel(void);
void navi_comaps_free_route(NaviComapsRouteResult * result);

/* ---- Nearest drivable road (END/START_NOT_ON_ROAD choice) ------------------
 * Never called from navi_comaps_route(): the client shows the snap point and its
 * distance and only routes to it after an explicit user confirmation. */
typedef struct NaviComapsSnapResult {
  /** NAVI_COMAPS_OK when a road was found; NAVI_COMAPS_ROUTE_NOT_FOUND when no
   *  routable edge lies within radius_m; NOT_INIT / NEED_MORE_MAPS otherwise. */
  NaviComapsCode code;
  double proj_lat;
  double proj_lon;
  /** Great-circle distance from the query point to the projection. */
  double distance_m;
  /** CoMaps feature index inside the leaf MWM, and the segment on that feature. */
  uint32_t feature_id;
  uint32_t segment_idx;
  /** MWM leaf name (e.g. "Norway_Oppland"); may be NULL. */
  char * mwm_name;
  char * error_message; /* UTF-8; may be NULL */
} NaviComapsSnapResult;

/* Nearest edge of `profile` within radius_m of (lat, lon). Caller frees with
 * navi_comaps_free_snap. */
NaviComapsCode navi_comaps_snap_to_road(NaviComapsProfile profile, double lat, double lon,
                                        double radius_m, NaviComapsSnapResult * out);
void navi_comaps_free_snap(NaviComapsSnapResult * result);

/* ---- Offline search (search::Engine over the registered MWMs) -------------
 * The shim opens its own FrozenDataSource on the same writable MWM directory
 * as RoutesBuilder (mmap-backed; RoutesBuilder's single pooled data source is
 * borrowed per route and cannot be shared with a long-lived search engine). */

typedef enum NaviComapsSearchKind {
  NAVI_COMAPS_SEARCH_FEATURE = 0,
  NAVI_COMAPS_SEARCH_LATLON = 1,
  NAVI_COMAPS_SEARCH_POSTCODE = 2,
} NaviComapsSearchKind;

typedef struct NaviComapsSearchResult {
  NaviComapsSearchKind kind;
  double lat;
  double lon;
  /** Metres from the search position, or -1 when no position was set. */
  double distance_m;
  uint32_t feature_id;
  char * name;          /* UTF-8; never NULL */
  char * address;       /* UTF-8; may be empty */
  char * feature_type;  /* localized type label; may be empty */
  char * mwm_name;      /* MWM leaf name; may be empty */
} NaviComapsSearchResult;

typedef struct NaviComapsSearchBatch {
  uint64_t request_id;
  /** Only the results appended since the previous batch of this request. */
  NaviComapsSearchResult * results;
  size_t count;
  /** 1 on the last batch of a request (results may be empty). */
  int is_final;
  /** 1 when the request ended because of navi_comaps_search_cancel / timeout. */
  int cancelled;
} NaviComapsSearchBatch;

/* Ownership of `batch` passes to the callback, which must release it with
 * navi_comaps_free_search_result. Called from a search worker thread. */
typedef void (*NaviComapsSearchCallback)(NaviComapsSearchBatch * batch, void * user_data);

/* Locale for ranking/categories (e.g. "nb", "en"). Empty/NULL -> "en". */
void navi_comaps_set_search_locale(char const * locale);
/* Viewport rect used as the search pivot area. Without one, a box around the
 * search position (or the whole world) is used. When a viewport is set the
 * engine runs in Viewport mode; clear it for Everywhere (To-field) search. */
void navi_comaps_set_search_viewport(double min_lat, double min_lon, double max_lat, double max_lon);
void navi_comaps_clear_search_viewport(void);
/* has_position = 0 clears the position. */
void navi_comaps_set_search_position(double lat, double lon, int has_position);

/* Starts an async search; returns a request id (> 0) or 0 on failure. Batches are
 * delivered to `callback`. is_category != 0 runs a pure category query (use the
 * string from navi_comaps_category_query). */
uint64_t navi_comaps_search(char const * query, int is_category, size_t max_results,
                            NaviComapsSearchCallback callback, void * user_data);
/* Cancels one request, or all in-flight requests when request_id == 0. */
void navi_comaps_search_cancel(uint64_t request_id);
/* Frees a batch delivered to the callback (results array and all strings). */
void navi_comaps_free_search_result(NaviComapsSearchBatch * batch);

/* JSON array of CoMaps DisplayedCategories keys (e.g. "category_fuel"). Caller frees
 * with navi_comaps_free_string. */
char * navi_comaps_category_keys(void);
/* Localized search string for a category key in `locale` (falls back to English).
 * Returns NULL when the key is unknown. Caller frees with navi_comaps_free_string. */
char * navi_comaps_category_query(char const * key, char const * locale);
void navi_comaps_free_string(char * s);

typedef struct NaviComapsAddress {
  /** NAVI_COMAPS_OK when anything was resolved; ROUTE_NOT_FOUND when nothing nearby. */
  NaviComapsCode code;
  char * house_number;  /* may be empty */
  char * street;        /* may be empty */
  char * formatted;     /* "street house" or best-effort label; may be empty */
  /** MWM leaf (country) containing the point; may be empty. */
  char * country_id;
  /** Metres from the query point to the matched building or street. */
  double distance_m;
} NaviComapsAddress;

NaviComapsCode navi_comaps_reverse_geocode(double lat, double lon, NaviComapsAddress * out);
void navi_comaps_free_address(NaviComapsAddress * address);

void navi_comaps_shutdown(void);

#ifdef __cplusplus
}
#endif
