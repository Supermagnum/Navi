# Densify joint detour (Bergen→Stavanger and long corridors)

Branch: `fix/densify-detour` (from `dev` after #140). Client only. No merge.
PR: https://github.com/Supermagnum/Navi/pull/141

## Problem

Geometric densify joints (chord midpoints snapped within 35 km) forced routes
off the best corridor. Bergen→Stavanger centre: single-shot **206.88 km** vs
densified **230.20 km** (~+11%). Joint was the chord mid
`(59.679072, 5.533872)` — not on E39. Dig tablet densify matched **228.21 km**
for the same reason. Real E39-class distance is ~207–210 km.

Same-stem coastal densify (restored in #140) remains required so tablet plans
never hold the ~259k-node Vestlandet single-shot owned graph (~985 MiB VmHWM).

## STEP 1 — Host measure (before fix)

Pack: `.packs/long-trip-packs` (vestlandet Ready; ostlandet symlink only).
Host harness: `directed-snap-diag` / `region-to-region-perf-matrix`
(stub PBF disables overlay; pack ferries used).

| Case | mode | distance_km | ETA min | ferries | Δ vs single-shot |
| --- | --- | ---: | ---: | --- | ---: |
| Bergen→Stav centre | single-shot (`NAVI_FORCE_SINGLE_SHOT=1`) | **206.88** | 181.4 | Halhjem@21.32\|Arsvågen@9.15 | — |
| Bergen→Stav centre | densify (geometric mid) | **230.20** | 209.3 | same pair (split across hops) | **+23.3 km / +11.3%** |
| Bergen→Stav station | single-shot | **205.51** | 179.6 | same | — |
| Bergen→Stav station | densify (geometric mid) | **228.83** | 207.6 | same | **+23.3 km / +11.4%** |

Diverge: hop joint at chord mid west of E39; leg1/leg2 stitch through coastal
roads instead of the single-shot Halhjem→Arsvågen corridor.

### Host STEP 1 — remaining trips (pack blockers)

Host `.packs/long-trip-packs` has **vestlandet-latest** (full) and
**ostlandet-latest** (symlink to an e2e fixture). Missing Ready packs block:

| Trip | Blocked by missing pack stem(s) |
| --- | --- |
| Raufoss→Tromsø | `trondelag-latest`, `nord-norge-latest` |
| Bergen long-trip on (multi-stem beyond Vestlandet) | N/A for Bergen→Stav (vestlandet only; measured above) |
| Oslo→Trondheim | `trondelag-latest` (and full ostlandet if symlink insufficient) |
| Bergen→Trondheim | `trondelag-latest` |
| Kristiansand→Tromsø | `sorlandet-latest` (or equiv.), `trondelag-latest`, `nord-norge-latest` |

Tablet SM-P613 holds all five Norway v9 landsdel packs under app
`files/long-trip-packs` — used for on-device acceptance below. Do not adb-pull
(~7 GiB) into host for this PR; re-run host STEP 1 when those stems are staged.

## STEP 2 — Fix: skeleton coarse path joints

Option **(a)** implemented: coarse pre-pass on a **reduced** graph
(motorway/trunk/primary/secondary + links + car ferries), place densify joints
on that A* path, refine hops on the full corridor as today.

- Filter runs during tile materialize (`with_densify_skeleton_only`) so peak
  RSS stays under a full corridor.
- Fallback: previous geometric/region densify if skeleton load or path fails.
- Same-stem coastal densify retained for memory; joints now track the best route.

## STEP 3 — Workaround

Kept same-stem coastal densify for tablet RSS. With skeleton joints, densified
distance matches single-shot within measurement noise on host (see below) — no
need to delete the densify gate.

## Host acceptance (after fix)

| Case | densified km | single-shot km | Δ% | ferries |
| --- | ---: | ---: | ---: | --- |
| Bergen→Stav centre (lt off) | **206.88** | 206.88 | **0.0** | Halhjem\|Arsvågen |
| Bergen→Stav centre (lt on) | **206.88** | (densify) | **0.0** | same |
| Bergen→Stav station | **205.51** | 205.51 | **0.0** | same |

Wall (host cold-ish, directed-snap-diag): centre densify ~4.1 s / lt ~5.2 s /
station ~4.4 s (includes skeleton pre-pass). Prior geometric densify was ~2.3–3.0 s.

## Tablet acceptance — SM-P613 `R52TB0JQEDE` (2026-10-03)

Build: densify branch `libnavi.so` (aarch64 release) + `installDebug`.
Test: `RegionToRegionPerfMatrixInstrumentedTest` (BUILD SUCCESSFUL, 143.8 s,
failures=0). No process LMK kill / ANR (only post-test `am force-stop`).

### Verdict summary

| Criterion | Result |
| --- | --- |
| Densified within 1% of single-shot, same ferries | **PASS** (host Δ0%; tablet Stavanger densify lt on+off identical 204.88 km, same fp) |
| Bergen→Stavanger centre ~207 km lt on and off | **PASS** — **204.88 km** both; ferries Halhjem\|Arsvågen (vs dig densify 228.21; vs host centre 206.88 → **−0.97%**) |
| Tromsø 4 days / 3 overnight; distance vs 1766.89 | **PASS** — **1766.89 km**, days=4, 3 lodging overnights (Treetop Ekne, Korgenfjellet Fjellstue, Bardu Hotel); geom `e79679c2…` unchanged |
| Peak RSS matrix max ≤933 MiB | **FAIL** — matrix max **1261.7** MiB (jumps at first `bergen_stavanger` densify+skeleton; pre-Stavanger cases ≤922.1) |
| No LMK/ANR | **PASS** |
| Non-densified dig parity (distance + geom hash) | **PASS** — see table |
| Wall vs dig current | Mixed — see wall table (most faster; Stavanger ~20 s densify+skeleton) |

### PROFILE_ROW (device)

| Case | eco | wall_ms | distance_km | peak_rss_mb | ferry_fp | geom_sha256 (prefix) | vs dig |
| --- | --- | ---: | ---: | ---: | --- | --- | --- |
| raufoss_bergen | true | 8079 | **459.71** | 906.5 | — | `8a8c6f7b…` | match dig |
| raufoss_bergen | false | 2248 | 485.45 | 906.5 | — | `540bb2ad…` | (lt-off default) |
| raufoss_bergen_warm | true | 2629 | **459.71** | 906.5 | — | `8a8c6f7b…` | match dig |
| raufoss_dombas | true | 5484 | **206.81** | 922.1 | — | `6a1360bc…` | match dig |
| raufoss_dombas | false | 2042 | **206.81** | 922.1 | — | `6a1360bc…` | match dig |
| bergen_forde | true | 3901 | **171.01** | 922.1 | Lavik - Oppedal@5.72 | `0bff0c85…` | match dig |
| bergen_forde | false | 1418 | **171.01** | 922.1 | Lavik - Oppedal@5.72 | `0bff0c85…` | match dig |
| bergen_stavanger | false | 20353 | **204.88** | 1195.2 | Halhjem\|Arsvågen | `9b66426c…` | **fixed** (dig was 228.21 / `1cf463d5…`) |
| bergen_stavanger | true | 20451 | **204.88** | **1261.7** | same | `9b66426c…` | same |
| bergen_stavanger_lt | false | 18966 | **204.88** | 1261.7 | same | `9b66426c…` | same |
| bergen_stavanger_lt | true | 18889 | **204.88** | 1261.7 | same | `9b66426c…` | same |
| raufoss_tromso | false | 35969 | **1766.89** | 1261.7 | 5 coastal legs | `e79679c2…` | match dig |

Notes:

- Same-stem coastal densify still forces hops=2 on Bergen→Stavanger even with
  `longTrip=false` (lt on and off share geom/distance/ferries).
- `peak_rss_mb` is process VmHWM; once Stavanger densify+skeleton peaks at
  1195–1261, later rows inherit that HWM.
- Dig SM-P613 matrix walls (docs/perf/region-to-region-graph-build.md, ~954 MiB
  peak era): bergen eco cold 15809→**8079**; warm 2514→**2629**; tromso
  50250→**35969**. Stavanger densify wall is new cost (~19–20 s) vs prior
  geometric densify path.

### RSS follow-up (open)

Skeleton pre-pass on Vestlandet coastal OD raises VmHWM above the ≤933 MiB
budget (and above dig densify ~954 MiB). Distance/ferry acceptance is met; RSS
needs a separate pass (drop skeleton owned graph sooner, or share filtered
tiles with hop loads).

## Harness

```bash
cargo run -p navi-ffi --release --bin region-to-region-perf-matrix -- \
  --pack-dir .packs/long-trip-packs

ANDROID_SERIAL=R52TB0JQEDE ./gradlew :app:installDebug :app:connectedDebugAndroidTest \
  -Pandroid.testInstrumentationRunnerArguments.class=no.navi.app.RegionToRegionPerfMatrixInstrumentedTest
```
