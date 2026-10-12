# CAT test report

Canonical CAT branch evidence lives in [`cat-test.md`](cat-test.md)
(dummy-`rigctld` CI, Hamlib packaging, corridor Run A/B, CSV import).

Benshi / issue #149 investigation: [`CAT-benshi-investigation.md`](CAT-benshi-investigation.md).

This filename matches issue #149's `docs/CAT-test-report.md` reference.

## Baseline (after origin/dev sync + CAT reapply)

Recorded on branch `CAT` after Step 0 / Step 2 push. Re-run after Benshi Step 3
host work:

| Check | Result |
|---|---|
| `cargo test -p navi-cat --lib` | PASS (incl. Benshi A–G unit tests) |
| `cargo test -p navi-plugin-cats --tests` | PASS (H/I/J + demo) |
| Hamlib dummy `rigctld` | PASS (`dummy_rigctld`) |
| Existing scenarios 1–12 | PASS (`scenarios.rs`) |

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

## Manual VR-N76

Not run in this environment (no Bluetooth / hardware). Checklist remains in
the issue; record results separately from simulator rows above.

**Status:** simulator-verified, hardware pending.

## Release

`versionName` **0.3.22-beta**, `versionCode` **27**, tag `v0.3.22-beta` (arm64-v8a APKs in `compiled/`).
