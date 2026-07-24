# APRS protocol

APRS (Automatic Packet Reporting System) is an amateur-radio packet protocol
for real-time tactical information: position, status, weather, and short
messages. Canonical reference: Bob Bruninga / TAPR,
[APRS Protocol Reference 1.0.1](https://www.aprs.org/doc/APRS101.PDF)
(`APRS101.PDF`).

Navi does **not** implement RF APRS decode in this codebase yet. Information-field
conventions below map cleanly onto T0 sensors, [`TrackStore`](../core/src/tracks/mod.rs)
overlays, and messaging UI. Station **display + range filtering** are
implemented; SDR receive is documented for a future
[`rtl-sdr-rs`](https://crates.io/crates/rtl-sdr-rs) path — see
[`APRS-SDR.md`](APRS-SDR.md).

## Packet shape (AX.25 UI frame)

Over RF, APRS rides AX.25 UI frames. The payload that carries meaning is the
**Information field**:

```text
Data Type ID (1 char)  +  APRS Data  +  optional Data Extension (often 7 bytes)
  +  optional Comment / weather / Mic-E status text
```

The Data Type ID selects the report family (`!`, `=`, `/`, `@`, `:`, `_`, `` ` ``,
`'` , `$`, …). Digipeater paths and callsign addressing live in the AX.25
header, not in these information-field encodings.

---

## Position, speed, and heading (course)

### Uncompressed position + CSE/SPD extension

A common mobile report looks like:

```text
!DDMM.mmN/DDDMM.mmW>CSE/SPD/...comment...
```

or with a timestamp (`/` or `@` Data Type ID):

```text
@DDHHMMzDDMM.mmN/DDDMM.mmW>CSE/SPD/...
```

| Field | Meaning |
|---|---|
| `DDMM.mmN` / `DDDMM.mmW` | Latitude / longitude (degrees + decimal minutes) |
| Symbol table + symbol code | Icon (car, house, WX station, …) |
| **CSE** | Course / heading over ground, **degrees true**, three digits `000`–`359` |
| **SPD** | Speed, **knots**, three digits (pad with leading zeros) |

The fixed **7-byte APRS Data Extension** `CSE/SPD` immediately follows the
symbol when present (course slash speed). Example: `180/045` = heading 180°,
45 kn.

Stations that do not move may omit the extension or send `.../...`.

### Compressed position (`/` … base-91)

Compressed lat/long can replace the long decimal-minute fields. After the
compressed coordinates and symbol, two base-91 bytes encode **course and
speed** together; an optional further pair can encode altitude (see below).
A compression-type byte `T` describes GPS vs other sources.

### Mic-E

Mic-E encodes position (and often course/speed) into the AX.25 destination
address and a short information field (Data Type `` ` `` or `'`). Useful for
trackers that must stay very short on air. Decoding follows Chapter 10 of
`APRS101.PDF`.

### NMEA passthrough

Raw `$GPRMC`, `$GPGGA`, `$GPGLL` sentences may be sent as information-field
content. Speed and course then come from the NMEA fields (e.g. RMC speed in
knots, course true), not from `CSE/SPD`.

---

## Altitude

Altitude is **not** a mandatory fixed field of every position report. It
appears in one of these places:

| Mechanism | Encoding | Units |
|---|---|---|
| **Comment text** | `/A=aaaaaa` anywhere in the comment | Feet, six digits (e.g. `/A=001234`) |
| **Mic-E status** | Base-91 altitude in the optional Mic-E status text | Feet (see APRS101 Ch. 10) |
| **Compressed position** | Optional two-byte base-91 altitude after course/speed | Feet |

GPS altitude is often noisy; many terrestrial beacons omit it.

Example comment fragment:

```text
>180/030/A=001850 En route
```

→ course 180°, 30 kn, altitude 1850 ft, free text “En route”.

---

## Barometric pressure (and other weather)

Weather is a **separate report family**, identified by symbol code `_`
(underscore = WX station). Position may be included in the same packet
(“complete weather report”) or the station may send positionless WX and
beacon position separately.

### Wind uses the CSE/SPD slots

For weather packets, the 7-byte extension that would be course/speed on a
mobile is reinterpreted as **wind**:

| Digits | Weather meaning | Units |
|---|---|---|
| `CSE` | Wind direction | Degrees |
| `SPD` | Wind speed | Knots (or mph depending on software; APRS101 documents the WX unit conventions) |

### Weather data string (comment-side keyed fields)

After wind, keyed measurements appear as letter + digits (order varies by
station software; parsers scan for known prefixes):

| Key | Meaning | Typical unit in APRS WX |
|---|---|---|
| `g` | Gust | same speed unit as wind |
| `t` | Temperature | °F (APRS conventional) |
| `r` | Rain last hour | hundredths of an inch |
| `p` | Rain last 24 h | hundredths of an inch |
| `P` | Rain since midnight | hundredths of an inch |
| `h` | Humidity | % (`00` means 100%) |
| **`b`** | **Barometric pressure** | **tenths of a millibar** (hPa × 10) |
| `s` | Snow last 24 h | inches |
| `L` / `l` | Luminosity | W/m² |

Example (illustrative):

```text
@041234z4903.50N/07201.75W_220/004g005t043r000p000P000h65b10130
```

Here `b10130` → 1013.0 mbar (hPa). There is **no separate “barometric
altitude”** field in classic APRS WX; pressure is station sea-level or
station pressure as the WX unit defines, and geometric altitude (if any)
still uses `/A=…` in a comment or Mic-E/compressed altitude when the station
also reports position that way.

Software / unit type suffixes (`S`, `u…`, Ultimeter / Davis tags) may trail
the measurements; see APRS101 weather chapter and `WX.TXT` historical notes.

---

## Text messages

APRS messaging is its own Data Type: **`:`** (colon).

```text
:ADDRESSEE:message text{msgid
```

| Part | Rules |
|---|---|
| Addressee | Exactly **9 characters**, left-justified, space-padded (callsign-SSID or bulletin/group name) |
| Message text | Printable ASCII; length limited (commonly up to ~67 chars of user text depending on path overhead) |
| `{msgid` | Optional message ID for ack/reject (`ackmsgid`, `rejmsgid`) |

Examples of related forms:

- **Message** — `:NOCALL-1 :Hello{01`
- **Ack** — `:NOCALL-1 :ack01`
- **Bulletin / announcement** — addressee like `BLN1     ` or `NWS-xxxxx`

General beacons and status lines (`>`, status Data Type, or free comment on a
position report) are **not** addressed messages; they are broadcast comment /
status text, not the `:ADDRESSEE:` directed message protocol.

---

## How the pieces fit together

```text
                    ┌─ CSE/SPD ── mobile: heading + speed (kn)
Position report ────┤
                    └─ Comment ── free text, /A=altitude (ft), PHG, etc.

Weather report ───── CSE/SPD reinterpreted as wind dir/speed
                    + keyed WX: t, h, b (pressure), rain, …

Mic-E / compressed ─ dense encoding of lat/lon (+ course/speed ± altitude)

: message ────────── directed text to a 9-char addressee (+ optional ack id)
```

| User-facing quantity | Where it lives in APRS |
|---|---|
| **Speed** | `SPD` in CSE/SPD extension; compressed cs bytes; Mic-E; or NMEA |
| **Heading / course** | `CSE` in CSE/SPD; compressed cs; Mic-E; or NMEA |
| **Altitude** | Comment `/A=aaaaaa` (feet); Mic-E status; optional compressed altitude |
| **Barometric pressure** | Weather report key **`b`** (tenths of a millibar), not the position CSE/SPD |
| **Weather (temp, rain, humidity, …)** | Weather report (`_` symbol) keyed fields after wind |
| **Text messages** | Data Type `:` addressed messages (and acks), distinct from beacon comments |

---

## Relevance to Navi

| APRS field | Navi sink |
|---|---|
| Position + course/speed | [`TrackStore`](../core/src/tracks/mod.rs) + MapLibre moving icons |
| `/A=` / Mic-E altitude | Elevation cross-check vs DEM (advisory; future) |
| WX `b` / `t` / `h` / rain | Optional weather layer (future) |
| `:` messages | In-app messaging when a radio/i-gate path exists |

### Moving icons (implemented)

- Core: `driver_break_core::tracks::TrackStore` — upsert by station id (in-place
  coordinate update, no duplicate markers), timeout **≤ 3600 s**, display range
  clamped to **50–150 km**.
- App: screen-space Compose overlay (native MapLibre Circle/Symbol layers do not
  paint reliably on the Automotive emulator GLES path; see README known issues).
  Test hooks push a full track snapshot after each upsert batch.
- Symbols for tests: hessu/aprs-symbols crops under `core/src/icons/aprs/` (see
  that directory’s `COPYRIGHT.md` — licensing is **per symbol**).

### Range filtering (implemented)

Limits which stations the UI should show, by Haversine distance from a centre
point. Implemented in `TrackStore::visible(center_lat, center_lon)` using
`haversine_km` (Earth radius **6371 km**).

```text
a = sin²(Δlat/2) + cos(lat1)·cos(lat2)·sin²(Δlon/2)
distance_km = 2 · R · asin(√a)     (equivalent atan2 form also fine)
```

**Process**

1. Centre = operator / GPS position (or a fixed area of interest).
2. Range = `TrackStore::range_km()` after `clamp_range`.
3. For each stored station, keep if `haversine_km(centre, station) ≤ range_km`.

**Navi clamps (unlike unlimited Navit-style “0 = global”)**

| Constant | Value | Meaning |
|---|---|---|
| `DISPLAY_RANGE_MIN_KM` | 50 | Floor — values below clamp up |
| `DISPLAY_RANGE_MAX_KM` | 150 | Ceiling — global / unlimited is **forbidden** |
| `STATION_TIMEOUT_MAX_S` | 3600 | Max last-heard timeout |

```rust
use driver_break_core::tracks::{TrackStore, DISPLAY_RANGE_MAX_KM};

let mut store = TrackStore::new(/* timeout_s */ 3600, /* range_km */ 100.0);
store.set_range_km(75.0); // clamped into 50..=150
let on_map = store.visible(/* lat */ 60.72, /* lon */ 10.61);
```

UniFFI: `FfiTrackStore`, `haversineKm`, `displayRangeMinKm` /
`displayRangeMaxKm`. Instrumented coverage:
`MovingIconInstrumentedTest`.

**Typical ranges (km)**

| Range km | Typical use |
|---|---|
| 50 | Dense metro / min clamp |
| 75–100 | Regional drive |
| 150 | Max clamp (wide area) |

When no stations appear: range too small, wrong centre, empty store, or
stations expired past `timeout_s`. When too many: lower `range_km` toward 50.

Filtering is applied on read (`visible`); upsert still stores all heard
stations until `expire`. Future SQL pre-filter is optional for huge DBs.

### Packet ingest / SDR (not implemented)

RF / AX.25 decode is **not** in-tree yet. Instrumented tests simulate beacons
via `FfiTrackStore.upsert`.

Planned receive stack:

| Piece | Role |
|---|---|
| [`rtl-sdr-rs`](https://crates.io/crates/rtl-sdr-rs) | Rust RTL-SDR USB IQ capture |
| APRS SDR DSP | FM discriminator + bit-timing PLL → AX.25 (see [`APRS-SDR.md`](APRS-SDR.md)) |
| Information-field parse | Map CSE/SPD, symbols, comments onto `TrackStation` |

Use a **non-zero IF offset** (typically 50–100 kHz) on real RTL-SDR hardware;
DC-centered tune is synthetic-test only. Full bit-stream expectations,
footguns, and hardware notes: [`APRS-SDR.md`](APRS-SDR.md).

Related: sensor tier (T0) in [`architecture.md`](architecture.md). Protocol
index: [`PROTOCOLS.md`](PROTOCOLS.md). Vehicle energy buses (separate from
APRS): [`ECU.md`](ECU.md).

## References

- [APRS Protocol Reference 1.0.1 (PDF)](https://www.aprs.org/doc/APRS101.PDF)
- [PROTOCOL.TXT (historical overview)](https://www.aprs.org/APRS-docs/PROTOCOL.TXT)
- [APRSpedia — weather field](https://aprspedia.com/doku.php?id=aprs_protocols%3Ainformation_field%3Aweather_field)
- [APRSpedia — position field](https://aprspedia.com/doku.php?id=aprs_protocols%3Ainformation_field%3Aposition_field)
- [`rtl-sdr-rs` on crates.io](https://crates.io/crates/rtl-sdr-rs) — planned IQ front-end
- [`rtl-sdr-rs` docs.rs](https://docs.rs/rtl-sdr-rs)
- [`APRS-SDR.md`](APRS-SDR.md) — DSP pipeline stages, RTL-SDR IF offset, fault table
