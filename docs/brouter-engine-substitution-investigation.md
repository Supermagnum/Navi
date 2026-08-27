# BRouter as an alternate engine for car/bike pack-miss

**Status:** investigation complete (Gaps 1–2 closed on SM-P613) — no production
code. Bike-only follow-up:
[`brouter-bike-aidl-fallback-spec.md`](brouter-bike-aidl-fallback-spec.md).
Car blocked by seasonal-conditional gap.  
**Date:** 2026-08-27 (updated same day with on-device AIDL + multi-OD pass)  
**Path:** `docs/brouter-engine-substitution-investigation.md`

Prior art (closed): [`brouter-pack-miss-investigation.md`](brouter-pack-miss-investigation.md)
tested converting `.rd5` into Navi packs. That path failed. This report does
**not** re-litigate it.

**Hiking is out of scope** (wetland / overnight still apply there).

---

## Question

On a **car or bike pack-miss**, is it faster and acceptable to hand the query
to **BRouter's own engine** and return that answer, instead of Navi's full
local PBF rebuild? This is **engine substitution**, not data conversion.
Navi's pack format and `pack_hit` semantics stay untouched.

---

## What was stood up

| Approach | Tested? | Notes |
|---|---|---|
| Public BRouter HTTP API (`brouter.de`, engine **1.7.10**) | **Yes** (first pass) | Same engine family as Android app. Warm timings only; not the deployment path. |
| Local JVM `btools.server.RouteServer` + corridor `.rd5` | Prepared under `/tmp/navi-brouter-engine/` | Ruled out as on-device shape (desktop JVM). |
| Rust reimplementation of the rd5 engine | **No usable port** | `brouter-client` is HTTP-only. |
| On-device Android app + AIDL (`btools.routingapp` **1.7.9**, F-Droid code 56) | **Yes** (this pass) | Installed on SM-P613 `R52TB0JQEDE`. Throwaway client `no.navi.brouterbench` under `/tmp/navi-brouter-aidl-harness/`. Tiles `E5_N60.rd5` + `E10_N60.rd5` pushed to app media dir. |

**Profiles:** `car-eco` (car), `trekking` (bike).  
**Anchor OD (continuity):** Espa → Atnbrufossen —
`11.2561239,60.5621914` → `10.2338420,61.8512500`.

---

## Part A — Prior pass (HTTP / feasibility; not re-run)

Navi baselines (reused):

| Profile | Pack-miss | Pack-hit |
|---|---:|---:|
| Car (Espa→Atnbrufossen) | ~**54 s** | ~**2.7 s** |
| Bicycle | miss ~motor-class PBF rebuild order; pack-hit Hedmark ~**1.7 s** | |

Public HTTP warm (engine 1.7.10, same OD):

| Profile | Query 1 | Query 2 | Track length |
|---|---:|---:|---:|
| `car-eco` | ~503 ms | ~457 ms | 189 477 m |
| `trekking` | ~1412 ms | ~1386 ms | 205 324 m |

Espa→Atnbrufossen acceptability (first pass, not re-checked for Friis on this
OD): car/bike tracks looked reasonable; **0** Friis bbox samples on that OD
(gap did not fire there). Desktop JVM ruled out; Android AIDL judged realistic
by APK size / OsmAnd precedent only — **timing not measured on device in Part A**.

---

## Part B — Gap 1: On-device AIDL timing (SM-P613)

**Device:** Samsung SM-P613, serial `R52TB0JQEDE` (physical tablet, not
emulator).  
**BRouter:** `btools.routingapp` 1.7.9 (~3.14 MiB APK), first-run baseDir
`/storage/emulated/0/Android/media/btools.routingapp`, corridor `.rd5` present.  
**Harness:** binds
`ComponentName("btools.routingapp", "btools.routingapp.BRouterService")` with
action `btools.routingapp.IBRouterService`; calls
`IBRouterService.getTrackFromParams` with `profile`, `lonlats`,
`trackFormat=gpx` (JSON for trekking on this OD exceeded the ~1 MiB Binder
reply limit and caused `DeadObjectException`; GPX fixed that).  
**Cold protocol:** `am force-stop btools.routingapp` (and the harness), then
bind + one query — wall time includes bind + route.  
**Warm protocol:** same bind, second and third queries (query-only ms).  
**OD:** Espa→Atnbrufossen. Three externally cold rounds per profile.

