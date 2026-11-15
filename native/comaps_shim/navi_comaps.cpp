// Navi thin routing shim over CoMaps RoutesBuilder (headless; no GLES).
#include "navi_comaps.h"

#include "routing/routes_builder/routes_builder.hpp"
#include "routing/navi_wetland_cost.hpp"
#include "routing/routing_options.hpp"

#include "indexer/classificator_loader.hpp"
#include "indexer/map_style.hpp"
#include "indexer/map_style_reader.hpp"

#include "platform/platform.hpp"
#include "platform/settings.hpp"

#include "app/organicmaps/sdk/core/jni_helper.hpp"

#include "geometry/latlon.hpp"

#include "base/logging.hpp"

#include <atomic>
#include <cstdlib>
#include <cstring>
#include <memory>
#include <mutex>
#include <string>

#ifdef __ANDROID__
#include <android/log.h>
#endif

namespace
{
std::mutex g_mu;
bool g_inited = false;
std::unique_ptr<routing::routes_builder::RoutesBuilder> g_builder;
std::atomic<bool> g_cancel{false};
std::atomic<int> g_avoidToll{0};
std::atomic<int> g_avoidMotorway{0};
std::atomic<int> g_avoidFerry{0};
std::atomic<int> g_avoidDirty{0};
std::atomic<int> g_optimize{NAVI_COMAPS_OPTIMIZE_TIME};
std::string g_turnsLocale = "nb";

NaviComapsCode MapCode(routing::RouterResultCode code)
{
  using routing::RouterResultCode;
  switch (code)
  {
  case RouterResultCode::NoError: return NAVI_COMAPS_OK;
  case RouterResultCode::RouteNotFound: return NAVI_COMAPS_ROUTE_NOT_FOUND;
  case RouterResultCode::NeedMoreMaps: return NAVI_COMAPS_NEED_MORE_MAPS;
  case RouterResultCode::Cancelled: return NAVI_COMAPS_CANCELLED;
  default: return NAVI_COMAPS_INTERNAL;
  }
}

routing::VehicleType MapProfile(NaviComapsProfile profile)
{
  switch (profile)
  {
  case NAVI_COMAPS_PROFILE_PEDESTRIAN: return routing::VehicleType::Pedestrian;
  case NAVI_COMAPS_PROFILE_BICYCLE: return routing::VehicleType::Bicycle;
  case NAVI_COMAPS_PROFILE_CAR:
  default: return routing::VehicleType::Car;
  }
}

void ApplyAvoidSettings(routing::VehicleType vt)
{
  using Opt = routing::RoutingOptions::Option;
  routing::RoutingOptions opts(0, vt);
  if (g_avoidToll.load())
    opts.Add(Opt::Toll);
  if (g_avoidMotorway.load() || g_optimize.load() == NAVI_COMAPS_OPTIMIZE_PREFER_NON_MOTORWAY)
    opts.Add(Opt::Motorway);
  if (g_avoidFerry.load())
    opts.Add(Opt::Ferry);
  if (g_avoidDirty.load())
    opts.Add(Opt::Dirty);
  routing::RoutingOptions::SaveOptionsToSettings(opts);
}
}  // namespace

extern "C" NaviComapsCode navi_comaps_init(char const * storage_path, char const * writable_path,
                                           char const * sidecar_dir)
{
  std::lock_guard lock(g_mu);
  if (!storage_path || !writable_path)
    return NAVI_COMAPS_INTERNAL;
  try
  {
    // Avoid LCRITICAL→abort when CountryInfoGetter cannot load polygons yet;
    // surface the failure as NAVI_COMAPS_INTERNAL instead.
    base::ScopedLogAbortLevelChanger abortGuard(base::NUM_LOG_LEVELS);
    setenv("NAVI_COMAPS_HEADLESS", "1", 0);
    jni::EnsureJvmFromProcess();
    auto & platform = GetPlatform();
    // On Android, GetReader scope `r` expects an APK/zip. Point both dirs at the
    // writable tree that holds classificator/types/packed_polygons + 261105/*.mwm
    // so scope `w` resolves resources; `r` simply fails open and falls through.
    platform.SetResourceDir(storage_path);
    platform.SetWritableDirForTests(writable_path);
    platform.SetSettingsDir(writable_path);
    if (sidecar_dir && sidecar_dir[0] != '\0')
      setenv("NAVI_WETLAND_SIDECAR_DIR", sidecar_dir, 1);
    else
      unsetenv("NAVI_WETLAND_SIDECAR_DIR");

    LOG(LINFO, ("navi_comaps_init resource=", storage_path, " writable=", writable_path));
    // Android kDefaultMapStyle is MapStyleDefaultLight (19). A bare
    // classificator::Load() fills GetLoadingStyle() which defaults to 0
    // (WalkingLight), leaving the current-style slot empty — feature type
    // indices then resolve to zero and types_mapping CHECKs abort.
    GetStyleReader().SetCurrentStyle(kDefaultMapStyle);
    g_builder = std::make_unique<routing::routes_builder::RoutesBuilder>(1 /* threads */);
    if (!g_builder)
    {
      LOG(LERROR, ("navi_comaps_init: RoutesBuilder null"));
      return NAVI_COMAPS_INTERNAL;
    }
    g_inited = true;
    g_cancel.store(false);
    LOG(LINFO, ("navi_comaps_init OK"));
    return NAVI_COMAPS_OK;
  }
  catch (std::exception const & e)
  {
    LOG(LERROR, ("navi_comaps_init exception:", e.what()));
#ifdef __ANDROID__
    __android_log_print(ANDROID_LOG_ERROR, "navi_comaps", "init: %s", e.what());
#endif
    return NAVI_COMAPS_INTERNAL;
  }
  catch (...)
  {
    LOG(LERROR, ("navi_comaps_init: unknown exception"));
#ifdef __ANDROID__
    __android_log_print(ANDROID_LOG_ERROR, "navi_comaps", "init: unknown exception");
#endif
    return NAVI_COMAPS_INTERNAL;
  }
}

