# CAT (Computer Aided Transceiver)

CAT control for amateur radio gear is **not implemented** yet. This document
defines the intended behaviour for a future `cat` plugin / host service so VFO
programming stays safe and predictable while driving.

Vehicle / energy telemetry is separate: see [`ECU.md`](ECU.md). Plugin overview:
[`plugins.md`](plugins.md). LoRa convoy status uses the same client/display
split (Navi does not implement the RF layer) over Meshtastic BLE:
[`plugins/lora-convoy-spec.md`](plugins/lora-convoy-spec.md).

---

## Goals

1. Talk to a mobile transceiver over CAT via **upstream Hamlib** (shared library
   on Android), not hand-written vendor dialect adapters.
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
     - shift / offset (duplex; apply Hamlib rptr_shift + rptr_offs)
     - CTCSS encode (and decode if the radio supports separate tones)
6. CAT: set VFO 1 RX/TX (or RX + offset), tone, and narrow FM mode via Hamlib.
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

Hamlib unit conventions (pass these to the FFI, not MHz floats):

| Concept | Hamlib unit | Example |
|---|---|---|
| Frequency (`freq_t`) | Hz | 145.7250 MHz → `145725000` |
| Duplex shift | `rig_set_rptr_shift(RIG_RPT_SHIFT_MINUS` / `PLUS` / `NONE)` | Sign comes from the shift enum, not the offset |
| Duplex offset | `rig_set_rptr_offs` absolute offset in Hz | 0.6 MHz → `600000` |
| CTCSS | Tenths of Hz | 88.5 Hz → `885` |
| DCS | Code number | As listed by the repeater / radio manual |
| Mode | `RIG_MODE_FM` with a narrow passband, or `RIG_MODE_FMN` if the backend supports it | If narrow is unsupported, fall back to `RIG_MODE_FM` with the backend’s default passband and log a warning |
| “VFO 1” | `RIG_VFO_A` (use `RIG_VFO_MAIN` on Main/Sub rigs) | Per-profile override allowed |

### Safety interlocks

- Default: **RX + memory/VFO program only**; PTT remains manual.
- Host calls `rig_get_ptt` before every programming sequence and **aborts** if TX
  is active or PTT state is unknown. Navi never calls `rig_set_ptt`.
- Abort if the radio profile / Hamlib model is unknown or the radio rejects the
  command.
- Hamlib is **not thread-safe**: one dedicated worker thread owns the rig handle;
  all CAT calls go through it with timeouts.
- Log frequency changes for the user; do not phone-home callsigns.

---

## Hamlib integration

### Do not use crates.io Hamlib crates

Do **not** add Rust Hamlib crates from crates.io (e.g. historical `hamlib` /
`hamlib-sys` bindings). They are years out of date, bind old Hamlib APIs, and
are unmaintained. Navi must not depend on them.

### Upstream Hamlib (C library)

Use **upstream Hamlib** (current 4.x line), **pinned to release tag 4.6.5**.
Path/unit docs and the network-path check below were verified against that tag;
the future Android build script must fetch that same tag. Cross-compile for
Android with the NDK and ship as a shared library:

| ABI | Role |
|---|---|
| `arm64-v8a` | Primary device |
| `armeabi-v7a` | 32-bit ARM devices |
| `x86_64` | Emulator |

Install as `libhamlib.so` under `jniLibs` for each ABI.

### Navi-owned FFI crate

Navi owns a minimal FFI crate (bindgen against the pinned headers, or
hand-written `extern` blocks) covering only:

```text
rig_init
rig_open
rig_close
rig_cleanup
rig_set_vfo
rig_set_freq
rig_set_mode
rig_set_rptr_shift
rig_set_rptr_offs
rig_set_ctcss_tone
rig_set_dcs_code
rig_get_ptt          (TX interlock; never rig_set_ptt)
rig_get_info         (or equivalent for model detection / logging)
```

No broad wrap of the entire Hamlib API.

### Build (described; not checked in yet)

A future `scripts/build-hamlib-android.sh` should:

1. Fetch the pinned Hamlib release tag source (**4.6.5**).
2. Cross-compile with autotools + the Android NDK toolchain for the three ABIs
   above.
