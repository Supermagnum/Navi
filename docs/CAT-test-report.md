# CAT test report

Canonical CAT branch evidence lives in [`cat-test.md`](cat-test.md)
(dummy-`rigctld` CI, Hamlib packaging, corridor Run A/B, CSV import).

Benshi / issue #149 investigation: [`CAT-benshi-investigation.md`](CAT-benshi-investigation.md).

This filename matches issue #149's `docs/CAT-test-report.md` reference.

## Baseline (Step 0 — 2026-10-12)

Branch `CAT` = `origin/CAT`; `origin/dev` had **0** commits not in CAT (no merge).

Prior open items (all closed before Step 4):

| Item | Status |
|---|---|
| Coverage J on Østerdalen | PASS — **93.3 %** talk / 3 switches |
| J in CI from reduced fixture | PASS — `testdata/cat/splat/` |
| Demo mode | PASS — FakeBenshiPeer 5 s/site; UI off by default |
| Android Benshi RFCOMM transport | PASS — injectable socket + unit tests |
| Emulator run K | PASS — SM-P613 3/3 injected |
| Sandbox grep (guest vs host) | PASS — I/O stays in `navi-cat` |

| Check | Result |
|---|---|
| `cargo test -p navi-cat --lib` | PASS (incl. mirror + Benshi A–G) |
| `cargo test -p navi-plugin-cats --tests` | PASS (H/I/J + demo) |
| Hamlib dummy `rigctld` | PASS (`dummy_rigctld`) |
| Existing scenarios 1–12 | PASS (`scenarios.rs`) |

## Step 1 — navi-server repeater mirror

Fixtures: `testdata/cat/server_mirror/` (captured **2026-10-12**; RadioID live,
RepeaterBook absent; synthetic RepeaterBook marked). Client Update button only;
no RepeaterBook/RadioID keys; no direct upstream contact.

