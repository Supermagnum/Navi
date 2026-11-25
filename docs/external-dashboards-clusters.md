# External dashboards, instrument clusters, and head units

**Status:** integration **design** is in-tree; a live USB export service is
**not shipped** yet. This page tells integrators what Navi can supply, which
fields are ready in the app today, and how a cable-connected dashboard or
cluster is intended to receive them.

**Implementation tracking:** [GitHub issue #259](https://github.com/Supermagnum/Navi/issues/259).

| Spec / source | Role |
|---|---|
| [`plugins/instrument-cluster-agl-spec.md`](plugins/instrument-cluster-agl-spec.md) | Planned export plugin (VSS / Kuksa.val + `navi.cluster.v1` JSON) |
| [`plugins.md`](plugins.md) — USB / Bluetooth | Host-mediated accessory I/O (USB-serial / AOA required for cable) |
| [`approach-instructions.md`](approach-instructions.md) | Next maneuver, icon, distance (implemented on the map) |
| [`plugins/imu-plugin.md`](plugins/imu-plugin.md) | Pitch / roll / heading (implemented, opt-in) |
| [`API.md`](API.md) | UniFFI helpers the host already uses for warnings / guidance |

**Do not confuse with:** CAT radio (USB to a transceiver), ECU / OBD input
([`ECU.md`](ECU.md)), or mirroring Navi’s full MapLibre surface into another OS.

---

## What this is for

Navi runs as the navigation brain on a phone, tablet, or Android head unit.
An **external dashboard**, **instrument cluster**, or second head-unit display
can show a subset of guidance without re-implementing routing:

- current speed
- next maneuver type + matching icon key
- distance to that maneuver
- eco mode on / off
- attitude: yaw / roll / heading (from the IMU mount model)
- (reserved) fuel use per Scandinavian mil and per US mile — see below

Transport for a wired cluster is a **USB cable**. The host opens USB (serial or
accessory); guests never talk to the wire directly.

---

## Implementation status (honest)

| Layer | Today |
|---|---|
| Speed, eco, next maneuver, icon, distance on Navi’s own HUD | **Implemented** |
| IMU pitch / roll / tilt-compensated heading | **Implemented** (opt-in plugin) |
| `navi.cluster.v1` / VSS field list | **Specified** |
| HostApi `nav_guidance_read` / `vehicle_signal_publish` | **Specified** — not in the ABI yet |
| USB accessory open/read/write for clusters | **Specified** in [`plugins.md`](plugins.md); CAT already uses host USB-serial as a pattern |
| Live USB stream of cluster JSON / VSS | **Not shipped** |
| Fuel L / Scandinavian mil and L / US mile on the export bus | **Not implemented** (reserved below; do not invent values in plugins) |

Until the export sink ships, integrators can prototype against the JSON schema
in this doc and the cluster spec, and against Navi’s on-device HUD behaviour as
the reference for timing and icon keys.

---

## Required signal set

Units below are the **wire** units. Display conversion (mph, mi, …) is the
cluster’s job unless noted.

| Signal | Wire field (JSON) | Source in Navi today | Export status |
|---|---|---|---|
| Speed | `position.speed_kmh` | GPS / route-sim ground speed (same as HUD) | Ready in app; not yet streamed |
| Next maneuver type | `maneuver.type` | Shared nav-guidance / approach box | Ready in app; not yet streamed |
| Maneuver icon | `maneuver.icon_key` | Navit-derived `nav_*` keys ([`icons.md`](icons.md), [`approach-instructions.md`](approach-instructions.md)) | Ready in app; not yet streamed |
| Distance to next maneuver | `maneuver.distance_m` | Metres internally (same as approach box) | Ready in app; not yet streamed |
| Eco on/off | `eco.enabled` | Drive / planner eco toggle | Ready in app; not yet streamed |
| Fuel — Scandinavian mil | `fuel.l_per_scandinavian_mil` | — | **Reserved / not implemented** |
| Fuel — US mile | `fuel.l_per_us_mile` | — | **Reserved / not implemented** |
| Yaw | `attitude.yaw_deg` | IMU vehicle-frame yaw (see conventions) | IMU live; not yet on cluster bus |
| Roll | `attitude.roll_deg` | IMU roll (right-side-down positive) | IMU live; not yet on cluster bus |
| Heading | `attitude.heading_deg` | IMU magnetic heading (no declination) **or** GPS course when IMU off | IMU / GPS; not yet on cluster bus |

### Maneuver icons

Publish an **`icon_key`** (and a stable `type` enum). Do **not** send SVG bytes
over USB. The cluster renders from a local asset pack aligned with Navi’s
`nav_*` set ([`icons.md`](icons.md)). Approach-box relevance (appear ≤ 750 m,
urgency ≤ 150 m) is documented in [`approach-instructions.md`](approach-instructions.md);
the host may either match that hide rule or always send the next-in-route
maneuver (configurable when the sink lands; default = match the approach box).

### Attitude conventions

From [`plugins/imu-plugin.md`](plugins/imu-plugin.md):

- Vehicle frame: +X right, +Y forward, +Z up
- Pitch: nose-up positive
- Roll: right-side-down positive
- Heading: magnetic degrees (no declination correction in v1)

Wire naming for integrators:

| Requested name | IMU / wire meaning |
|---|---|
| **Yaw** | Heading about vertical (`yaw_deg` / magnetic heading) |
| **Roll** | `roll_deg` |
| **Heading** | Same as yaw for flat-road dashboards; prefer IMU when the plugin is on and calibrated |

Calibrate the mount once (level garage floor) before trusting pitch/roll.

### Fuel usage (reserved — not implemented)

These fields are part of the **intended** cluster payload so dashboards can show
consumption in two mile systems. They are **not** computed or published today.
Do **not** stub fake numbers in camping or other plugins.

| Field | Meaning |
|---|---|
| `fuel.l_per_scandinavian_mil` | Litres per **Scandinavian mil** (1 mil = **10 km**) |
| `fuel.l_per_us_mile` | Litres per **US statute mile** (1 mi ≈ 1.609344 km) |

When fuel economy is implemented later, values should derive from the same
consumption model the eco / tank UI already uses (canonical litres in core),
converted explicitly. Until then clusters must omit `fuel` or set both leaves
to `null`.

---

## Transport: USB cable

Clusters and external dashboards that sit on a wire use **USB**:

1. User enables the future **Instrument cluster** export toggle (opt-in; off by
   default) and grants the Android USB permission for the accessory.
2. Host opens **USB-serial** or **USB accessory (AOA)** — same mediation model as
   [`plugins.md`](plugins.md) (`accessory_open` / `read` / `write` / `close`).
3. Host writes framed `navi.cluster.v1` JSON (or VSS updates) at a modest rate
   (about **1–5 Hz**). Never starve map / audio.
4. Disabling the toggle or unplugging the cable closes the session.

**Not for this path:** Bluetooth-only companion tablets (allowed later via the
same accessory API), cloud upload, or ECU *inbound* OBD traffic.

CAT radio already bridges USB-serial on the host for Hamlib
([`CAT.md`](CAT.md)); cluster export should reuse that host USB pattern, not
open sockets from WASM.

### Suggested USB framing (when coding starts)

Until a binary framing is fixed, use **newline-delimited JSON** (one
`navi.cluster.v1` object per line, UTF-8). Keep polyline optional / down-sampled
so a full snapshot fits a typical USB-serial MTU budget. VSS / Kuksa.val remains
the preferred path on AGL HMIs that already speak VSS
([`instrument-cluster-agl-spec.md`](plugins/instrument-cluster-agl-spec.md)).

---

## `navi.cluster.v1` snapshot (integrator view)

Additive fields for attitude and reserved fuel relative to the cluster spec:

```json
{
  "schema": "navi.cluster.v1",
  "ts": "2026-11-25T09:30:00Z",
  "position": {
    "lat": 60.56218,
    "lon": 11.25604,
    "heading_deg": 12.0,
    "speed_kmh": 72.5,
    "h_acc_m": 5.0
  },
  "attitude": {
    "yaw_deg": 12.0,
    "roll_deg": -1.5,
    "pitch_deg": 0.8,
    "heading_deg": 12.0,
    "imu_calibrated": true
  },
  "maneuver": {
    "type": "turn_right",
    "icon_key": "nav_turn_right",
    "distance_m": 320,
    "street_name": "Kvikneveien"
  },
  "eco": {
    "enabled": true,
    "profile": "Car"
  },
  "fuel": {
    "l_per_scandinavian_mil": null,
    "l_per_us_mile": null
  },
  "route_active": true
}
```

Full warning / trip / rest / polyline rules stay in
[`instrument-cluster-agl-spec.md`](plugins/instrument-cluster-agl-spec.md).
Null / omit rules for unknown speed limits, inactive routes, and hidden
maneuvers apply unchanged.

---

## How an integrator should proceed

1. **Match Navi’s HUD** for speed, eco leaf, and approach icon/distance on a
   device running a current sideload (`compiled/navi-release.apk`).
2. **Enable IMU** and calibrate if you need yaw / roll / heading from the mount.
3. **Implement a USB-serial consumer** that parses `navi.cluster.v1` lines; keep
   a local map from `icon_key` → glyph.
4. **Treat `fuel.*` as optional** until Navi publishes non-null values.
5. When contributing the host sink: add HostApi capabilities and a log/USB sink
   first (no WASM network); follow the sandbox diagram in the cluster spec.
   Do not change unrelated plugins to fake fuel economy.

---

## Related docs

| Doc | Why |
|---|---|
| [`plugins/instrument-cluster-agl-spec.md`](plugins/instrument-cluster-agl-spec.md) | VSS paths, warnings, sandbox, AGL boundary |
| [`plugins.md`](plugins.md) | USB/Bluetooth accessory rules |
| [`approach-instructions.md`](approach-instructions.md) | Maneuver phases and icon behaviour |
| [`icons.md`](icons.md) | Asset locations for `nav_*` keys |
| [`plugins/imu-plugin.md`](plugins/imu-plugin.md) | Pitch / roll / heading |
| [`hud-layout.md`](hud-layout.md) | On-device chrome layout (reference, not the wire format) |
| [`suggested-hardware.md`](suggested-hardware.md) | Head-unit notes |
