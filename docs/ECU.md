# ECU / vehicle energy protocols

Readable derivation formulas (MAF fuel rate, J1939 scaling, MegaSquirt,
range, eco segment energy):
[`mathematical-formulas.md`](mathematical-formulas.md).

Navi does **not** poll a vehicle ECU in this codebase yet. Live telemetry is an
extension point: a future T1 plugin (WASM or native) implements
`driver_break_core::ecu::LiveEnergyProvider` and feeds
`LiveEnergySnapshot` into eco cost refinement via
`refine_energy_cost` / graph reweight.

This document describes the wire protocols the ECU plugin is expected to speak
so a host adapter can map bytes cleanly onto that snapshot. Target families
called out in `core/src/ecu/mod.rs`:

| Family | Typical link | Primary use in Navi |
|---|---|---|
| **OBD-II** (ISO 15765-4 / ELM327) | Bluetooth / USB serial (AT + hex PID) | Passenger car fuel rate, RPM, MAF |
| **SAE J1939** | CAN (250/500 kbit/s) | Truck / heavy vehicle fuel economy, SoC |
| **MegaSquirt** (MSQ / TunerStudio) | Serial (RS-232 / USB-serial) | Aftermarket ECU fuel / AFR / MAP; **flex-fuel** ethanol % when a composition sensor is fitted |

Until a plugin is loaded, the core uses `NoLiveEnergy` (always `None`). Fuel
learning falls back to persisted `FuelConfig` (tank capacity / fuel added)
rather than live rate.

---

## Snapshot contract (Navi side)

```rust
pub struct LiveEnergySnapshot {
    pub fuel_rate_l_h: Option<f64>,      // ICE / liquid fuel
    pub state_of_charge_pct: Option<f64>, // EV / HEV battery
    pub power_kw: Option<f64>,           // instantaneous electrical or shaft power
}
```

| Snapshot field | Typical OBD-II source | Typical J1939 source | MegaSquirt |
|---|---|---|---|
| `fuel_rate_l_h` | PID `5E` (fuel rate) or derived from MAF/`01 0C`+`01 10` | PGN 65257 (LFE) / 65266 | Pulse width × RPM → L/h (scale with flex ethanol % when available) |
| `state_of_charge_pct` | PID `5B` (hybrid/EV SoC) where supported | PGN 65280 / OEM proprietary | N/A (ICE) |
| `power_kw` | Derived (current × voltage) or OEM | PGN 61444 / torque × speed | MAP/load × displacement model |
| *(future)* ethanol / fuel blend | PID `52` (ethanol %) where supported | Rare / OEM | Flex fuel composition sensor → % ethanol (+ optional fuel temp) |

### Electric vehicles

For `CarElectric` / `TruckElectric` / `MotorcycleElectric`, prefer SoC and
traction power over liquid fuel rate:

```rust
LiveEnergySnapshot {
    fuel_rate_l_h: None,
    state_of_charge_pct: Some(72.0),
    power_kw: Some(-15.0), // negative = regen while descending / braking
}
```

`EcoConfig::for_profile` already sets non-zero `regen_efficiency` on electric
profiles so graph PE costs credit downhill recovery. Live `power_kw` / SoC feed
HUD range estimates and future `refine_energy_cost` extensions; liquid `5E`
remains the ICE path. See planned `ecu` plugin in [`plugins.md`](plugins.md).

`refine_energy_cost(predicted_joules, distance_m, live)` prefers
`fuel_rate_l_h` when present: it converts rate × travel time at
`DEFAULT_CRUISE_SPEED_M_S` into joule-equivalent cost for the edge. Missing
live data leaves the elevation/physics prediction unchanged.

Profiles that care most about live energy: `Car`, `CarElectric`, `Truck`,
`TruckElectric`, `Motorcycle`, `MotorcycleElectric`. Hiking/cycling ignore
ECU snapshots.

---

## 1. OBD-II (ELM327-style)

### Physical / framing

Most Android adapters expose a **serial** stream (SPP / USB CDC). The adapter
firmware (ELM327 or compatible) accepts ASCII AT commands and returns ASCII
hex responses for ISO-TP / CAN OBD requests.

Typical settings: **38400** or **115200** baud, 8N1, no flow control.

### Session bootstrap (example)