extern "C" NaviComapsCode navi_comaps_register_mwms(void)
{
  // RoutesBuilder reloads local country files from WritableDir on each task.
  return g_inited ? NAVI_COMAPS_OK : NAVI_COMAPS_NOT_INIT;
}

extern "C" void navi_comaps_set_wetland_lookup(NaviComapsWetlandLookupFn fn)
{
  navi_spike::WetlandSidecar::SetLookup(
      reinterpret_cast<navi_spike::WetlandLookupFn>(fn));
}

extern "C" void navi_comaps_set_wetland_hook(int enabled)
{
  if (!enabled)
  {
    navi_spike::WetlandSidecar::SetLookup(nullptr);
    navi_spike::WetlandSidecar::SetBikeEnabled(false);
  }
  // When enabling, the Rust shim must call navi_comaps_set_wetland_lookup
  // with a SidecarReader-backed callback.
}

extern "C" void navi_comaps_set_bike_wetland(int enabled)
{
  navi_spike::WetlandSidecar::SetBikeEnabled(enabled != 0);
}

extern "C" void navi_comaps_set_avoid_options(int avoid_toll, int avoid_motorway, int avoid_ferry,
                                              int avoid_dirty)
{
  g_avoidToll.store(avoid_toll != 0);
  g_avoidMotorway.store(avoid_motorway != 0);
  g_avoidFerry.store(avoid_ferry != 0);
  g_avoidDirty.store(avoid_dirty != 0);
}

extern "C" void navi_comaps_set_optimize(NaviComapsOptimize optimize)
{
  g_optimize.store(static_cast<int>(optimize));
}

extern "C" void navi_comaps_set_turns_locale(char const * locale)
{
  std::lock_guard lock(g_mu);
  if (locale && locale[0] != '\0')
    g_turnsLocale = locale;
  else
    g_turnsLocale = "en";
  settings::Set("locale", g_turnsLocale);
}

extern "C" NaviComapsCode navi_comaps_route(NaviComapsProfile profile, double from_lat, double from_lon,
                                            double to_lat, double to_lon, NaviComapsRouteResult * out)
{
  if (!out)
    return NAVI_COMAPS_INTERNAL;
  std::memset(out, 0, sizeof(*out));
  std::lock_guard lock(g_mu);
  if (!g_inited || !g_builder)
  {
    out->code = NAVI_COMAPS_NOT_INIT;
    return out->code;
  }
  if (g_optimize.load() == NAVI_COMAPS_OPTIMIZE_DISTANCE)
  {
    out->code = NAVI_COMAPS_UNSUPPORTED;
    out->error_message = strdup(
        "CoMaps RoutesBuilder has no distance-weight mode; use TIME or PREFER_NON_MOTORWAY");
    return out->code;
  }
  if (g_cancel.exchange(false))
  {
    out->code = NAVI_COMAPS_CANCELLED;
    return out->code;
  }
  try
  {
    auto const vt = MapProfile(profile);
    ApplyAvoidSettings(vt);
    settings::Set("locale", g_turnsLocale);
    routing::routes_builder::RoutesBuilder::Params params(
        vt, ms::LatLon(from_lat, from_lon), ms::LatLon(to_lat, to_lon));
    auto result = g_builder->ProcessTask(params);
    out->code = MapCode(result.m_code);
    out->build_time_s = result.m_buildTimeSeconds;
    if (!result.IsCodeOK() || result.m_routes.empty())
      return out->code;
    auto const & route = result.m_routes.front();
    out->length_m = route.m_distance;
    out->time_s = route.m_eta;
    auto const wps = route.GetWaypoints();
    out->point_count = wps.size();
    if (!wps.empty())
    {
      out->points = static_cast<NaviComapsLatLon *>(std::malloc(sizeof(NaviComapsLatLon) * wps.size()));
      for (size_t i = 0; i < wps.size(); ++i)
      {
        out->points[i].lat = wps[i].m_lat;
        out->points[i].lon = wps[i].m_lon;
      }
    }
    if (!route.m_turnsJson.empty())
      out->turns_json = strdup(route.m_turnsJson.c_str());
    else
      out->turns_json = strdup("[]");
    return out->code;
  }
  catch (std::exception const & e)
  {
    out->code = NAVI_COMAPS_INTERNAL;
    out->error_message = strdup(e.what());
    return out->code;
  }
}

extern "C" void navi_comaps_cancel(void) { g_cancel.store(true); }

extern "C" void navi_comaps_free_route(NaviComapsRouteResult * result)
{
  if (!result)
    return;
  std::free(result->points);
  std::free(result->error_message);
  std::free(result->turns_json);
  result->points = nullptr;
  result->error_message = nullptr;
  result->turns_json = nullptr;
  result->point_count = 0;
}

extern "C" void navi_comaps_shutdown(void)
{
  std::lock_guard lock(g_mu);
  g_builder.reset();
  g_inited = false;
  unsetenv("NAVI_WETLAND_SIDECAR_DIR");
}