3. Disable Android-unneeded bits: C++ / Perl / Python / Tcl bindings, readline,
   and libusb-dependent backends (unless Navi later builds libusb for those
   backends).
4. Produce `libhamlib.so` artifacts suitable for packaging under `jniLibs`.
5. Be cacheable in CI (keyed on pinned tag + NDK version + script hash).

Do not invent ad-hoc vendor CAT parsers in Rust or Kotlin when Hamlib already
covers the radio.

### Licensing

Hamlib is **LGPL-2.1+**. Ship it **dynamically linked** (`libhamlib.so`). Keep
the pinned source/tag and the build script public so users can rebuild or
replace the `.so` (LGPL replacement requirement).

Navi itself is **GPL-3.0-or-later** (`LICENSE`, root `Cargo.toml`). Dynamically
linking an LGPL-2.1+ shared library into a GPL-3.0-or-later application is
compatible; there is **no license conflict** for this plan. Static linking or
shipping without a replaceable `.so` / corresponding source would be a
problem — do not do that.

---

## Android transport

Non-rooted Android apps cannot open `/dev/ttyUSB*` or `/dev/ttyACM*`. Hamlib
expects a device path (`rig_pathname` / `-r`). Options:

| Option | Role | Notes |
|---|---|---|
| **(a) USB serial → loopback TCP** | **Primary** | Kotlin uses Android `UsbManager` (e.g. usb-serial-for-android) and bridges bytes to a loopback TCP socket. Hamlib gets `127.0.0.1:<port>` as the rig path. |
| **(b) Bluetooth SPP → loopback TCP** | Secondary | Same loopback bridge for radios with Bluetooth CAT. |
| **(c) Remote `rigctld`** | Secondary | Hamlib **NET rigctl** model to a `rigctld` on another box (e.g. Pi in the vehicle) over Wi-Fi/LAN. |

Rust / Hamlib usage is the same in all three cases; only the Kotlin bridge (or
none, for remote `rigctld`) differs.

**Hamlib 4 network-path verification (tag 4.6.5):** Confirmed. The `rigctl`
man page documents `-r` / `--rig-file` as accepting a network `address:port`
(example `127.0.0.1:12345`). Hamlib detects such pathnames and opens
`RIG_PORT_NETWORK` (TCP). That is the intended path for (a) and (b): a
transparent serial-byte bridge on loopback, with a normal Kenwood/Yaesu/Icom
(etc.) model number. Option (c) uses the separate **NET rigctl** model against
`rigctld` (default port 4532), not raw serial framing.

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
| `cat_vfo_set` | Input: freq_out_mhz, shift_mhz, ctcss_hz, mode=`NFM`, vfo=`1`. Host executes CAT via Hamlib |

Hamlib provides vendor dialects (Kenwood, Yaesu, Icom CI-V, …). Navi’s radio
profile maps to a Hamlib rig **model number** plus port settings (baud, data
bits, stop bits, handshake, CI-V address where relevant). Navi does **not**
write its own dialect adapters. Document baud rates and CI-V address per radio
profile when a profile lands (common starting points: 9600 or 38400 8N1 —
verify per manual).

---

## Testing

Verify on the Android emulator with real repeater data (OSM import around the
Innlandsnettet example):

1. Run Hamlib **dummy** rig (model 1) and/or `rigctld` on the host.
2. From the emulator, reach the host at `10.0.2.2:4532` using the Hamlib
   **NET rigctl** model.
3. Drive auto-tune / `cat_vfo_set` with Innlandsnettet-style NFM parameters.
4. Log frequency, shift, and tone read back from the dummy / `rigctld` session.

---

## Status

| Piece | Status |
|---|---|
| Hamlib Android build (NDK) | Not implemented |
| Hamlib FFI crate | Not implemented |
| Android USB/BT serial bridge | Not implemented |
| CAT serial adapters | Replaced by Hamlib; not implemented |
| Onboard repeater DB / OSM import | Not implemented |
| RepeaterBook sync | Not implemented |
| Auto-tune → VFO 1 | Specified here; not implemented |
| Plugin capability wiring | Proposed in [`plugins.md`](plugins.md) |
