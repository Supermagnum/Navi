# Wired e-bike telemetry for DIY display / BMS builders

**Status:** documentation / open protocol spec only. No serial reader, HostApi
capability, or product plugin is implemented yet. A future contributor should be
able to build the host + plugin path from this document alone.

**Scope:** **wired only** — USB-serial (UART) primary; CAN bus optional.
Bluetooth, BLE, and any other wireless transport are **out of scope** for this
spec.

This is aimed at someone building a **custom** e-bike display, motor controller,
or BMS who wants that hardware to talk to Navi over a cable. It is **not** a
guide to reverse-engineering Bosch, Bafang, Shimano STEPS, Brose, Yamaha, TQ, or
other commercial proprietary buses (see [Commercial systems](#5-commercial-systems-separate-path)).

Related:

- Physics-based Electric Cycle defaults (no hardware): `EbikeConfig`, climb /
  range estimates — [`mathematical-formulas.md`](mathematical-formulas.md),
  README “Electric cycle specs”
- Car / truck live energy plugin shape: [`ECU.md`](ECU.md), [`plugins.md`](plugins.md)
- Linux USB-serial GNSS pattern (gpsd): [`build-linux.md`](build-linux.md)
  (Sensors)

---

## 1. Standards landscape (honest)

**There is no open, cross-vendor standard** for e-bike motor power draw,
estimated time or range remaining, or regenerative-braking status — over any
transport. Commercial systems keep a proprietary link (often CAN or UART)
between *their* controller, display, and battery pack. Third-party apps and
aftermarket displays either reverse-engineer those dialects or stay locked to
one vendor ecosystem.

**There is also no open *wired* equivalent** of the Bluetooth SIG Cycling Power
Service. Cycling power meters are a wireless-device category by design; that
GATT profile does not define a UART/CAN sentence set you can drop onto a cable.
For a wired head-unit ↔ controller/BMS link, an integrator must either:

1. Speak a vendor proprietary format (fragile, per-brand), or
2. Define a small open protocol and implement both ends (DIY controller + nav
   host).

Navi chooses **(2)** for DIY builders. That is a deliberate fit for this
project, not a gap to apologize for:

- Wired is simpler and more robust for a **permanently mounted** head unit
  (cable from controller/BMS to the computer; no pairing or connection-state
  machine).
- It matches how Navi already consumes wired GPS on Linux: a USB-serial GNSS
  device → **gpsd** → host code, with **no wireless stack** in the path
  ([`build-linux.md`](build-linux.md)).

Other DIY / open-hardware e-bike projects are **invited to adopt the same frame
format** so displays, BMS boards, and nav apps can interoperate without each
inventing a private dialect.

---

## 2. Physical / transport layer

### 2.1 Primary: USB-serial (UART)

**Recommended** for most DIY builds.

| Parameter | Value |
|---|---|
| Physical | USB CDC / FTDI / CH340 / CP210x (or similar) USB-to-UART between DIY MCU and head unit |
| Line | 3.3 V UART levels on the DIY side unless the adapter is documented otherwise |
| Framing | 8 data bits, no parity, 1 stop bit (**8N1**) |
| Baud | **115200** (default for this protocol) |
| Flow control | None (not required at 1 Hz ASCII) |
| Device node (Linux example) | `/dev/ttyUSB0` or `/dev/ttyACM0` (check `dmesg` / `lsusb`) |

**Why 115200:** common USB-serial default, plenty of headroom for ~1 Hz ASCII
sentences, and easy to open in `screen` / `minicom` / `picocom` while bringing
up firmware. If a builder’s MCU cannot sustain 115200 reliably, **9600 8N1** is
an allowed alternate — the host must be configurable; sentences are identical.

Debugging without Navi:

```bash
# Example: 115200 8N1, raw terminal
picocom -b 115200 /dev/ttyUSB0
# or
screen /dev/ttyUSB0 115200
```

Expect one `$NAVIPWR,...` line per second (see [§3](#3-navi-open-wired-protocol-navipwr)).

This is the same *class* of integration as pointing gpsd at a USB GNSS dongle
(`/dev/ttyUSB0`); only the sentence grammar and consumer differ.

### 2.2 Secondary: CAN bus

Optional for builders whose controller already speaks CAN (for example many
**VESC**-based DIY e-bike setups). Requires a **CAN-USB** adapter (SocketCAN on
Linux, or a vendor USB-CAN dongle) — more specialized than plain UART, so this
is **not** the primary path.

Logical payload should match the `$NAVIPWR` fields ([§3](#3-navi-open-wired-protocol-navipwr)).
A suggested mapping for a single periodic frame (DIY / open use; not a vendor
CANopen profile):

| | Suggestion |
|---|---|
| Bitrate | 250 kbit/s or 500 kbit/s (document which your board uses) |
| Frame ID | `0x7E0` (11-bit standard ID; change only if your bus already owns it) |
| Period | 1 Hz (same as UART) |
| Payload (8 bytes, little-endian) | `int16` power_w; `uint8` battery_pct; `int16` est_minutes; `uint16` est_distance_m; `uint8` flags |

`flags` bit0 = optional fields valid; bit1 = wheel_speed present in a follow-up
frame if you need RPM/speed on CAN (keep UART as the rich debug path).

Host duty remains the same: native code opens SocketCAN / USB-CAN, decodes into
the same snapshot type used for UART ([§4](#4-how-this-connects-to-navis-architecture)).

### 2.3 Explicitly out of scope

- Bluetooth Classic, BLE, Wi-Fi, Zigbee, or proprietary RF “display links”
- Pairing UX, advertising, or connection-state machines
- Speaking commercial OEM display protocols over the same cable “by accident”

---

## 3. Navi open wired protocol (`NAVIPWR`)

Navi defines a simple **ASCII sentence** protocol so hobbyist builders can
verify traffic with a serial terminal before any app work. Spirit is close to
NMEA: `$TALKER,...*CS` with a XOR checksum.

### 3.1 Talker and rate

| Item | Rule |
|---|---|
| Sentence ID | `$NAVIPWR` |
| Direction | DIY controller / BMS / display MCU → head unit (Navi host) |
| Default rate | **1 Hz** (one sentence per second). Burst or higher rates are allowed; host should keep only the latest sample |
| Line ending | `\r\n` (CRLF). Parsers should also accept `\n`-only |
| Character set | ASCII printable fields; no embedded commas or `*` inside values |

### 3.2 Field convention — signed power (regen folded in)

**One convention only:** instantaneous electrical power is a **signed** integer
in watts.

| Sign | Meaning |
|---|---|
| Positive | Traction / assist draw (motor consuming battery) |
| Zero | Coast / idle (no measurable motor power) |
| Negative | **Regenerative braking / recovery** into the pack |

There is **no** separate regen boolean in `$NAVIPWR`. Regen is `power_w < 0`.
Do not emit both a flag and a signed wattage with conflicting meaning.

This matches the spirit of `LiveEnergySnapshot.power_kw` in [`ECU.md`](ECU.md)
(negative power = regen).

### 3.3 Sentence format

```text
$NAVIPWR,<power_w>,<battery_pct>,<est_minutes>,<est_distance_m>,<motor_rpm>,<wheel_kmh>*<hh>
```

| Field | Type | Required | Meaning |
|---|---|---|---|
| `power_w` | signed integer | **yes** | Instantaneous power in watts (signed; see §3.2) |
| `battery_pct` | integer 0–100 | **yes** | Pack state of charge percent as reported by the BMS/DIY firmware |
| `est_minutes` | integer ≥ 0, or empty | no | Estimated **minutes** of ride remaining at the *current* draw / recovery rate, computed on the DIY hardware |
| `est_distance_m` | integer ≥ 0, or empty | no | Estimated **metres** remaining at current draw (same DIY estimate) |
| `motor_rpm` | integer ≥ 0, or empty | no | Motor RPM if known (cross-check / debug) |
| `wheel_kmh` | decimal or integer, or empty | no | Wheel / vehicle speed in km/h from the DIY side (cross-check vs GPS) |
| `hh` | two hex digits | **yes** | Checksum (below) |

Empty optional fields are allowed (consecutive commas), e.g. no RPM:

```text
$NAVIPWR,180,72,95,18500,,22.5*52
```

**Who computes remaining time/distance?** The DIY firmware. It has direct access
to pack voltage/current and BMS SoC; Navi’s Electric Cycle physics model only
*estimates* from route elevation and configured Wh/torque. Live
`est_minutes` / `est_distance_m` are a genuine accuracy improvement when
present; they **supplement** the physics model, they do not replace it when the
cable is unplugged.

### 3.4 Checksum

Same algorithm as classic NMEA:

1. Take all characters **between** `$` and `*` (exclusive) — i.e. starting at
   `N` of `NAVIPWR`.
2. XOR all those bytes.
3. Emit the result as two uppercase hexadecimal digits after `*`.

Pseudo-code:

```text
cs = 0
for each byte c in sentence after '$' and before '*':
    cs = cs XOR c
print uppercase hex of cs, width 2
```

Hosts **must** reject sentences with a bad checksum. During bring-up, builders
can temporarily log raw lines before enabling the check.

### 3.5 Examples

Traction, ~3 h remaining estimate:

```text
$NAVIPWR,220,68,180,32000,950,28.0*5B
```

Regen on a descent (negative watts; empty optional estimates):

```text
$NAVIPWR,-95,71,,,,*62
```

Minimal required fields only:

```text
$NAVIPWR,40,55,,,,*41
```

### 3.6 Mapping toward a future Navi snapshot

When a plugin/host path is implemented, map into the existing energy extension
point (same family as ECU), for example:

| `$NAVIPWR` field | Suggested core / HUD use |
|---|---|
| `power_w` | `LiveEnergySnapshot.power_kw = power_w / 1000.0` (signed) |
| `battery_pct` | `LiveEnergySnapshot.state_of_charge_pct` |
| `est_minutes` / `est_distance_m` | HUD “live remaining” overlay when Electric Cycle (or future e-bike profile) is active; do not delete physics-based plan % |
| `wheel_kmh` | Optional consistency check vs GPS speed (log / soft warn; do not override GPS position) |

`fuel_rate_l_h` stays unset for e-bike telemetry.

### 3.7 Versioning

- This document defines **`NAVIPWR` v1** (fields above).
- Adding fields: append new comma-separated fields **before** `*`; old parsers
  ignore trailing unknowns if they split on commas carefully.
- Renaming or changing signed-power meaning requires a **new sentence ID**
  (e.g. `$NAVIPWR2`), not a silent reinterpretation of v1.

---

## 4. How this connects to Navi’s architecture

### 4.1 Plugin-shaped, like ECU — not core

Live OBD-II / J1939 / MegaSquirt polling is already scoped as a **deferred
plugin**, not trusted-core logic ([`ECU.md`](ECU.md), [`plugins.md`](plugins.md)
§ ECU). A wired e-bike telemetry reader should follow the **same shape**:

1. **Trusted native host** opens `/dev/ttyUSB*` (or SocketCAN), parses
   `$NAVIPWR`, validates checksum, holds the latest sample.
2. A thin **WASM guest** (optional) or host service publishes a normalized
   snapshot into eco / HUD paths via HostApi / UniFFI.
3. Core routing and Electric Cycle **physics estimates remain the default**
   with zero hardware attached.

### 4.2 Supplement, do not replace physics

| Mode | Behavior |
|---|---|
| No cable / no plugin | Existing `EbikeConfig` + route climb/range math only |
| Cable + valid `$NAVIPWR` | Prefer live SoC / power / DIY remaining estimates for HUD and optional `refine_energy_cost`; keep plan-time physics as fallback and as offline planning |

Unplugging the cable or checksum failures must fall back cleanly — never leave
the UI stuck on a stale “connected” estimate without a timeout (suggested:
invalidate samples older than **3 s** at 1 Hz).

### 4.3 Sandbox constraint (WASM cannot open serial)

Guest plugins compile to `wasm32-unknown-unknown` **without** WASI filesystem
or raw device access ([`plugins.md`](plugins.md)). A WASM plugin **cannot** open
a serial port itself.

Required design (same category as instrument-cluster / AGL host-mediated I/O
and structurally like `navi-linux` reading gpsd):

| Layer | Duty |
|---|---|
| Native host | Own the UART/CAN fd; parse; rate-limit; sanitize |
| Proposed capability | e.g. `ebike_telemetry_read` |
| Proposed HostApi | e.g. `read_ebike_serial_telemetry()` → latest snapshot bytes/struct into the guest buffer, or host pushes into core without a guest |

Until that capability exists in `plugin-host`, a Linux/`navi-desktop` native
service may write snapshots into core the same way other host-only sensors do
today.

### 4.4 Platforms

| Platform | Note |
|---|---|
| Linux head unit / SBC | Natural fit: USB-serial node + future reader thread (cf. gpsd pattern) |
| Android Automotive | Possible later via USB host / accessory; still host-owned I/O, not WASM sockets |
| Emulator | No real UART — use a replay of `$NAVIPWR` lines from a file for tests |

---

## 5. Commercial systems (separate path)

Reading **existing commercial** e-bike controller ↔ display ↔ BMS buses
(Bosch, Bafang OEM displays, Shimano STEPS, Brose, Yamaha, TQ, …) means
reverse-engineering **each** proprietary UART/CAN dialect, keeping up with
firmware changes, and accepting legal/ToS risk per vendor.

That work is:

- **Fragile and high-maintenance** (per vendor, often undocumented)
- **Different in character** from implementing `$NAVIPWR` on DIY hardware
- **Lower priority / higher risk** relative to the open DIY path in this doc

It should **not** be pursued in the same effort as shipping a `$NAVIPWR` host
reader. If someone later documents a vendor dialect, keep it in a separate doc
and treat it as optional, best-effort adapters — never as a requirement for
Electric Cycle mode to work.

---

## 6. Builder checklist

1. Emit `$NAVIPWR` at 1 Hz, **115200 8N1** (or documented 9600), CRLF, valid XOR
   checksum.
2. Use **signed watts** for regen; no parallel regen flag.
3. Compute optional `est_minutes` / `est_distance_m` on the bike electronics when
   you can; leave fields empty when you cannot.
4. Verify with `picocom` / `screen` before integrating with Navi.
5. Expect a future Navi plugin/host to **consume** this stream; until then,
   physics-only Electric Cycle planning remains the shipped path.
6. Prefer this open format over cloning a commercial bus unless you accept the
   maintenance cost in §5.

---

## 7. Document history

| Version | Notes |
|---|---|
| 2026-07 | Initial DIY wired spec (`NAVIPWR` v1); UART primary, CAN secondary; plugin architecture notes; commercial RE signpost |
