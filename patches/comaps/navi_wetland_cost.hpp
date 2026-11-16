#pragma once
// Navi wetland cost hook for CoMaps EdgeEstimator.
// Lookups are supplied by the Navi FFI shim (Rust SidecarReader), not by
// embedding a zstd/NVSC parser inside CoMaps.
// SoftAvoid ×5 (weight/ETA); HardAvoid → enormous penalty (avoid).

#include "routing/segment.hpp"

#include <cstdint>

namespace navi_spike
{

enum class WetlandClass : uint8_t
{
  None = 0,
  SoftAvoid = 1,
  HardAvoid = 2,
};

using WetlandLookupFn = WetlandClass (*)(uint32_t featureId, uint32_t segmentIdx, bool forward);

class WetlandSidecar
{
public:
  // Install / clear the lookup provided by navi_comaps (Rust).
  static void SetLookup(WetlandLookupFn fn);
  static bool Enabled();
  static WetlandClass Lookup(uint32_t featureId, uint32_t segmentIdx, bool forward);
  static double CostMultiplier(WetlandClass c);
};

inline double ApplyWetlandCost(routing::Segment const & segment, double weightSeconds)
{
  if (!WetlandSidecar::Enabled())
    return weightSeconds;
  auto const cls = WetlandSidecar::Lookup(
      segment.GetFeatureId(), segment.GetSegmentIdx(), segment.IsForward());
  return weightSeconds * WetlandSidecar::CostMultiplier(cls);
}

}  // namespace navi_spike
