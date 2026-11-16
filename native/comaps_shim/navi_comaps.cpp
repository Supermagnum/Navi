// Navi thin routing shim over CoMaps RoutesBuilder (headless; no GLES).
#include "navi_comaps.h"

#include "routing/routes_builder/routes_builder.hpp"
#include "routing/navi_wetland_cost.hpp"
#include "routing/routing_callbacks.hpp"
#include "routing/routing_options.hpp"

#include "search/displayed_categories.hpp"
#include "search/engine.hpp"
#include "search/result.hpp"
#include "search/reverse_geocoder.hpp"
#include "search/search_params.hpp"

#include "storage/country_info_getter.hpp"
#include "storage/storage.hpp"
#include "storage/storage_defines.hpp"

#include "indexer/categories_holder.hpp"
#include "indexer/classificator_loader.hpp"
#include "indexer/data_source.hpp"
#include "indexer/map_style.hpp"
#include "indexer/map_style_reader.hpp"
#include "indexer/mwm_set.hpp"

#include "platform/country_file.hpp"
#include "platform/local_country_file.hpp"
#include "platform/local_country_file_utils.hpp"
#include "platform/platform.hpp"
#include "platform/settings.hpp"

#include "app/organicmaps/sdk/core/jni_helper.hpp"

#include "geometry/latlon.hpp"
#include "geometry/mercator.hpp"
#include "geometry/rect2d.hpp"

#include "base/logging.hpp"

#include "defines.hpp"

#include <algorithm>
#include <atomic>
#include <cstdlib>
#include <cstring>
#include <limits>
#include <map>
#include <memory>
#include <mutex>
#include <sstream>
#include <string>
#include <utility>
#include <vector>

#ifdef __ANDROID__
#include <android/log.h>
#endif

