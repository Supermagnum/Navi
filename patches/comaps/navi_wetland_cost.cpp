#include "routing/navi_wetland_cost.hpp"

#include <mutex>
#include <string>
#include <vector>

namespace navi_spike
{
namespace
{
WetlandLookupFn g_lookup = nullptr;
ElevationLookupFn g_elevLookup = nullptr;
bool g_bikeEnabled = false;
std::mutex g_namesMu;
std::vector<std::string> g_mwmNames;
constexpr double kSoftMult = 5.0;
constexpr double kHardMult = 1.0e6;
}  // namespace

void WetlandSidecar::SetLookup(WetlandLookupFn fn) { g_lookup = fn; }

bool WetlandSidecar::Enabled() { return g_lookup != nullptr; }

void WetlandSidecar::SetBikeEnabled(bool enabled) { g_bikeEnabled = enabled; }

bool WetlandSidecar::BikeEnabled() { return g_bikeEnabled && Enabled(); }

WetlandClass WetlandSidecar::Lookup(routing::NumMwmId mwmId, uint32_t featureId, uint32_t segmentIdx, bool forward)
{
  if (!g_lookup || mwmId == routing::kFakeNumMwmId)
    return WetlandClass::None;
  std::lock_guard<std::mutex> lock(g_namesMu);
  if (mwmId >= g_mwmNames.size() || g_mwmNames[mwmId].empty())
    return WetlandClass::None;
  return g_lookup(g_mwmNames[mwmId].c_str(), featureId, segmentIdx, forward);
}

void WetlandSidecar::ClearMwmNames()
{
  std::lock_guard<std::mutex> lock(g_namesMu);
  g_mwmNames.clear();
}

void WetlandSidecar::SetMwmName(routing::NumMwmId mwmId, std::string const & name)
{
  std::lock_guard<std::mutex> lock(g_namesMu);
  if (g_mwmNames.size() <= mwmId)
    g_mwmNames.resize(static_cast<size_t>(mwmId) + 1);
  g_mwmNames[mwmId] = name;
}

char const * WetlandSidecar::MwmName(routing::NumMwmId mwmId)
{
  if (mwmId == routing::kFakeNumMwmId)
    return nullptr;
  std::lock_guard<std::mutex> lock(g_namesMu);
  if (mwmId >= g_mwmNames.size() || g_mwmNames[mwmId].empty())
    return nullptr;
  return g_mwmNames[mwmId].c_str();
}

double WetlandSidecar::CostMultiplier(WetlandClass c)
{
  switch (c)
  {
  case WetlandClass::SoftAvoid: return kSoftMult;
  case WetlandClass::HardAvoid: return kHardMult;
  case WetlandClass::None: return 1.0;
  }
  return 1.0;
}

void WetlandSidecar::SetElevationLookup(ElevationLookupFn fn) { g_elevLookup = fn; }

bool WetlandSidecar::ElevationLookup(routing::NumMwmId mwmId, uint32_t featureId, uint32_t pointIdx,
                                     int32_t * outAltitudeDm)
{
  if (!g_elevLookup || !outAltitudeDm || mwmId == routing::kFakeNumMwmId)
    return false;
  std::lock_guard<std::mutex> lock(g_namesMu);
  if (mwmId >= g_mwmNames.size() || g_mwmNames[mwmId].empty())
    return false;
  return g_elevLookup(g_mwmNames[mwmId].c_str(), featureId, pointIdx, outAltitudeDm);
}

}  // namespace navi_spike
