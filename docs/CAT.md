# CAT (Computer Aided Transceiver)

CAT control for amateur radio is implemented on the `CAT` branch. This document
is the product and safety specification for the WASM guest in
[`plugins/CATS-plugin/`](../plugins/CATS-plugin/) and the host services
(`navi-cat`, Hamlib FFI, Android transports, repeater DB).

Vehicle / energy telemetry is separate: see [`ECU.md`](ECU.md). Plugin overview:
[`plugins.md`](plugins.md). LoRa convoy status uses the same client/display
split (Navi does not implement the RF layer) over Meshtastic BLE:
[`plugins/lora-convoy-spec.md`](plugins/lora-convoy-spec.md).

---

## Architecture decisions (locked)

### 1. Single `RigBackend` trait

Gating, the VFO 1 command sequence, **app-side read-back verification**, and
PTT/DCD interlocks live in one shared implementation behind `RigBackend`:

| Backend | Where used |
|---|---|
| TCP `rigctld` (extended `+`, `RPRT n`) | Desktop, CI, remote NET rigctl |
| Hamlib FFI (`navi-hamlib-sys`) | Android onboard; `rig_pathname = 127.0.0.1:<port>` for USB/BT loopback bridges; NET rigctl model for remote |

The shared test suite runs against both backends. A faulty or malicious WASM
guest cannot bypass host interlocks or skip read-back.

### 2. Hamlib version: latest stable (not a fixed 4.6.5 pin)

[`scripts/build-hamlib-android.sh`](../scripts/build-hamlib-android.sh) resolves
the **latest stable** Hamlib release tag, records tag + NDK version in
`scripts/hamlib-android.lock` for CI caching, and CI fails if the resolved tag
changes without re-verifying `dump_caps` parser fixtures. Path/network docs
below were originally verified against 4.6.5; re-verify on each lock bump.

### 3. crates.io Hamlib crates stay rejected