namespace
{
std::mutex g_mu;
std::atomic<bool> g_inited{false};
std::unique_ptr<routing::routes_builder::RoutesBuilder> g_builder;
std::atomic<bool> g_cancel{false};
std::atomic<int> g_avoidToll{0};
std::atomic<int> g_avoidMotorway{0};
std::atomic<int> g_avoidFerry{0};
std::atomic<int> g_avoidDirty{0};
std::atomic<int> g_optimize{NAVI_COMAPS_OPTIMIZE_TIME};
std::string g_turnsLocale = "nb";

void ResetSearch();

void SetErrorMessage(NaviComapsRouteResult * out, char const * msg)
{
  if (!out)
    return;
  std::free(out->error_message);
  out->error_message = msg ? strdup(msg) : nullptr;
}

NaviComapsCode MapCode(routing::RouterResultCode code, std::string & message)
{
  using routing::RouterResultCode;
  message = routing::ToString(code);
  switch (code)
  {
  case RouterResultCode::NoError:
  case RouterResultCode::HasWarnings:
    return NAVI_COMAPS_OK;
  case RouterResultCode::Cancelled:
    return NAVI_COMAPS_CANCELLED;
  case RouterResultCode::RouteNotFound:
  case RouterResultCode::RouteNotFoundRedressRouteError:
  case RouterResultCode::TransitRouteNotFoundNoNetwork:
  case RouterResultCode::TransitRouteNotFoundTooLongPedestrian:
  case RouterResultCode::IntermediatePointNotFound:
    message = "No route found";
    return NAVI_COMAPS_ROUTE_NOT_FOUND;
  case RouterResultCode::NeedMoreMaps:
  case RouterResultCode::RouteFileNotExist:
  case RouterResultCode::InconsistentMWMandRoute:
  case RouterResultCode::FileTooOld:
  case RouterResultCode::PointsInDifferentMWM:
    message = "Missing or outdated maps — download the regions covering this route";
    return NAVI_COMAPS_NEED_MORE_MAPS;
  case RouterResultCode::StartPointNotFound:
  case RouterResultCode::NoCurrentPosition:
    message = "Start is not on a routable road for this profile";
    return NAVI_COMAPS_START_NOT_ON_ROAD;
  case RouterResultCode::EndPointNotFound:
    message = "Destination is not on a routable road for this profile";
    return NAVI_COMAPS_END_NOT_ON_ROAD;
  case RouterResultCode::InternalError:
    // True CoMaps internal — still map to a user-facing no-route rather than INTERNAL.
    message = "Routing failed (engine error)";
    return NAVI_COMAPS_ROUTE_NOT_FOUND;
  default:
    message = std::string("Routing failed (") + routing::ToString(code) + ")";
    return NAVI_COMAPS_ROUTE_NOT_FOUND;
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

namespace
{
std::atomic<bool> g_platformLocked{false};
}  // namespace

extern "C" void navi_comaps_set_platform_locked(int locked) { g_platformLocked.store(locked != 0); }

extern "C" NaviComapsCode navi_comaps_init(char const * storage_path, char const * writable_path,
                                           char const * sidecar_dir)
{
  std::lock_guard lock(g_mu);
  if (!storage_path || !writable_path)
    return NAVI_COMAPS_INTERNAL;
  try
  {
    base::ScopedLogAbortLevelChanger abortGuard(base::NUM_LOG_LEVELS);
    setenv("NAVI_COMAPS_HEADLESS", "1", 0);
    jni::EnsureJvmFromProcess();
    auto & platform = GetPlatform();
    // Drape (Framework) live: it already owns these directories and the style; moving the
    // resource dir back to the MWM root would break its lazy APK reads (fonts, symbols).
    bool const locked = g_platformLocked.load();
    if (!locked)
    {
      platform.SetResourceDir(storage_path);
      platform.SetWritableDirForTests(writable_path);
      platform.SetSettingsDir(writable_path);
    }
    if (sidecar_dir && sidecar_dir[0] != '\0')
      setenv("NAVI_WETLAND_SIDECAR_DIR", sidecar_dir, 1);
    else
      unsetenv("NAVI_WETLAND_SIDECAR_DIR");

    LOG(LINFO, ("navi_comaps_init resource=", storage_path, " writable=", writable_path,
                " platform_locked=", locked));
    if (!locked)
      GetStyleReader().SetCurrentStyle(kDefaultMapStyle);
    g_builder = std::make_unique<routing::routes_builder::RoutesBuilder>(1 /* threads */);
    if (!g_builder)
    {
      LOG(LERROR, ("navi_comaps_init: RoutesBuilder null"));
      return NAVI_COMAPS_INTERNAL;
    }
    g_inited.store(true);
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
  if (!g_inited.load())
    return NAVI_COMAPS_NOT_INIT;
  // Newly downloaded MWMs: drop the search data source so it re-registers lazily.
  ResetSearch();
  return NAVI_COMAPS_OK;
}

extern "C" void navi_comaps_set_wetland_lookup(NaviComapsWetlandLookupFn fn)
{
  navi_spike::WetlandSidecar::SetLookup(
      reinterpret_cast<navi_spike::WetlandLookupFn>(fn));
}

namespace
{
NaviComapsElevLookupFn g_elevC = nullptr;
bool ElevLookupWrap(char const * mwmName, uint32_t featureId, uint32_t pointIdx, int32_t * outDm)
{
  return g_elevC != nullptr && g_elevC(mwmName, featureId, pointIdx, outDm) != 0;
}
}  // namespace

extern "C" void navi_comaps_set_elevation_lookup(NaviComapsElevLookupFn fn)
{
  g_elevC = fn;
  navi_spike::WetlandSidecar::SetElevationLookup(fn ? &ElevLookupWrap : nullptr);
}

extern "C" void navi_comaps_set_wetland_hook(int enabled)
{
  if (!enabled)
  {
    navi_spike::WetlandSidecar::SetLookup(nullptr);
    navi_spike::WetlandSidecar::SetBikeEnabled(false);
  }
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
  if (!g_inited.load() || !g_builder)
  {
    out->code = NAVI_COMAPS_NOT_INIT;
    SetErrorMessage(out, "CoMaps session not initialised");
    return out->code;
  }
  if (g_optimize.load() == NAVI_COMAPS_OPTIMIZE_DISTANCE)
  {
    out->code = NAVI_COMAPS_UNSUPPORTED;
    SetErrorMessage(out,
                    "CoMaps RoutesBuilder has no distance-weight mode; use TIME or PREFER_NON_MOTORWAY");
    return out->code;
  }
  if (g_cancel.exchange(false))
  {
    out->code = NAVI_COMAPS_CANCELLED;
    SetErrorMessage(out, "Cancelled");
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
    std::string msg;
    out->code = MapCode(result.m_code, msg);
    out->build_time_s = result.m_buildTimeSeconds;
    LOG(LINFO, ("navi_comaps_route code=", msg, " navi=", static_cast<int>(out->code),
                " build_s=", out->build_time_s, " from=", from_lat, from_lon, " to=", to_lat, to_lon));
#ifdef __ANDROID__
    __android_log_print(ANDROID_LOG_INFO, "navi_comaps",
                        "route code=%s navi=%d build_s=%.3f", msg.c_str(),
                        static_cast<int>(out->code), out->build_time_s);
#endif
    if (!result.IsCodeOK() || result.m_routes.empty())
    {
      SetErrorMessage(out, msg.c_str());
      return out->code;
    }
    auto const & route = result.m_routes.front();
    out->length_m = route.m_distance;
    out->time_s = route.m_eta;
    out->ascent_m = route.m_ascentM;
    out->descent_m = route.m_descentM;
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
    out->turns_json = strdup(route.m_turnsJson.empty() ? "[]" : route.m_turnsJson.c_str());
    out->speed_limits_json =
        strdup(route.m_speedLimitsJson.empty() ? "[]" : route.m_speedLimitsJson.c_str());
    out->elevation_json =
        strdup(route.m_elevationJson.empty() ? "[]" : route.m_elevationJson.c_str());
    out->router_mode =
        strdup(route.m_routerMode.empty() ? "Unknown" : route.m_routerMode.c_str());
    return out->code;
  }
  catch (std::exception const & e)
  {
    out->code = NAVI_COMAPS_INTERNAL;
    SetErrorMessage(out, e.what());
#ifdef __ANDROID__
    __android_log_print(ANDROID_LOG_ERROR, "navi_comaps", "route exception: %s", e.what());
#endif
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
  std::free(result->elevation_json);
  std::free(result->router_mode);
  std::free(result->speed_limits_json);
  result->speed_limits_json = nullptr;
  result->points = nullptr;
  result->error_message = nullptr;
  result->turns_json = nullptr;
  result->elevation_json = nullptr;
  result->router_mode = nullptr;
  result->point_count = 0;
}

extern "C" NaviComapsCode navi_comaps_snap_to_road(NaviComapsProfile profile, double lat, double lon,
                                                   double radius_m, NaviComapsSnapResult * out)
{
  if (!out)
    return NAVI_COMAPS_INTERNAL;
  std::memset(out, 0, sizeof(*out));
  std::lock_guard lock(g_mu);
  if (!g_inited.load() || !g_builder)
  {
    out->code = NAVI_COMAPS_NOT_INIT;
    out->error_message = strdup("CoMaps session not initialised");
    return out->code;
  }
  if (!(radius_m > 0.0))
    radius_m = 500.0;
  try
  {
    auto const vt = MapProfile(profile);
    ApplyAvoidSettings(vt);
    auto const snap = g_builder->FindClosestRoad(vt, mercator::FromLatLon(lat, lon), radius_m);
    if (!snap.m_found)
    {
      out->code = NAVI_COMAPS_ROUTE_NOT_FOUND;
      out->error_message = strdup("No routable road within search radius");
      return out->code;
    }
    auto const ll = mercator::ToLatLon(snap.m_point);
    out->code = NAVI_COMAPS_OK;
    out->proj_lat = ll.m_lat;
    out->proj_lon = ll.m_lon;
    out->distance_m = snap.m_distanceM;
    out->feature_id = snap.m_featureId;
    out->segment_idx = snap.m_segId;
    out->mwm_name = snap.m_mwmName.empty() ? nullptr : strdup(snap.m_mwmName.c_str());
    LOG(LINFO, ("navi_comaps_snap_to_road from=", lat, lon, " proj=", ll.m_lat, ll.m_lon,
                " dist_m=", snap.m_distanceM));
    return out->code;
  }
  catch (std::exception const & e)
  {
    out->code = NAVI_COMAPS_INTERNAL;
    out->error_message = strdup(e.what());
    return out->code;
  }
}

extern "C" void navi_comaps_free_snap(NaviComapsSnapResult * result)
{
  if (!result)
    return;
  std::free(result->mwm_name);
  std::free(result->error_message);
  result->mwm_name = nullptr;
  result->error_message = nullptr;
}

// ---- Search -----------------------------------------------------------------

namespace
{
// Owns the search-side data source and engine. Lazily built on first use so the
// maps downloaded after navi_comaps_init() are picked up; ResetSearch() drops it.
struct SearchState
{
  FrozenDataSource dataSource;
  // Owned copy; CountryInfoGetter keeps a raw pointer into this map.
  // Without SetAffiliations, GetMatchedRegions SEGVs on the 2nd query.
  storage::Affiliations affiliations;
  std::shared_ptr<storage::CountryInfoGetter> infoGetter;
  std::unique_ptr<search::Engine> engine;
};

std::mutex g_searchMu;
std::shared_ptr<SearchState> g_search;
std::string g_searchLocale = "en";
bool g_hasViewport = false;
m2::RectD g_viewport;
bool g_hasPosition = false;
m2::PointD g_position;
std::atomic<uint64_t> g_nextRequestId{1};
std::map<uint64_t, std::weak_ptr<search::ProcessorHandle>> g_requests;

char * Dup(std::string const & s) { return strdup(s.c_str()); }

std::shared_ptr<SearchState> EnsureSearch()
{
  std::lock_guard lock(g_searchMu);
  if (g_search)
    return g_search;
  if (!g_inited.load())
    return nullptr;

  auto state = std::make_shared<SearchState>();
  state->infoGetter = storage::CountryInfoReader::CreateCountryInfoGetter(GetPlatform());
  if (!state->infoGetter)
    return nullptr;
  // Mirror TestSearchEngine::InitAffiliations — Storage loads countries.txt
  // affiliations that Geocoder::FillLocalitiesTable looks up by name.
  {
    storage::Storage storage;
    if (auto const * aff = storage.GetAffiliations())
      state->affiliations = *aff;
    state->infoGetter->SetAffiliations(&state->affiliations);
  }

  std::vector<platform::LocalCountryFile> localFiles;
  platform::FindAllLocalMapsAndCleanup(std::numeric_limits<int64_t>::max(), localFiles);
  // World overlays live at the writable root (Phase 1 layout). FindAllLocalMaps
  // can miss them when country leaves sit only under releases/{ver}/ — register
  // World explicitly so city/localities search works.
  {
    bool hasWorld = false;
    for (auto const & f : localFiles)
    {
      if (f.GetCountryFile().GetName() == WORLD_FILE_NAME)
      {
        hasWorld = true;
        break;
      }
    }
    if (!hasWorld)
    {
      platform::CountryFile world(WORLD_FILE_NAME);
      platform::LocalCountryFile worldFile(GetPlatform().WritableDir(), world, 0 /* version */);
      worldFile.SyncWithDisk();
      if (worldFile.OnDisk(MapFileType::Map))
        localFiles.push_back(worldFile);
      else
        LOG(LWARNING, ("navi search: World.mwm missing at", GetPlatform().WritableDir()));
    }
  }
  size_t registered = 0;
  for (auto const & localFile : localFiles)
  {
    // World carries city/country localities; WorldCoasts is render-only.
    if (localFile.GetCountryFile().GetName() == WORLD_COASTS_FILE_NAME)
      continue;
    auto const res = state->dataSource.RegisterMap(localFile);
    if (res.second == MwmSet::RegResult::Success)
      ++registered;
    else
      LOG(LWARNING, ("navi search: cannot register", localFile));
  }
  LOG(LINFO, ("navi search: registered", registered, "of", localFiles.size(), "maps"));

  search::Engine::Params params(g_searchLocale, 1 /* numThreads */);
  state->engine = std::make_unique<search::Engine>(state->dataSource, GetDefaultCategories(),
                                                   *state->infoGetter, params);
  g_search = state;
  return g_search;
}

void ResetSearch()
{
  std::shared_ptr<SearchState> old;
  {
    std::lock_guard lock(g_searchMu);
    for (auto & [id, weak] : g_requests)
      if (auto h = weak.lock())
        h->Cancel();
    g_requests.clear();
    old = std::move(g_search);
  }
  // Engine destructor joins its worker threads outside the lock.
  old.reset();
}

m2::RectD SearchViewport()
{
  if (g_hasViewport && g_viewport.IsValid())
    return g_viewport;
  if (g_hasPosition)
    return mercator::RectByCenterXYAndSizeInMeters(g_position, 50000.0);
  return mercator::Bounds::FullRect();
}

NaviComapsSearchResult ConvertResult(search::Result const & r, bool hasPosition, m2::PointD const & position)
{
  NaviComapsSearchResult out;
  std::memset(&out, 0, sizeof(out));
  switch (r.GetResultType())
  {
  case search::Result::Type::LatLon: out.kind = NAVI_COMAPS_SEARCH_LATLON; break;
  case search::Result::Type::Postcode: out.kind = NAVI_COMAPS_SEARCH_POSTCODE; break;
  default: out.kind = NAVI_COMAPS_SEARCH_FEATURE; break;
  }
  auto const center = r.GetFeatureCenter();
  auto const ll = mercator::ToLatLon(center);
  out.lat = ll.m_lat;
  out.lon = ll.m_lon;
  out.distance_m = hasPosition ? mercator::DistanceOnEarth(position, center) : -1.0;
  std::string mwm;
  if (r.GetResultType() == search::Result::Type::Feature)
  {
    auto const & fid = r.GetFeatureID();
    out.feature_id = fid.m_index;
    if (fid.m_mwmId.IsAlive())
      mwm = fid.m_mwmId.GetInfo()->GetCountryName();
  }
  out.name = Dup(r.GetString());
  out.address = Dup(r.GetAddress());
  // Do not call GetLocalizedFeatureType(): on Android that goes through
  // Utils.getStringValueByKey and aborts when Platform has no Context (headless
  // instrumented tests and early session open before the activity). Name +
  // address are enough for the search suite and the To-field UI.
  out.feature_type = Dup(std::string());
  out.mwm_name = Dup(mwm);
  return out;
}

void DeliverBatch(NaviComapsSearchCallback cb, void * user, uint64_t id, search::Results const & results,
                  size_t & emitted, bool hasPosition, m2::PointD const & position)
{
  bool const isFinal = results.IsEndMarker();
  std::vector<NaviComapsSearchResult> fresh;
  for (size_t i = emitted; i < results.GetCount(); ++i)
  {
    auto const & r = results[i];
    // Keep Suggests that have a point — exact town queries often surface the
    // city as a Suggest first; dropping them left town_* suite rows empty.
    if (!r.HasPoint())
      continue;
    fresh.push_back(ConvertResult(r, hasPosition, position));
  }
  emitted = results.GetCount();
  if (fresh.empty() && !isFinal)
    return;

  auto * batch = static_cast<NaviComapsSearchBatch *>(std::calloc(1, sizeof(NaviComapsSearchBatch)));
  batch->request_id = id;
  batch->count = fresh.size();
  if (!fresh.empty())
  {
    batch->results =
        static_cast<NaviComapsSearchResult *>(std::malloc(sizeof(NaviComapsSearchResult) * fresh.size()));
    std::copy(fresh.begin(), fresh.end(), batch->results);
  }
  batch->is_final = isFinal ? 1 : 0;
  batch->cancelled = results.IsEndedCancelled() ? 1 : 0;
  cb(batch, user);
}

void FreeResultStrings(NaviComapsSearchResult & r)
{
  std::free(r.name);
  std::free(r.address);
  std::free(r.feature_type);
  std::free(r.mwm_name);
}
}  // namespace

extern "C" void navi_comaps_set_search_locale(char const * locale)
{
  std::shared_ptr<SearchState> state;
  std::string newLocale;
  {
    std::lock_guard lock(g_searchMu);
    g_searchLocale = (locale && locale[0] != '\0') ? locale : "en";
    newLocale = g_searchLocale;
    state = g_search;
  }
  if (state && state->engine)
    state->engine->SetLocale(newLocale);
}

extern "C" void navi_comaps_set_search_viewport(double min_lat, double min_lon, double max_lat,
                                                double max_lon)
{
  std::lock_guard lock(g_searchMu);
  m2::RectD const rect(mercator::FromLatLon(min_lat, min_lon), mercator::FromLatLon(max_lat, max_lon));
  g_hasViewport = rect.IsValid();
  g_viewport = rect;
}

extern "C" void navi_comaps_clear_search_viewport(void)
{
  std::lock_guard lock(g_searchMu);
  g_hasViewport = false;
}

extern "C" void navi_comaps_set_search_position(double lat, double lon, int has_position)
{
  std::lock_guard lock(g_searchMu);
  g_hasPosition = has_position != 0;
  if (g_hasPosition)
    g_position = mercator::FromLatLon(lat, lon);
}

extern "C" uint64_t navi_comaps_search(char const * query, int is_category, size_t max_results,
                                       NaviComapsSearchCallback callback, void * user_data)
{
  if (!query || !callback)
    return 0;
  try
  {
    auto const state = EnsureSearch();
    if (!state)
      return 0;

    uint64_t const id = g_nextRequestId.fetch_add(1);
    bool hasPosition;
    m2::PointD position;
    search::SearchParams params;
    {
      std::lock_guard lock(g_searchMu);
      hasPosition = g_hasPosition;
      position = g_position;
      params.m_inputLocale = g_searchLocale;
      params.m_viewport = SearchViewport();
    }
    params.m_query = query;
    if (hasPosition)
      params.m_position = position;
    // Prefer Viewport when the app set a bias box (matches CoMaps SearchAPI).
    bool const useViewport = g_hasViewport && g_viewport.IsValid();
    params.m_mode = useViewport ? search::Mode::Viewport : search::Mode::Everywhere;
    params.m_maxNumResults =
        max_results > 0
            ? max_results
            : (useViewport ? search::SearchParams::kDefaultNumResultsInViewport
                           : search::SearchParams::kDefaultNumResultsEverywhere);
    params.m_batchSize = search::SearchParams::kDefaultBatchSizeEverywhere;
    params.m_suggestsEnabled = false;
    // Address formatting calls into Android Utils.getStringValueByKey; without a
    // Platform Context (headless tests / early open) that aborts the process.
    // Names are enough for ranking and the To-field; addresses stay empty here.
    params.m_needAddress = false;
    params.m_needHighlighting = false;
    params.m_categorialRequest = is_category != 0;

    auto emitted = std::make_shared<size_t>(0);
    params.m_onResults = [callback, user_data, id, hasPosition, position, emitted](search::Results const & results)
    { DeliverBatch(callback, user_data, id, results, *emitted, hasPosition, position); };

    auto handle = state->engine->Search(std::move(params));
    {
      std::lock_guard lock(g_searchMu);
      // Drop handles of finished requests so the map does not grow without bound.
      for (auto it = g_requests.begin(); it != g_requests.end();)
        it = it->second.expired() ? g_requests.erase(it) : std::next(it);
      g_requests[id] = handle;
    }
    return id;
  }
  catch (std::exception const & e)
  {
    LOG(LERROR, ("navi_comaps_search exception:", e.what()));
    return 0;
  }
}

extern "C" void navi_comaps_search_cancel(uint64_t request_id)
{
  std::lock_guard lock(g_searchMu);
  if (request_id == 0)
  {
    for (auto & [id, weak] : g_requests)
      if (auto h = weak.lock())
        h->Cancel();
    g_requests.clear();
    return;
  }
  auto const it = g_requests.find(request_id);
  if (it == g_requests.end())
    return;
  if (auto h = it->second.lock())
    h->Cancel();
  g_requests.erase(it);
}

extern "C" void navi_comaps_free_search_result(NaviComapsSearchBatch * batch)
{
  if (!batch)
    return;
  for (size_t i = 0; i < batch->count; ++i)
    FreeResultStrings(batch->results[i]);
  std::free(batch->results);
  std::free(batch);
}

extern "C" char * navi_comaps_category_keys(void)
{
  try
  {
    search::DisplayedCategories categories(GetDefaultCategories());
    std::ostringstream json;
    json << '[';
    bool first = true;
    for (auto const & key : categories.GetKeys())
    {
      if (!first)
        json << ',';
      first = false;
      json << '"' << key << '"';
    }
    json << ']';
    return Dup(json.str());
  }
  catch (std::exception const & e)
  {
    LOG(LERROR, ("navi_comaps_category_keys exception:", e.what()));
    return nullptr;
  }
}

extern "C" char * navi_comaps_category_query(char const * key, char const * locale)
{
  if (!key)
    return nullptr;
  try
  {
    search::DisplayedCategories categories(GetDefaultCategories());
    std::string const want = (locale && locale[0] != '\0') ? locale : "en";
    std::string preferred;
    std::string english;
    bool found = false;
    categories.ForEachSynonym(key, [&](std::string const & name, std::string const & lang)
    {
      found = true;
      if (lang == want && preferred.empty())
        preferred = name;
      if (lang == "en" && english.empty())
        english = name;
    });
    if (!found)
      return nullptr;
    return Dup(preferred.empty() ? english : preferred);
  }
  catch (std::exception const & e)
  {
    LOG(LERROR, ("navi_comaps_category_query exception:", e.what()));
    return nullptr;
  }
}

extern "C" void navi_comaps_free_string(char * s) { std::free(s); }

namespace
{
std::mutex g_leafMu;
std::shared_ptr<storage::CountryInfoGetter> g_leafInfo;
}  // namespace

extern "C" char * navi_comaps_leaf_for_point(double lat, double lon)
{
  if (!g_inited.load())
    return nullptr;
  try
  {
    std::shared_ptr<storage::CountryInfoGetter> info;
    {
      std::lock_guard lock(g_leafMu);
      if (!g_leafInfo)
        g_leafInfo = storage::CountryInfoReader::CreateCountryInfoGetter(GetPlatform());
      info = g_leafInfo;
    }
    if (!info)
      return nullptr;
    return strdup(info->GetRegionCountryId(mercator::FromLatLon(lat, lon)).c_str());
  }
  catch (std::exception const & e)
  {
    LOG(LERROR, ("navi_comaps_leaf_for_point exception:", e.what()));
    return nullptr;
  }
}

extern "C" NaviComapsCode navi_comaps_reverse_geocode(double lat, double lon, NaviComapsAddress * out)
{
  if (!out)
    return NAVI_COMAPS_INTERNAL;
  std::memset(out, 0, sizeof(*out));
  out->distance_m = -1.0;
  auto const state = EnsureSearch();
  if (!state)
  {
    out->code = NAVI_COMAPS_NOT_INIT;
    return out->code;
  }
  try
  {
    auto const center = mercator::FromLatLon(lat, lon);
    search::ReverseGeocoder coder(state->dataSource);

    std::string const countryId = state->infoGetter->GetRegionCountryId(center);
    out->country_id = Dup(countryId);

    search::ReverseGeocoder::Address addr;
    coder.GetNearbyAddress(center, 200.0 /* maxDistanceM */, addr, true /* placeAsStreet */);
    if (addr.IsValid())
    {
      out->code = NAVI_COMAPS_OK;
      out->house_number = Dup(addr.GetHouseNumber());
      out->street = Dup(addr.GetStreetName());
      out->formatted = Dup(addr.FormatAddress());
      out->distance_m = addr.GetDistance();
      return out->code;
    }

    // No exact address: fall back to the nearest named street of the leaf MWM.
    if (!countryId.empty())
    {
      auto const mwmId = state->dataSource.GetMwmIdByCountryFile(platform::CountryFile(countryId));
      if (mwmId.IsAlive())
      {
        auto const streets = coder.GetNearbyStreets(mwmId, center);
        for (auto const & street : streets)
        {
          if (street.m_name.empty())
            continue;
          out->code = NAVI_COMAPS_OK;
          out->street = Dup(street.m_name);
          out->formatted = Dup(street.m_name);
          out->distance_m = street.m_distanceMeters;
          return out->code;
        }
      }
    }
    out->code = NAVI_COMAPS_ROUTE_NOT_FOUND;
    return out->code;
  }
  catch (std::exception const & e)
  {
    LOG(LERROR, ("navi_comaps_reverse_geocode exception:", e.what()));
    out->code = NAVI_COMAPS_INTERNAL;
    return out->code;
  }
}

extern "C" void navi_comaps_free_address(NaviComapsAddress * address)
{
  if (!address)
    return;
  std::free(address->house_number);
  std::free(address->street);
  std::free(address->formatted);
  std::free(address->country_id);
  address->house_number = nullptr;
  address->street = nullptr;
  address->formatted = nullptr;
  address->country_id = nullptr;
}

extern "C" void navi_comaps_shutdown(void)
{
  ResetSearch();
  std::lock_guard lock(g_mu);
  g_builder.reset();
  g_inited.store(false);
}
