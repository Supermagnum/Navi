# LoRa convoy status plugin (Meshtastic-based)

**Status:** specification only — not implemented.  
**Path:** `docs/plugins/lora-convoy-spec.md`  
**Architecture:** planned WASM guest via `plugin-host` / `plugin-sdk` and
capability-gated `HostApi` ([`plugins.md`](../plugins.md)). The Meshtastic
device API, both BLE links, and protobuf encode/decode live in the **trusted
native host** (same split as CAT serial adapters and ECU Bluetooth). Until the
[wasmtime upgrade gate](../plugins.md#gate-upgrade-wasmtime-before-shipping-any-product-plugin)
lands, a host-native service may push snapshots into core without a WASM
guest.
**System requirements** (all plugins): user **enable/disable** toggle; any
device link uses host-mediated **USB** / **Bluetooth**
([`plugins.md` — enable/disable](../plugins.md#enable--disable-required),
[USB/Bluetooth](../plugins.md#external-device-io--usb-and-bluetooth-required)).

Working title / id suggestion: `lora_convoy` / `convoy`.

Share location, speed, fuel level, and vehicle battery charge between vehicles
in a convoy over a LoRa mesh, so that one driver's low fuel is visible to the
rest of the convoy before it becomes a problem — without requiring the
vehicles to stay in direct radio range of each other.

---

## Disclaimer (must appear in the plugin UI)

Convoy status is **informational**. Mesh delivery is best-effort: packets can
be delayed, duplicated, dropped, or arrive out of order. Fuel and battery
percentages may be manual, stale, or wrong. This plugin is **not** a collision-
avoidance system, **not** a substitute for voice / visual contact, and **not**
a guarantee that another vehicle is where the last packet said it was. The
driver remains responsible for the vehicle.

---

## Goals

1. Gather this vehicle's own status from onboard sources and/or a passenger-
   operable Android companion, then broadcast it on a schedule over Meshtastic.
2. Listen for other vehicles' telemetry arriving via the mesh, keep a last-
   known table keyed by vehicle, and render it (map overlay + list, with a
   configurable low-fuel / low-charge warning).
3. Treat Meshtastic firmware as the radio and mesh layer. Navi is a client /
   display layer only — the same relationship Navi already has with the CAT
   plugin ([`CAT.md`](../CAT.md)).
4. Keep the Meshtastic radio BLE link and the Android companion BLE link as
   **two independent sessions**, even though both are BLE.
5. Structure inbound message dispatch so a later text-message packet type can
   be added without restructuring the telemetry path.

## Non-goals

- Implementing the plugin in this documentation pass.
- Writing flood routing, deduplication, hop-limiting, or rebroadcast-collision
  handling. Meshtastic firmware already does that.
- Talking to raw LoRa modules (SX127x / SX126x SPI, etc.). This plugin talks
  only to a Meshtastic node's device API.
- Extending Meshtastic's upstream `Telemetry` protobuf in firmware. Vehicle
  fuel and vehicle battery-charge have no existing Meshtastic equivalent
  (device battery is not vehicle fuel/charge). First pass uses a **private
  portnum** payload instead.
- Duplicating GNSS position in the custom payload when the node already
  publishes Meshtastic `Position`.
- Driver-facing data entry on the Navi device itself while moving (see
  [External Android input](#external-android-input-safety-requirement)).
- Showing convoy status on the Android companion (read-back). Write-only from
  Android to Navi is sufficient for this phase.
- Text messaging (compose/display). Transport exists in Meshtastic; Navi UI is
  out of scope here. Dispatch is structured so it can be added later.
- Linking `plugin-host` into a shipped binary before the wasmtime gate.

---

## Why Meshtastic, not raw LoRa

LoRa's chirp-spread-spectrum modulation gives range but not mesh behaviour —
that has to be built separately. Rather than writing flood routing,
deduplication, hop-limiting, and rebroadcast-collision handling from scratch,
this plugin reuses Meshtastic firmware and hardware for the LoRa radio and
mesh layer. Navi never touches raw radio packets.

This is the same client/display split as CAT: the transceiver (or here, the
Meshtastic node) owns RF; Navi owns pairing UI, encoding of Navi-side state,
and display.

---

## Relationship to existing Navi surfaces

| Surface | Role today | This plugin |
|---|---|---|
| CAT ([`CAT.md`](../CAT.md), `cat` in [`plugins.md`](../plugins.md)) | Host-mediated serial/USB to a mobile transceiver; Navi does not implement the radio | **Same shape.** Host-mediated BLE to a Meshtastic node; Navi does not implement LoRa or mesh routing. |
| ECU ([`ECU.md`](../ECU.md)) | Future `LiveEnergySnapshot` (fuel rate / SoC) from OBD/J1939 | **Onboard source** for `fuel_pct` / `battery_pct` when a snapshot exists. Convoy broadcast is a *consumer* of that snapshot, not a second ECU poller. |
| DIY e-bike telemetry ([`ebike-telemetry-diy.md`](../ebike-telemetry-diy.md)) | Wired `$NAVIPWR` SoC | Same: optional onboard source for `battery_pct` on Electric Cycle. |
| APRS `TrackStore` ([`APRS.md`](../APRS.md), `core/src/tracks/`) | Station overlay, 50–150 km display clamp, timeout | **Reuse the overlay pattern** (upsert by id, last-heard, range clamp). Do **not** overload APRS symbol keys. Convoy rows carry fuel/battery/seq in a dedicated store. |
| Safety / resupply ([`safety-resupply.md`](safety-resupply.md)) | Pre-departure fuel-gap lookahead on the planned corridor | Unrelated. Convoy status is live peer telemetry, not POI gap analysis. |
| Plugin enable/disable | Required for every plugin | Disabled = no BLE sessions, no mesh TX, no overlay. |

Display range follows the same ~150 km class of filters as tracks and CAT
repeater search ([`plugins.md` design rule 8](../plugins.md#design-rules-for-all-plugins))
unless the user raises a documented clamp. A typical convoy is much smaller;
the clamp is an upper bound, not a promise of mesh diameter.

---

## Dependencies

| Piece | Role |
|---|---|
| Rust crate [`meshtastic`](https://crates.io/crates/meshtastic) (`meshtastic/rust`) | Device API client. Enable **`bluetooth-le`** and **`tokio`**. Default convoy deployment: BLE to a Meshtastic node (tablets/phones). **USB serial** (e.g. Meshstick on a USB-host head unit) uses the same crate path; TCP remains for bench only. |
| Meshtastic firmware | Flashed on the LoRa radio (any currently supported Meshtastic board). This plugin talks to that node's device API. |
| Android companion app (out of tree) | Passenger-operable BLE writer for manual `fuel_pct` / `battery_pct`. Not the Meshtastic Android app. |
| Host BLE central | Opens two separate GATT sessions (radio vs companion). WASM never opens BLE. |

The `meshtastic` crate is **not** in the workspace today. When implementation
starts, add it as an unaltered crates.io dependency (host-native service, not
inside WASM) and list it under Planned in [`crates.md`](../crates.md).

---

## Recommended radio hardware

The convoy path is a **Meshtastic-flashed node** paired to Navi over the
device API. BLE is the default deployment on tablets and phones; **USB serial**
is the natural fit on Android head units that expose a USB host port (OTG).

### Meshstick (USB, recommended for head units)

**Meshstick USB-To-SPI SX1262 TCXO LoRa USB Stick** — USB plug-and-play
Meshtastic / LoRa mesh node (SX1262 + TCXO).

| Property | Why it matters for convoy |
|---|---|
| USB plug-and-play | Suits car head units and other Android hosts with USB host; no separate BLE pairing step for the radio link when wired. |
| SX1262 + TCXO | Current-generation LoRa modem with stable frequency reference for mesh timing. |
| Unique device identity | Each stick has a distinct identity suitable for Meshtastic node id / convoy store keys. |
| Secure traceability | Factory or supply-chain traceability supports fleet inventory and accountability (which physical node belongs to which vehicle). |
| Flat mounting face | One flat side is well suited to a windscreen or other flat surface with a sticky pad — practical in-cab placement without a bulky bracket. |

Confirm Meshtastic firmware support and the exact USB serial device path on
the target head unit at implementation time. Bench/debug may use the same
stick on a desktop host via USB serial (`meshtastic` crate).

### BLE Meshtastic boards (tablets / phones)

Any Meshtastic-supported board with a working **BLE device API** remains
valid for convoy use (e.g. common ESP32-based nodes). Prefer boards with
stable GATT MTU and documented Meshtastic BLE behaviour. Mounting is
board-specific; the Meshstick’s flat face is called out above as a deliberate
vehicle-mounting advantage.

---

## Architecture

```text
[Onboard sources: GPS, ECU/e-bike snapshot]
        |
        v
[Navi host: merge + encode VehicleStatus]
        |                           ^
        |                           |  BLE GATT (session B, companion)
        |                           |
        |                  [Android companion: manual fuel/battery]
        |
        v
[Meshtastic node (BLE session A)] --> LoRa mesh
        |
        v
[other vehicles' Meshtastic nodes] --> [their Navi: decode + table + UI]
```

Each vehicle runs Navi plus a Meshtastic-flashed radio. Navi's job:

1. Gather this vehicle's own status (onboard and/or Android companion).
2. Push it out as a mesh packet on a schedule (private portnum; see
   [Data model](#data-model)).
3. Listen for other vehicles' telemetry (and Position) arriving via the mesh.
4. Maintain a last-known status table and render it.

Everything below that — LoRa modulation, flood routing, duplicate suppression,
hop-count limits, SNR-based rebroadcast delay — is Meshtastic firmware.

```text
Host process (trusted)
  convoy_mesh_session     -- BLE central --> Meshtastic GATT (radio)
  convoy_companion_session -- BLE central --> companion GATT (phone)
  convoy_dispatch         -- PortNum switch: Position | ConvoyStatus | (future Text)
  convoy_store            -- keyed by vehicle_id, seq, last_heard
  overlay / HUD           -- warn if fuel_pct / battery_pct below threshold

WASM guest (after wasmtime gate)
  tick: read store snapshot, decide warn chips / list sort
  must not open BLE or raw sockets
```

---

## Data model

### Position (do not duplicate)

Use Meshtastic's built-in `Position` (`PortNum::PositionApp = 3`) when the
node has GNSS. Latitude / longitude in that message are i32 at 1e-7 degrees
(Meshtastic convention). Navi already has a host GNSS fix; prefer the
**Meshtastic Position** for *other* vehicles (that is what travelled over
LoRa). For *this* vehicle's outgoing Position, let the node send its own
Position as configured in firmware; Navi should not emit a second Position
payload unless the node has no GNSS and the host fix is opted-in for mesh TX.

### Convoy status (new payload)

Fuel percentage and **vehicle** battery-charge percentage have no Meshtastic
equivalent (`DeviceMetrics.battery_level` is the radio's battery). Do **not**
extend upstream `telemetry.proto` in firmware for the first pass — that would
couple Navi to a Meshtastic firmware fork.

Use a **private portnum** in the 256–511 range (`PortNum::PrivateApp = 256` is
the documented start of the private band). Encode a compact payload Navi
controls:

```text
VehicleStatus {
    vehicle_id: u8,     // convoy-facing id; see open questions
    speed_kmh: u8,      // 0–255; values above 255 km/h clamp
    fuel_pct: u8,       // 0–100, or 255 = unknown
    battery_pct: u8,    // 0–100 vehicle traction/LV charge, or 255 = unknown
    seq: u16,           // monotonic per sender; wrapping compare on receive
    source: u8,         // 0 = Onboard, 1 = AndroidManual
}
```

Keep the encoded size small (airtime). First-pass encoding: protobuf or a
fixed 8-byte little-endian layout. Either is fine as long as every Navi in
the convoy shares the same codec and portnum. Prefer protobuf if the host
already depends on `prost` via `meshtastic`; otherwise the 8-byte layout is
enough.

`lat` / `lon` are **not** in this struct. Join Position and VehicleStatus in
the store by Meshtastic node id (and/or `vehicle_id`). If Position has not
arrived yet, show status without a map marker.

`source` records which input won for fuel/battery on that packet (see
[Merge rule](#merge-rule-first-pass)).

### Identity

Meshtastic node ids are 32-bit (derived from the radio MAC). A convoy-facing
`u8` is for the UI (1–8 cars, human-assigned). First-pass recommendation:

- **Wire identity** for dedup / store key: Meshtastic node id (`u32`).
- **Display id:** optional convoy-assigned `u8` carried in `VehicleStatus`.
  If unset (0), UI shows the node short name from `NodeInfo`.

Whether those stay 1:1 is an [open question](#open-questions--decisions-needed-before-implementation).
Until decided, key the table by node id and treat `vehicle_id` as a label.

---

## Transmit path

1. Plugin enabled and Meshtastic session up; otherwise no TX.
2. On a configurable interval (default starting point: **45 s**; user-tunable;
   see airtime below), build `VehicleStatus` from the merge rule.
3. Host asks the Meshtastic node to send on the private portnum (broadcast on
   the configured channel). Hop limit is the node's/channel default unless the
   user sets a convoy-specific hop limit in plugin settings (do not hard-code
   a hop count that fights firmware).
4. Do **not** send on every GNSS fix. Interval + “send immediately if
   fuel/battery crosses the warning threshold” is enough.
5. Honour plugin disable: close the radio session and stop TX.

Privacy ([`plugins.md` rule 7](../plugins.md#design-rules-for-all-plugins)):
location on the mesh is opted-in by enabling this plugin. There is no
phone-home of VIN, callsign, or location to the internet. Channel PSK is
Meshtastic's; Navi does not invent a second crypto layer.

### Airtime budget

LoRa airtime on typical Meshtastic “LongFast” settings is on the order of
one second per small packet, multiplied by hops. Defaults must stay
conservative:

| Setting | First-pass default | Notes |
|---|---|---|
| Broadcast interval | 45 s | Configurable; floor e.g. 15 s to prevent accidental flood |
| Payload | Position (firmware) + ~8–20 byte status | No verbose JSON on air |
| Extra TX | On crossing warn threshold | Rate-limit extra TX (e.g. at most one extra per interval) |

Tune interval against the convoy's modem preset; document the chosen preset
in plugin settings (read-only from node config if the API exposes it).

---

## Receiving side logic

Table keyed by Meshtastic **node id**, storing the most recent `VehicleStatus`,
the most recent `Position`, and `last_heard` (host monotonic and/or Unix
time).

- Replace a stored `VehicleStatus` only if incoming `seq` is **newer** using
  unsigned wrapping compare (`wrapping_sub` / half-range). That handles
  packets arriving out of order via different mesh paths, and u16 wrap.
- Replace Position independently (Meshtastic Position has its own time fields).
  A newer status with no new Position keeps the last Position.
- Mark an entry **stale** in the UI if no update arrives within
  `timeout = 3 * broadcast_interval` (use the local interval as the estimate
  if peer interval is unknown). Keep the row; do not delete immediately.
  Drop from the overlay after a longer expiry (align with `TrackStore`
  timeout, clamped to `STATION_TIMEOUT_MAX_S`).
- Configurable warning threshold on `fuel_pct` and `battery_pct` (defaults
  e.g. 20). Unknown (`255`) never warns. A stale low-fuel row should still
  warn, with stale chrome, so a missed packet does not hide a known problem.
- Range: same 50–150 km clamp class as `TrackStore` unless the user raises a
  documented clamp.

---

## Merge rule (first pass)

Onboard sensors (ECU SoC / fuel level, e-bike `$NAVIPWR`, host GNSS speed)
and Android companion writes are merged with **last-write-wins** on
`fuel_pct` / `battery_pct`. Whichever value arrived most recently is what
goes on the next mesh packet. `source` records that winner.

Override-vs-supplement (companion only fills in when no onboard source
exists) is a later refinement, not required now. Speed always comes from
host GNSS / last fix, not from Android.

---

## External Android input (safety requirement)

The driver must not interact with Navi's own device while driving to report a
manual fuel reading. That input goes through a passenger-operable Android
companion.

### Pairing precedent

Navi's other hardware plugins (ECU, CAT, e-bike) treat Navi as the **BLE
central / serial client** and the accessory as the peripheral. Match that:

- The companion app advertises a well-known Navi convoy-input GATT service.
- The host pairing UI lists it like any other accessory; the user selects it.
- Navi opens `accessory_open(..., mode=ble)` for that device id.

Do **not** fold this into the Meshtastic session. Different device, different
service UUIDs, different handler.

(If implementation finds that advertising from the phone is awkward in
practice, flipping so Navi is the GATT server is an allowed local change —
keep the two handlers separate either way.)

### First-pass characteristics

Write-only, Android to Navi. No notify/indicate in this phase.

| Characteristic | Type | Meaning |
|---|---|---|
| `fuel_pct` | uint8 | 0–100; ignore >100 except a documented “unknown” sentinel if needed |
| `battery_pct` | uint8 | 0–100 vehicle charge |

Service and characteristic UUIDs are assigned at implementation (document in
this spec when frozen). Companion app is out of tree; Navi only specifies the
GATT contract.

Read-back (showing convoy status on the phone) is a future extension.

---

## UI (Navi)

- Map overlay: one marker per non-expired peer, distinct from APRS icons.
- List / HUD chip: vehicle label, speed, fuel %, battery %, age, stale flag.
- Low fuel / low charge: visually obvious (colour + optional alert-sound
  category later). Must not require anyone to open a submenu to notice.
- Settings: enable, pair radio, pair companion, interval, warn thresholds,
  display range.
- Debug files: `Documents/debug/lora-convoy/`
  ([`plugins.md` debug files](../plugins.md#debug-files-usbmtp)).

---

## Future: text messaging

Not implemented in this phase. Meshtastic already has `PortNum::TextMessageApp = 1`.
When this is picked up, remaining work is Navi UI (compose/display) and
whether manual Android input and text entry share the companion BLE path.

### Dispatch now (required)

Inbound host decode switches on `PortNum` **before** touching `VehicleStatus`:

```text
match portnum {
    PositionApp     -> convoy_store.upsert_position(...)
    PrivateApp/convoy -> convoy_store.upsert_status(...)  // seq gate
    TextMessageApp  -> queue for future UI (drop or log in this phase)
    other           -> ignore
}
```

Do not parse telemetry inside the Position handler or Position inside the
status handler. A later text path adds a third arm only.

---

## Host vs guest

| Duty | Owner |
|---|---|
| BLE open/close, Meshtastic `StreamApi`, protobuf on the wire | Host |
| Companion GATT client session | Host (separate from radio) |
| Merge onboard + companion; interval TX | Host |
| `convoy_store` + seq/stale logic | Host (so WASM timeout cannot drop packets) |
| Overlay / list / warn chrome | Host UI; guest may format chips after wasmtime gate |
| Enable/disable, pairing consent | Host UI |

Proposed capabilities (not in ABI yet):

| Capability | Purpose |
|---|---|
| `accessory_*` | Two BLE sessions (radio, companion) |
| `convoy_status_read` | Snapshot of the last-known table for the guest |
| `convoy_warn_config_read` / `plugin_kv` | Thresholds, interval |
| `position_read` | This vehicle's fix for outgoing speed / optional Position |
| `ecu_read` / `ebike_telemetry_read` | Optional onboard fuel/SoC |
| `log` | Host log; durable files under `Documents/debug/lora-convoy/` |

TX of mesh packets is **host-gated** (plugin enabled). This is intentional
TX, not CAT PTT; still no TX when disabled.

---

## Safety and RF

- Default: telemetry broadcast only; no automatic text, no remote control of
  other vehicles.
- Do not key an amateur voice transmitter (CAT remains separate).
- Meshtastic legal/ISM band and duty-cycle limits are the operator's
  responsibility; Navi only keeps interval/payload conservative.
- Confirm the actual board at implementation time — BLE pairing and MTU vary
  slightly by Meshtastic-supported hardware.

---

## Implementation checklist (future)

1. Host-native Meshtastic BLE session (`bluetooth-le` + `tokio`); honour
   enable/disable (close on off).
2. Private portnum + `VehicleStatus` codec; Position join by node id.
3. `convoy_store` with wrapping `seq`, stale timeout, range clamp.
4. Overlay + list + warn thresholds.
5. Second BLE session: companion GATT writes for fuel/battery; last-write-wins.
6. Interval TX + threshold-crossing extra TX with rate limit.
7. Dispatch switch that already names a text-message arm.
8. USB-visible debug under `Documents/debug/lora-convoy/`.
9. After wasmtime gate: optional WASM guest for chip formatting only.

---

## Open questions / decisions needed before implementation

1. Does `vehicle_id` map 1:1 to Meshtastic's node id, or does the convoy need
   a separate human-assigned id scheme? (Recommendation above: node id as
   store key, `u8` as optional label.)
2. Broadcast interval — configurable; default 45 s is a starting point, not
   measured on the convoy's modem preset.
3. Should manual Android fuel/battery input override onboard readings, or
   only fill in when no onboard source exists? First pass is last-write-wins;
   this question is the later refinement.
4. Recommended radio hardware is documented in
   [Recommended radio hardware](#recommended-radio-hardware) (Meshstick USB
   for head units; BLE boards for tablets). Confirm BLE vs USB session wiring
   and MTU on the actual board at implementation time.
5. Frozen GATT UUIDs and whether Navi stays BLE central for the companion.
6. Whether outgoing Position is always the node's GNSS or may use the host
   GNSS when the node has no GPS module.