The evaluation table under [Rust packages](#rust-packages) is the permanent
record of what was checked and why it was rejected. Navi uses **own**
`navi-cat` (TCP) + **own** `navi-hamlib-sys` (FFI). Do not add crates.io
Hamlib bindings as dependencies.

### 4. Plugin location and sandbox

**All plugin logic** (repeater selection, 150 km auto-tune decisions, network
follow / hysteresis / pinning, UI state, logging) lives in
`plugins/CATS-plugin/` and runs inside wasmtime. Radio I/O, transports, Hamlib,
and the repeater DB live in the host and are exposed only via HostApi:
`cat_status`, `repeater_query`, `cat_vfo_set`, `cat_network_follow`.

### 5. Read-back verification (app feature)

Every programming operation succeeds only when the radio (or dummy) reports
values that match the request (tolerances below). APIs return the **reported**
state. Unverified fields count as failure for auto-tune gating. On mismatch:
retry once with the full sequence; then fail, name the field (requested vs
reported), stop follow safely.

---

## Goals

1. Talk to a mobile transceiver via **Hamlib** (`rigctld` or in-process FFI),
   not hand-written vendor dialects.
2. Only drive radios whose Hamlib backend is **Stable** and supports the
   functions auto-tune needs.
3. Look up nearby **NFM** (narrow FM or other modes that the radio can do)
   amateur **repeaters**.
4. If one is within **150 km**, program **VFO 1** with output frequency, duplex
   offset/shift, and CTCSS/DCS (subtone).
5. For repeaters in a **network** (e.g. LA5MR / Innlandsnettet), automatically
   follow the closest member site while driving.
6. Never key the transmitter automatically.

---

## Radio control via Hamlib

Navi does not implement Kenwood `FA`/`FB`, Yaesu, Icom CI-V or other dialects
itself. The host uses a `RigBackend`:

```text
# Desktop / CI / remote
Navi cat service  ──TCP──▶  rigctld -m <model> …  ──CAT──▶  radio

# Android onboard
Navi cat service  ──FFI──▶  libhamlib.so  ──TCP loopback──▶  USB/BT bridge  ──▶  radio
```

Benefits:

- One adapter in Navi covers every radio Hamlib supports.
- Hamlib separates shift **direction** (`R +` / `R -`) from **offset magnitude**
  (`O <Hz>`) and converts to each radio's own convention, so Navi never
  handles vendor sign conventions.
- Every command returns `RPRT 0` on success or a negative error code, giving
  one uniform error path.

Model number, serial port and baud rate are part of the radio profile in the
host config. Verify baud rate against the radio manual.

### Rust packages

Existing crates (checked September 2026) — **evaluated and rejected**:

| Crate | What it is | Covers auto-tune needs? |
|---|---|---|
| [`hamlib-client`](https://crates.io/crates/hamlib-client) 1.1.0 | Async (tokio) rigctld client | **No.** Getters only (freq, mode, VFO, split, info); no set, shift, offset, CTCSS or `dump_caps`. Licence **GPL-3.0-only** |
| [`rigctld`](https://crates.io/crates/rigctld) 0.1.0 | rigctld client (extended response protocol) + helper to start/stop the daemon | **No.** Only get/set frequency and mode. Tests already use the dummy rig. Last release 2023 |
| [`hamlib-sys`](https://github.com/MatthewIsHere/hamlib-sys) | Unsafe FFI bindings to libhamlib 4.0 | Full C API incl. `rig_caps.status`, but raw `unsafe`, links old libhamlib; unmaintained |

**Decision:** Navi ships its own `navi-cat` crate (TCP `RigBackend` + shared
program/verify) and `navi-hamlib-sys` (minimal FFI). Protocol commands:

| Purpose | TCP | FFI |
|---|---|---|
| Gating | `\dump_caps` | `rig_caps.status` + setter ptrs |
| Program VFO 1 | `V`, `F`, `M`, `R`, `O`, `C` (`D` for DCS) | matching `rig_set_*` |
| Read-back | `v`, `f`, `m`, `r`, `o`, `c`, `d`, `t`, `\get_dcd` | see FFI list below |
| Interlocks | `t` (get PTT), `\get_dcd` | `rig_get_ptt`, `rig_get_dcd` |

Never bind or send `rig_set_ptt` / `T`. Use the extended response protocol
(prefix `+`) so every TCP reply ends in an explicit `RPRT n` line.

### Backend gating (Stable only)

At connect, the service sends `\dump_caps` (or reads FFI caps) and parses the
capability report. Auto-tune is enabled only if **all** of these hold:

| `dump_caps` line | Required value |
|---|---|
| `Backend status` | `Stable` |
| `Can set Repeater Shift` | `Y` |
| `Can set Repeater Offset` | `Y` |
| `Can set CTCSS Tone` | `Y` |

- **Fail closed:** a missing or unparseable line counts as a failure.
- Re-verify parser fixtures when `scripts/hamlib-android.lock` changes.
- If gating fails, show the reason in the UI and keep auto-tune disabled.
- Optional user override: allow **Beta** backends with an explicit warning.
  Alpha / Untested are never allowed.
- Test builds may allow the dummy rig (model 1) regardless of its reported
  status (see [Testing](#testing)).

Equivalent FFI check: `rig_caps.status == RIG_STATUS_STABLE` and non-null
`set_rptr_shift`, `set_rptr_offs`, `set_ctcss_tone`.

### Command sequence (VFO 1)

Example for LA5TRR (145.725 MHz, −0.6 MHz, 88.5 Hz):

```text
V VFOA            # VFO 1
F 145725000       # frequency_out, Hz
M FM 12500        # narrow FM (or FMN where the backend supports it)
R -               # shift direction
O 600000          # offset magnitude, Hz
C 885             # CTCSS, tenths of Hz
```

After the full set sequence (frequency first), read back **all** fields: VFO,
frequency, mode + passband, shift direction, offset, CTCSS or DCS. Compare with:

| Field | Tolerance |
|---|---|
| Frequency | Exact Hz, or within the backend-reported step if step-rounded |
| CTCSS | Exact in tenths of Hz |
| Mode, shift, DCS | Exact |

A mismatch retries once with the full sequence; then fails with the field named
(requested vs reported). Unreadable fields are unverified = failure for
auto-tune.

---

## Auto-tune algorithm (VFO 1)

```text
1. Read current position (GPS / last fix).
2. Query repeater sources (onboard DB first; optional RepeaterBook sync).
3. Filter: amateur repeater, modulation = NFM / narrow FM (e.g. 11K2F3E),
   distance ≤ 150 km (Haversine).
4. Pick best candidate (nearest usable, or user-selected from a short list).
5. Resolve:
     - frequency_out (repeater downlink / mobile receive)
     - shift direction + offset magnitude
     - CTCSS encode (inherit network default if the site has none)
6. Interlocks: backend gating passed, PTT off (`t` returns 0).
7. CAT: program VFO 1 (see command sequence), then read back and verify.
8. UI: show callsign, network, distance, freq, shift, tone; require user
   confirm if "auto-apply" is off.
```

**150 km** matches the upper display-range clamp used for APRS tracks
(`DISPLAY_RANGE_MAX_KM`). Do not auto-tune beyond that without an explicit
user override.

### Frequency / offset conventions

| Field | Meaning for the mobile | Hamlib |
|---|---|---|
| Output / `frequency_out` | Frequency the repeater transmits (mobile **listens** here) | `F`, Hz |
| Shift / offset | Duplex split so the mobile **transmits** on input (e.g. −0.6 MHz on 2 m) | `R` sign + `O` Hz |
| CTCSS | Access tone required by the repeater | `C`, tenths of Hz |
| DCS | Digital access code (if used instead of CTCSS) | `D` |

European OSM tags sometimes use a comma as decimal separator (`-0,6 Mhz`);
normalize to MHz with `.` before converting to Hz.

---

## Network follow mode

When the selected repeater belongs to a `type=network` relation (e.g. LA5MR),
Navi can automatically retune to the **closest member site** as the vehicle
moves. Because network members are linked, switching sites keeps the same
conversation audible.

### Behaviour

```text
Every position update (throttled, e.g. every 10 s):
1. Candidates = NFM member sites of the active network within 150 km.
2. Best = nearest candidate.
3. If Best ≠ current site AND hysteresis rules pass AND interlocks pass:
     retune VFO 1 to Best (full command sequence + read-back).
4. If no member is within 150 km: stay on current site, notify user,
   leave follow mode.
```

### Hysteresis (avoid flapping between sites)

A switch happens only when **all** of these hold:

| Rule | Default (tunable) |
|---|---|
| New site is closer by at least a margin | 5 km **or** 20 %, whichever is larger |
| Condition has held continuously | 30 s |
| Minimum time since last switch | 60 s |

Distance is a proxy for coverage; terrain can make a farther site better. The
user can **pin** a site, which suspends follow mode until unpinned.

### Interlocks for follow mode

- Never switch while PTT is active.
- Never switch while receiving, if the backend supports carrier detect
  (`get_dcd`). If it does not, rely on the dwell rules above.
- Only switch within the **same network**. Leaving the network requires the
  normal auto-tune flow (and confirmation if auto-apply is off).
- Follow mode is opt-in per session and shown clearly in the UI.
- Every switch is logged and announced briefly (callsign + frequency).

---

## Safety interlocks (summary)

- Default: **RX + memory/VFO program only**; PTT remains manual.
- No programming while the radio is transmitting.
- No auto-tune unless backend gating has passed.
- Abort if rigctld is unreachable, a command returns a non-zero `RPRT`, or
  read-back does not match.
- Log frequency changes for the user; do not phone-home callsigns.

---

## Hamlib integration

### Do not use crates.io Hamlib crates

Do **not** add Rust Hamlib crates from crates.io (e.g. historical `hamlib` /
`hamlib-sys` bindings). They are years out of date, bind old Hamlib APIs, and
are unmaintained. Navi must not depend on them.

### Upstream Hamlib (C library)

Use **upstream Hamlib** (current 4.x line). Track the **latest stable** release
tag via [`scripts/build-hamlib-android.sh`](../scripts/build-hamlib-android.sh);
the resolved tag and NDK version are recorded in `scripts/hamlib-android.lock`.
Cross-compile for Android with the NDK and ship as a shared library:

| ABI | Role |
|---|---|
| `arm64-v8a` | Primary device |
| `armeabi-v7a` | 32-bit ARM devices |
| `x86_64` | Emulator |

Install as `libhamlib.so` under `jniLibs` for each ABI.

### Navi-owned FFI crate

Navi owns a minimal FFI crate (`navi-hamlib-sys`) covering only:

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
rig_get_vfo
rig_get_freq
rig_get_mode
rig_get_rptr_shift
rig_get_rptr_offs
rig_get_ctcss_tone
rig_get_dcs_code
rig_get_ptt          (TX interlock; never bind rig_set_ptt)
rig_get_dcd
rig_get_info         (or equivalent for model detection / logging)
# caps: status + non-null set_rptr_shift / set_rptr_offs / set_ctcss_tone
```

No broad wrap of the entire Hamlib API.

### Build

[`scripts/build-hamlib-android.sh`](../scripts/build-hamlib-android.sh):

1. Resolve the latest stable Hamlib release tag; write
   `scripts/hamlib-android.lock` (tag + NDK version).
2. Cross-compile with autotools + the Android NDK toolchain for the three ABIs
   above.
3. Disable Android-unneeded bits: C++ / Perl / Python / Tcl bindings, readline,
   and libusb-dependent backends (unless Navi later builds libusb for those
   backends).
4. Produce `libhamlib.so` artifacts suitable for packaging under `jniLibs`.
5. Be cacheable in CI (keyed on lock file + script hash). CI fails if the
   resolved tag changes without re-verifying `dump_caps` fixtures.

Do not invent ad-hoc vendor CAT parsers in Rust or Kotlin when Hamlib already
covers the radio.

### Licensing

Hamlib is **LGPL-2.1+**. Ship it **dynamically linked** (`libhamlib.so`). Keep
the lock file, source tag and the build script public so users can rebuild or
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

**Hamlib 4 network-path verification:** Confirmed on the 4.x line (originally
checked at tag 4.6.5; re-check when `hamlib-android.lock` bumps). The `rigctl`
man page documents `-r` / `--rig-file` as accepting a network `address:port`
(example `127.0.0.1:12345`). Hamlib detects such pathnames and opens
`RIG_PORT_NETWORK` (TCP). That is the intended path for (a) and (b): a
transparent serial-byte bridge on loopback, with a normal Kenwood/Yaesu/Icom
(etc.) model number. Option (c) uses the separate **NET rigctl** model against
`rigctld` (default port 4532), not raw serial framing.

---

## Repeater data sources

### Onboard database (preferred offline)

Populate by importing and cross-referencing the sources in
[Repeater data import and cross-referencing](#repeater-data-import-and-cross-referencing):

1. **OSM** — nodes/ways tagged
   `communication:amateur_radio:repeater=yes` (and related frequency / CTCSS /
   shift / modulation tags), grouped by `type=network` relations.
2. **User AnyTone CPS CSV** — channel / zone / gps-roaming / offset exports.
3. **OpenRepeater** / **RadioID** — community / DMR directories (downloadable).
4. **Bundled extract** — regional SQLite table (callsign, lat, lon, freq_out_mhz,
   shift_mhz, ctcss_hz, modulation, network_id) shipped or built from the above.
5. **RepeaterBook** — optional online sync only after written API permission
   (see below); never required for the 150 km search.

### RepeaterBook (optional online)

RepeaterBook remains an optional source as sketched here, but its API requires
**written permission from RepeaterBook** for use inside an app. Until that
permission is obtained, RepeaterBook sync stays **disabled**, and no other
import path, auto-tune path, or onboard DB build may depend on it.

When permission exists and the user enables network: query by position/bbox,
filter to modes the radio profile supports, upsert into onboard DB with expiry.
API keys and ToS stay host-side. Offline search must still work from OSM,
CSV, OpenRepeater, RadioID, and the last successful local imports.

---

## Repeater data import and cross-referencing

**Status:** specified here; **not implemented**. Doc-only planning for the
future `cat` plugin / host importer.

### Sources the plugin must accept

#### 1. AnyTone CPS CSV exports

Accept sample-shaped exports such as `channel.csv`, `zone.csv`,
`gps-roaming.csv`, and `offset.csv`.

Encoding and newlines: accept **UTF-8** and **Windows-1252**, **CRLF** or **LF**.

**`channel.csv`** — one row per channel. Observed key columns:

| Column | Role |
|---|---|
| Channel Name | Display / match key (max 16 chars in CPS) |
| Receive Frequency / Transmit Frequency | MHz |
| Channel Type | `A-Analog` or `D-Digital` (DMR) |
| Band Width | `12.5K` / `25K` |
| CTCSS/DCS Encode / Decode | Analog access tones |
| RX Color Code, Slot | DMR |
| Contact / Contact TG/DMR ID | DMR talkgroup / contact |
| APRS RX and other APRS columns | Used to **exclude** APRS rows (see Filtering) |

There are **no coordinates** in `channel.csv`.

Name patterns observed:

- Max **16 characters**.
- Analog-style: `Town CALLSIGN` (e.g. `Innland LA5MR`).
- DMR-style: `Town 242` / `Town lokal` (often **no callsign**).
- Norwegian letters are transliterated and truncated (`Toensberg`, `Bodoe`,
  `Mjoesa`, `Aalesund`, `Kr.sund N`).

One physical DMR repeater often appears as **several channels** (different
TG/slot). Deduplicate to one repeater by **RX + TX frequency + color code**.

**`zone.csv`** — zones with pipe-separated member names and RX/TX frequencies.
Zone names such as `ANALOG INNLANDET` are a **region hint only**, not a
precise location.

**`gps-roaming.csv`** — zone roaming points as degrees + minutes with N/S and
E/W flags and a radius. All-zero rows mean unused.

**`offset.csv`** — may be empty (header only); still accept the file.

#### 2. OpenRepeater (openrepeater.org)

Community directory, **CC0** data, downloadable. Covers FM, DMR, D-STAR,
Fusion/YSF, M17, AX.25, and related modes. Use as a position- and
parameter-rich merge source after filtering (see below).

#### 3. RadioID (radioid.net)

DMR repeater data. Useful for DMR identity / TG context and coordinates when
matching AnyTone digital channels and OSM/OpenRepeater rows.

#### 4. RepeaterBook (repeaterbook.com)

Optional online source as already described under
[RepeaterBook (optional online)](#repeaterbook-optional-online). Remains
**disabled** until written API permission is obtained; nothing else may depend
on it.

#### 5. OpenStreetMap

`communication:amateur_radio:repeater` nodes and `type=network` relations, as
already described in this document (see
[Example: Innlandsnettet OSM relation](#example-innlandsnettet-osm-relation)).

### Cross-referencing

Match records across sources in this order:

1. **Callsign** (when present).
2. **RX + TX frequency pair**, plus **color code** for DMR or **CTCSS/DCS** for
   analog.
3. **Town / channel name**, after normalizing CPS transliteration and 16-char
   truncation (so `Toensberg` can meet `Tønsberg`, `Kr.sund N` can meet
   `Kristiansund`, etc.).

**Position priority** (store accuracy with the record):

1. OSM node coordinates.
2. Exact lat/lon from OpenRepeater, RadioID, or RepeaterBook (when enabled).
3. Maidenhead locator centre as **fallback only**. A 6-character locator such
   as `JP65OU` is a subsquare several km across (roughly 4–5 km at Norwegian
   latitudes); never treat it as precise GPS.

When sources **disagree** on frequency, shift, or tone: keep **all** values,
prefer **OSM** or the **user’s own CSV**, and **flag the conflict in the UI**.

**CSV-only** repeaters with no match elsewhere get **no position** and are
**not** offered for distance-based auto-tune; they remain available for
**manual** selection.

### Filtering

- **Exclude APRS** stations and channels (digipeaters, iGates, AX.25 packet):
  by source mode/type, by channel name containing `APRS`, by APRS columns in
  the CSV, and by known APRS frequencies (e.g. **144.800 MHz** in Europe).
- **Simplex** entries (`RX = TX`, e.g. `Channel VFO A`) are **not** repeaters.
- Apply the existing mode rule: only repeaters whose mode the **radio profile**
  supports.

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
| `communication:amateur_radio:repeater:frequency_out` | `145.7250` | VFO RX (MHz → Hz) |
| `communication:amateur_radio:repeater:shift` | `-0,6 Mhz` | `R -`, `O 600000` |
| `communication:amateur_radio:repeater:ctcss` | `88.5` | `C 885`; may inherit network `88.5hz` |
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
2. **Fill gaps:** if a node omits CTCSS, inherit the relation's
   `repeater:ctcss`.
3. **Prefer linked coverage:** when several members are in range, pick the
   nearest NFM site and show "same network" so the operator knows linked
   audio is likely.
4. **Program VFO 1** from the chosen node's `frequency_out` + `shift` + tone.
5. **Follow:** in network follow mode, keep retuning to the nearest member as
   the vehicle moves (see [Network follow mode](#network-follow-mode)).

This is the model for an onboard "repeater network" table: one network row,
many site rows, shared defaults overridden per site.

---

## HostApi

| Capability | Behaviour |
|---|---|
| `cat_status` | Output: connected, model, backend status, gating result + reason, PTT state, last reported VFO |
| `repeater_query` | Input: lat, lon, radius_km (≤ 150), optional network_id. Output: JSON list of sites (with network_id, conflict flags) |
| `cat_vfo_set` | Input: freq_out_mhz, shift_mhz, ctcss_hz, mode=`NFM`, vfo=`1`. Host executes via `RigBackend`, verifies read-back; returns **reported** state |
| `cat_network_follow` | Input: network_id, enabled, optional pinned site. Host runs follow loop and emits switch events; each switch returns reported state |

Radio control lives in the host (`navi-cat`), not WASM. Guest code is only in
`plugins/CATS-plugin/`.

---

## Testing

Use the Hamlib **dummy rig** (model 1) for development and CI:

```text
rigctld -m 1 -t 4532
```

The dummy stores frequency, mode, VFO, shift, offset, tone and PTT in memory.
Every programming assertion uses (1) the app's read-back and (2) an
**independent** second rigctld connection. A mismatch-injection proxy rewrites
one reply to prove detection + single retry + safe stop. A recording proxy
asserts that no `T` / set-PTT is ever sent.

### Required scenarios (summary)

1. LA5MR network follow Espa → Dombås (hysteresis, PTT/DCD, pinning, mismatch stop).
2. Non-networked repeaters along the same route (query order, no APRS, auto-tune FM).
3. Single non-networked FM site — full VFO 1 read-back.
4. Non-networked DMR — import/dedupe; do not program unless profile+backend can read back.
5. Cross-source conflicts flagged; preferred source programmed and verified.
6. Filtering: APRS, simplex, CSV-only without position.
7. Gating parser fixtures (Stable / Beta / missing / Alpha).
8. Error paths: no daemon, kill mid-sequence, RPRT≠0, unverified field.
9. Never transmit.
10. Server-file repeaters (if present) match OSM fixtures.
11. Sandbox boundary: guest cannot skip read-back / program while PTT / ungated backend.
12. RepeaterBook stays off; no requests to repeaterbook.com.

### Emulator vs real device

| Layer | What it proves |
|---|---|
| JVM/Robolectric bridge unit tests | Fake USB/BT sockets through loopback bridge (reconnect, partial writes, disconnect) |
| Emulator instrumentation | Real `libhamlib.so` + CATS-plugin in wasmtime; remote `rigctld` via `10.0.2.2:4532`; USB/BT via injected boundaries (SPP data path to a real radio is **not** reproducible on emulator) |
| Real device (manual) | USB OTG + Bluetooth SPP with a physical radio; confirm read-back for every field after checking Hamlib backend status for that model |

### Fixture sources

See [`testdata/cat/SOURCES.md`](../testdata/cat/SOURCES.md) and
[`scripts/fetch-cat-fixtures.sh`](../scripts/fetch-cat-fixtures.sh).

### Server-file repeater check (Innlandet)

Recorded under Testing / SOURCES after the read-only pack/pmtiles/POI search
for `communication:amateur_radio:repeater=yes` and relation `18780801`. If
server files lack complete repeater tags, the client extracts from local PBF /
OSM fixtures; a future server-side bake is noted but **not** implemented on
navi-server from this branch.

### Test report (CAT branch)

| Item | Result |
|---|---|
| Hamlib lock | tag **4.7.2**, NDK **30.0.14904198** (`scripts/hamlib-android.lock`) |
| Geofabrik | Trailing-slash dated-URL retry; oppland/hedmark → ostlandet; Sweden PBF filename dedupe; soft-PASS logs `place_index_skipped=1` |
| Elsa long-route | Prior campaign: 8 corridor regions auto-downloaded, Via `(none)`, no forced packs |
| OSM changeset callsigns | See [`testdata/cat/SOURCES.md`](../testdata/cat/SOURCES.md) — LA2DRR, LA2HRR, LA2JRR, … LA5TRR, LA5MR, … |
| Non-networked corridor | [`testdata/cat/non_networked.json`](../testdata/cat/non_networked.json) (OSM-only; APRS excluded) |
| Server-file repeaters | **Partial** (21 hits in ostlandet fixture PBF incl. relation 18780801; incomplete vs current OSM) |
| RepeaterBook / repeatermap.de | No data committed; cross-check / disabled only |
| OpenRepeater Norway | Empty export (count=0) |
| RadioID | Terms forbid bulk redistribute; no data |
| Desktop unit tests | `navi-cat` gating/program/repeater; `navi-plugin-cats` select/follow |
| Emulator / hardware | Loopback bridge JVM tests; real USB/BT/radio still required for field confirmation |
| Still needs real hardware | USB OTG + Bluetooth SPP read-back on a physical transceiver after checking Hamlib backend status for that model |

Plugin logic lives only under `plugins/CATS-plugin/`. Host radio safety is in `navi-cat`.

---

## Future: rentable ham radio shacks (not implemented)

Reference OSM way: <https://www.openstreetmap.org/way/395284738>.

Fetch current tags for that way when documenting; search taginfo for
rentable/guest amateur radio station tag combinations and usage counts.
Possible Navi surfaces (doc only): map layer/POI, search, “near route” on long
trips; fields such as operator, website, booking, bands/equipment if tagged.
Other data sources beyond OSM may exist — list when researching. **No code**
for this on the `CAT` branch.

---

## Status

| Piece | Status |
|---|---|
| Architecture decisions (`RigBackend`, latest Hamlib, rejected crates) | Specified here |
| Plugin path `plugins/CATS-plugin/` + wasmtime sandbox | Specified here |
| Read-back verification (app-side) | Specified here |
| `navi-cat` + TCP/FFI backends | Implementing on `CAT` |
| Backend gating (`dump_caps`) | Implementing on `CAT` |
| Onboard repeater DB / OSM / CSV / OpenRepeater / RadioID | Implementing on `CAT` |
| Cross-source merge / conflict UI | Implementing on `CAT` |
| RepeaterBook sync | Disabled until written API permission |
| Auto-tune → VFO 1 / network follow | Implementing on `CAT` |
| Hamlib Android build script + lock | Implementing on `CAT` |
| Android USB/BT/remote transports | Implementing on `CAT` |
| Dummy-rig + scenario test suite | Implementing on `CAT` |
| HostApi `cat_*` / `repeater_query` | Implementing on `CAT` |
| Future ham-shacks | Doc only; not implemented |
