# Densify joint detour (Bergen→Stavanger and long corridors)

Branch: `fix/densify-detour` (from `dev` after #140). Client only.
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

### Host STEP 1 — remaining trips (pack blockers)

Host `.packs/long-trip-packs` has **vestlandet-latest** (full) and
**ostlandet-latest** (symlink to an e2e fixture). Missing Ready packs block
Raufoss→Tromsø, Oslo→Trondheim, Bergen→Trondheim, Kristiansand→Tromsø
(`trondelag-latest`, `nord-norge-latest`, `sorlandet-latest` as applicable).

Tablet SM-P613 holds all five Norway v9 landsdel packs under app
`files/long-trip-packs` — used for on-device acceptance below.

## STEP 2 — Fix: skeleton coarse path joints

Option **(a)** implemented: coarse pre-pass on a **reduced** graph
(motorway/trunk/primary + links + car ferries + pier stubs that touch ferry
nodes), place densify joints on that A* path, refine hops on the full corridor
as today.

- Filter runs during tile materialize (`with_densify_skeleton_only`).
- Flag is **process-wide `AtomicBool`** (not TLS): rayon tile workers must see
  it. TLS left workers loading the full ~259k-node Vestlandet graph and pushed
  tablet VmHWM to ~1.2 GiB.
- Slim filter (no secondary network) → ~15.6k nodes / ~26.5k edges on
  Bergen→Stavanger host (was ~259k / ~538k with the TLS bug).
- Skeleton is not corridor-cached; owned graph is dropped before densify hops.
- Corridor cache cleared between densify hops so hop graphs do not stack.
- `plan_perf` peak RSS samples **VmRSS since `begin_plan`** (not process-lifetime
  VmHWM, which re-imported prior rows into every later matrix case).
- Fallback: previous geometric/region densify if skeleton load or path fails.
- Same-stem coastal densify retained for hop memory; joints now track E39.

## Host acceptance (after fix)

| Case | densified km | single-shot km | Δ% | ferries |
| --- | ---: | ---: | ---: | --- |
| Bergen→Stav centre (lt off) | **206.88** | 206.88 | **0.0** | Halhjem\|Arsvågen |
| Bergen→Stav centre (lt on) | **206.88** | (densify) | **0.0** | same |
| Bergen→Stav station | **205.51** | 205.51 | **0.0** | same |

Skeleton joint (centre): `(59.410012, 5.453971)` on the coarse path (vs geometric
mid `59.679, 5.534`). Host peak_rss for isolated Stavanger densify ~417 MiB;
skeleton graph ~15.6k nodes.

## Tablet acceptance — SM-P613 `R52TB0JQEDE` (2026-10-03, VmRSS gate)

Build: densify branch `libnavi.so` (aarch64 release) + `installDebug`.
Test: `RegionToRegionPerfMatrixInstrumentedTest` (BUILD SUCCESSFUL).
Force-stop cold. No process LMK kill / ANR.

### Verdict summary

| Criterion | Result |
| --- | --- |
| Densified within 1% of single-shot, same ferries | **PASS** (host Δ0%; tablet Stavanger **204.88 km**, Halhjem\|Arsvågen) |
| Peak RSS matrix max ≤933 MiB | **PASS** — matrix max **876.2** MiB (`raufoss_dombas` eco cold) |
| No LMK/ANR | **PASS** |
| Non-densified dig parity (distance + geom hash) | **PASS** |
| Tromsø 4 days / 3 overnight; 1766.89 km | **PASS** — geom `e79679c2…` unchanged |
| Wall vs dig (no case >15% slower) | **PASS** — all within ~+4% |

### PROFILE_ROW (device, after VmRSS + hop-cache fix)

| Case | eco | wall_ms | distance_km | peak_rss_mb | ferry_fp | vs dig |
| --- | --- | ---: | ---: | ---: | --- | --- |
| raufoss_bergen | true | 7792 | **459.71** | 859.5 | — | match dig |
| raufoss_bergen | false | 2227 | 485.45 | 689.6 | — | (lt-off default) |
| raufoss_bergen_warm | true | 2499 | **459.71** | 693.5 | — | match dig |
| raufoss_dombas | true | 5173 | **206.81** | **876.2** | — | match dig |
| raufoss_dombas | false | 1952 | **206.81** | 708.4 | — | match dig |
| bergen_forde | true | 3994 | **171.01** | 784.9 | Lavik - Oppedal@5.72 | match dig |
| bergen_forde | false | 1409 | **171.01** | 609.1 | Lavik - Oppedal@5.72 | match dig |
| bergen_stavanger | false | 9660 | **204.88** | 660.9 | Halhjem\|Arsvågen | **fixed** (dig densify was 228.21) |
| bergen_stavanger | true | 8889 | **204.88** | 655.6 | same | same |
| bergen_stavanger_lt | false | 8910 | **204.88** | 664.6 | same | same |
| bergen_stavanger_lt | true | 8766 | **204.88** | 646.1 | same | same |
| raufoss_tromso | false | 34297 | **1766.89** | 747.9 | 5 coastal legs | match dig |

**Matrix max peak RSS: 876.2 MiB ≤ 933.**

Stale claim of **930.9 MiB** (earlier process-lifetime VmHWM contamination that
reported **952.4** on a dirty run) is superseded by this table.

## Harness

```bash
cargo run -p navi-ffi --release --bin region-to-region-perf-matrix -- \
  --pack-dir .packs/long-trip-packs

ANDROID_SERIAL=R52TB0JQEDE ./gradlew :app:installDebug :app:connectedDebugAndroidTest \
  -Pandroid.testInstrumentationRunnerArguments.class=no.navi.app.RegionToRegionPerfMatrixInstrumentedTest
```
