# Phase 4 — routing parity notes

## Carry-over: World release switching

- Root `World.mwm` / `WorldCoasts.mwm` + `world-release.json` must match
  `active.json` before routing (`assert_world_matches_active`).
- `activate_release` requires World overlays in staging, promotes them to the
  MWM root, and removes them from `releases/{version}/leaves/`.
- Unit tests (`mwm::release`, 3 passed): switch 261105→261112; interrupted
  staging refused; World version mismatch detected.
- Upstream note (text only): `docs/notes/navi-server-world-storage-rule.md`.

## Carry-over: release-build performance (Samsung R52TB0JQEDE)

Unsigned release APK **~46 MiB** (`app-release-unsigned.apk`); debug ~52 MiB
with UniFFI surface.

| Metric | Release | Debug (Phase 3) |
|--------|--------:|----------------:|
| Cold init | ~64 ms | ~59 ms |
| Idle PSS (Østlandet) | ~119 MiB | ~158 MiB |
| Hamar→Elverum build | ~0.41 s | ~0.40 s |
| Elverum→Lillehammer | ~1.7 s | ~1.6 s |
| Lillehammer→Drammen | **~11.0 s** | ~11.6 s |
| Foot wetland | ~0.15 / 0.25 s | similar |

**Lillehammer→Drammen > 3 s:** CoMaps IndexRouter Joints A\* on a ~225 km
multi-MWM car route (~10.5 s inside `Route build time`). Not missing leaves,
not debug asserts (release native). **Navi-side packaging OK — no fix.**

Artifacts: `test-artifacts/mwm-phase4/release-baseline/out/` (gitignored).

## Parity suite

Fixture: `core/tests/fixtures/mwm/parity-suite-ostlandet.json` (40 routes).

| Side | Where | Result |
|------|-------|--------|
| MWM | Samsung `ComapsPhase4SuiteInstrumentedTest` | **39/40 OK** (`valdresflye_car` INTERNAL — coords outside loaded leaves) |
| `.rkyv` | Host `mwm_parity_rkyv_host` (`NAVI_PACK_DIR=navi-gate/packs-fu38-nogav`) | **29/40 OK** (no bike tiles; some snap/NO_ROUTE) |

### Length-close baselines (good parity)

| Route | MWM m | .rkyv m |
|-------|------:|--------:|
| Hamar→Elverum | 31364 | 31376 |
| Elverum→Lillehammer | 87860 | 87450 |
| Moss→Horten ferry | 12836 | 11950 |
| Oslo turn restrict | 2123 | 2082 |

### Significant length diffs (>10%) — causes

| Route | MWM m | .rkyv m | Cause |
|-------|------:|--------:|-------|
| Lillehammer→Drammen (Time) | 225176 | 190204 | **Fastest vs Navi default:** MWM Time ≈ E6/E18; `.rkyv` ≈ PreferNonMotorway (192001 m on MWM) |
| Lillehammer→Drammen PreferNonMotorway | 192001 | 190204 | Aligned (product switch works) |
| Gjøvik→Oslo | 172189 | 120333 | Data/costing — MWM keeps longer motorway-ish corridor |
| Oslo oneway grid | 4141 | 1074 | Urban access / one-way handling difference (data) |
| Rv4 toll Gjøvik | 13392 | 10745 | Toll/access costing difference |
| Private road bypass | 5792 | 4856 | Access restriction handling |
| Hedmark wetland 3 | 11924 | 9412 | Wetland Soft/Hard on MWM foot hook |
| Gravel hedmark / foot path | +10–14% | | Surface costing difference |
| Bike routes | OK | LOAD_FAIL | No `.rkyv` bike tiles in host pack dir |
| Foot wetland pin | 16245 | SNAP_FAIL | Host `.rkyv` snap miss (not a MWM bug) |
| Valdresflye car | INTERNAL | SNAP_FAIL | Outside Østlandet leaf coverage |

