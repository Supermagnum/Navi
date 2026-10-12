# CAT Benshi / VR-N76 investigation (issue #149)

Sources reviewed for the native Bluetooth backend. Protocol is ported from
HTCommander and cross-checked with benlink; application logic is not copied.
Apache-2.0 attribution belongs in source headers and in [`CAT.md`](CAT.md).

**Scope of this task:** simulator / CI only. No real Bluetooth scan, pair, or
RFCOMM to a physical radio. The issue's "Hardware verification (manual, VR-N76)"
checklist stays pending in [`CAT.md`](CAT.md).

---

## Sources

| Source | Path / URL | Role |
|---|---|---|
| HTCommander fork | https://github.com/Supermagnum/HTCommander | Primary protocol + Android RFCOMM |
| Protocol blog | `docs/blogs/radio-command-protocol.md` | Framing, command table, payload layouts |
| Dart models | `src/lib/radio/gaia_protocol.dart`, `radio_models.dart`, `radio.dart`, `utils.dart` | Codecs and session behaviour |
| Android transport | `src/android/.../BluetoothClassicPlugin.kt` | SPP UUID, insecure RFCOMM, channel probe |
| btsnoop tool | `src/tools/btsnoop_decoder` | HCI → GAIA decode (UV-Pro firmware capture sample) |
| benlink | https://github.com/khusmann/benlink | Second description; sub-audio / DCS map |
| Navi CAT docs | [`CAT.md`](CAT.md), [`cat-test.md`](cat-test.md) | Existing Hamlib path, follow, RepeaterBook disabled |
| SPLAT coverage | https://github.com/Supermagnum/splat `test-results/LA5MR.geojson.bz2` | Oracle for coverage score J only |
| Issue comment | #149 (Supermagnum, 2026-10-07) | RepeaterBook App #114 / server-side copy idea |

---

## Protocol summary (confirmed from HTCommander)

### Framing

- Bluetooth Classic RFCOMM, SPP UUID `00001101-0000-1000-8000-00805F9B34FB`
  (`BluetoothClassicPlugin.kt` line 50).
- GAIA wrapper: `FF 01 <cs> <len> <group16BE> <cmd16BE> <data...>`.
- BLE GATT uses the same command bytes without the `FF 01` wrapper (Linux/iOS/web
  in HTCommander). Navi Android path uses Classic + GAIA.
- Header big-endian. Group 2 = basic. Replies set bit `0x8000` on command id.
  Status byte at offset 4 (`0` = success); payload from offset 5.
- Audio RFCOMM UUID `39144315-32FA-40DB-85ED-FBFEBA2D86E6` exists in HTCommander;
  **Navi must never open it** (issue never-transmit rule).

### Commands used by the backend (group 2)

| Id | Name | Use |
|---|---|---|
| 4 | `GET_DEV_INFO` | Identity / caps (`vendorId`, `productId`, `supportVfo`, `supportDmr`, `channelCount`) |
| 6 | `REGISTER_NOTIFICATION` | `htStatusChanged` (1), `freqModeStatusChanged` (14) |
| 9 | `EVENT_NOTIFICATION` | Inbound only |
| 10 / 11 | `READ_SETTINGS` / `WRITE_SETTINGS` | channel A/B + `doubleChannel`; write A only |
| 13 / 14 | `READ_RF_CH` / `WRITE_RF_CH` | 25-byte channel record (scratch method) |
| 20 | `GET_HT_STATUS` | `isInTx`, `isSq`, `isInRx`, `currChId`, selected VFO |
| 35 | `FREQ_MODE_SET_PAR` | Frequency-mode tune (RX/TX/sub-audio/flags/step) |
| 36 | `FREQ_MODE_GET_STATUS` | Read tuned frequency (partial; see Q3) |

Refuse before I/O (non-exhaustive): `HT_SEND_DATA` (31), `STORE_SETTINGS` (12),
`DO_PROG_FUNC` (66), group 10 firmware VM, anything on the audio link.

### Encodings

