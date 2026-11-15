# Phase 4 — routing parity notes

## Carry-over: World release switching

- Root `World.mwm` / `WorldCoasts.mwm` + `world-release.json` must match
  `active.json` before routing (`assert_world_matches_active`).
- `activate_release` requires World overlays in staging, promotes them to the
  MWM root, and removes them from `releases/{version}/leaves/`.
- Unit tests (`mwm::release`, 3 passed): switch 261105→261112; interrupted
  staging refused; World version mismatch detected.
- Upstream note (text only): `docs/notes/navi-server-world-storage-rule.md`.

## Phase 4 closeout (Samsung R52TB0JQEDE)

Device: physical Samsung only (`adb -s R52TB0JQEDE`). Test:
`ComapsPhase4CloseoutInstrumentedTest`.

### 1) Valdresflye car — no longer INTERNAL

| Endpoint | Result |
|----------|--------|
| Suite coords `61.39,8.80 → 61.45,8.95` | **`END_NOT_ON_ROAD`** — "Destination is not on a routable road for this profile" |
| Beitostølen / Rv51 corridor `61.244,8.92 → 61.386,8.812` | **OK** (~20.9 km, ascent 680 m) |

**Cause:** CoMaps `EndPointNotFound` (finish not on a *car*-routable edge). Not a
Navi shim bug, not a missing leaf for the Rv51 corridor, and not surfaced as a
seasonal/winter closure code (CoMaps has no `RoadClosed` result — closed roads
become `RouteNotFound` / point-not-found). Foot may still snap; car does not at
the original pin.

**Fix (Navi):** `MapCode` maps every CoMaps business code to a clear Navi code;
`InternalError` → user-facing no-route (never `INTERNAL`). Failed routes skip
turns/elev export (avoids `CHECK(!m_subrouteAttrs.empty())` abort). Unit tests:
`map_code_never_leaves_business_as_internal`.

### 2) Long-route performance (Lillehammer → Drammen)

| Build | Hook | `build_s` | Mode |
|-------|------|----------:|------|
| Unpatched CoMaps SO (pin fd11cc6f, no Navi patches) | n/a | *(car cost path identical — wetland only in foot/bike estimators)* | Joints |
| Patched, hook off | off | **~11.4 s** | Joints |
| Patched, hook on | on | ~same for car (wetland unused) | Joints |

Logcat: `Navi IndexRouter mode: Joints`. Upstream
`IndexRouter::AreMwmsNear`: neighbour MWM rect overlap **or** great-circle
distance **< 300 km** → Joints A\*; else LeapsOnly. Lillehammer–Drammen ≈ 152 km
straight → Joints by design (same as CoMaps app).

**Not** a per-edge FFI regression: car does not call the wetland callback.
**Target < 3 s:** not met on this device for this corridor under Joints.
**Decision needed:** lower `kCloseMwmPointsDistanceM`, force LeapsOnly for long
car routes, or accept Joints latency.

### 3) Instruction text + voice (nb / en)

Navi generates text from enriched CoMaps turns (`comaps_instructions.rs`) —
no CoMaps Java strings. Ferry legs scan segments between turn points for
`RoutingOptions::Ferry`.

| Route | Steps | Notes |
|------:|------:|-------|
| Hamar→Elverum | 19 | Roundabouts + street/ref |
| Oslo ring | 9 | OK |
| Moss→Horten ferry | 15 | Step 7: `Ta fergen til 19` / `Take the ferry to 19` |

Spoken-prompt log on device: `out-phase4-closeout/spoken-prompts.log`.

### 4) Ascent / descent + elevation profile

`FfiComapsRoute.{ascent_m,descent_m,elevation_json}` exported. JSON shape:

```json
{"comaps_ascent_m":..., "comaps_descent_m":..., "ascent_m":..., "descent_m":...,
 "points":[{"ele_m":..., "ele_comaps_m":..., "source":"comaps"|"sidecar"}]}
```

EL02 overrides are leaf-scoped (feature ids are per-MWM) and accepted only when
within 75 m of the CoMaps altitude (guards residual index mismatches).

| Route | CoMaps ascent/descent | With overrides | sidecar pts |
|-------|----------------------:|---------------:|------------:|
| Rv7 Hallingdal | 678 / 277 | **703 / 302** | 21 |
| Rv27 | 1605 / 1491 | **1642 / 1528** | 50 |
| Lillehammer→Drammen | — | **2748 / 2941** | (profile on) |

Climb UI today reads `eco_climb_m` from the `.rkyv` plan report
(`DiagnosticLog` / e-bike status). MWM totals are on `FfiComapsRoute` for
Phase 5 plan integration.

### 5) Flaky `flag_off_blocks_fetch`

Fixed with `with_mwm_client_override` + `FLAG_TEST_LOCK`. **39** `mwm::` lib
tests pass under default cargo test threads (no `--test-threads=1`).

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

Artifacts: `test-artifacts/mwm-phase4/release-baseline/out/` (gitignored).

## Parity suite

Fixture: `core/tests/fixtures/mwm/parity-suite-ostlandet.json` (40 routes).

| Side | Where | Result |
|------|-------|--------|
| MWM | Samsung `ComapsPhase4SuiteInstrumentedTest` | **39/40 OK** (valdresflye now `END_NOT_ON_ROAD`, not INTERNAL) |
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
| Valdresflye car | END_NOT_ON_ROAD | SNAP_FAIL | Dest not on car-routable road (see closeout) |

## Navi costing on MWM

| Feature | Result |
|---------|--------|
| Wetland Soft×5 / Hard≈forbid (foot) | Confirmed Phase 3 pin + suite |
| Boardwalk exemption | Hook present; ~25% recall known |
| Bike wetland option | `comaps_session_set_bike_wetland`; default **off** |
| Elevation | Ascent/descent + profile JSON; EL02 override callback wired |
| Ferries | Tagged `is_ferry` + Navi nb/en ferry wording |

## Product options (switchable; defaults for owner)

| Option | API | Recommendation |
|--------|-----|----------------|
| Optimize | `FfiComapsOptimize::{Time, Distance, PreferNonMotorway}` | Default **Time**. `Distance` → Unsupported. Use **PreferNonMotorway** for E6 vs Rv4. |
| Bike wetland | `comaps_session_set_bike_wetland` | Default **off**. |
| Avoid flags | `comaps_session_set_avoid_options` | All off until UI exposes them. |

## Decisions needed

1. **Long-route Joints (~11 s):** accept upstream behaviour, or change mode
   selection (threshold / force LeapsOnly) for multi-MWM car routes under 300 km?
2. **Valdresflye product UX:** keep `END_NOT_ON_ROAD`, or snap to nearest
   car road / suggest Rv51 corridor?
3. **EL02 hit rate on car corridors:** investigate feature/point index mapping
   if mountain routes should show sidecar deltas in Phase 5.
