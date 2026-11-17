#pragma once
// Navi wetland cost hook for CoMaps EdgeEstimator.
// Lookups are supplied by the Navi FFI shim (Rust SidecarReader), not by
// embedding a zstd/NVSC parser inside CoMaps.
// SoftAvoid ×5 (weight/ETA); HardAvoid → enormous penalty (avoid).

#include "routing/segment.hpp"

#include <cstdint>
#include <string>

namespace navi_spike
{

enum class WetlandClass : uint8_t
{
  None = 0,
  SoftAvoid = 1,
  HardAvoid = 2,
};

/** `mwmName` is the CoMaps leaf id (e.g. "Norway_Oppland"): feature ids are per-MWM. */
using WetlandLookupFn = WetlandClass (*)(char const * mwmName, uint32_t featureId, uint32_t segmentIdx,
                                         bool forward);

/** Returns true when an EL02 override exists for this MWM leaf; altitude in decimetres. */
using ElevationLookupFn = bool (*)(char const * mwmName, uint32_t featureId, uint32_t pointIdx,
                                   int32_t * outAltitudeDm);

class WetlandSidecar
{
public:
  static void SetLookup(WetlandLookupFn fn);
  static bool Enabled();
  static void SetBikeEnabled(bool enabled);
  static bool BikeEnabled();
  static WetlandClass Lookup(routing::NumMwmId mwmId, uint32_t featureId, uint32_t segmentIdx, bool forward);
  static void ClearMwmNames();
  static void SetMwmName(routing::NumMwmId mwmId, std::string const & name);
  /** Pointer valid until the next ClearMwmNames/SetMwmName; nullptr if unknown. */
  static char const * MwmName(routing::NumMwmId mwmId);
  static double CostMultiplier(WetlandClass c);

  static void SetElevationLookup(ElevationLookupFn fn);
  static bool ElevationLookup(routing::NumMwmId mwmId, uint32_t featureId, uint32_t pointIdx,
                              int32_t * outAltitudeDm);
};

inline double ApplyWetlandCost(routing::Segment const & segment, double weightSeconds)
{
  if (!WetlandSidecar::Enabled())
    return weightSeconds;
  auto const cls = WetlandSidecar::Lookup(segment.GetMwmId(), segment.GetFeatureId(), segment.GetSegmentIdx(),
                                          segment.IsForward());
  return weightSeconds * WetlandSidecar::CostMultiplier(cls);
}

inline double ApplyWetlandCostForBike(routing::Segment const & segment, double weightSeconds)
{
  if (!WetlandSidecar::BikeEnabled())
    return weightSeconds;
  return ApplyWetlandCost(segment, weightSeconds);
}

}  // namespace navi_spike
