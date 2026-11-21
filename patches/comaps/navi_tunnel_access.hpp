#pragma once
// Navi non-motor tunnel access for CoMaps Pedestrian/Bicycle estimators.
//
// Product rule: hiking / bicycle / e-bike must not use a tunnel unless it has
// at least one of:
//   - hwtag|yesfoot (hiking) or hwtag|yesbicycle (bike)  [foot/bicycle=yes|designated]
//   - a sidewalk type (highway|footway|sidewalk), or yesfoot from sidewalk=* encoding
//   - highway = footway / cycleway / path
// Otherwise HardAvoid ×1e6 (same pattern as wetland / vehicle limits).

#include "routing/geometry.hpp"
#include "routing/segment.hpp"

#include "routing_common/vehicle_model.hpp"

namespace navi_spike
{

inline constexpr double kTunnelHardAvoidMult = 1.0e6;

inline bool IsPedestrianOrCyclePathHighway(std::optional<routing::HighwayType> hw)
{
  if (!hw)
    return false;
  using routing::HighwayType;
  return *hw == HighwayType::HighwayFootway || *hw == HighwayType::HighwayCycleway ||
         *hw == HighwayType::HighwayPath;
}

/** Hiking / pedestrian: yesfoot, sidewalk type, or footway/cycleway/path. */
inline bool TunnelAllowedForPedestrian(routing::RoadGeometry const & road)
{
  if (!road.IsTunnel())
    return true;
  if (IsPedestrianOrCyclePathHighway(road.GetHighwayType()))
    return true;
  if (road.HasSidewalkType())
    return true;
  return road.HasYesFoot();
}

/** Bicycle / e-bike: yesbicycle, sidewalk type, or footway/cycleway/path. */
inline bool TunnelAllowedForBicycle(routing::RoadGeometry const & road)
{
  if (!road.IsTunnel())
    return true;
  if (IsPedestrianOrCyclePathHighway(road.GetHighwayType()))
    return true;
  if (road.HasSidewalkType())
    return true;
  return road.HasYesBicycle();
}

inline double ApplyPedestrianTunnelAccessCost(routing::RoadGeometry const & road, double weightSeconds)
{
  if (TunnelAllowedForPedestrian(road))
    return weightSeconds;
  return weightSeconds * kTunnelHardAvoidMult;
}

inline double ApplyBicycleTunnelAccessCost(routing::RoadGeometry const & road, double weightSeconds)
{
  if (TunnelAllowedForBicycle(road))
    return weightSeconds;
  return weightSeconds * kTunnelHardAvoidMult;
}

}  // namespace navi_spike
