# CAT (Computer Aided Transceiver)

CAT control for amateur radio gear is **not implemented** yet. This document
defines the intended behaviour for a future `cat` plugin / host service so VFO
programming stays safe and predictable while driving.

Vehicle / energy telemetry is separate: see [`ECU.md`](ECU.md). Plugin overview:
[`plugins.md`](plugins.md).

---

## Goals

1. Talk to a mobile transceiver over serial/USB CAT (vendor dialect).
2. Look up nearby **NFM** (narrow FM) amateur **repeaters**.
3. If one is within **150 km**, program **VFO 1** with output frequency, duplex
   offset/shift, and CTCSS/DCS (subtone).
4. Never key the transmitter automatically.

---

## Auto-tune algorithm (VFO 1)

```text
1. Read current position (GPS / last fix).
2. Query repeater sources (onboard DB first; optional RepeaterBook sync).
3. Filter: amateur repeater, modulation = NFM / narrow FM (e.g. 11K2F3E),
   distance ≤ 150 km (Haversine). Prefer same network / linked sites when tagged.
4. Pick best candidate (nearest usable, or user-selected from a short list).
5. Resolve:
     - frequency_out (repeater downlink / mobile receive)
     - shift / offset (duplex; apply vendor sign convention for VFO TX)
     - CTCSS encode (and decode if the radio supports separate tones)
6. CAT: set VFO 1 RX/TX (or RX + offset), tone, and narrow FM mode.
7. UI: show callsign, distance, freq, shift, tone; require user confirm if
   “auto-apply” is off.
```

**150 km** matches the upper display-range clamp used for APRS tracks
(`DISPLAY_RANGE_MAX_KM`). Do not auto-tune beyond that without an explicit
user override.

### Frequency / offset conventions

| Field | Meaning for the mobile |
|---|---|
| Output / `frequency_out` | Frequency the repeater transmits (mobile **listens** here) |
| Shift / offset | Duplex split so the mobile **transmits** on input (e.g. −0.6 MHz on 2 m) |
| CTCSS / DCS | Access tone required by the repeater |

European OSM tags sometimes use a comma as decimal separator (`-0,6 Mhz`);
normalize to MHz with `.` before CAT.

### Safety interlocks

- Default: **RX + memory/VFO program only**; PTT remains manual.
- No auto-tune while the radio is transmitting.
- Abort if CAT dialect is unknown or the radio rejects the command.
- Log frequency changes for the user; do not phone-home callsigns.

---

## Repeater data sources

### Onboard database (preferred offline)

Populate from:

1. **OSM** — nodes/ways tagged
   `communication:amateur_radio:repeater=yes` (and related frequency / CTCSS /
   shift / modulation tags), optionally grouped by `type=network` relations.
2. **Bundled extract** — regional SQLite table (callsign, lat, lon, freq_out_mhz,
   shift_mhz, ctcss_hz, modulation, network_id) shipped or user-imported.
3. Optional sync from RepeaterBook (or similar) when the user enables network —
   merge into the onboard DB; never require cloud for the 150 km search.

### RepeaterBook (optional online)

When enabled: query by position/bbox, filter NFM, upsert into onboard DB with
expiry. API keys and ToS stay host-side. Offline search must still work from the
last successful sync / OSM import.

---

## Example: Innlandsnettet OSM relation

OSM relation
[**18780801**](https://www.openstreetmap.org/relation/18780801)
(`LA5MR` / Sambandstjenesten Innlandet) is a **`type=network`** of amateur radio
repeaters:

| Tag | Example value |
|---|---|
| `type` | `network` |
| `name` | `LA5MR` |
| `communication:amateur_radio:repeater` | `yes` |
| `communication:amateur_radio:repeater:ctcss` | `88.5hz` (network-wide default) |
| `operator` | `Sambandstjenesten Innlandet` |
| `website` | <https://innlandsnettet.no/> |

Members are individual repeater **nodes** (and some linking **ways**). A member
node such as
[5576656416](https://www.openstreetmap.org/node/5576656416) (`LA5TRR`) carries
site-level RF parameters:

| Tag | Example | Use for CAT |
|---|---|---|
| `communication:amateur_radio:callsign` | `LA5TRR` | UI label |
| `communication:amateur_radio:repeater:frequency_out` | `145.7250` | VFO RX (MHz) |
| `communication:amateur_radio:repeater:shift` | `-0,6 Mhz` | Duplex offset (−0.6 MHz) |
| `communication:amateur_radio:repeater:ctcss` | `88.5` | Subtone (Hz); may inherit network `88.5hz` |
| `communication:amateur_radio:repeater:modulation` | `11K2F3E` | Treat as **NFM** for auto-tune filter |
| `ele` / mast tags | optional | Planning only |

### How the network relation helps CAT

```text
                    type=network  LA5MR
                   CTCSS default 88.5 Hz
                            │
     ┌──────────┬───────────┼───────────┬──────────┐
     ▼          ▼           ▼           ▼          ▼
  LA5TRR     …sites…     (nodes)     (nodes)    (ways)
  145.725
  shift −0.6
  11K2F3E
```

1. **Discover** candidates: all member nodes with repeater=yes inside 150 km.
2. **Fill gaps:** if a node omits CTCSS, inherit the relation’s
   `repeater:ctcss`.
3. **Prefer linked coverage:** when several members are in range, prefer the
   nearest NFM site; optionally show “same network” so the operator knows
   hand-off / linked audio is likely (Innlandsnettet-style).
4. **Program VFO 1** from the chosen node’s `frequency_out` + `shift` + tone.

This is the model for an onboard “repeater network” table: one network row,
many site rows, shared defaults overridden per site.

---

## HostApi sketch

| Capability | Behaviour |
|---|---|
| `repeater_query` | Input: lat, lon, radius_km (≤ 150). Output: JSON list of NFM sites |
| `cat_vfo_set` | Input: freq_out_mhz, shift_mhz, ctcss_hz, mode=`NFM`, vfo=`1`. Host executes CAT |

Dialects (Kenwood `FA`/`FB`, Yaesu, Icom CI-V, …) are **host adapters**, not
WASM. Document baud rates per radio profile when an adapter lands
(common starting points: 9600 or 38400 8N1 — verify per manual).

---

## Status

| Piece | Status |
|---|---|
| CAT serial adapters | Not implemented |
| Onboard repeater DB / OSM import | Not implemented |
| RepeaterBook sync | Not implemented |
| Auto-tune → VFO 1 | Specified here; not implemented |
| Plugin capability wiring | Proposed in [`plugins.md`](plugins.md) |
