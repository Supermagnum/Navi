#include "routing/navi_wetland_cost.hpp"

namespace navi_spike
{
namespace
{
WetlandLookupFn g_lookup = nullptr;
constexpr double kSoftMult = 5.0;
constexpr double kHardMult = 1.0e6;
}  // namespace

void WetlandSidecar::SetLookup(WetlandLookupFn fn) { g_lookup = fn; }

bool WetlandSidecar::Enabled() { return g_lookup != nullptr; }

WetlandClass WetlandSidecar::Lookup(uint32_t featureId, uint32_t segmentIdx, bool forward)
{
  if (!g_lookup)
    return WetlandClass::None;
  return g_lookup(featureId, segmentIdx, forward);
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

}  // namespace navi_spike
