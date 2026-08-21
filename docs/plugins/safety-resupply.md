# Safety plugin — resupply planning (specification)

**Status:** specification only — not implemented.  
**Path:** `docs/plugins/safety-resupply.md`  
**Architecture:** planned WASM guest via `plugin-host` / `plugin-sdk` and
capability-gated `HostApi` ([`plugins.md`](../plugins.md)). Planning logic stays
out of the trusted routing core; the host supplies route corridor samples, POI
hits, and (optionally) weather / user confirmations.
**System requirements** (all plugins): user **enable/disable** toggle; any
device link uses host-mediated **USB** / **Bluetooth**
([`plugins.md` — enable/disable](../plugins.md#enable--disable-required),
[USB/Bluetooth](../plugins.md#external-device-io--usb-and-bluetooth-required)).

Working title / id suggestion: `safety` / `resupply_safety`.

This page explains the plugin’s planning pipeline and how the pieces fit
together. It is adapted from the Navit *Safety* plugin design for Navi’s
offline-first Rust core and Android Automotive HUD — not a claim that the
plugin ships today.

**Not the same as** core [`SafetyConfig`](../architecture.md) (POI search radii,
overnight building/glacier distances). This plugin adds **fuel/water resupply
lookahead** with confidence scoring and remote/arid buffers.

---

## Disclaimer (must appear in the plugin UI)

Resupply plans are **conservative guidance**, not a guarantee that fuel, water,
or POIs will be available. Map and third-party data go stale; consumption varies
with weather, load, and driving style. **The user remains responsible** for
carrying adequate reserves and verifying stops.

---

## Pipeline overview

The plugin turns POI reliability and terrain into a conservative resupply plan:

1. **Configuration** — thresholds and buffers (conservative defaults).
2. **POI confidence scoring** — which stops may count as resupply.
3. **Remote-mode detection** — whether conservative planning applies.
4. **Buffer selection** — standard, remote, or arid buffer.
5. **Lookahead** — worst gap between reliable stops; warn if it exceeds usable
   range.
6. **Confirmation cache** — persist user confirmations across sessions.

The orchestrator (`safety_plan`, planned module) runs steps 3–5 for a single
resource (fuel or water) per call.

---

## Configuration

A plugin-owned config struct (suggested name `ResupplySafetyConfig`, to avoid
clashing with core `SafetyConfig`) holds the planning fields. Defaults should
match the cautious Navit-era values, for example:

| Setting | Default idea |
|---|---|
| Remote mode | `Auto` |
| Fuel buffers (standard / remote / arid) | 25 / 85 / 140 km |
| Water buffers (standard / remote / arid) | 5 / 20 / 30 km |
| POI density threshold (fuel) | ~80 km average spacing |
| POI density threshold (water, foot) | ~40 km (half of fuel) |

Defaults are intentionally cautious so users in populated areas rarely need to
change anything. Persist via host `plugin_kv` / `ConfigStore` (same pattern as
other plugin settings), not by mutating core rest/eco JSON keys.

---

## POI confidence scoring

`score_poi` maps a POI’s source and age to **High**, **Medium**, **Low**, or
**Unknown**:

- Live chain-operator APIs and NREL-class feeds rank **High**.
- An OSM / NREL match within 30 days is **High**, otherwise **Medium**.
- iOverlander within 60 days is **Medium**, otherwise **Low**.
- An OSM chain operator is **Medium**; an independent operator is **Medium**
  within 12 months and **Low** beyond that.
- Any source is capped at **Low** when the data is older than three years or the
  POI is in a known depopulating region (flag supplied by the caller when a
  census feed exists).
- A **user confirmation on the current trip** overrides everything to **High**.

Only **Medium** or **High** stops count as resupply
(`confidence_counts_as_resupply`). **Low** and **Unknown** stops are planned
around as if they did not exist.

Navi already indexes amenity/fuel/water-style POIs from OSM
([`poi.md`](../poi.md)); the plugin scores those records (plus any host-fed
live sources) rather than re-parsing the regional `.osm.pbf` inside WASM.

---

## Remote-mode detection

`koppen_lookup` classifies a coordinate into a Köppen zone using a compact,
built-in table of the major arid belts. It is a coarse approximation, not a
full gridded Köppen–Geiger dataset. `koppen_triggers_remote` returns true for
any group-B (desert / semi-arid) zone.

A second automatic signal is **POI density**: the orchestrator measures the
average spacing between confirmed (Medium-or-higher) stops over the route,
including the legs from the start to the first stop and from the last stop to
the destination. When that spacing exceeds `poi_density_threshold_km` (halved
for water on foot, matching the ~80 km fuel / ~40 km water defaults) the route
is treated as sparse.

Remote mode then follows configuration:

| Mode | Behaviour |
|---|---|
| **Always** | Remote planning is always on |
| **Auto** | On when the Köppen trigger is enabled and the sampled midpoint is group-B, **or** when POI density is sparse |
| **Off** | Remote planning is never auto-enabled |

In arid remote planning the result also carries a **desert consumption warning**
(gated by `desert_consumption_warning`) noting that real consumption may exceed
the kinematic / tank model.

---

## Buffer selection and usable range

With remote mode decided, the orchestrator selects the buffer: the **arid**
buffer in desert zones (when remote is active), the **remote** buffer for other
remote terrain, and the **standard** buffer in populated terrain. The **usable
range** is the full tank or water load minus the selected buffer (never
negative).

Vehicle tank / water capacity should come from host settings (e.g. fuel config /
profile defaults) via a read capability — not hard-coded in the guest.

---

## Lookahead gap detection

`lookahead_plan` walks the reliable stops in order and measures each leg,
including start→first and last→destination. It reports:

- the largest gap and where it begins,
- whether it exceeds the usable range,
- the shortfall (how far short).

That is what enables a **warning before departure** rather than discovering the
gap mid-journey. Surface the result in the Drive HUD / route tools UI (Compose),
not a Navit-style OSD widget.

---

## Heat stress and water (foot travel)

For hiking / foot profiles, a heat module maps a WBGT value to a risk level
(caution at 28 °C, warning at 32 °C, danger at 35 °C). Water use is estimated
from:

- a resting term scaled by `body_weight_kg`,
- an exertion term for strenuous activity,
- a heat term above 25 °C.

`heat_plan` combines these from configuration: the water requirement always uses
the configured body weight; the risk level and avoid-exertion flag are only
reported when `heat_stress_warnings` is enabled.

Until a weather plugin feeds live WBGT ([`plugins.md`](../plugins.md) weather
idea), the foot-travel temperature source may be limited to a manual WBGT value
from settings or a debug/test field.

---

## Confirmation cache

Confirmations are stored keyed by POI and trip so a stop the user verifies stays
**High** for the rest of the trip and across sessions. Prefer host
`plugin_kv` / SQLite beside the app data directory (same offline pattern as
`ConfigStore` / `navi.db`), gated so the plugin still builds when persistence is
unavailable (in-memory confirmations for the session only).

---

## Host capabilities (proposed)

Reuse today: `log`, `position_read`, `poi_query`.

| Proposed capability | Purpose |
|---|---|
| `route_read` | Active corridor samples / polyline for lookahead |
| `safety_config_read` | Core overnight distances if the UI shows them beside resupply advice |
| `fuel_config_read` / tank capacity | Usable range inputs |
| `plugin_kv` / `storage` | Confirmation cache + plugin settings |
| `weather_read` | Optional WBGT / ambient for heat model |
| `announce` / HUD chip write | Speak or show plan summary (host-owned TTS / Compose) |

Add each capability to `plugin-host` before shipping a guest that needs it.

---

## Live integration (planned)

When the plugin is enabled and a motor or hiking route is calculated:

1. Host scans the route corridor (Navit prior art: ~25 km sample spacing, ~50 km
   total width — tune for Navi’s bbox planner).
2. Collect fuel or water POIs from the onboard POI index.
3. Score them, run `safety_plan`, and push chips / warnings into the Drive HUD.
4. Redraw or refresh on route and position updates.
5. User actions: set remote mode, confirm or deny a POI, speak/show the plan.

Core routing must keep working with the plugin disabled (offline-first rule in
[`plugins.md`](../plugins.md)).

---

## Not yet wired (deferred even after a first guest lands)

- Chain-operator API queries (network client and API keys; host-side opt-in only).
- Bundled census depopulation dataset (scoring consumes a per-POI flag when
  callers supply it).
- Automatic weather / WBGT feed (manual value until weather plugin exists).

---

## Relation to other Navi docs

| Doc | Relation |
|---|---|
| [`plugins.md`](../plugins.md) | Host status, isolation, capability sketch |
| [`poi.md`](../poi.md) | OSM categories the scorer will consume |
| [`ECU.md`](../ECU.md) | Live fuel rate / SoC may refine usable range later |
| [`plugins/right-to-roam-camping-spec.md`](right-to-roam-camping-spec.md) | Overnight *camping* suggestions; different problem from fuel/water gaps |
| Core `SafetyConfig` | POI radii / building & glacier distance — shared overnight safety, not this pipeline |