| Case | Result |
|---|---|
| `current.json` RadioID only | PASS — notes RepeaterBook not available yet |
| Synthetic RepeaterBook present | PASS — parses without code change path |
| Checksum mismatch | PASS — rejected; last good kept |
| Empty / failed bundles | PASS — last good kept |
| Attribution on imported rows | PASS |
| `exclude_recommended` / `position_null` | PASS — skipped by auto-tune `query_near`; visible in UI list |
| RadioID = DMR info-only | By design — Benshi backend skips DMR (#149) |

## Issue #149 tests A–K

| Test | Status | Notes |
|---|---|---|
| A Fake Benshi peer | PASS | `FakeBenshiPeer` in `navi-cat::benshi::sim` |
| B Codec | PASS | GAIA / channel / settings / FREQ_MODE round-trips |
| C Program + read-back | PASS | Both tuning methods; second connection channel read |
| D Never transmit | PASS | Forbidden cmds refused before I/O |
| E Interlocks | PASS | PTT refuse; squelch defer |
| F VFO 2 untouched | PASS | Fingerprint + background scratch; defer variant |
| G Gating | PASS | VR-N76 `0x0002`; unknown fail closed |
| H Espa→Dombås | PASS | Navi car-router polyline `espa_dombas_e6.polyline` |
| I Østerdalen leave-network | PASS | Decision C after LA9AR on `espa_litjfossen_rv3_skansen.polyline` + past-network |
| J Coverage score ≥ 85 % | PASS | See scores below; reduced SPLAT in `testdata/cat/splat/` |
| K Emulator H/I + demo | PASS (injected socket) | `BenshiCorridorEmulatorKTest` on SM-P613 (3/3); host FakeBenshiPeer H/I/demo logs below; no real BT / no AVD |

## Coverage J (Navi polylines + SPLAT talk)

Scorer: `navi-cat::coverage`. Routes: `testdata/cat/routes/*.polyline`
(`DATA_SOURCE=real_pbf`, not great circles).

| Corridor | Talk coverage | Switches | any_talk samples |
|---|---|---|---|
| Espa→Dombås (E6) | **100.0 %** | **2** | 127 |
| Espa→Litjfossen via Skansen (Rv 3) | **93.3 %** | **3** | 134 |

Full vs reduced fixture (when `CAT_SPLAT_GEOJSON=/tmp/LA5MR.geojson`): Espa→Dombås
100.0 %/2 vs 100.0 %/2 (±2.0 tolerance). See `testdata/cat/splat/SOURCES.md`.

## Observed vs expected switch logs (H / I / demo)

### H Espa→Dombås (expected = observed)

```text
H switch @ 0.0 km → LA5MR (osm-node-1473491257)
H switch @ 133.0 km → LA6JRR (osm-way-1194875539)
```

VFO 2 fingerprint unchanged; selected VFO unchanged when background retune used.

### I Østerdalen / Rv 3 via Skansen (expected = observed)

```text
I switch @ 0.0 km → LA5MR (osm-node-1473491257)
I switch @ 90.9 km → LA5TRR (osm-node-5576656416)
I switch @ 153.9 km → LA9AR (osm-way-1005486272)
decision C after LA9AR: leave at frac≈0.979 km≈372.7 last=osm-way-1005486272
```

Decision C: hold LA9AR, notify, exit network follow (no silent fall-in).

### Demo mode (5 s / site, FakeBenshiPeer)

| Route | Steps | Cadence | Deferral | VFO 2 / selected VFO |
|---|---|---|---|---|
| Espa→Dombås | 2 | 5 s ± 0.5 s | logged (squelch), not skipped | byte-identical |
| Espa→Litjfossen | 3 | 5 s ± 0.5 s | logged (squelch), not skipped | byte-identical |

UI: off by default (`CAT_DEMO_MODE_DEFAULT_ENABLED = false`), labelled
“Demo mode (5 s/site) — not for driving”.

## Manual VR-N76 (host BlueZ)

### Step 5 — read-only reachability (2026-10-12) — PASS

| Item | Value |
|---|---|
| MAC | `38:D2:00:01:4B:E7` |
| RFCOMM | channel 1 (also answered on 4); never audio UUID |
| Identity | vendor `0x01`, productId `0x0103` (gate allowlist: `0x0002` \| `0x0103`) |
| GET_HT_STATUS | selected VFO **B** (`doubleChannel=2`); idle, squelch closed |
| VFO A (ch 0) | RX 434.6500 / TX 432.6500 MHz |
| VFO B (ch 6) | RX 145.5125 / TX 145.5125 MHz |
| Round-trips | ~11–24 ms |
| Artifacts | `testdata/cat/hardware/vr-n76/` + `STEP5_SUMMARY.md` |

Protocol fix from hardware: GET_DEV_INFO request payload `[3]`.

### Step 6 — 5 s demo runs (2026-10-12)

Scratch channel **0**; VFO B selected → scratch method; never transmit.

#### Route 1 Espa→Dombås — PASS

| t | Site | Requested RX | Read-back | Method |
|---|---|---|---|---|
| 0 s | LA5MR | 145.6250 | match (−0.6 / 88.5) | scratch_channel |
| 5 s | LA6JRR | 145.6625 | match | scratch_channel |

VFO B fingerprint identical. Log: `testdata/cat/hardware/vr-n76/STEP6_espa-dombas_*.log`.

#### Route 2 Espa→Litjfossen (Østerdalen) — PASS

| t | Site | Requested RX | Read-back | Method |
|---|---|---|---|---|
| 0 s | LA5MR | 145.6250 | match | scratch_channel |
| 5 s | LA5TRR | 145.7250 | match | scratch_channel |
| 10 s | LA9AR | 145.6000 | match | scratch_channel |

VFO B fingerprint identical. Log: `testdata/cat/hardware/vr-n76/STEP6_osterdalen_*.log`.

Operator notes:
- First runs with VFO B selected: only saw channel name **NAV** (scratch write), not RX changes (expected — display follows selected VFO).
- Operator requested select VFO A (display prep; not product auto-tune). Channel B content unchanged.
- Re-runs with VFO A selected: both routes PASS; operator confirmed RX steps visible ~every 5 s.
- Test-harness bug: earlier shell used bluetoothctl connect/disconnect between routes — fixed; `benshi-hw-demo` now holds **one** RFCOMM session for all routes.
- Leave decision: operator chose **145.625000** (LA5MR −0.6 / 88.5). Applied single-session; VFO B fingerprint unchanged. Log `STEP6_leave-la5mr_*.log`.

**Status:** simulator-verified; hardware Steps 5–6 PASS.

## Open questions answered by hardware (2026-10-12)

| Q | Finding |
|---|---|
| Q1 VFO / FREQ_MODE | With VFO B selected, scratch channel used; display follows selected VFO. Operator select-VFO-A made RX steps visible. `support_vfo` clear on this firmware → scratch primary. |
| Q3 FREQ_MODE_GET_STATUS | Replied (~11 ms); raw saved under `testdata/cat/hardware/vr-n76/`. |
| Q6 one control connection | Only one RFCOMM control link; hold session (no bluetoothctl churn). |
| productId | Live unit reports `0x0103` (259); gate allows `0x0002` \| `0x0103`. |
| GET_DEV_INFO | Request payload must be `[3]`. |

## Not tested

Hamlib on real radios; Android tablet RFCOMM to physical radio; UV-Pro / GA-5WB; live RepeaterBook; transmit/PTT/audio RFCOMM.

## Release

`versionName` **0.3.23-beta**, `versionCode` **28**, tag `v0.3.23-beta` (arm64-v8a APKs in `compiled/`).