```text
ATZ          → ELM327 v1.5
ATE0         → OK          (echo off)
ATL0         → OK          (linefeeds off)
ATS0         → OK          (spaces off — optional)
ATH0         → OK          (headers off)
ATSP0        → OK          (auto protocol)
0100         → 41 00 BE 3F A8 13   (supported PIDs 01–20)
```

### Mode 01 PIDs useful for energy

Request format: `01 XX` (mode 01, PID `XX`). Response starts with `41 XX` then
data bytes.

| PID | Name | Response data | Decode (sketch) |
|---|---|---|---|
| `0C` | Engine RPM | A, B | `((A*256)+B)/4` rpm |
| `0D` | Vehicle speed | A | `A` km/h |
| `10` | MAF air flow | A, B | `((A*256)+B)/100` g/s |
| `2F` | Fuel tank level | A | `A * 100 / 255` % |
| `5B` | Hybrid battery SoC | A | `A * 100 / 255` % |
| `5E` | Engine fuel rate | A, B | `((A*256)+B)/20` L/h |

### Example: read fuel rate → snapshot

```text
Host → adapter:  015E
Adapter → host:  415E0064
```

Decode (spaces optional depending on `ATS`):

- Mode/PID echo: `41 5E`
- Data `00 64` → `((0*256)+100)/20` = **5.0 L/h**

Map to Navi:

```rust
LiveEnergySnapshot {
    fuel_rate_l_h: Some(5.0),
    state_of_charge_pct: None,
    power_kw: None,
}
```

### Example: derive approximate L/h from MAF (when PID 5E missing)

Gasoline rule of thumb (stoichiometric AFR ≈ 14.7, density ≈ 0.74 kg/L):

```text
maf_g_s = ((A*256)+B)/100
fuel_g_s ≈ maf_g_s / 14.7
fuel_l_h ≈ (fuel_g_s * 3600) / 740
```

```text
0110 → 411000C8     # A=0, B=200 → MAF = 2.00 g/s
fuel_l_h ≈ (2.0/14.7)*3600/740 ≈ 0.66 L/h  (idle-ish)
```

### Example: EV / hybrid SoC

```text
015B → 415B80       # A=128 → 128*100/255 ≈ 50.2 %
```

```rust
LiveEnergySnapshot {
    fuel_rate_l_h: None,
    state_of_charge_pct: Some(50.2),
    power_kw: None,
}
```

### Error / no-data patterns

```text
NO DATA
UNABLE TO CONNECT
BUS INIT: ERROR
?                 # bad AT command
```

A plugin should treat these as “no snapshot this cycle” (`None` from
`latest()`), not as zero fuel rate.

### References

- SAE J1979 / ISO 15031-5 (Mode/PID definitions)
- ELM327 AT Command set (Elm Electronics)
- ISO 15765-4 (CAN diagnostic transport)

---

## 2. SAE J1939 (heavy vehicles)

### Physical / framing

J1939 rides CAN (typically **250 kbit/s** or **500 kbit/s**). Messages are
identified by a 29-bit CAN ID encoding priority, PGN, and source address.
Payload is usually 8 bytes (or multipacket via BAM/TP.CM for longer PDUs).

Navi never opens a raw SocketCAN socket from the trusted core; a T1 plugin or
Android accessory would own the interface and publish snapshots only.

### PGNs relevant to energy

| PGN | Name | Notes |
|---|---|---|
| **61444** (0xF004) | Electronic Engine Controller 1 (EEC1) | Engine speed, torque % |
| **65262** (0xFEEE) | Engine Temperature 1 | Coolant, etc. |
| **65257** (0xFEE9) | Fuel Economy (Liquid) | Instantaneous + average fuel rate |
| **65266** (0xFEF2) | Fuel Consumption (Liquid) | Trip fuel used |
| **65263** (0xFEEF) | Engine Fluid Level/Pressure 1 | Oil / fuel pressure |
| Proprietary | OEM SoC / HV battery | EV trucks — vendor-specific SPNs |

Exact SPN bit layouts are in the SAE J1939 Digital Annex. Below are
**illustrative** decodes for documentation — verify against the DA for a given
vehicle year.

### Example: Fuel Economy (LFE) → L/h

Illustrative LFE layout (PGN 65257), little-endian style common on J1939:

