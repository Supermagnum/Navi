# Instrument cluster / AGL export plugin (specification)

**Status:** specification only — not implemented.  
**Path:** `docs/plugins/instrument-cluster-agl-spec.md`  
**Architecture:** planned WASM guest via `plugin-host` / `plugin-sdk` and
capability-gated `HostApi` ([`plugins.md`](../plugins.md)).

Working title / id suggestion: `instrument_cluster` / `agl_signal_export`.

This plugin **exports** Navi’s own navigation state outward to open-source
instrument clusters and Automotive Grade Linux (AGL) environments. It is **not**
an ECU / vehicle-input path ([`ECU.md`](../ECU.md) remains separate and deferred)
and **not** a full AGL Application Framework packaging effort (see § AGL scope).

---

## Goals

1. Publish a stable, read-only set of nav / vehicle-adjacent signals so a
   third-party cluster (or AGL HMI that already consumes VSS) can show speed,
   limits, next turn, ETA, break timing, eco status, **approach warnings**
   (Norwegian road signs, children-zone proximity, speed cameras, seasonal
   closures), overspeed chrome, and optionally a mini-map polyline.
2. Prefer **COVESA Vehicle Signal Specification (VSS)** via a
   **Kuksa.val-compatible databroker** as the primary interoperability layer.
3. Offer a **versioned JSON** fallback for hobbyist clusters that do not run
   Kuksa.val.
4. Keep all real network / IPC in the **trusted native host**; the WASM guest
   only decides *what* and *when* to publish through a narrow HostApi capability.

## Non-goals

- Reading fuel, engine, or cluster-originated vehicle data (ECU plugin).
- Packaging Navi as an AGL `afm` widget or integrating with Wayland/Weston
  surface sharing.
- Opening sockets, D-Bus, or gRPC from inside the sandboxed WASM module.
- Becoming a second source of truth for maneuvers / breaks / warnings — this
  plugin is a **third consumer** of the same guidance and warning state already
  used by the approach-instruction box
  ([`approach-instructions.md`](../approach-instructions.md)),
  `RoadSignWarningBox` / speed-camera chrome ([`road-signs.md`](../road-signs.md)),
  and planned voice guidance ([`voice-guidance.md`](../voice-guidance.md)).
- Rasterizing or shipping SVG icon bytes over VSS/JSON (publish `icon_key` /
  `code` / phase / distance; the cluster renders its own glyph or fetches a
  local asset pack).
- Playing alert audio (that belongs to
  [`custom-alert-sounds-spec.md`](custom-alert-sounds-spec.md) and
  [`adaptive-speed-warning-spec.md`](adaptive-speed-warning-spec.md)).

---

## Published signals (Navi → outside)