- Frequency: 32-bit BE; top 2 bits modulation (0 = FM), low 30 bits Hz.
- Sub-audio in `FREQ_MODE_SET_PAR`: 16-bit; CTCSS in 0.01 Hz (88.5 Hz → 8850);
  0 = none. Channel record uses the same 16-bit fields at bytes 14/16 of the
  reply payload (`RadioChannelInfo.fromBytes`).
- DCS (benlink `SubAudioMap`): raw value `< 6700` treated as DCS code; CTCSS as
  `Hz * 100` in range 67–254.1. HTCommander docs label the field "CTCSS/DCS"
  without spelling the DCS bit layout in the blog; prefer benlink's map when
  encoding DCS (**inferred**, confirm on hardware for edge codes).
- Settings: bit-packed. Always copy last-read raw block and change only needed
  bits (`toByteArrayWith`), matching HTCommander.
- `FREQ_MODE_SET_PAR` trailing constant `0x61A8` (25000) sent unchanged; flags
  word `0x0000` for ordinary VFO mode (high byte `0x0A` is satellite screen only).
- Notifications cleared by power cycle; re-register on every connect.

### Identity gating

HTCommander `RadioDevInfo.name` (`radio_models.dart` ~90–102):

| `productId` | Name |
|---|---|
| `0x0001` | VR-N7500 |
| `0x0002` | VR-N76 |
| `0x0003` | SA-888S |
| `0x0004` | HG-UV98 |
| `0x0005` | HAM-AIO |

Issue allowlist for this task: **VR-N76 `0x0002` only**. UV-Pro / GA-5WB
identities are not in that table (**needs hardware**). Unknown / malformed /
truncated `GET_DEV_INFO` fails closed.

### Transport notes (Android)

- Prefer **insecure** RFCOMM (`createInsecureRfcommSocketToServiceRecord`);
  radios reject authenticated/secure sockets (plugin comments ~290–304).
- If UUID/SDP channel fails, probe fixed RFCOMM channels (observed 1 and 4).
- Cancel discovery before connect when `BLUETOOTH_SCAN` is granted.
- Permissions: `BLUETOOTH_CONNECT`, `BLUETOOTH_SCAN`.
- Queue commands one-at-a-time with reply timeout and limited retries
  (`radio.dart` read queue comments ~186–191).

---

## Open questions 1–10

Evidence tags: **confirmed from source** / **inferred** / **needs hardware**.

### 1. Which VFO does `FREQ_MODE_SET_PAR` act on?

**Needs hardware** (design-critical).

Sources say "tune the VFO" / "frequency mode" without naming A vs selected
(`radio-command-protocol.md` satellite section; `RadioFreqModePar` docs).
Satellite tracking in HTCommander forces `doubleChannel: 0` before the loop
(`radio.dart` ~860–866), which avoids the question rather than answering it.

**Safe design until confirmed:** send frequency mode only when HT status shows
VFO 1 selected or dual-channel off; otherwise use scratch channel A
(`WRITE_RF_CH` + `WRITE_SETTINGS` with `channelA` only, `doubleChannel` and
`channelB` preserved).

### 2. How is bandwidth (12.5 kHz) set in frequency mode?

**Needs hardware** for the live VFO path.

Channel record sets narrow/wide via bit `0x10` in flags byte
(`RadioChannelInfo`, byte 18). `FREQ_MODE_SET_PAR` has no bandwidth field; the
trailing `0x61A8` is documented as an opaque constant (25000), not bandwidth.
If frequency mode cannot set 12.5 kHz, fall back to scratch channel
(**inferred** from issue Design §3).

### 3. Exact `FREQ_MODE_GET_STATUS` reply layout; does it return sub-audio?

**Partially confirmed from source.**

`radio.dart` `_handleFreqModeStatus` (~3089–3100): reply status at `[4]`,
frequency at `[5..8]` (mod + Hz). It does **not** parse TX or sub-audio from
command 36.

`freqModeStatusChanged` notification 14 (~3108–3120): RX `[5..8]`, TX `[9..12]`,
sub-audio `[13..16]`, flags `[17..18]`; low flags byte is the reliable
frequency-mode indicator.