```text
Byte0-1  Instantaneous fuel rate   resolution 0.05 L/h per bit
Byte2-3  Average fuel rate         resolution 0.05 L/h per bit
…
```

CAN frame (example):

```text
ID  0x18FEE900   (prio=6, PGN=FEE9, SA=0)
Data  64 00  C8 00  FF FF FF FF
```

Instantaneous raw = `0x0064` = 100 → `100 * 0.05` = **5.0 L/h**.

```rust
LiveEnergySnapshot {
    fuel_rate_l_h: Some(5.0),
    state_of_charge_pct: None,
    power_kw: None,
}
```

### Example: EEC1 engine speed

```text
ID  0x0CF00400
Data  …  …  XX YY  …     # engine speed often SPN 190, 0.125 rpm/bit
```

Engine speed alone does not fill `LiveEnergySnapshot`; combine with fuel rate
or estimated BSFC when LFE is absent.

### Broadcast vs request

Many fuel PGNs are **broadcast** periodically (e.g. 100 ms–1 s). Others need a
PGN request (PGN 59904 / 0xEA00) to the engine controller address. Prefer
passive listen for eco reweight so the plugin does not flood the bus.

### References

- SAE J1939-71 (application layer / PGNs)
- SAE J1939 Digital Annex (SPN resolutions)
- ISO 11898 (CAN physical)

---

## 3. MegaSquirt (TunerStudio / MS serial)

### Physical / framing

MegaSquirt and compatible aftermarket ECUs speak a **binary or ASCII serial**
protocol over RS-232 / USB-serial (often **115200** 8N1). TunerStudio and
MegaTunix document the command set; common family:

| Command | Meaning |
|---|---|
| `Q` / signature query | ECU firmware identity string |
| `A` / realtime table | Burst of current runtime variables |
| `r` / page read | Read calibration page |

Exact offsets depend on firmware (MS1 / MS2 / MS3 / Speeduino). Plugins must
key off the signature string before interpreting the realtime block.

### Example: identity

```text
Host → ECU:  "Q\n"     (or binary equivalent)
ECU  → Host: "MS3 Format 0262.14      \0..."
```

### Example: realtime → fuel rate sketch

A simplified MS2-style approach (offsets are firmware-specific — treat as
pattern, not copy-paste numbers):

1. Request realtime block (`A`).
2. Read **RPM**, **pulse width** (injector open time), **injector flow**
   (from calibration / settings page).
3. Convert:

```text
duty = (pw_ms * rpm) / 1200.0          # roughly, 4-stroke
fuel_l_h ≈ injector_cc_min * duty * n_inj * 0.06 / 1000
```

Map the result to `fuel_rate_l_h`. AFR / MAP can refine eco models later but
are not required for the first `LiveEnergySnapshot` fill.

### Example: Speeduino / MS-compatible ASCII

Some firmwares accept:

```text
A
```

and return a fixed-length binary blob. Others use:

```text
r\x00\x00\x00…   # page/offset/length
```

Always verify against the firmware’s protocol PDF before shipping a plugin.

### Flex fuel (fuel composition)

MegaSquirt **supports flex-fuel / fuel-composition sensing** so a vehicle can run
gasoline–ethanol blends (E0 through E85/E100) without a fixed tune for one
blend. This is a first-class MS capability (especially on **MS3** / MS3X), not a
rare OEM-only feature, and an ECU plugin for Navi should expose it when the
firmware reports a live ethanol percentage.

**What the sensor does**

A Continental / GM / Ford-style **fuel composition sensor** sits in the fuel
line and reports:

| Quantity | Typical encoding (GM/Continental-class) |
|---|---|
| Ethanol content | Square-wave **frequency**: ~**50 Hz** = 0% ethanol, ~**150 Hz** = 100% ethanol (linear between) |
| Fuel temperature | **Pulse width** of the same signal (e.g. ~1 ms ≈ −40 °C, ~5 ms ≈ 125 °C) when temperature decoding is enabled |

The signal is **digital frequency**, not a 0–5 V analogue voltage — it must land
on a digital / flex input. On **MS3X**, the harness typically has a dedicated
**FLEX** pin. On base MS3 boards, a spare digital input (e.g. JS7/JS11 with
hardware prep) is used and selected in TunerStudio
(**Fuel Settings → Fuel Sensor Settings (Flex)**).

