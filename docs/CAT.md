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

1. Talk to a mobile transceiver via **Hamlib** (`rigctld`), not hand-written
   vendor dialects.
2. Only drive radios whose Hamlib backend is **Stable** and supports the
   functions auto-tune needs.
3. Look up nearby **NFM** (narrow FM or other modes that the radio can do ) amateur **repeaters**.
4. If one is within **150 km**, program **VFO 1** with output frequency, duplex
   offset/shift, and CTCSS/DCS (subtone).
5. For repeaters in a **network** (e.g. LA5MR / Innlandsnettet), automatically
   follow the closest member site while driving.
6. Never key the transmitter automatically.

---

## Radio control via Hamlib

Navi does not implement Kenwood `FA`/`FB`, Yaesu, Icom CI-V or other dialects
itself. The host runs `rigctld` for the configured radio and the `cat` host
service talks to it over TCP (default port 4532) using the rigctld protocol.

```text
Navi cat service  ──TCP──▶  rigctld -m <model> -r <serial port> -s <baud>  ──CAT──▶  radio
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

Existing crates (checked September 2026):

| Crate | What it is | Covers auto-tune needs? |
|---|---|---|
| [`hamlib-client`](https://crates.io/crates/hamlib-client) 1.1.0 | Async (tokio) rigctld client | **No.** Getters only (freq, mode, VFO, split, info); no set, shift, offset, CTCSS or `dump_caps`. Licence **GPL-3.0-only** |
| [`rigctld`](https://crates.io/crates/rigctld) 0.1.0 | rigctld client (extended response protocol) + helper to start/stop the daemon | **No.** Only get/set frequency and mode. Tests already use the dummy rig. Last release 2023 |
| [`hamlib-sys`](https://github.com/MatthewIsHere/hamlib-sys) | Unsafe FFI bindings to libhamlib 4.0 | Full C API incl. `rig_caps.status`, but raw `unsafe`, links libhamlib into Navi |

**Decision:** Navi ships its own small rigctld client in the `cat` host
crate (e.g. `navi-cat`), speaking the rigctld TCP protocol directly with
`tokio::net::TcpStream`. The protocol is line-based and only a handful of
commands are needed:

| Purpose | Command(s) |
|---|---|
| Gating | `\dump_caps` |
| Program VFO 1 | `V`, `F`, `M`, `R`, `O`, `C` (`D` for DCS) |
| Read-back | `f`, `m`, `r`, `o`, `c` (`d`) |
| Interlocks | `t` (PTT), `\get_dcd` (if supported) |

Use the extended response protocol (prefix `+`) so every reply ends in an
explicit `RPRT n` line, which simplifies error handling.

Rationale:

- The existing clients lack the set/shift/tone/caps commands, so either one
  would need forking anyway.
- `hamlib-client` is GPL-3.0-only; depending on it would constrain Navi's
  licence. Talking to rigctld over TCP keeps Hamlib (LGPL/GPL) in a separate
  process with no linking.
- `hamlib-sys` would give direct access to `rig_caps.status`, but brings
  `unsafe` FFI, build-time dependency on libhamlib-dev, and loses process
  isolation (a backend crash would take Navi down). Keep it as a fallback
  option only.
- `rigctld`'s daemon start/stop helper is a useful reference for launching
  `rigctld` from the host; the same pattern can be reused in `navi-cat`.

Re-check crates.io before implementation in case a more complete client has
appeared; if one does, it must cover every command above and have a licence
compatible with Navi.

### Backend gating (Stable only)

At connect, the service sends `\dump_caps` and parses the capability report.
Auto-tune is enabled only if **all** of these hold:

| `dump_caps` line | Required value |
|---|---|
| `Backend status` | `Stable` |
| `Can set Repeater Shift` | `Y` |
| `Can set Repeater Offset` | `Y` |
| `Can set CTCSS Tone` | `Y` |

- **Fail closed:** a missing or unparseable line counts as a failure.
- The exact `dump_caps` wording has changed slightly between Hamlib versions;
  pin and verify against the Hamlib version shipped with the host.
- If gating fails, show the reason in the UI (e.g. "backend is Beta",
  "radio cannot set CTCSS via CAT") and keep auto-tune disabled.
- Optional user override: allow **Beta** backends with an explicit warning.
  Alpha / Untested are never allowed.
- Test builds may allow the dummy rig (model 1) regardless of its reported
  status (see [Testing](#testing)).

Equivalent check if Hamlib is linked via FFI instead: `rig_caps.status ==
RIG_STATUS_STABLE` and non-null `set_rptr_shift`, `set_rptr_offs`,
`set_ctcss_tone`.

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

After programming, read back (`f`, `m`, `r`, `o`, `c`) and compare. Some
radios reset offset or tone when frequency changes, so always send frequency
first and verify the full state afterwards. A mismatch counts as a failure.

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

## Repeater data sources

### Onboard database (preferred offline)

Populate from:

1. **OSM** — nodes/ways tagged
   `communication:amateur_radio:repeater=yes` (and related frequency / CTCSS /
   shift / modulation tags), grouped by `type=network` relations.
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

## HostApi sketch

| Capability | Behaviour |
|---|---|
| `cat_status` | Output: connected, model, backend status, gating result + reason, PTT state |
| `repeater_query` | Input: lat, lon, radius_km (≤ 150), optional network_id. Output: JSON list of NFM sites (with network_id) |
| `cat_vfo_set` | Input: freq_out_mhz, shift_mhz, ctcss_hz, mode=`NFM`, vfo=`1`. Host executes via rigctld, verifies read-back |
| `cat_network_follow` | Input: network_id, enabled, optional pinned site. Host runs follow loop and emits switch events |

Radio control lives in the host (rigctld client), not WASM.

---

## Testing

Use the Hamlib **dummy rig** (model 1) for development and CI:

```text
rigctld -m 1 -t 4532
```

The dummy stores frequency, mode, VFO, shift, offset, tone and PTT in memory,
so tests can program VFO 1 through the normal path and assert the result with
read-back.

Test cases:

- Full auto-tune sequence for a known site (e.g. LA5TRR) → verify all values.
- CTCSS inheritance from network default.
- PTT interlock: set `T 1` on the dummy, confirm Navi refuses to program.
- Network follow: simulated GPS track across several LA5MR sites; check that
  switches happen at the right points and hysteresis prevents flapping.
- Error paths: rigctld not running, rigctld killed mid-sequence, out-of-range
  values. The dummy accepts almost everything, so rejection handling must be
  tested this way.
- Gating parser: feed saved `dump_caps` outputs (Stable, Beta, missing lines)
  and check the result.

The dummy does not reproduce radio-specific behaviour (timing, settings reset
on frequency change, unsupported functions). Final verification on the real
radio is still required.

---

## Status

| Piece | Status |
|---|---|
| `navi-cat` rigctld client (own crate) | Not implemented |
| Backend gating (`dump_caps`) | Not implemented |
| Onboard repeater DB / OSM import | Not implemented |
| RepeaterBook sync | Not implemented |
| Auto-tune → VFO 1 | Specified here; not implemented |
| Network follow mode | Specified here; not implemented |
| Dummy-rig test suite | Not implemented |
| Plugin capability wiring | Proposed in [`plugins.md`](plugins.md) |
