# External dashboards, instrument clusters, and head units

**Status:** **Shipped** — HostApi caps, `instrument-cluster` guest, guidance
assembly, opt-in Android session, USB accessory NDJSON writer, live JSON-UDP,
and a Kuksa/VSS TCP bridge (`navi.vss.batch.v1`, default `127.0.0.1:55556`) with
host/port settings. Use `scripts/cluster-kuksa-bridge.py` (or your own feeder)
to forward bridge lines into a Kuksa.val databroker if needed.

**Implementation tracking:** [GitHub issue #259](https://github.com/Supermagnum/Navi/issues/259).

| Spec / source | Role |
|---|---|
| [`plugins/instrument-cluster/`](../plugins/instrument-cluster/) | Reference WASM guest (`instrument-cluster`) — guidance → `navi.cluster.v1` via log sink |
| [`plugins/instrument-cluster-agl-spec.md`](plugins/instrument-cluster-agl-spec.md) | Export plugin detail (VSS / Kuksa.val + `navi.cluster.v1` JSON) |
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
| HostApi `nav_guidance_read` / `vehicle_signal_publish` | **In ABI** — assembly + selectable sinks |
| Reference guest `plugins/instrument-cluster` | **Shipped** (staged into APK assets; opt-in, default off) |
| Live HUD → guidance snapshot on device | **Wired** via `ClusterPluginRuntime` + `cluster_assemble_guidance_json` |
| USB accessory open/read/write for clusters | **Shipped** — AOA permission + NDJSON write (`ClusterUsbExporter`) |
| Live JSON-UDP | **Shipped** (configurable host/port, default loopback `:8765`) |
| Kuksa / VSS | **Shipped** as TCP bridge (`navi.vss.batch.v1`); optional feeder → gRPC databroker |
| Fuel L / Scandinavian mil and L / US mile on the export bus | **Not implemented** (reserved below; do not invent values in plugins) |

Enable **Instrument cluster export** in Map / plugin settings (default off),
pick a sink, then consume `navi.cluster.v1` as below. Timing and icon keys should
still match Navi’s on-device HUD.

---

## Related open-source dashboards

These projects are useful **consumers or companion UIs** for Navi’s export.
Navi does not vendor them; wire them yourself to `navi.cluster.v1` (JSON-UDP /
USB NDJSON) or to the Kuksa/VSS TCP bridge.

| Project | Link | Fit with Navi |
|---|---|---|
| **OpenCluster** | [emdzej/opencluster](https://github.com/emdzej/opencluster) | ESP32 + LVGL skinnable gauges; CAN cluster bus ([`CLUSTER_BUS_SPEC.md`](https://github.com/emdzej/opencluster/blob/main/docs/CLUSTER_BUS_SPEC.md)). Typical path: Navi → JSON → small bridge MCU → OpenCluster CAN (`0x101` speed, custom IDs for maneuver). See [example setup](#example-opencluster-style-dashboard-with-navi). |
| **/dev/dash (DevDash)** | [devdash-project/devdash](https://github.com/devdash-project/devdash) | Qt 6 / QML dual cluster + head-unit framework (Jetson or desktop). Add an `IProtocolAdapter` that parses `navi.cluster.v1` UDP/NDJSON into the DataBroker. |
| **FIS-Display** | [Supermagnum/FIS-Display](https://github.com/Supermagnum/FIS-Display) | VW Passat B6 FIS/MFA via Pico 2 W + 3LB. Historically Navit D-Bus → serial; with Navi, map `maneuver.*` / speed / eco from `navi.cluster.v1` into the same host→Pico serial lines. |

Also see the in-tree VSS overlay
[`plugins/vss/Vehicle.Private.Navi.vspec`](plugins/vss/Vehicle.Private.Navi.vspec)
and bridge helper [`scripts/cluster-kuksa-bridge.py`](../scripts/cluster-kuksa-bridge.py).

---

## Required signal set

Units below are the **wire** units. Display conversion (mph, mi, …) is the
cluster’s job unless noted.

| Signal | Wire field (JSON) | Source in Navi today | Export status |
|---|---|---|---|
| Speed | `position.speed_kmh` | GPS / route-sim ground speed (same as HUD) | On export bus when plugin enabled |
| Next maneuver type | `maneuver.type` | Shared nav-guidance / approach box | On export bus when plugin enabled |
| Maneuver icon | `maneuver.icon_key` | Navit-derived `nav_*` keys ([`icons.md`](icons.md), [`approach-instructions.md`](approach-instructions.md)) | On export bus when plugin enabled |
| Distance to next maneuver | `maneuver.distance_m` | Metres internally (same as approach box) | On export bus when plugin enabled |
| Eco on/off | `eco.enabled` | Drive / planner eco toggle | On export bus when plugin enabled |
| Fuel — Scandinavian mil | `fuel.l_per_scandinavian_mil` | — | **Reserved / not implemented** |
| Fuel — US mile | `fuel.l_per_us_mile` | — | **Reserved / not implemented** |
| Yaw | `attitude.yaw_deg` | IMU vehicle-frame yaw (see conventions) | On export bus when IMU + cluster enabled |
| Roll | `attitude.roll_deg` | IMU roll (right-side-down positive) | On export bus when IMU + cluster enabled |
| Heading | `attitude.heading_deg` | IMU magnetic heading (no declination) **or** GPS course when IMU off | On export bus when plugin enabled |

### Maneuver icons

Publish an **`icon_key`** (and a stable `type` enum). Do **not** send SVG bytes
over USB. The cluster renders from a local asset pack aligned with Navi’s
`nav_*` set ([`icons.md`](icons.md)). Approach-box relevance (appear ≤ 750 m,
urgency ≤ 150 m) is documented in [`approach-instructions.md`](approach-instructions.md);
the host may either match that hide rule or always send the next-in-route
maneuver (default = match the approach box).

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

1. User enables **Instrument cluster export** (opt-in; off by default) and
   grants the Android USB permission when using the USB accessory sink.
2. Host opens **USB accessory (AOA)** via `ClusterUsbExporter`, or sends
   JSON-UDP / Kuksa TCP to configured endpoints.
3. Host writes framed `navi.cluster.v1` JSON (or VSS bridge batches) at a modest
   rate (about **1–5 Hz**). Never starve map / audio.
4. Disabling the toggle or unplugging the cable closes the session.

**Not for this path:** Bluetooth-only companion tablets (allowed later via the
same accessory API), cloud upload, or ECU *inbound* OBD traffic.

CAT radio already bridges USB-serial on the host for Hamlib
([`CAT.md`](CAT.md)); cluster export should reuse that host USB pattern, not
open sockets from WASM.

### USB / UDP framing

Use **newline-delimited JSON** (one `navi.cluster.v1` object per line, UTF-8)
on USB accessory, or one UDP datagram per snapshot for JSON-UDP. Keep polyline
optional / down-sampled so a full snapshot fits a typical USB MTU budget. VSS /
Kuksa.val (via the TCP bridge) remains preferred on AGL HMIs that already speak
VSS ([`instrument-cluster-agl-spec.md`](plugins/instrument-cluster-agl-spec.md)).

---

## Example: OpenCluster-style dashboard with Navi

Goal: drive an [OpenCluster](https://github.com/emdzej/opencluster) ESP32 + LVGL
cluster from Navi without re-implementing routing. OpenCluster’s native bus is
CAN ([cluster bus spec](https://github.com/emdzej/opencluster/blob/main/docs/CLUSTER_BUS_SPEC.md));
Navi speaks `navi.cluster.v1` JSON. A thin **bridge** sits between them.

```text
  [ Phone / head unit: Navi ]
           |  instrument-cluster plugin (opt-in)
           |  sink: JSON-UDP  or  USB AOA NDJSON
           v
  [ Bridge: ESP32 / Pico / Linux SBC ]
           |  map JSON -> OpenCluster CAN
           |  e.g. speed -> 0x101, nav text -> custom IDs
           v
  [ OpenCluster display node(s) — LVGL gauges ]
```

### 1. Enable Navi export

1. Install a build that includes the `instrument-cluster` guest
   (`scripts/build-plugin-wasm.sh` stages it into the APK).
2. In Map / plugin settings, turn on **Instrument cluster export**.
3. Cycle the sink to **JSON UDP** (easiest for desk bring-up) or **USB accessory
   NDJSON** (cable in the car).
4. For JSON-UDP, set host/port to the bridge (example: bridge Wi‑Fi IP,
   port `8765`). Defaults are loopback `127.0.0.1:8765`.
5. Optionally enable the **IMU** plugin and calibrate if you want yaw / roll /
   heading on the cluster.

### 2. Desk bring-up (no car hardware)

On a laptop on the same LAN as the phone (USB tethering / hotspot also works):

```bash
# Listen for one snapshot (JSON-UDP sink)
python3 - <<'PY'
import socket
s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
s.bind(("0.0.0.0", 8765))
print("waiting for navi.cluster.v1 ...")
data, addr = s.recvfrom(65535)
print("from", addr)
print(data.decode("utf-8", errors="replace"))
PY
```

Or use the Kuksa/VSS TCP bridge sink and
[`scripts/cluster-kuksa-bridge.py`](../scripts/cluster-kuksa-bridge.py):

```bash
python3 scripts/cluster-kuksa-bridge.py --host 0.0.0.0 --port 55556
```

Confirm you see `"schema":"navi.cluster.v1"` (UDP) or
`Vehicle.Private.Navi.*` / `Vehicle.Speed` lines (bridge).

### 3. Bridge mapping (OpenCluster CAN)

OpenCluster expects master→display vehicle frames such as **`0x101` drivetrain**
(speed in km/h × 10). A minimal mapping from one Navi snapshot:

| Navi field | OpenCluster idea |
|---|---|
| `position.speed_kmh` | CAN `0x101` speed_hi/speed_lo = round(speed_kmh × 10) |
| `attitude.heading_deg` / `yaw_deg` | Custom ID or skin overlay (not in stock 0x100–0x102) |
| `maneuver.type`, `maneuver.icon_key`, `maneuver.distance_m`, `maneuver.street_name` | Custom CAN IDs or a side channel; render with a local `nav_*` icon pack ([`icons.md`](icons.md)) |
| `eco.enabled` | Cluster warning/flag bit or dedicated gauge |
| `warning.*` | Map to OpenCluster warning flags or a text strip |
| `fuel.*` | Omit until Navi publishes non-null values |

Pseudo-code for the bridge (run on the ESP32 master or a Linux SBC with SocketCAN):

```python
import json

def speed_0x101(doc: dict) -> bytes:
    speed = (doc.get("position") or {}).get("speed_kmh") or 0.0
    raw = max(0, min(3000, int(round(float(speed) * 10))))
    return bytes([(raw >> 8) & 0xFF, raw & 0xFF, 0, 0, 0, 0, 0, 0])

def on_navi_line(line: str) -> None:
    doc = json.loads(line)
    if doc.get("schema") != "navi.cluster.v1":
        return
    # send_can(0x101, speed_0x101(doc))
    man = doc.get("maneuver") or {}
    # send_can(0x1A0, encode_nav_hint(man))  # define 0x1A0 in your fork
```

Keep the OpenCluster display nodes as **passive listeners** on the cluster bus;
do not open sockets from Navi’s WASM guest.

### 4. DevDash and FIS-Display variants

**[/dev/dash](https://github.com/devdash-project/devdash)** — implement a
`NaviClusterAdapter : IProtocolAdapter` that binds UDP `8765` (or reads USB
CDC NDJSON), parses `navi.cluster.v1`, and publishes properties the QML cluster
already binds (speed, heading, turn distance). Use simulator mode until the
adapter is wired.

**[FIS-Display](https://github.com/Supermagnum/FIS-Display)** — keep the Pico +
3LB firmware; replace the Navit D-Bus host script with a small process that
reads Navi JSON-UDP/USB and emits the same newline ASCII serial protocol the
Pico already expects (maneuver icon, street, distance). Navi stays the nav
brain; the Pico stays the 3LB middleman.

### 5. Safety notes

- Export is **opt-in** and off by default; disabling stops publishes and closes
  USB.
- Do not invent fuel economy numbers.
- Vehicle / OEM bus work (FIS 3LB, car CAN) is at your own risk — validate on
  the bench first (same disclaimer as FIS-Display).

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
2. **Enable Instrument cluster export** and verify JSON-UDP or USB NDJSON with
   the listeners in the [OpenCluster example](#example-opencluster-style-dashboard-with-navi).
3. **Enable IMU** and calibrate if you need yaw / roll / heading from the mount.
4. **Bridge into your UI** — OpenCluster CAN, DevDash adapter, or FIS-Display
   serial — with a local map from `icon_key` → glyph ([`icons.md`](icons.md)).
5. **Treat `fuel.*` as optional** until Navi publishes non-null values.
6. Keep all sockets / USB / CAN in the **trusted host or bridge**, never in WASM
   ([`instrument-cluster-agl-spec.md`](plugins/instrument-cluster-agl-spec.md)).

---

## Related docs and projects

| Doc / project | Why |
|---|---|
| [`plugins/instrument-cluster-agl-spec.md`](plugins/instrument-cluster-agl-spec.md) | VSS paths, warnings, sandbox, AGL boundary |
| [`plugins/vss/Vehicle.Private.Navi.vspec`](plugins/vss/Vehicle.Private.Navi.vspec) | Pinned VSS overlay leaves |
| [`scripts/cluster-kuksa-bridge.py`](../scripts/cluster-kuksa-bridge.py) | TCP bridge → path=value feeder |
| [`plugins.md`](plugins.md) | USB/Bluetooth accessory rules |
| [`approach-instructions.md`](approach-instructions.md) | Maneuver phases and icon behaviour |
| [`icons.md`](icons.md) | Asset locations for `nav_*` keys |
| [`plugins/imu-plugin.md`](plugins/imu-plugin.md) | Pitch / roll / heading |
| [`hud-layout.md`](hud-layout.md) | On-device chrome layout (reference, not the wire format) |
| [`suggested-hardware.md`](suggested-hardware.md) | Head-unit notes |
| [OpenCluster](https://github.com/emdzej/opencluster) | ESP32 LVGL cluster (example above) |
| [DevDash](https://github.com/devdash-project/devdash) | Qt dual-display dashboard framework |
| [FIS-Display](https://github.com/Supermagnum/FIS-Display) | VW Passat B6 FIS via Pico + 3LB |