**What the ECU does with it**

- **MS2 / simpler modes:** scale fueling with an ethanol-dependent multiplier
  (more ethanol → longer pulsewidth).
- **MS3 Flex Blend:** blend (or switch) between calibration tables
  (VE / spark / enrichments, etc.) as ethanol % changes — two-endpoint (or
  multi) maps interpolated by the sensor reading.

Higher ethanol content needs **more injected volume** for the same energy and
usually **more spark advance**; the ECU already applies that when flex is
enabled. Water contamination can fool composition sensors (ethanol is
hygroscopic) — treat extreme or stuck readings as suspect.

**Navi plugin implications**

1. After the signature/`Q` identify step, parse the realtime block for the
   firmware’s ethanol / flex channel (name varies by MS2/MS3/Speeduino build —
   key off signature, do not hard-code one offset for all firmwares).
2. Prefer the ECU’s **already-corrected** injector pulse width when computing
   `fuel_rate_l_h` (MS has already flexed PW). Do not apply a second ethanol
   multiplier on top of flexed PW.
3. Still record **ethanol %** (and fuel temp if present) for:
   - HUD / range context (“running ~E70”),
   - refining energy density / AFR assumptions if deriving rate another way,
   - future extension of `LiveEnergySnapshot` or `FuelConfig` (blend is not in
     the snapshot struct yet — liquid L/h remains the primary live field).
4. OBD-II PID `52` (ethanol fuel %) is the OEM parallel; MegaSquirt flex is the
   aftermarket parallel. See also AFR/ethanol notes in
   [`mathematical-formulas.md`](mathematical-formulas.md).

### References

