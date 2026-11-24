#include "routing/navi_vehicle_cost.hpp"

#include "routing/navi_wetland_cost.hpp"

#include <cmath>
#include <mutex>
#include <string>
#include <vector>

namespace navi_spike
{
namespace
{
VehicleLimitLookupFn g_lookup = nullptr;
double g_heightM = 0.0;
double g_widthM = 0.0;
double g_lengthM = 0.0;
double g_totalKg = 0.0;
double g_axleKg = 0.0;
double g_bogieKg = 0.0;
bool g_anyDim = false;

// Mirror of WetlandSidecar name registry: RoutesBuilder fills wetland names;
// we re-query via WetlandSidecar::Lookup's registry by duplicating SetMwmName
// calls is unnecessary — resolve through a friend accessor added in 0007.
char const * ResolveMwmName(routing::NumMwmId mwmId)
{
  return WetlandSidecar::MwmName(mwmId);
}

uint16_t MetresToCm(double m)
{
  if (!(m > 0.0) || !std::isfinite(m))
    return 0;
  double const cm = std::round(m * 100.0);
  if (cm <= 0.0)
    return 0;
  if (cm >= 65535.0)
    return 65535;
  return static_cast<uint16_t>(cm);
}

uint16_t KgTo10kg(double kg)
{
  if (!(kg > 0.0) || !std::isfinite(kg))
    return 0;
  double const v = std::round(kg / 10.0);
  if (v <= 0.0)
    return 0;
  if (v >= 65535.0)
    return 65535;
  return static_cast<uint16_t>(v);
}

bool StrictExceeds(uint16_t vehicle, uint16_t limit)
{
  return limit != 0 && vehicle > limit;
}
}  // namespace

void VehicleSidecar::SetLookup(VehicleLimitLookupFn fn) { g_lookup = fn; }

bool VehicleSidecar::Enabled() { return g_lookup != nullptr && g_anyDim; }

void VehicleSidecar::SetVehicle(double heightM, double widthM, double lengthM, double totalWeightKg,
                                double axleWeightKg, double bogieWeightKg)
{
  g_heightM = heightM;
  g_widthM = widthM;
  g_lengthM = lengthM;
  g_totalKg = totalWeightKg;
  g_axleKg = axleWeightKg;
  g_bogieKg = bogieWeightKg;
  g_anyDim = (MetresToCm(heightM) != 0) || (MetresToCm(widthM) != 0) || (MetresToCm(lengthM) != 0)
             || (KgTo10kg(totalWeightKg) != 0) || (KgTo10kg(axleWeightKg) != 0)
             || (KgTo10kg(bogieWeightKg) != 0);
}

void VehicleSidecar::ClearVehicle()
{
  g_heightM = g_widthM = g_lengthM = 0.0;
  g_totalKg = g_axleKg = g_bogieKg = 0.0;
  g_anyDim = false;
}

bool VehicleSidecar::ExceedsLimits(routing::NumMwmId mwmId, uint32_t featureId, uint32_t segmentIdx)
{
  if (!g_lookup || !g_anyDim)
    return false;

  char const * namePtr = ResolveMwmName(mwmId);
  if (!namePtr || !*namePtr)
    return false;
  // Copy before unlock races on WetlandSidecar::MwmName storage.
  std::string const name(namePtr);

  uint16_t maxH = 0, maxW = 0, maxL = 0, maxWt = 0, maxAx = 0, maxBg = 0;
  if (!g_lookup(name.c_str(), featureId, segmentIdx, &maxH, &maxW, &maxL, &maxWt, &maxAx, &maxBg))
    return false;

  if (StrictExceeds(MetresToCm(g_heightM), maxH))
    return true;
  if (StrictExceeds(MetresToCm(g_widthM), maxW))
    return true;
  if (StrictExceeds(MetresToCm(g_lengthM), maxL))
    return true;
  if (StrictExceeds(KgTo10kg(g_totalKg), maxWt))
    return true;
  if (StrictExceeds(KgTo10kg(g_axleKg), maxAx))
    return true;
  if (StrictExceeds(KgTo10kg(g_bogieKg), maxBg))
    return true;
  return false;
}

}  // namespace navi_spike