**Read-back policy:** prefer notification 14 (and/or re-read) for RX/TX/tone;
do not assume command 36 alone proves tone match until hardware confirms.

### 4. Does the radio leave frequency mode cleanly on disconnect? Restore channel?

**Partially confirmed from source.**

All-zero `FREQ_MODE_SET_PAR` drops frequency mode and restores normal channel
state (protocol blog teardown; `RadioFreqModePar.stop()`). That is an explicit
stop, not disconnect.

Behaviour on RFCOMM drop without stop: **needs hardware**. Design: on graceful
disconnect send stop payload when frequency mode was entered by Navi; on hard
drop, reconnect + re-register notifications and treat VFO 1 as unknown until
read-back.

### 5. Accept `FREQ_MODE_SET_PAR` / channel write while scanning or dual-watch?

**Needs hardware.**

HT status exposes `isScan` and `doubleChannel`. Design: if `isScan`, defer
program (same class as squelch defer). Dual-watch with VFO 2 selected uses
scratch / VFO-1-addressed path only (see Q1).

### 6. More than one control connection at a time?

**Needs hardware.**

HTCommander keeps one control map entry per address; no multi-client protocol
doc. Simulator assumes single peer. Product: one Navi session owns the link.

### 7. Can HT status distinguish RX on VFO 1 vs VFO 2 for carrier-detect?

**Inferred (usable with caveats); needs hardware to trust.**

`RadioHtStatus` has `doubleChannel` (selected VFO) and `currChId` (channel being
received). Protocol blog: `doubleChannel` is the live "which VFO is the operator
on" indicator; `currChId` is the active receive channel id.