### car-eco (all repeats)

| Round | cold total_ms | cold query_ms | cold bind_ms | warm2 query_ms | warm3 query_ms |
|---:|---:|---:|---:|---:|---:|
| 1 | 1243.814 | 1090.563 | 153.125 | 514.320 | 481.114 |
| 2 | 1260.378 | 1114.151 | 146.112 | 539.258 | 500.431 |
| 3 | 1318.552 | 1119.520 | 198.903 | 545.869 | 499.453 |

| | cold total_ms | warm query_ms (n=6) |
|---|---:|---:|
| range | **1244–1319** | **481–546** |
| median | 1260 | ~507 |

### trekking (all repeats)

| Round | cold total_ms | cold query_ms | cold bind_ms | warm2 query_ms | warm3 query_ms |
|---:|---:|---:|---:|---:|---:|
| 1 | 1665.631 | 1464.977 | 200.561 | 823.647 | 408.658 |
| 2 | 1640.310 | 1449.497 | 190.715 | 782.637 | 445.933 |
| 3 | 1600.178 | 1443.791 | 156.288 | 796.366 | 459.694 |

| | cold total_ms | warm query_ms (n=6) |
|---|---:|---:|
| range | **1600–1666** | **409–824** |
| median | 1640 | ~621 |

**One-shot smoke (first query after tile push, car-eco):** total ~4221 ms
(bind ~231, query ~3990). Process-cold repeats above are the variance set;
first mmap/JIT can be higher. Both remain ≪ ~54 s.

**Stability:** Final suite — 18/18 OK; no hangs, bind failures, or crashes.
Earlier JSON-format trekking replies failed Binder size; GPX is the workable
AIDL return format for long trekking tracks.

### vs Part A desktop/HTTP warm numbers

| Profile | Part A HTTP warm | SM-P613 AIDL warm | SM-P613 AIDL cold (bind+query) |
|---|---:|---:|---:|
| car-eco | ~0.5 s | **~0.48–0.55 s** (holds) | **~1.24–1.32 s** |
| trekking | ~1.4 s | **~0.41–0.82 s** (improves vs HTTP on this OD) | **~1.60–1.67 s** |

Cold AIDL vs ~54 s pack-miss: car ~**40×** faster; bike ~**32×** faster
(median cold totals).

Artifacts: `/tmp/navi-brouter-aidl-harness/results/aidl_bench.json`.

---

## Part B — Gap 2: Route acceptability (5 OD pairs)

BRouter via the same AIDL path. Navi via local
`ostlandet-latest.osm.pbf` bbox plans (not pack-hit). Per-OD results — not
averaged.

### Pair 1 — Espa → Atnbrufossen (original)

| | |
|---|---|
| Coords | `60.5621914,11.2561239` → `61.8512500,10.2338420` |
| Profiles | car-eco, trekking |
| BRouter | car ~189.5 km; trekking ~205.3 km (Part A distances; Gap 1 timing) |
| Navi | car pack-hit ~162.5 km (prior evidence) |
| Friis | **Not re-checked** on this OD (Part A: 0 bbox hits) |
| Verdict | **Pass** with cost-model distance caveat (~17% longer car) |

### Pair 2 — Friisvegen conditional (required car case)

| | |
|---|---|
| Coords | `61.539733,10.261215` → `61.703064,10.560902` |
| Profile | car-eco |
| BRouter | 30.702 km; **524** pts in Friis bbox; **430** within 10 m of way `361797686` — route **uses** Friisvegen |
| Calendar | 2026-08-27 (outside Nov–Jun) — summer use is seasonally valid |
| AIDL date | **No departure-date parameter** on `getTrackFromParams` — winter cannot be requested |
| Navi summer | 29.969 km; 697 Friis bbox pts; 12 conditional edges on path |
| Navi winter (2026-01-15) | **78.175 km**; **0** Friis bbox pts; **0** conditional edges — detours |
| Verdict | **Fail for car engine substitution** whenever Navi would honour a winter
  / future departure. Summer geometry matches Navi’s summer use of Friis;
  BRouter cannot reproduce Navi’s January exclusion. |

