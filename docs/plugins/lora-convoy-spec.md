# LoRa mesh party plugin (Meshtastic-based)

**Status:** specification only — not implemented.
**Path:** `docs/plugins/lora-mesh-party-spec.md`
**Supersedes:** `docs/plugins/lora-convoy-spec.md` (renamed and generalized — see
[Relationship to the earlier convoy-only spec](#relationship-to-the-earlier-convoy-only-spec)).
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

Working title / id suggestion: `mesh_party` / `party` (core roster layer).
Convoy vehicle telemetry is a separate optional extension: `party_convoy`.

Share a live roster — who's on the mesh, what to call them, and where they
are — between members of a group over Meshtastic, so anyone in range knows
who else is present without requiring direct radio line-of-sight. Convoy
vehicles additionally share fuel, battery, and speed so one driver's low
fuel is visible to the rest of the group before it becomes a problem.

---

## Disclaimer (must appear in the plugin UI)

Party status is **informational**. Mesh delivery is best-effort: packets can
be delayed, duplicated, dropped, or arrive out of order. Fuel and battery
percentages may be manual, stale, or wrong. This plugin is **not** a collision-
avoidance system, **not** a substitute for voice / visual contact, and **not**
a guarantee that another member is where the last packet said they were. Each
person/vehicle remains responsible for themself.

---

## Relationship to the earlier convoy-only spec

The original draft of this document scoped everything to vehicles. Two
things changed that:

1. **Everything except fuel/battery/speed is not vehicle-specific.** A roster
   of "who is node X, what do I call them, where are they, what icon do they
   get" is exactly as useful to hunters, hikers, search-and-rescue teams, and
   herders as it is to a convoy of cars. Splitting it out means those groups
   get the plugin without carrying vehicle fields they'll never use.
2. **Pairing in plain sight (a parking lot, a trailhead, a muster point)
   turned out to need a different safeguard than GPS-stationary gating.**
   See [Identity claim flow](#identity-claim-flow).

So the plugin is now two layers:

- **Core (`mesh_party`):** channel provisioning by PIN, identity claim,
  roster (id ↔ node id ↔ name ↔ role ↔ position ↔ last-heard), map overlay
  with per-role icons. Useful standalone for any group.
- **Optional extension (`party_convoy`):** adds `VehicleStatus`
  (fuel/battery/speed) on top of a roster entry, for members who are
  vehicles. A hiking party or hunting group simply never enables this
  extension; the roster and dispatch switch don't care either way.

Other role-specific extensions (a herder's "collar has no button" proxy
claim, a SAR "need assistance" flag) follow the same optional-extension
pattern — see [Other role-specific extensions](#other-role-specific-extensions-future).

---

## Goals

1. Let one member of a group provision a shared, private Meshtastic channel
   for the whole group by having everyone type the **same short PIN** —
   no QR scanning required.
2. Let each member claim a small integer **party id**, mapped to a
   human-chosen **name**, via a short, visually-confirmed ritual that works
   even when every member is standing in the same parking lot or trailhead
   in plain sight of each other.
3. Maintain a **roster**: party id ↔ Meshtastic node id ↔ name ↔ role ↔
   last-known position ↔ last-heard, and render it as a map overlay with a
   role-appropriate icon per member.
4. (Convoy extension) Gather a vehicle's own fuel/battery/speed from onboard
   sources and/or a passenger-operable Android companion device, broadcast
   it on a schedule, and warn when any convoy member is low.
5. Treat Meshtastic firmware as the radio and mesh layer. Navi is a client /
   display layer only — the same relationship Navi already has with the CAT
   plugin ([`CAT.md`](../CAT.md)).
6. Keep the Meshtastic radio BLE link and the Android companion BLE link as
   **two independent sessions**, even though both are BLE.
7. Structure inbound message dispatch so a later text-message packet type
   (and future role-specific extensions) can be added without restructuring
   the roster or telemetry paths.

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
- Duplicating GNSS position in custom payloads when the node already
  publishes Meshtastic `Position`.
- Duplicating member names in custom payloads. Names ride on Meshtastic's
  existing `NodeInfo` (`long_name`/`short_name`); the roster reads them from
  the node-id-keyed `NodeInfo` cache instead of carrying a name field.
- **PIN-derived channel provisioning is a collision-avoidance convenience,
  not a security mechanism.** A 4-character alphanumeric PIN is a small
  keyspace (~1.7M combinations at base-36). It is enough to keep an
  unrelated nearby group from accidentally landing in the same roster; it is
  not cryptographic secrecy. See [Channel provisioning](#channel-provisioning-pin-derived).
- Driver-facing data entry on the Navi device itself while moving (see
  [External companion input](#external-companion-input-safety-requirement)).
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
| ECU ([`ECU.md`](../ECU.md)) | Future `LiveEnergySnapshot` (fuel rate / SoC) from OBD/J1939 | **Onboard source** for `fuel_pct` / `battery_pct` when a snapshot exists (convoy extension only). |
| DIY e-bike telemetry ([`ebike-telemetry-diy.md`](../ebike-telemetry-diy.md)) | Wired `$NAVIPWR` SoC | Same: optional onboard source for `battery_pct` on Electric Cycle (convoy extension only). |
| APRS `TrackStore` ([`APRS.md`](../APRS.md), `core/src/tracks/`) | Station overlay, 50–150 km display clamp, timeout | **Reuse the overlay pattern** (upsert by id, last-heard, range clamp). Do **not** overload APRS symbol keys. Roster rows carry their own icon/role and live in a dedicated store. |
| Safety / resupply ([`safety-resupply.md`](safety-resupply.md)) | Pre-departure fuel-gap lookahead on the planned corridor | Unrelated. Convoy status is live peer telemetry, not POI gap analysis. |
| Plugin enable/disable | Required for every plugin | Disabled = no BLE sessions, no mesh TX, no overlay. |

Display range follows the same ~150 km class of filters as tracks and CAT
repeater search ([`plugins.md` design rule 8](../plugins.md#design-rules-for-all-plugins))
unless the user raises a documented clamp. A typical party/convoy is much
smaller; the clamp is an upper bound, not a promise of mesh diameter.

---

## Dependencies

| Piece | Role |
|---|---|
| Rust crate [`meshtastic`](https://crates.io/crates/meshtastic) (`meshtastic/rust`) | Device API client. Enable **`bluetooth-le`** and **`tokio`**. Default deployment: BLE to a Meshtastic node (tablets/phones). **USB serial** (e.g. Meshstick on a USB-host head unit) uses the same crate path; TCP remains for bench only. |
| Meshtastic firmware | Flashed on the LoRa radio (any currently supported Meshtastic board). This plugin talks to that node's device API, including sending the **admin** message used for PIN-derived channel provisioning. |
| Android companion app (out of tree) | Passenger-operable BLE writer for manual `fuel_pct` / `battery_pct` (convoy extension), or proxy identity claims for members with no screen/button (see [herder proxy claim](#other-role-specific-extensions-future)). Not the Meshtastic Android app. |
| Host BLE central | Opens two separate GATT sessions (radio vs companion). WASM never opens BLE. |

The `meshtastic` crate is **not** in the workspace today. When implementation
starts, add it as an unaltered crates.io dependency (host-native service, not
inside WASM) and list it under Planned in [`crates.md`](../crates.md).

---

## Recommended radio hardware

The party/convoy path is a **Meshtastic-flashed node** paired to Navi over the
device API. BLE is the default deployment on tablets and phones; **USB serial**
is the natural fit on Android head units that expose a USB host port (OTG).

### Meshstick (USB, recommended for head units)

**[Meshstick USB-To-SPI SX1262 TCXO LoRa USB Stick](https://www.elecrow.com/meshstick-usb-to-spi-sx1262-tcxo-lora-usb-stick-usb-plug-and-play-meshtastic-lora-mesh-node.html)**
(Elecrow) — USB plug-and-play Meshtastic / LoRa mesh node (SX1262 + TCXO).

| Property | Why it matters |
|---|---|
| USB plug-and-play | Suits car head units and other Android hosts with USB host; no separate BLE pairing step for the radio link when wired. |
| SX1262 + TCXO | Current-generation LoRa modem with stable frequency reference for mesh timing. |
| Unique device identity | Each stick has a distinct identity suitable for Meshtastic node id / roster keys. |
| Secure traceability | Factory or supply-chain traceability supports fleet inventory and accountability (which physical node belongs to which member). |
| Flat mounting face | One flat side is well suited to a windscreen or other flat surface with a sticky pad — practical in-cab placement without a bulky bracket. |

Confirm Meshtastic firmware support and the exact USB serial device path on
the target head unit at implementation time. Bench/debug may use the same
stick on a desktop host via USB serial (`meshtastic` crate).

### BLE Meshtastic boards (tablets / phones / handheld for hikers, hunters)

Any Meshtastic-supported board with a working **BLE device API** remains
valid (e.g. common ESP32-based nodes). Prefer boards with stable GATT MTU
and documented Meshtastic BLE behaviour. Mounting is board-specific; the
Meshstick's flat face is called out above as a deliberate vehicle-mounting
advantage. For foot-based members (hikers, hunters, herders on a handheld),
a small handheld board with its own battery is the natural fit; no vehicle
mounting is implied.

---

## Architecture

```text
[One-time: PIN-derived channel provisioning]
   Member A types PIN --> Navi derives {channel_name, psk} --> admin write to node A
   Member B types same PIN --> same derivation --> admin write to node B
   ... (node reboots after channel config changes)

[Per-session: identity claim]
   Member presses "Claim ID" --> claim window opens (visual + optional
   haptic/LED) --> PartyIdClaim broadcast --> peers see "new — confirm?"
   in roster --> group visually confirms which physical unit is which

[Ongoing]
[Onboard sources: GPS, (convoy) ECU/e-bike snapshot]
        |
        v
[Navi host: merge + encode roster/status]
        |                           ^
        |                           |  BLE GATT (session B, companion)
        |                           |
        |                  [Android companion: manual fuel/battery,
        |                   or proxy claim for a collar/tag with no UI]
        |
        v
[Meshtastic node (BLE session A)] --> LoRa mesh
        |
        v
[other members' Meshtastic nodes] --> [their Navi: decode + roster + UI]
```

Each member runs Navi plus a Meshtastic-flashed radio. Navi's job:

1. Provision the shared channel once, from a PIN (see below).
2. Claim a party id and broadcast it during a visually-confirmed window.
3. Gather this member's own status (onboard and/or Android companion) —
   convoy extension only.
4. Push status out as a mesh packet on a schedule (private portnum; see
   [Data model](#data-model)).
5. Listen for other members' identity claims, telemetry, and `Position`
   arriving via the mesh.
6. Maintain a roster / last-known status table and render it with
   role-appropriate icons.

Everything below that — LoRa modulation, flood routing, duplicate suppression,
hop-count limits, SNR-based rebroadcast delay — is Meshtastic firmware.

```text
Host process (trusted)
  channel_provision       -- admin write: derive {name, psk} from PIN, push to node
  party_mesh_session      -- BLE central --> Meshtastic GATT (radio)
  party_companion_session -- BLE central --> companion GATT (phone), convoy extension
  party_dispatch          -- PortNum switch: Position | PartyIdClaim | ConvoyStatus | (future Text)
  party_roster            -- keyed by node_id: party_id, name (from NodeInfo), role, position, last_heard
  overlay / HUD           -- role icon per member; warn if fuel_pct / battery_pct below threshold

WASM guest (after wasmtime gate)
  tick: read roster snapshot, decide icon/warn chip layout, list sort
  must not open BLE, raw sockets, or send the admin channel-provision message
```

---

## Channel provisioning (PIN-derived)

**Problem it solves:** groups like hunters, hikers, herders, or convoy
members meeting at a trailhead or parking lot want to get everyone onto the
same private Meshtastic channel without QR-scanning each other's phones.

**Mechanism:** a short PIN (default 4 alphanumeric characters, longer
optional) is run through a key-derivation function (HKDF-SHA256 or
equivalent, with a fixed Navi-specific salt/context string) to
deterministically produce a channel name and a valid-format PSK:

```text
pin: "K7X2"
  -> HKDF(pin, salt="navi-party-v1")
  -> channel_name: "navi-k7x2"   (derived, human-legible enough to sanity-check)
  -> psk: 32 bytes                (valid AES-256 key; low-entropy input, not a security claim)
```

Any member who types the same PIN derives the identical `{name, psk}` pair.
Navi then sends the standard Meshtastic **admin `SetChannel`** message to
push that channel config to the member's own node — the same mechanism the
Meshtastic app itself uses when a user scans a channel QR code, just
triggered by typed input instead of a camera scan.

- **Not a security boundary.** A 4-character alphanumeric PIN is a small
  keyspace. Its job is to keep an unrelated nearby group's mesh from
  colliding with this one by accident, not to keep out a motivated
  eavesdropper. State this in the UI (e.g. "Anyone with this PIN nearby can
  join — use a longer PIN for stronger privacy"). Longer PINs/passphrases
  feed the same KDF; only the UI changes.
- **LoRa region and modem preset are untouched by this flow.** Those remain
  a one-time per-node setup outside the PIN flow (see
  [Why Meshtastic, not raw LoRa](#why-meshtastic-not-raw-lora) — region/
  preset must already match for any two nodes to hear each other at all).
- **No separate Create/Join step.** Because derivation is deterministic,
  there's no real "creation" — just "first person types it, everyone else
  types the same thing." A single "Set party PIN" action covers both cases.
- **Expect a reboot.** Changing channel config on a Meshtastic node
  typically restarts the radio. The UI should say so up front rather than
  let the user wonder why the node dropped off mid-flow.
- This is the **only** admin-write capability this plugin needs; everything
  else (claims, status, position) is ordinary mesh TX/RX, not device
  reconfiguration.

---

## Identity claim flow

**Problem it solves:** naive designs (e.g. gating claims on GPS-stationary
state) don't work when the whole group is standing together in a parking
lot or trailhead — GPS drift and "is that vehicle actually stopped" are the
wrong signals. In plain sight, the real risk is **misattribution**: two
people claiming an id close together in time, or a claim landing on the
wrong physical unit in the roster. The fix is a short, operator-gated,
*visually confirmable* ritual, not a motion check.

### Ritual

1. Member presses **"Claim ID"** on their unit (or, for a member with no
   screen/button, a companion app claims on their behalf — see
   [Other role-specific extensions](#other-role-specific-extensions-future)).
2. The unit enters a **~10–15 s claim window** and shows an unmistakable
   local indicator — a large on-screen id/color, plus a distinct LED or
   vibration pattern on hardware that supports it. This indicator, not the
   PIN and not motion state, is the actual collision guard: everyone
   standing nearby can see which physical unit is claiming right now.
3. Navi suggests the **lowest unclaimed id** in its local roster; the member
   confirms or overrides before the claim broadcasts.
4. `PartyIdClaim` broadcasts on the private portnum.
5. Peers receiving a claim during their own idle state show it immediately
   in the roster with a **"new — confirm?"** affordance, so a mistake can be
   caught on the spot while the group is still together.
6. **Only one claim window open at a time**, enforced socially ("wait your
   turn") — matches how these groups already run muster-point roll calls;
   no protocol-level lock is needed.
7. If a claim arrives for an id **already held by a different node id**,
   surface a conflict warning ("id 1 also claimed by node X") rather than
   silently overwriting — the same seq-gate philosophy as `VehicleStatus`,
   applied to identity instead of telemetry.

### Naming

Do **not** carry a name string in `PartyIdClaim` — that would duplicate
Meshtastic's own `NodeInfo` (`long_name` / `short_name`, sent via
`PortNum::NodeinfoApp`), cost extra airtime, and need its own truncation
rules. Instead:

- The member sets their node's long name (e.g. "John motorcycle", "Sara
  truck", "Claus car") using ordinary Meshtastic tooling (the Meshtastic
  app, or Navi surfacing the same admin field) before or during setup.
- Navi caches `NodeInfo` per node id (it needs this cache anyway to join
  `Position` and status by node id) and reads the name out of it for
  display: `party_id 1 -> node_id -> NodeInfo.long_name -> "John motorcycle"`.

### Persistence

Node id is stable hardware identity. Once a `party_id ↔ node_id` mapping is
claimed, Navi persists it locally keyed by node id. A group that regroups
next week doesn't need to redo the ritual — the roster repopulates as each
node reappears on the mesh. "Claim ID" is only needed for new members or to
reassign an id.

---

## Data model

### Position (do not duplicate)

Use Meshtastic's built-in `Position` (`PortNum::PositionApp = 3`) when the
node has GNSS. Latitude / longitude in that message are i32 at 1e-7 degrees
(Meshtastic convention). Navi already has a host GNSS fix; prefer the
**Meshtastic Position** for *other* members (that is what travelled over
LoRa). For *this* member's outgoing Position, let the node send its own
Position as configured in firmware; Navi should not emit a second Position
payload unless the node has no GNSS and the host fix is opted-in for mesh TX.

### Identity claim (new payload — core)

```text
PartyIdClaim {
    party_id: u8,     // 1, 2, 3... suggested by Navi as lowest unclaimed, confirmable
    role: u8,          // 0 = vehicle, 1 = hiker, 2 = hunter, 3 = herder/handler,
                        // 4 = SAR personnel, 5 = animal/collar (proxy-claimed), ...
    pin_tag: [u8; 4],  // ASCII; matches the derived channel's PIN as a display-only
                        // sanity check, not an auth check (the channel PSK already
                        // gates who can decode this packet at all)
}
```

`role` selects the roster's map/list icon. Node id (`from` field on the mesh
packet) plus the cached `NodeInfo.long_name` supplies the display name —
see [Identity claim flow](#identity-claim-flow).

### Roster (core store)

Keyed by Meshtastic **node id**:

```text
RosterEntry {
    node_id: u32,          // wire identity / store key
    party_id: u8,          // human-facing label from PartyIdClaim
    role: u8,
    name: &str,            // from cached NodeInfo.long_name; not stored in any Navi packet
    position: Position?,   // most recent, independent of status
    last_heard: Time,
    claimed_at: Time,
    stationary_hint: bool, // best-effort, see note below; never gates the claim
}
```

`stationary_hint` is optional telemetry a member's own unit may include
(e.g. "my GNSS displacement has been under ~5–8 m for the last 10–15 s") so
peers can display a caution chip on a claim made while a unit appeared to be
moving. It is advisory only and never blocks or delays a claim — the
visual-confirm ritual is the real safeguard, not motion state.

### Convoy status (optional extension payload)

Fuel percentage and **vehicle** battery-charge percentage have no Meshtastic
equivalent (`DeviceMetrics.battery_level` is the radio's battery). Do **not**
extend upstream `telemetry.proto` in firmware for the first pass — that would
couple Navi to a Meshtastic firmware fork.

Use a **private portnum** in the 256–511 range (`PortNum::PrivateApp = 256`
is the documented start of the private band), a different sub-type from
`PartyIdClaim` on the same band. Encode a compact payload Navi controls:

```text
VehicleStatus {
    party_id: u8,       // matches the roster entry claimed above
    speed_kmh: u8,       // 0–255; values above 255 km/h clamp
    fuel_pct: u8,        // 0–100, or 255 = unknown
    battery_pct: u8,     // 0–100 vehicle traction/LV charge, or 255 = unknown
    seq: u16,            // monotonic per sender; wrapping compare on receive
    source: u8,          // 0 = Onboard, 1 = AndroidManual
}
```

Keep the encoded size small (airtime). First-pass encoding: protobuf or a
fixed layout. Either is fine as long as every member in the party shares the
same codec and portnum. Prefer protobuf if the host already depends on
`prost` via `meshtastic`; otherwise a fixed byte layout is enough.

`lat` / `lon` are **not** in this struct. Join Position and `VehicleStatus`
in the roster by Meshtastic node id (via `party_id`). If Position has not
arrived yet, show status without a map marker.

`source` records which input won for fuel/battery on that packet (see
[Merge rule](#merge-rule-convoy-extension)).

---

## Transmit path

1. Plugin enabled and Meshtastic session up; otherwise no TX.
2. **Channel provisioning** (one-time or on PIN change): admin `SetChannel`
   write, expect node reboot.
3. **Identity claim**: on "Claim ID" press, open the local claim window,
   show the visual indicator, broadcast `PartyIdClaim` once the member
   confirms the suggested/overridden id.
4. **(Convoy extension) Status**: on a configurable interval (default
   starting point: 4 s; user-tunable; see airtime below), build
   `VehicleStatus` from the merge rule and send. Do **not** send on every
   GNSS fix — interval plus "send immediately if fuel/battery crosses the
   warning threshold" is enough. Rate-limit the extra send (e.g. at most one
   extra per interval).
5. Hop limit is the node's/channel default unless the user sets a
   party-specific hop limit in plugin settings (do not hard-code a hop
   count that fights firmware).
6. Honour plugin disable: close the radio session and stop TX.

Privacy ([`plugins.md` rule 7](../plugins.md#design-rules-for-all-plugins)):
location on the mesh is opted-in by enabling this plugin. There is no
phone-home of identity, callsign, or location to the internet. Channel PSK
(whether QR-shared or PIN-derived) is Meshtastic's; Navi does not invent a
second crypto layer beyond the documented PIN-derivation convenience above.

### Airtime budget

LoRa airtime on typical Meshtastic "LongFast" settings is on the order of
one second per small packet, multiplied by hops. Defaults must stay
conservative:

| Setting | First-pass default | Notes |
|---|---|---|
| Broadcast interval (convoy status) | 4 s | Configurable; floor e.g. 15 s to prevent accidental flood |
| Identity claim | one-off per claim, not periodic | Re-broadcast only on explicit override/reassign |
| Payload | Position (firmware) + ~8–20 byte status/claim | No verbose JSON on air |
| Extra TX (convoy) | On crossing warn threshold | Rate-limit extra TX (e.g. at most one extra per interval) |

Tune interval against the party's modem preset; document the chosen preset
in plugin settings (read-only from node config if the API exposes it).

---

## Receiving side logic

Roster keyed by Meshtastic **node id**, storing the most recent
`PartyIdClaim`, the most recent `VehicleStatus` (if the convoy extension is
in use), the most recent `Position`, and `last_heard` (host monotonic and/or
Unix time).

- **Claims:** accept a new `party_id` claim for a node id not already in the
  roster. For a `party_id` already held by a *different* node id, surface a
  conflict rather than overwrite (see [ritual step 7](#ritual)).
- **Status (convoy):** replace a stored `VehicleStatus` only if incoming
  `seq` is **newer** using unsigned wrapping compare (`wrapping_sub` /
  half-range). That handles packets arriving out of order via different mesh
  paths, and u16 wrap.
- **Position:** replace independently (Meshtastic Position has its own time
  fields). A newer status/claim with no new Position keeps the last
  Position.
- Mark an entry **stale** in the UI if no update arrives within
  `timeout = 3 * broadcast_interval` (convoy extension; use the local
  interval as the estimate if peer interval is unknown). Keep the row; do
  not delete immediately. Drop from the overlay after a longer expiry (align
  with `TrackStore` timeout, clamped to `STATION_TIMEOUT_MAX_S`).
- Configurable warning threshold on `fuel_pct` and `battery_pct` (convoy
  extension; defaults e.g. 20). Unknown (`255`) never warns. A stale
  low-fuel row should still warn, with stale chrome, so a missed packet does
  not hide a known problem.
- Range: same 50–150 km clamp class as `TrackStore` unless the user raises a
  documented clamp.

---

## Merge rule (convoy extension)

Onboard sensors (ECU SoC / fuel level, e-bike `$NAVIPWR`, host GNSS speed)
and Android companion writes are merged with **last-write-wins** on
`fuel_pct` / `battery_pct`. Whichever value arrived most recently is what
goes on the next mesh packet. `source` records that winner.

Override-vs-supplement (companion only fills in when no onboard source
exists) is a later refinement, not required now. Speed always comes from
host GNSS / last fix, not from the companion app.

---

## External companion input (safety requirement)

The driver must not interact with Navi's own device while driving to report
a manual fuel reading. That input goes through a passenger-operable Android
companion device. The same companion path is reused for a **proxy identity
claim** on behalf of a member with no screen or button — see
[Other role-specific extensions](#other-role-specific-extensions-future).

### Pairing precedent

Navi's other hardware plugins (ECU, CAT, e-bike) treat Navi as the **BLE
central / serial client** and the accessory as the peripheral. Match that:

- The companion app advertises a well-known Navi GATT service.
- The host pairing UI lists it like any other accessory; the user selects it.
- Navi opens `accessory_open(..., mode=ble)` for that device id.

Do **not** fold this into the Meshtastic session. Different device, different
service UUIDs, different handler.

(If implementation finds that advertising from the phone is awkward in
practice, flipping so Navi is the GATT server is an allowed local change —
keep the two handlers separate either way.)

### First-pass characteristics

Write-only, companion device to Navi. No notify/indicate in this phase.

| Characteristic | Type | Meaning |
|---|---|---|
| `fuel_pct` | uint8 | 0–100; ignore >100 except a documented "unknown" sentinel if needed |
| `battery_pct` | uint8 | 0–100 vehicle charge |
| `proxy_claim_role` | uint8 | present only when claiming on behalf of a screenless member (see below) |

Service and characteristic UUIDs are assigned at implementation (document in
this spec when frozen). Companion app is out of tree; Navi only specifies
the GATT contract.

Read-back (showing party/convoy status on the phone) is a future extension.

---

## UI (Navi)

- Map overlay: one marker per non-expired roster member, icon selected by
  `role`, distinct from APRS icons.
- List / HUD chip: member label, role icon, (convoy) speed/fuel %/battery %,
  age, stale flag.
- Low fuel / low charge (convoy extension): visually obvious (colour +
  optional alert-sound category later). Must not require anyone to open a
  submenu to notice.
- **Set party PIN** screen: single action, type PIN, confirm derived channel
  name shown back for sanity-check, expect a node reboot.
- **Claim ID** screen: large id/color indicator during the claim window,
  suggested lowest-unclaimed id with override, and a peers' "new — confirm?"
  affordance rendered on everyone else's roster view.
- Settings: enable, pair radio, pair companion, party PIN, claim/reassign id,
  role, (convoy) interval, warn thresholds, display range.
- Debug files: `Documents/debug/lora-party/`
  ([`plugins.md` debug files](../plugins.md#debug-files-usbmtp)).

---

## Future: text messaging

Not implemented in this phase. Meshtastic already has `PortNum::TextMessageApp = 1`.
When this is picked up, remaining work is Navi UI (compose/display) and
whether manual companion input and text entry share the companion BLE path.

### Dispatch now (required)

Inbound host decode switches on `PortNum` **before** touching any specific
payload:

```text
match portnum {
    PositionApp        -> party_roster.upsert_position(...)
    PrivateApp/claim    -> party_roster.upsert_claim(...)      // conflict check
    PrivateApp/convoy   -> party_roster.upsert_status(...)     // seq gate, convoy extension only
    TextMessageApp      -> queue for future UI (drop or log in this phase)
    other                -> ignore
}
```

Do not parse one payload type inside another type's handler. A later text
path, or a later role-specific extension, adds a new arm only.

---

## Other role-specific extensions (future)

Same optional-extension pattern as convoy status; not designed in this pass,
flagged so the dispatch switch and roster schema don't need restructuring
when they land:

- **Herder / handler proxy claim.** A dog or livestock collar/tag typically
  has no screen or button. The handler's companion app claims an id and role
  ("collar" / "livestock") on the animal's behalf via the
  `proxy_claim_role` companion characteristic above; the resulting
  `PartyIdClaim` still originates from the collar's own Meshtastic node
  (assuming it has one) or, if the collar has no radio of its own, is
  broadcast by the handler's own node with a distinguishing role — a
  decision to make at implementation time, not fixed here.
- **SAR "need assistance" flag.** A single bit or small enum in a
  role-specific status extension, parallel to `VehicleStatus`, surfaced as
  an urgent roster state (distinct colour/icon, does not wait for the
  stale-timeout logic that fuel/battery warnings use).
- **Hunter/hiker-specific chrome.** No new payload identified yet; the
  visual claim-window indicator should be dimmable/defeatable once the
  claim ritual is done, since a bright blinking screen is fine at the
  trailhead but not once the group has split up in the field. This is a UI
  behaviour, not a protocol change.

---

## Host vs guest

| Duty | Owner |
|---|---|
| BLE open/close, Meshtastic `StreamApi`, protobuf on the wire | Host |
| Admin `SetChannel` write (PIN-derived provisioning) | Host — the only admin-write this plugin performs |
| Companion GATT client session | Host (separate from radio) |
| Merge onboard + companion; interval TX (convoy extension) | Host |
| `party_roster` + claim-conflict / seq / stale logic | Host (so WASM timeout cannot drop packets) |
| Overlay / list / warn / icon chrome | Host UI; guest may format chips after wasmtime gate |
| Enable/disable, pairing consent, PIN entry, claim-window UI | Host UI |

Proposed capabilities (not in ABI yet):

| Capability | Purpose |
|---|---|
| `accessory_*` | Two BLE sessions (radio, companion) |
| `mesh_admin_write` | The single elevated action: PIN-derived `SetChannel`. Never used for anything else in this plugin. |
| `party_roster_read` | Snapshot of the roster for the guest |
| `party_warn_config_read` / `plugin_kv` | Thresholds, interval, PIN, role |
| `position_read` | This member's fix for outgoing speed / optional Position |
| `ecu_read` / `ebike_telemetry_read` | Optional onboard fuel/SoC (convoy extension) |
| `log` | Host log; durable files under `Documents/debug/lora-party/` |

TX of mesh packets is **host-gated** (plugin enabled). This is intentional
TX, not CAT PTT; still no TX when disabled. `mesh_admin_write` is gated
further: reachable only from the explicit "Set party PIN" screen, never
triggered automatically or from any other flow.

---

## Safety and RF

- Default: telemetry/roster broadcast only; no automatic text, no remote
  control of other members' units.
- Do not key an amateur voice transmitter (CAT remains separate).
- Meshtastic legal/ISM band and duty-cycle limits are the operator's
  responsibility; Navi only keeps interval/payload conservative.
- Confirm the actual board at implementation time — BLE pairing and MTU vary
  slightly by Meshtastic-supported hardware.
- The PIN-derived channel is a **collision-avoidance convenience**, not an
  authentication system — document this plainly in-product (see
  [Channel provisioning](#channel-provisioning-pin-derived)). Groups needing
  real secrecy should use a long PIN/passphrase or the standard QR-shared
  random PSK instead.
- The identity-claim visual/LED window, not GPS motion state, is the
  safeguard against misattribution during in-person pairing. No
  stationary-GPS gate is enforced by the protocol.

---

## Implementation checklist (future)

1. Host-native Meshtastic BLE session (`bluetooth-le` + `tokio`); honour
   enable/disable (close on off).
2. PIN → HKDF → `{channel_name, psk}` → admin `SetChannel` write; handle the
   expected node reboot in the UI flow.
3. `PartyIdClaim` codec + claim-window UI (visual indicator, suggested id,
   override, peer confirm affordance) + conflict handling in `party_roster`.
4. `NodeInfo` cache for display names; join by node id.
5. (Convoy extension) Private portnum `VehicleStatus` codec; Position join
   by node id.
6. `party_roster` with wrapping `seq` (status), claim conflicts, stale
   timeout, range clamp.
7. Overlay + list + role icons + (convoy) warn thresholds.
8. Second BLE session: companion GATT writes for fuel/battery and proxy
   claims; last-write-wins merge.
9. (Convoy extension) Interval TX + threshold-crossing extra TX with rate
   limit.
10. Dispatch switch that already names a text-message arm and leaves room
    for future role-specific extensions (herder proxy, SAR flag).
11. USB-visible debug under `Documents/debug/lora-party/`.
12. After wasmtime gate: optional WASM guest for chip/icon formatting only.

---

## Open questions / decisions needed before implementation

1. Default PIN length (4 alphanumeric proposed) and whether the UI should
   nudge longer PINs for groups wanting stronger privacy, versus leaving it
   a single fixed length.
2. Exact HKDF salt/context string and derived channel-name scheme (e.g.
   `navi-<pin>` vs a hash-based name) — needs to be frozen once, since
   changing it later breaks compatibility between old and new Navi builds
   on the same PIN.
3. Whether `party_id` is purely a display label (recommendation above:
   node id is the real store key) or whether some deployments want it
   enforced as globally unique across sessions.
4. Broadcast interval for convoy status — configurable; default 4 s is a
   starting point, not measured on the party's modem preset.
5. Should manual companion fuel/battery input override onboard readings, or
   only fill in when no onboard source exists? First pass is
   last-write-wins; this question is the later refinement.
6. Recommended radio hardware is documented in
   [Recommended radio hardware](#recommended-radio-hardware) (Meshstick USB
   for head units; BLE boards for tablets/handhelds). Confirm BLE vs USB
   session wiring and MTU on the actual board at implementation time.
7. Frozen GATT UUIDs for the companion service, including the
   `proxy_claim_role` characteristic, and whether Navi stays BLE central for
   the companion.
8. Whether outgoing Position is always the node's GNSS or may use the host
   GNSS when the node has no GPS module.
9. Icon set and whether `role` values are a fixed enum or an
   organization-configurable icon-id mapping.
10. Herder proxy-claim wiring: does the collar/tag have its own Meshtastic
    node (claim originates there) or does the handler's node broadcast on
    its behalf with a distinguishing role byte? Not decided in this pass.
