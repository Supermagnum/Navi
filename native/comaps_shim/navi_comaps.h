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

typedef enum NaviComapsCode {
  NAVI_COMAPS_OK = 0,
  NAVI_COMAPS_NOT_INIT = 1,
  NAVI_COMAPS_ROUTE_NOT_FOUND = 2,
  NAVI_COMAPS_NEED_MORE_MAPS = 3,
  NAVI_COMAPS_CANCELLED = 4,
  NAVI_COMAPS_INTERNAL = 5,
  NAVI_COMAPS_UNSUPPORTED = 6,
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
  /* JSON array of turn steps from Route::GetTurnsForDisplay; may be NULL/"[]". */
  char * turns_json;
} NaviComapsRouteResult;

/* storage_path: directory containing CoMaps data/ + writable maps dir with .mwm.
 * sidecar_dir: directory of {Leaf}.navi-sidecar.v1.zst (sets NAVI_WETLAND_SIDECAR_DIR).
 * Returns NAVI_COMAPS_OK on success. */
NaviComapsCode navi_comaps_init(char const * storage_path, char const * writable_path,
                                char const * sidecar_dir);

/* Register/refresh MWMs under writable_path (Phase 1 layout). */
NaviComapsCode navi_comaps_register_mwms(void);

/* Enable/disable wetland Soft×5 / Hard≈forbid (env gate + loader). */
void navi_comaps_set_wetland_hook(int enabled);
/* Install Rust SidecarReader-backed lookup; pass NULL to clear. */
typedef uint8_t (*NaviComapsWetlandLookupFn)(uint32_t feature_id, uint32_t segment_idx, int forward);
void navi_comaps_set_wetland_lookup(NaviComapsWetlandLookupFn fn);

/** Bike wetland Soft/Hard (default off). Foot/hike always apply when hook on. */
void navi_comaps_set_bike_wetland(int enabled);

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
void navi_comaps_shutdown(void);

#ifdef __cplusplus
}
#endif