### Pair 3 — Mixed surface / tracktype bike (required)

| | |
|---|---|
| Coords | `61.225795,10.462599` → `61.358655,10.733665` |
| Profile | trekking |
| BRouter | 37.311 km; highway mix includes track 3.95 km, path 4.49 km, unclassified 13.2 km, tertiary 14.5 km; surfaces gravel 18.0 km, compacted 8.9 km; tracktype grade2/grade3 present |
| Navi bicycle | 25.160 km; path/track/unclassified heavy; end snap ~460 m |
| Verdict | **Pass** as trekking (exercises gravel/track/path). Distance ~48% longer
  than Navi — profile divergence, not an asphalt-only cheat |

### Pair 4 — Hamar → Lillehammer (general car)

| | |
|---|---|
| Coords | `60.7945,11.0679` → `61.1153,10.4662` |
| Profile | car-eco |
| BRouter | 61.280 km |
| Navi car | 57.308 km; motorway/trunk/primary/secondary mix |
| Verdict | **Pass** (~7% longer) |

### Pair 5 — Elverum → Rena (general bike)

| | |
|---|---|
| Coords | `60.8819,11.5624` → `61.1348,11.3649` |
| Profile | trekking |
| BRouter | 34.733 km |
| Navi bicycle | 33.612 km; secondary / cycleway / footway / residential |
| Verdict | **Pass** (~3% longer) |

Artifacts: `/tmp/navi-brouter-aidl-harness/results/gap2/`.

---

## Gate evaluation (this pass)

Criteria (unchanged), evaluated **per profile** from SM-P613 AIDL + Gap 2:

| # | Condition | Car | Bike |
|---|---|---|---|
| 1 | Cold-path AIDL meaningfully faster than ~54 s pack-miss | **Pass** (median ~1.26 s) | **Pass** (median ~1.64 s) |
| 2 | Route acceptability across full OD set, including conditional case | **Fail** — Friis OD: no winter/departure-date; would drive a closure Navi respects in January | **Pass** — bike ODs (1, 3, 5) acceptable; Navi bicycle already ignores `motor_vehicle:conditional` |
| 3 | AIDL stable across Gap 1 repeats (no crash/hang/bind fail) | **Pass** | **Pass** (GPX required for long replies) |

**Car: gate not cleared** (condition 2). Do not write a car product spec.  
**Bike: gate cleared.** Follow-up:
[`brouter-bike-aidl-fallback-spec.md`](brouter-bike-aidl-fallback-spec.md).

---

## Integration sketch (superseded for bike by the linked spec)

Car remains blocked. Historical sketch for reference:

```text
plan(car|bike):
  1. try Navi indexed packs → pack_hit (unchanged)
  2. pack_miss:
       if bike and BRouter available → engine=brouter (see bike spec)
       else if car → do NOT use BRouter until seasonal/date story exists
       else → existing local PBF rebuild
```

- Never set `pack_hit=true` for a BRouter answer.
- Hiking stays on Navi miss path only.
- Prefer Android AIDL service; never desktop `RouteServer` in-process.

---

## Plain conclusion

Gaps from the first pass are **closed**. On-device AIDL cold path on the
SM-P613 is **~1.2–1.7 s** for Espa→Atnbrufossen (car/bike), vs ~54 s
pack-miss — warm car matches the old ~0.5 s HTTP class; warm bike is
similar or better. Binding was stable across the measurement matrix once
long replies used GPX.

**Car engine substitution is blocked** by the Friisvegen case: BRouter will
use the conditional road when it is the short path, and AIDL exposes no
departure date, so it cannot match Navi’s winter exclusion (~30 km summer vs
~78 km winter on the test OD).

**Bike engine substitution clears the gate** for a limited external fallback
spec (timing, multi-OD sanity including mixed surface/tracktype, AIDL
stability). That spec is written separately; car stays out until a
conditional/date story exists (or product explicitly accepts the seasonal
gap).
