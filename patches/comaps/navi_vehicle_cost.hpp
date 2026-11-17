#pragma once
// Navi vehicle-limit cost hook for CoMaps CarEstimator.
// Lookups come from the Navi FFI shim (Rust SidecarReader).
//
// CarEstimator has no impassable-edge API in this pin: a segment that exceeds a
// posted limit is multiplied by 1e6 (same HardAvoid pattern as wetland).

#include "routing/segment.hpp"

#include <cstdint>

namespace navi_spike
{

/** Return 1 and fill limit fields when a record exists; 0 = no record.
 *  Zero in a limit field means that dimension does not block. */
using VehicleLimitLookupFn = int (*)(char const * mwmName, uint32_t featureId, uint32_t segmentIdx,
                                     uint16_t * maxHeightCm, uint16_t * maxWidthCm, uint16_t * maxLengthCm,
                                     uint16_t * maxWeight10kg, uint16_t * maxAxle10kg, uint16_t * maxBogie10kg);

class VehicleSidecar
{
public:
  static void SetLookup(VehicleLimitLookupFn fn);
  static bool Enabled();

  /** Metres / kilograms. Pass 0 (or negative) for a dimension to leave it unused. */
  static void SetVehicle(double heightM, double widthM, double lengthM, double totalWeightKg,
                         double axleWeightKg, double bogieWeightKg);
  static void ClearVehicle();

  static bool ExceedsLimits(routing::NumMwmId mwmId, uint32_t featureId, uint32_t segmentIdx);
};

inline double ApplyVehicleLimitCost(routing::Segment const & segment, double weightSeconds)
{
  if (!VehicleSidecar::Enabled())
    return weightSeconds;
  if (VehicleSidecar::ExceedsLimits(segment.GetMwmId(), segment.GetFeatureId(), segment.GetSegmentIdx()))
    return weightSeconds * 1.0e6;
  return weightSeconds;
}

}  // namespace navi_spike