If `currChId == settings.channelA` (or VFO 1's channel) while VFO 2 is selected,
treat carrier as VFO 1 and allow deferral rules per issue. If ambiguous, defer
as today. **Transmitting (`isInTx`) always blocks**, either VFO.

### 8. Does changing channel A while VFO 2 is selected disturb VFO 2 RX?

**Needs hardware.**

HTCommander notes that the radio "only commits the channel of the currently
selected VFO" for some settings writes (`radio.dart` ~638–641), which is why
they select VFO B when changing B. That suggests writing `channelA` while B is
selected may be accepted without stealing focus (**inferred**), but brief audio
glitches are unknown. Protecting VFO 2 beats a timely switch: if scratch write
disturbs B in testing, defer until VFO 1 is selected.

### 9. After last LA5MR member (LA9AR) goes out of range?

**Decided (checkpoint): C.**

Hold the last networked site, notify the user, and exit network follow.
Non-networked sites only via the normal auto-tune flow (with confirmation if
auto-apply is off). Written into [`CAT.md`](CAT.md) under Network follow;
Østerdalen replay (test I) must assert this sequence.

Also accepted at checkpoint:

- **Q1 default:** frequency mode only when VFO 1 selected / dual off; otherwise
  scratch channel A.
- **Q3 default:** after each `FREQ_MODE_SET_PAR`, wait for a **fresh**
  notification 14 (arriving after the write) with timeout; stale notifications
  do not count. Mismatch / timeout → unverified → normal mismatch-retry. If
  notification 14 is unreliable in the simulator, fall back to scratch channel
  for that device.
- **#114:** RepeaterBook's own Python Client app, not a Navi PR. Proposal only;
  client stays keyless; RepeaterBook stays disabled.

### 10. `vendorId` / `productId` for BTECH UV-Pro and Radioddity GA-5WB?

**Needs hardware.**

Not in HTCommander product table. benlink lists UV-Pro and GA-5WB as supported
but does not publish numeric IDs in `dev_info.py`. btsnoop sample in HTCommander
is a UV-Pro firmware upgrade capture — decode that log later for IDs without
live pairing if the capture includes `GET_DEV_INFO`. Until then fail closed.

---

## Contradictions that affect design

1. **Navi PR #114 vs issue comment "App #114".** Navi PR #114 is unrelated
   ("Stay in Country…"). "App #114" is RepeaterBook's registered application
   **RepeaterBook Python Client** (see MicaelJarniac/repeaterbook docs issue #58).
   Not a Navi PR to implement.
2. **FREQ_MODE read-back:** issue asks for RX/TX/sub-audio match; HTCommander
   command 36 only exposes frequency. Use notification 14 / channel read for
   full verify.
3. **benlink vs HTCommander on VR-N76:** benlink marks VR-N76 untested; HTCommander
   maps `0x0002` → VR-N76. Navi gates on HTCommander IDs; simulator uses `0x0002`.

No other source contradiction blocks the Design section; Q1 remains the main
hardware-gated branch.

---

## RepeaterBook server-copy proposal (docs only)

Issue comment: keep a server-side copy so the client never needs an API key.

### Facts

- RepeaterBook HTTP export requires an application token (`rbuapp_…`) and a
  matching User-Agent; HTCommander stores the token in settings
  (`repeaterbook_client.dart`).
- Third-party Python client is registered as **App #114**; that does not grant
  Navi permission.
- Navi today: RepeaterBook importer always disabled
  ([`CAT.md`](CAT.md), scenario 12, `import_repeaterbook` errors).

### Proposal (navi-server later — **not implemented in this task**)

1. Obtain **written permission** from RepeaterBook for a server-side mirror
   (required; do not ship without it).
2. Server job (navi-server): periodically pull export API with server-held
   credentials; store a normalized regional extract (callsign, lat, lon,
   freq_out, shift, tone, mode, network_id, as-of timestamp).
3. App fetch: plain **read-only HTTPS GET** of that extract (or pack sidecar),
   same trust model as other pack downloads — **no RepeaterBook API key in the
   client**.
4. Client merge: upsert into onboard repeater DB with expiry; OSM / CSV /
   OpenRepeater remain primary offline sources.
5. Until permission + server exist: keep RepeaterBook **disabled** in the app
   (current behaviour).

---

## Fixture note (prerequisite)

Committed `testdata/cat/osm/la5mr/relation_18780801_full.xml` still lists
**Bagn** and is dated `2026-10-01`. Issue: relation now has **11** members
including LA9AR, LA6JRR, LA5MR Høykorset (Bagn replaced). Re-fetch, update
`FETCH_DATE.txt`, regenerate Run A/B, and extend Østerdalen corridor fixtures
before scoring H/I/J.

SPLAT oracle: `test-results/LA5MR.geojson.bz2` in Supermagnum/splat (present).

---

## Design mapping (implementation checklist)

| Issue section | Host (`navi-cat`) | Guest (`plugins/CATS-plugin`) | Android |
|---|---|---|---|
| `BenshiBackend` on `RigBackend` | yes | unchanged select/follow | — |
| VFO 1 only; never touch VFO 2 / selection | yes | — | — |
| Frequency mode + scratch channel | yes | — | — |
| Read-back + VFO 2 before/after | yes | — | — |
| Interlocks PTT / squelch | yes | — | — |
| Never-transmit allowlist | yes (before I/O) | cannot bypass | never open audio UUID |
| Native gate `productId == 0x0002` | yes | — | — |
| RFCOMM + GAIA queue | transport trait | — | injected socket in tests |
| UI backend picker / scratch slot / status | prefs + sheet | — | `CatStatusSheet` |
| Demo mode 5 s steps | host orchestration | follow timings unchanged | labelled UI |

---

## Merge / baseline (Step 0)

Recorded after bringing `CAT` onto current `origin/dev` and re-applying CAT
code (dev had reverted the prior CAT merge).

| Check | Result |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo test -p driver-break-core --lib` | PASS (709 passed, 14 ignored) |
| `cargo test -p navi-ffi --lib` | PASS (17 passed) |
| `cargo test -p navi-cat --tests` | PASS (units + dummy_rigctld + gating + import + never_transmit + scenarios) |
| Kotlin CAT unit tests | see [`cat-test.md`](cat-test.md) / follow-up baseline after merge fix |

Shared UI/UniFFI files needed manual re-application on top of `dev` (online place
search + CAT) after the revert/reapply; details in the Step 0 commit message /
test report.
