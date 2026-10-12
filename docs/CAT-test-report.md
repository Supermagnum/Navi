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
| `cargo test -p navi-cat --tests` | PASS (Hamlib scenarios + corridor H/I) |
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
| H Espa→Dombås | PASS | `corridor_benshi::h_espa_dombas_follow_and_program` |
| I Østerdalen leave-network | PASS | Decision C after LA9AR range loss |
| J Coverage score ≥ 85 % | PASS (offline oracle run) | Espa→Dombås GC vs SPLAT talk: **91.5 %**, **2** switches (`any_talk_samples=246`). Scorer in `navi-cat::coverage` (unit-tested); full GeoJSON not committed (~187 MB). Re-run: set `CAT_SPLAT_GEOJSON` to a local extract of Supermagnum/splat `test-results/LA5MR.geojson.bz2`. |
| K Emulator H/I | PENDING | Android emulator + bridged simulator |

## Demo mode

Host planner in `navi-cat::demo` (`DEMO_STEP_SECS = 5`, labelled steps). UI wiring
and emulator play-through still pending.

## Manual VR-N76

Not run in this environment (no Bluetooth / hardware). Checklist remains in
the issue; record results separately from simulator rows above.