| Field | Source in Navi | Notes |
|---|---|---|
| Current speed | GPS / sensor thread (ground speed) | Same class of fix that feeds the map puck |
| Posted speed limit | Current road segment maxspeed when known | Omit / null when unknown — never invent |
| Overspeed (HUD) | `OverspeedHud.isOverspeed` | Display gate only (`MARGIN_KMH` / GNSS accuracy); not a spoken tier |
| Next maneuver type | Shared nav-guidance state | Icon / enum aligned with approach box |
| Next maneuver distance | Shared nav-guidance state | Metres internally |
| Next street name | Shared nav-guidance state | Prefer OSM `name`, else `ref`; omit if neither |
| Trip ETA / time-to-destination | Active route ETA | Only when a route is active |
| Time / distance to next required break | Same inputs as Drive HUD break line | **No-route guard:** must not publish stale break fields when no corridor is active (same rule as `formatBreakHudLine` / `routePlanned`) |
| Eco-mode on/off | Drive / planner eco setting | |
| Travel profile | Active `TravelProfile` | Navi-specific |
| Route polyline | Active planned corridor geometry | For clusters with a mini-map; may be empty |
| **Active approach warning** | Same merged chrome as the map host | Road sign, children-zone proximity, speed camera, or seasonal closure — see [§ Approach warnings](#approach-warnings-road-signs-cameras-closures) |

Publish cadence: host-driven poll of the guest at a modest rate (e.g. 1–5 Hz)
under existing fuel/timeout budgets — never starve T2 UI/audio
([`plugins.md`](../plugins.md) design rules).

---

## Approach warnings (road signs, cameras, closures)

Clusters that show only next-turn + speed miss the safety chrome already on
Navi’s map. Export the **same merged warning** the Android host would show —
do not invent a second distance clock or a second jurisdiction gate.

### Warning categories (v1 export)

| Category | Host source (today) | Notes |
|---|---|---|
| `road_sign` | `nearest_road_sign_warning_json` | Tagged `traffic_sign=NO:…` / `hazard=*`; Norway jurisdiction; 750 / 150 / 25 m phases ([`road-signs.md`](../road-signs.md)) |
| `children_proximity` | `nearest_school_proximity_warning_json` | Corridor fallback for `amenity=school` / `kindergarten` / `leisure=playground`; code `142`, `source=children_proximity`; explicit tagged `142` **outranks** this |
| `speed_camera_point` / `speed_camera_section` | `nearest_speed_camera_warning_json` | First-run opt-in + jurisdiction pack; section enter/exit fields when average-speed |
| `seasonal_closure` | Route-plan / conditional-access eval | When a hard-filtered conditional way affects the active corridor |
| `overspeed` | `OverspeedHud` + limit | Boolean (+ optional delta); not the adaptive spoken tier table |

**Merge priority** (must match Compose): explicit road sign &gt; children
proximity &gt; speed camera (same order documented for
[`custom-alert-sounds-spec.md`](custom-alert-sounds-spec.md)). Publish **one**
primary `warning` object for the cluster HMI; optional `warnings[]` may list
suppressed candidates for debug sinks only.

### Snapshot fields for the primary warning

| Field | Meaning |
|---|---|
| `category` | One of the ids above |
| `phase` | `appear` / `urgency` (omit or null when hidden / none) |
| `distance_m` | Along approach model |
| `code` | Catalogue code when applicable (e.g. `142`, `109`) |
| `icon_key` | Raster key (e.g. `no_sign_142`) — not SVG bytes |
| `label` / `name_en` | Same strings as `RoadSignWarningBox` / camera box |
| `source` | e.g. `children_proximity` vs tagged catalogue |
| Camera extras | `kind`, `applicable_limit_kmh`, `zone_remaining_m`, … when category is camera |

When no warning is active, set `warning` to `null` (and clear VSS warning
leaves). Do **not** leave a previous sign’s code/distance after hide / reroute /
jurisdiction decline.

Underskilt / compound-sign limitation from [`road-signs.md`](../road-signs.md)
applies: export the **base sign only** that the host shows.

---

## Protocol 1 — VSS + Kuksa.val (primary)

### Why VSS

AGL’s vehicle-signal access in current releases goes through **Kuksa.val** and
**VSS**. Hobbyist / COVESA-ecosystem clusters increasingly speak the same model.
Exporting VSS correctly therefore interoperates with AGL’s **data layer without
AGL-specific code** — a strong signal this is the right primary protocol.

References:

- [COVESA Vehicle Signal Specification](https://covesa.global/vehicle-signal-specification/)
- [COVESA/vehicle_signal_specification](https://github.com/COVESA/vehicle_signal_specification)
- Eclipse Kuksa.val databroker (gRPC / VISS-style clients)

### Standard VSS paths (use when they fit)

Exact leaf names evolve across VSS releases; plugins should pin a documented
**VSS catalog version** (e.g. v4.x / v5.x) in the host config. Illustrative
mappings:

| Navi field | Prefer standard path | Unit / type |
|---|---|---|
| Position | `Vehicle.CurrentLocation.Latitude` / `.Longitude` / `.Heading` / `.HorizontalAccuracy` / `.Timestamp` | deg, deg, deg, m, ISO-8601 |
| Speed | `Vehicle.Speed` | km/h (or catalog unit — convert explicitly) |
| Destination selected | `Vehicle.Cabin.Infotainment.Navigation.DestinationSet` (and children such as lat/lon where defined) | bool / coords |
| Nav mute / volume (if ever mirrored) | `Vehicle.Cabin.Infotainment.Navigation.Mute` / `.Volume` | optional; Navi may leave unset |

Stock VSS **Navigation** branches today emphasize destination / map / mute /
volume more than turn-by-turn guidance. **Do not** force next-maneuver, break
timers, eco, or polyline into ill-fitting standard leaves.

### Vendor extension namespace (Navi-specific)

Document and publish under a stable private overlay, for example:

```text
Vehicle.Private.Navi.*
```

(or an equivalent overlay overlay file merged into the host’s VSS catalog).

| Navi field | Suggested path |
|---|---|
| Next maneuver type | `Vehicle.Private.Navi.Guidance.NextManeuver.Type` |
| Next maneuver distance (m) | `Vehicle.Private.Navi.Guidance.NextManeuver.DistanceM` |
| Next street name | `Vehicle.Private.Navi.Guidance.NextManeuver.StreetName` |
| Trip ETA (unix s or ISO) | `Vehicle.Private.Navi.Trip.Eta` |
| Remaining trip duration (s) | `Vehicle.Private.Navi.Trip.RemainingDurationS` |
| Remaining trip distance (m) | `Vehicle.Private.Navi.Trip.RemainingDistanceM` |
| Minutes to break | `Vehicle.Private.Navi.Rest.MinutesToBreak` |
| Break distance (m), optional | `Vehicle.Private.Navi.Rest.DistanceToBreakM` |
| Rest fields valid | `Vehicle.Private.Navi.Rest.Active` (false when no route / reminders off) |
| Eco enabled | `Vehicle.Private.Navi.Eco.Enabled` |
| Travel profile | `Vehicle.Private.Navi.Eco.Profile` (string enum) |
| Route polyline (encoded) | `Vehicle.Private.Navi.Route.Polyline` (see JSON schema encoding) |
| Route active | `Vehicle.Private.Navi.Route.Active` |
| Overspeed (HUD) | `Vehicle.Private.Navi.Speed.Overspeed` (bool) |
| Warning active | `Vehicle.Private.Navi.Warning.Active` |
| Warning category | `Vehicle.Private.Navi.Warning.Category` |
| Warning phase | `Vehicle.Private.Navi.Warning.Phase` |
| Warning distance (m) | `Vehicle.Private.Navi.Warning.DistanceM` |
| Warning code | `Vehicle.Private.Navi.Warning.Code` |
| Warning icon key | `Vehicle.Private.Navi.Warning.IconKey` |
| Warning label | `Vehicle.Private.Navi.Warning.Label` |
| Warning source | `Vehicle.Private.Navi.Warning.Source` |

When `Route.Active` is false, clear or withhold Rest.* and trip remaining fields
(and set `Rest.Active=false`) so clusters never display a stale break countdown.
When no approach warning is shown, set `Warning.Active=false` and clear or
withhold the other `Warning.*` leaves.

---

## Protocol 2 — JSON broadcast (fallback)

For clusters that do not run Kuksa.val: the **trusted host** (not WASM) may
emit the same logical fields as versioned JSON over a **local-only** transport:

| Transport | Default idea | Notes |
|---|---|---|
| UDP broadcast | `127.0.0.1` or link-local multicast, fixed port (configurable) | Fire-and-forget; document MTU / polyline size limits |
| WebSocket | `ws://127.0.0.1:<port>/navi/v1` | Prefer loopback; bind interfaces explicitly in host settings |

No cloud upload. Opt-in in host settings; off by default (privacy /
[`plugins.md`](../plugins.md) rules).

### Schema `navi.cluster.v1`

```json
{
  "schema": "navi.cluster.v1",
  "ts": "2026-07-24T02:00:00Z",
  "position": {
    "lat": 60.079265,
    "lon": 11.149638,
    "heading_deg": 210.0,
    "speed_kmh": 72.5,
    "h_acc_m": 5.0
  },
  "speed_limit_kmh": 80,
  "overspeed": false,
  "route_active": true,
  "maneuver": {
    "type": "turn_right",
    "distance_m": 320,
    "street_name": "Storgata"
  },
  "warning": {
    "category": "children_proximity",
    "phase": "urgency",
    "distance_m": 67.2,
    "code": "142",
    "icon_key": "no_sign_142",
    "name_en": "Children",
    "label": "Children zone: Vallset skole",
    "source": "children_proximity"
  },
  "trip": {
    "eta": "2026-07-24T04:15:00Z",
    "remaining_duration_s": 5400,
    "remaining_distance_m": 98000
  },
  "rest": {
    "active": true,
    "minutes_to_break": 42,
    "distance_to_break_m": 56000
  },
  "eco": {
    "enabled": true,
    "profile": "Truck"
  },
  "route": {
    "polyline": "lat,lon;lat,lon;..."
  }
}
```

Example tagged road-sign `warning` (same schema, different category/source):

```json
{
  "category": "road_sign",
  "phase": "appear",
  "distance_m": 410,
  "code": "109",
  "icon_key": "no_sign_109",
  "name_en": "School",
  "label": "School",
  "source": "traffic_sign"
}
```

**Null / omit rules**

- `speed_limit_kmh`: omit or `null` if unknown.
- `overspeed`: `false` when limit unknown or HUD gate not met.
- `maneuver`: `null` when no upcoming guidance (same hide rules as approach box
  when far from a turn — or always publish next-in-route if the cluster prefers
  continuous guidance; host setting, default = match approach-box relevance).
- `warning`: `null` when no active approach chrome (hidden / passed / declined
  jurisdiction / cameras opted out). Never leave a previous sign after hide.
- When `route_active` is **false**: `trip` and `rest` must be `null` (or
  `rest.active=false` with other rest fields null). Never leave a previous
  trip’s break timer in the payload. Children-zone proximity and corridor
  cameras may still clear with no route; tagged roadside signs may remain if
  the host evaluates them on idle GPS (match map behaviour).
- `route.polyline`: empty string or omit when no active corridor; host may
  down-sample for UDP size.

Semver: breaking JSON changes bump `navi.cluster.vN`. Additive optional fields
(`warning`, `overspeed`) may appear within `v1` without a bump if clients
ignore unknowns.

---

## Sandbox design — host-mediated publish (critical)

WASM guests **must not** open sockets, D-Bus, or gRPC. WASI networking is not
linked in Navi’s plugin host ([`plugins.md`](../plugins.md)).

```text
  [ WASM plugin: instrument_cluster ]
           |  decides what/when
           v
  HostApi: vehicle_signal_publish / nav_state_read  (capability-gated)
           |
           v
  [ Trusted native host ]
      ├── Kuksa.val / VSS client   (primary)
      ├── JSON UDP / WebSocket    (fallback)
      └── optional log/no-op sink (tests)
```

The plugin’s job is orchestration and field selection. The host owns:

- connecting to the databroker,
- serialising VSS updates,
- binding local JSON transports,
- rate limits, authentication to Kuksa (if any), and user opt-in toggles.

This matches the existing isolation principle: a third-party `.wasm` never gets
arbitrary network access, even when the *feature* is “networking.”

### Proposed HostApi capabilities

| Capability | Direction | Purpose |
|---|---|---|
| `nav_guidance_read` (new) | host → guest | JSON blob: speed, limit, overspeed, maneuver, **merged warning**, ETA, break, eco, profile, polyline, `route_active` |
| `vehicle_signal_publish` (new) | guest → host | Publish one or more path/value pairs (VSS path string + typed value), or a whole `navi.cluster.v1` document for the JSON sink |
| `position_read` | existing | Fallback / corroboration |
| `log` | existing | Debug |

Host assembly for `nav_guidance_read` must reuse the same UniFFI warning
helpers the map already calls (`nearest_road_sign_warning_json`,
`nearest_school_proximity_warning_json`, `nearest_speed_camera_warning_json`,
jurisdiction / opt-in gates) — see [`API.md`](../API.md).

Suggested import sketch (exact ABI TBD when implemented):

```text
navi.nav_guidance_read(out_ptr, out_len) -> i32
navi.vehicle_signal_publish(ptr, len) -> i32   # UTF-8 JSON batch
```

Batch publish preferred over per-signal calls to stay within fuel budgets.

### Reference / no-op host sink (first implementation milestone)

When coding starts, ship a **log sink**: host prints or records published paths
without requiring a live Kuksa.val instance — same spirit as
`plugin-host/tests/isolation.rs` validating load/call without a third-party
ecosystem. Later swap the sink for real Kuksa / UDP backends behind the same
capability.

---

## Manifest / capability declaration

Publishing off-device (even to loopback) is a **non-default** capability. The
manifest must declare it explicitly; the existing load path already rejects
unknown or host-disallowed capabilities before instantiation
(`PluginHost::load_dir` + isolation tests).

Example `plugin.json`:

```json
{
  "name": "instrument_cluster",
  "version": "0.1.0",
  "entry": "plugin_main",
  "capabilities": [
    "log",
    "position_read",
    "nav_guidance_read",
    "vehicle_signal_publish"
  ],
  "fuel_limit": 5000000,
  "timeout_ms": 500,
  "wasm": "plugin.wasm"
}
```

UI requirement: before enabling, show that the plugin may export location,
speed, route guidance, and **approach warnings** (road signs, children-zone
proximity, speed cameras when opted in) to a **user-configured** local
databroker / JSON endpoint. No silent grant.

Until `vehicle_signal_publish` and `nav_guidance_read` exist on
`Capability` / HostApi, this guest **cannot** load — add the enum variants and
imports first ([`plugins.md`](../plugins.md) capability sketch).

---

## AGL scope boundary

| In scope | Out of scope |
|---|---|
| Export nav state as VSS signals a Kuksa.val client can write | Packaging Navi as an AGL `afm` application |
| Document that AGL already consumes VSS/Kuksa, so correct VSS export is AGL-data-compatible | Weston / Wayland surface sharing or embedding Navi’s MapLibre view into AGL |
| Optional JSON fallback for non-AGL hobby clusters | Replacing AGL’s own nav / IVI stack |

Larger future efforts (native AGL app, compositor integration) are legitimate
but separate projects.

---

## Relation to other docs

| Doc | Relation |
|---|---|
| [`plugins.md`](../plugins.md) | Host isolation, capability gating, roadmap entry |
| [`approach-instructions.md`](../approach-instructions.md) | Shared next-maneuver + 750 / 150 / 25 m phases |
| [`road-signs.md`](../road-signs.md) | Catalogue, jurisdiction, children-zone proximity merge |
| [`current-street.md`](../current-street.md) | Speed / limit / overspeed chrome |
| [`plugins/custom-alert-sounds-spec.md`](custom-alert-sounds-spec.md) | Same warning categories for audio; this plugin exports state only |
| [`plugins/adaptive-speed-warning-spec.md`](adaptive-speed-warning-spec.md) | Spoken overspeed tiers — not duplicated here beyond HUD `overspeed` bool |
| [`voice-guidance.md`](../voice-guidance.md) | Same maneuver guidance state, different consumer |
| [`hud-layout.md`](../hud-layout.md) / Drive HUD | Break / ETA / eco display rules; no-route guard |
| [`API.md`](../API.md) | UniFFI warning / speed-limit helpers the host snapshot must call |
| [`ECU.md`](../ECU.md#6-vehicle-side-protocol-reference-obd-2-can) | Opposite direction (vehicle → Navi); OBD-2/CAN DIY cluster reference — do not conflate |
| [`PROTOCOLS.md`](../PROTOCOLS.md) | Index entry for VSS/JSON export |

---

## Implementation checklist (when picked up)

1. Add `Capability::{NavGuidanceRead, VehicleSignalPublish}` + HostApi methods +
   linker wiring; extend isolation tests (deny without grant; allow with mock
   log sink).
2. Host: assemble `nav_guidance_read` snapshot from Android/Linux sensor +
   active route + guidance + rest HUD inputs + **merged approach warning**
   (road sign / children proximity / camera / closure) + HUD overspeed
   (respect `route_active`, jurisdiction, camera opt-in).
3. Reference WASM guest: read snapshot → `vehicle_signal_publish` batch each
   call (including `Vehicle.Private.Navi.Warning.*`); host logs paths.
4. Host backends behind a trait: `LogSink`, then `KuksaSink`, then `JsonUdpSink`
   / `JsonWsSink`.
5. Document pinned VSS catalog version + overlay `.vspec` for
   `Vehicle.Private.Navi.*` (including Warning.*).
6. User settings: enable plugin, choose sink, databroker URL / JSON port;
   default **off**.
7. Tests: Vallset-style children-zone / tagged `109` fixtures publish
   `warning` then clear to `null` after hide; opted-out cameras never appear.