Overlap % is a coarse point-sample metric; length-similar routes with low
overlap are often parallel corridors (E6 vs Rv4) — see GeoJSON pairs.

### GeoJSON diff pairs (gitignored)

Listed in `test-artifacts/mwm-phase4/parity/geojson-diff-pair-paths.txt`.
Directory: `test-artifacts/mwm-phase4/parity/geojson-diff-pairs/`.

Full summaries: `parity-mwm-summary.json`, `parity-rkyv-summary.json`,
`parity-comparison.json`.

## Navi costing on MWM

| Feature | Result |
|---------|--------|
| Wetland Soft×5 / Hard≈forbid (foot) | Confirmed Phase 3 pin + suite; extra hedmark routes mixed (some no Soft/Hard on path) |
| Boardwalk exemption | Hook present; `boardwalk_proxy_hedmark` delta 0 (no Soft/Hard on path or proxy miss; ~25% recall known) |
| Bike wetland option | `comaps_session_set_bike_wetland`; default **off** (recommendation). Same endpoints showed no length delta when toggled on this corridor |
| Elevation sidecar | Rv7 / Rv27 routed OK; length/time unchanged with hook — **ascent/descent not yet exported** on `FfiComapsRoute` (gap). Valdresflye car outside leaves |
| Ferries | CoMaps takes ferry (Moss→Horten; ~10.5 km ferry step in turns). `.rkyv` also OK. Navi avoid-ferry policy not applied unless `set_avoid_options(ferry=true)` |

## Missing `.rkyv` features on MWM (estimates)

| Feature | Status | Effort |
|---------|--------|--------|
| Truck / mobile-home | Missing | L |
| Avoid toll/motorway/ferry/dirty | Wired (`set_avoid_options`) | S (UI) |
| Eco / DATEX / danger overlays | Missing | L |
| Ferry policy parity with Navi defaults | Gap | M |
| True shortest (distance weight) | Unsupported in RoutesBuilder | Blocked upstream |
| Elevation ascent/descent + profile API | Partial (hook only) | M |
| Boardwalk field recall | ~25% | follow-up |
| Turn instructions → Navi TTS/`maneuvers_json` | Street names OK; **textual instruction empty**; no lane→Navi mapping yet | M |
| Bike packs on host parity | N/A (MWM OK) | — |

Implemented for suite: wetland hook, PreferNonMotorway switch, bike wetland
flag, turns JSON export, avoid options ABI.

## Turn-by-turn (device, locale `nb`)

| Route | Turns | Sample streets | Notes |
|-------|------:|----------------|-------|
| Hamar→Elverum | 19 | Vangsvegen, Chausseen | OK |
| Oslo ring | 9 | Ullevålsveien, Colletts gate | OK |
| Moss→Horten ferry | 15 | Dronningens gate; ~10.5 km blank-street step = ferry | Ferry not labelled in JSON |

**Gaps:** `instruction` field empty (CoMaps TTS/`GetTurnNotification` without
string tables in headless); no roundabout exit→Navi voice mapping yet; ferry
leg not tagged as ferry in turns JSON; lane guidance not exported.

## Product options (switchable; defaults for owner)

| Option | API | Recommendation |
|--------|-----|----------------|
| Optimize | `FfiComapsOptimize::{Time, Distance, PreferNonMotorway}` | Default **Time**. `Distance` → Unsupported. Use **PreferNonMotorway** for E6 vs Rv4 (Lillehammer→Drammen: 225 km / 9697 s vs 192 km / 11379 s). |
| Bike wetland | `comaps_session_set_bike_wetland` | Default **off**. |
| Avoid flags | `comaps_session_set_avoid_options` | All off until UI exposes them. |

## Known unrelated issue

`mwm::download::tests::flag_off_blocks_fetch` can flake under parallel
`cargo test` because `set_mwm_client_enabled_override` is process-global
(other tests set `Some(true)`). Passes with `--test-threads=1`. Not fixed in
Phase 4.