- MegaSquirt / MSEXTRA serial protocol notes (msextra.com)
- TunerStudio communications documentation
- Speeduino serial protocol (speeduino.com)
- MS3 / MS3X hardware manuals — Flex / fuel composition sensor input
  ([MS3X hardware PDF](https://www.msextra.com/doc/pdf/MS3XV357_Hardware-1.5.pdf);
  TunerStudio **Fuel Sensor Settings (Flex)**)
- [Flex fuel with MegaSquirt (background)](https://www.megamanual.com/flexfuel.htm)

---

## 4. Plugin integration sketch

Future T1 path (not implemented):

1. Host opens the transport (Bluetooth SPP, USB serial, or SocketCAN).
2. WASM/native plugin polls at a modest rate (e.g. 1–5 Hz) under fuel/timeout
   limits — see [`plugins.md`](plugins.md).
3. Plugin calls into core (or posts on the snapshot bus) with
   `LiveEnergySnapshot`.
4. Routing reweight / live edge cost uses `refine_energy_cost`.

Capability gating (proposed, not in HostApi yet): e.g. `ecu_read` — plugins
must not get raw filesystem or unrestricted network; serial ownership stays
with the Android host process.

Unit-style usage of the existing core API without hardware:

```rust
use driver_break_core::ecu::{refine_energy_cost, LiveEnergySnapshot};

let live = LiveEnergySnapshot {
    fuel_rate_l_h: Some(6.5),
    state_of_charge_pct: None,
    power_kw: None,
};
let cost = refine_energy_cost(/* predicted */ 1.2e6, /* distance_m */ 500.0, Some(&live));
```

---

## 5. Safety and privacy

- Do **not** write diagnostic clear / programming commands from a navigation
  plugin (`04` clear DTCs, J1939 proprietary config, MS burn). Read-only.
- Treat VIN and OEM proprietary PIDs as sensitive; do not log them by default.
- On bus errors or ignition-off, clear the snapshot so routing falls back to
  physics + `FuelConfig` rather than stale L/h.

---

## 6. Vehicle-side protocol reference (OBD-2 / CAN)

Context: aftermarket/DIY instrument cluster hardware (TFT/IPS displays, ESP32
or STM32-based builds) needs to read data **from** the vehicle. That is a
separate concern from Navi’s planned **outbound** nav-data export to clusters
([`plugins/instrument-cluster-agl-spec.md`](plugins/instrument-cluster-agl-spec.md)).
This section is reference material for hardware builders, not part of the
`LiveEnergyProvider` plugin implementation described above.

### Why OBD-2 over raw CAN for hobbyist builds

- Raw CAN bus messages are largely proprietary per manufacturer and not
  publicly documented; building a full custom cluster against raw CAN
  requires reverse-engineering each vehicle’s message IDs.
- OBD-2 exposes a standardized, documented subset of vehicle data
  specifically so this reverse-engineering isn’t required for common signals
  (speed, RPM, coolant temp, throttle position, fuel level, etc.).

### Protocol stack

| Layer | Standard | Role |
|---|---|---|
| Diagnostic request/response | SAE J1979 / ISO 15031-5 | Standardized Mode/PID format (Mode 01 = current data, Mode 03 = DTCs, etc.) |
| Transport (multi-frame over CAN) | ISO 15765-2 (ISO-TP) | Carries OBD-2 requests/responses over CAN’s 8-byte frame limit |
| Manufacturer-specific diagnostics | ISO 14229 (UDS) | Protocol is standardized; per-make/model identifier tables (Mode 22) are generally proprietary/undocumented |
| Common hobbyist interface | ELM327 command set | De facto standard AT-command interface used by cheap OBD dongles; not a vehicle protocol itself |

### Where to obtain the standards themselves

| Standard | Publisher | Access |
|---|---|---|
| SAE J1979 (OBD-2 diagnostic test modes) | SAE International | Paid standard, purchasable at [sae.org](https://www.sae.org/); summarized/mirrored informally on many OBD hobbyist sites and [Wikipedia’s OBD-II PIDs](https://en.wikipedia.org/wiki/OBD-II_PIDs) page, which lists the common Mode 01 PIDs and formulas without requiring purchase |
| ISO 15031-5 (equivalent to J1979, international) | ISO | Paid standard via [iso.org](https://www.iso.org/) or national standards bodies |
| ISO 15765-2 (ISO-TP transport) | ISO | Paid standard via [iso.org](https://www.iso.org/) |
| ISO 14229 (UDS) | ISO | Paid standard via [iso.org](https://www.iso.org/); widely summarized in reverse-engineering/security-research writeups since the framing (Mode 22, negative response codes, etc.) is public even where per-vehicle identifiers aren’t |
| SAE J1850, ISO 9141-2, ISO 14230 (older non-CAN OBD physical layers) | SAE / ISO | Paid standards; mostly legacy, relevant only for pre-2008 US vehicles or pre-2001 EU vehicles where OBD-2 wasn’t yet CAN-based |
| CAN 2.0 / CAN FD (physical/data-link layer under all of the above) | Bosch (original spec), ISO 11898 | Bosch’s original CAN 2.0 spec is freely available as a PDF from Bosch; ISO 11898 itself is paid via [iso.org](https://www.iso.org/) |

Note: the underlying diagnostic modes/PIDs (which bytes mean what) are
effectively public knowledge despite the formal standards being paywalled —
they are documented for free by hobbyist and security-research communities
([Wikipedia’s OBD-II PIDs](https://en.wikipedia.org/wiki/OBD-II_PIDs),
[ELM327 datasheet / AT command reference](https://www.elmelectronics.com/wp-content/uploads/2016/07/ELM327DS.pdf),
[opendbc](https://github.com/commaai/opendbc), various OBD forums) to a level
sufficient for a DIY build. Purchase of the formal standard is mainly relevant
for the transport-layer/timing details (ISO-TP framing, UDS negative response
handling) if building a protocol stack from scratch rather than using an
existing library.

### How the data is actually carried (wire format)

For builders who need to know what’s actually on the wire, not just where to
buy the spec:

- **CAN frame basics:** a standard CAN 2.0A frame carries an 11-bit
  identifier and up to 8 data bytes; CAN 2.0B extends the identifier to 29
  bits (used for most OBD-2 and many manufacturer buses); CAN FD extends the
  payload up to 64 bytes per frame but is not universal in older vehicles.
- **OBD-2 request/response over CAN:** requests are sent to CAN ID `0x7DF`
  (functional/broadcast query) or a specific ECU’s request ID (commonly in
  the `0x7E0`–`0x7E7` range); the responding ECU replies on its paired ID
  (commonly `0x7E8`–`0x7EF`). Each 8-byte CAN frame’s first byte(s) encode
  the payload length and, for multi-frame messages, ISO-TP sequencing
  (single frame / first frame / consecutive frame / flow control), followed
  by the Mode byte (e.g. `0x01` for current data), the PID byte, and then
  the data bytes themselves.
- **Example:** a Mode 01 PID `0x0D` (vehicle speed) request is
  `[0x02, 0x01, 0x0D, 0x00, 0x00, 0x00, 0x00, 0x00]` (length=2, mode=01,
  PID=0D); the response’s data byte A directly equals speed in km/h,
  needing no scaling — while other PIDs use documented formulas (e.g. RPM =
  `((A*256)+B)/4`).
- **UDS (Mode 22) structure:** request format is `[length, 0x22, DID_high,
  DID_low, …]` where DID is a 2-byte Data Identifier; unlike Mode 01 PIDs,
  DIDs and their scaling formulas are manufacturer-defined and not published
  in a universal table, which is the core reason UDS-based signals need
  vehicle-specific reverse-engineering or a community DBC file.
- **DBC file role:** a DBC file is essentially a lookup table mapping CAN ID
  + bit offset + bit length + scale/offset to a named, human-readable
  signal; it’s the practical artifact that turns raw frames as described
  above into usable values, and is why opendbc-style resources matter more
  in practice than the formal standards documents for anything beyond
  generic OBD-2 PIDs.

This stays at reference depth (enough to know what tool/library to reach for
and roughly what’s on the wire), not a full protocol implementation guide.
Prefer linking out rather than reproducing full PID tables here:
[Wikipedia OBD-II PIDs](https://en.wikipedia.org/wiki/OBD-II_PIDs),
[ELM327 datasheet](https://www.elmelectronics.com/wp-content/uploads/2016/07/ELM327DS.pdf),
[opendbc](https://github.com/commaai/opendbc).

### Where public CAN documentation exists

- [opendbc](https://github.com/commaai/opendbc) (comma.ai) — open collection of
  DBC files with reverse-engineered CAN message definitions across many
  production vehicles.
- DBC file format (Vector) — de facto standard for describing CAN message
  layout (ID, signal bit position, scaling, units); usable with tools like
  SavvyCAN.
- Car Hacking Village / academic CAN research — periodic publication of
  vehicle-specific CAN maps, primarily security-research motivated.
- Marque-specific enthusiast forums — often the best source for a specific
  vehicle’s proprietary signals, since other builders publish their findings.

### Note on existing aftermarket CAN-bus decoder boxes

Many Android head units read body-CAN signals (steering wheel controls,
parking sensor distance, door status) via third-party “canbus box” adapters.
These work by embedding a proprietary, reverse-engineered per-vehicle
signal map in the box’s firmware — this mapping is not published as a
spec, which is why it isn’t a usable documentation source, only evidence
that the signals have been reverse-engineered by someone.

### Recommendation for aftermarket cluster hardware

Build against OBD-2 (J1979 + ISO-TP) for standardized signals as the primary
path. For richer data (individual sensors, warning states, proprietary
values) not exposed via generic OBD-2, either locate an existing opendbc /
community DBC file for the target vehicle or budget separate time for CAN
sniffing and correlation — there is no universal public spec for that layer.

### Relation to other docs

| Doc | Relation |
|---|---|
| [`plugins/instrument-cluster-agl-spec.md`](plugins/instrument-cluster-agl-spec.md) | Opposite direction (Navi → cluster); outbound VSS/JSON export — not vehicle input |
| [`PROTOCOLS.md`](PROTOCOLS.md) | Index of vehicle / radio wire docs |
| §1 OBD-II above | ELM327 / Mode 01 path into `LiveEnergySnapshot` |

---

## Status in this repository

| Piece | Status |
|---|---|
| `LiveEnergySnapshot` / `LiveEnergyProvider` / `NoLiveEnergy` | Present |
| `refine_energy_cost` used from graph reweight | Present |
| OBD-II / J1939 / MegaSquirt polling | **Not implemented** |
| MegaSquirt flex-fuel (composition sensor) | **Documented** above; not polled yet — plugin should read ethanol % when firmware exposes it |
| HostApi `ecu_read` capability | **Not implemented** |
| Android Bluetooth OBD UX | **Not implemented** |

When an implementation lands, update this file with the chosen adapter stack,
baud defaults, and any vehicle-specific quirks, and link it from
[`PROTOCOLS.md`](PROTOCOLS.md).
