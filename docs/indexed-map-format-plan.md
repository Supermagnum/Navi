# Indexed map format evaluation (phased)

**Live status doc** — update the phase table and evidence sections as work
proceeds. Investigation canvases and timing harnesses are evidence, not status.

Related:

- [`status.md`](status.md) (canonical doc map)
- [`future-proofing-audit-2026-07.md`](future-proofing-audit-2026-07.md) (tracked open item)
- README known issues (planning latency)
- [`brouter-pack-miss-investigation.md`](brouter-pack-miss-investigation.md) (Phase 1: official `.rd5` is not a Navi pack-hit path; Phase 2 spec not written)
- Route-start vs OsmAnd/Navit investigation (canvas)
- Graph-cache audit (canvas)

| Field | Value |
|---|---|
| Opened | 2026-08-06 |
| Owner track | Routing / plan-time I/O |
| Current phase | **Phase 4 + 4b complete** — graph/POI/barrier + wetland packs; `.navigph` deprecated |
| Next | Tools UI Ostlandet download soak; optional hiking overnight/POI pack fold-in |
| Last updated | 2026-08-07 |

## Phase status

| Phase | Intent | Status | Go/no-go |
|---|---|---|---|
| 0 | Decision framework + targets | **Done** | N/A (defines bars) |
| 1a | Confirm ≤2 s / ≥10× is achievable when a graph artifact is already on disk | **Done** | **Met** (warm `.navigph` — see honesty note) |
| 1b | Graph-only PoC: **first** plan via prebuilt spatial index that still **materializes** a trip `RouteGraph` | **Done** | **NO-GO** — SQLite R*Tree on SM-P613 |
| 1c | Graph-only PoC: **first** plan via `rkyv`+`memmap2` **zero-copy** (no owned `RouteGraph`) ± precomputed Δh | **Done** | **GO** — SM-P613 hedmark (see evidence) |
| 2 | Extend PoC to POI/barrier under the same load model | **Done** | **GO** — SM-P613 hedmark (see evidence) |
| 3 | Real format + migration design | **Done (design)** | **Approved** 2026-08-07 |
| 4 | Full implementation | **Done** | M0–M6 on SM-P613 (see Phase 4) |
| **4b** | Wetland indexed pack | **Done** | **GO** — SM-P613 (see Phase 4b) |

---

## Phase 0 — Decision framework

### What “meaningfully closed” means

Reference hardware: **4 GB-class** tablet (SM-P613 / Automotive baseline in
README). Reference corridor: **Espa→Atnbrufossen** (existing fixture / known
diagnostic OD).

| Metric | Target (testable) | Why this bar |
|---|---|---|
| **Time to A\*** (`graph_build_ms`) on **first use** of a trip/region when an indexed pack was built **offline ahead of time** (provision / convert), **not** after a prior plan of that bbox | **≤ 2.0 s** on reference hardware for the Espa corridor OD | Today cold `graph_build_ms` is ~15–27 s; ≤2 s is a clear product win without requiring “as fast as OsmAnd” |
| **Speedup vs cold PBF `graph_build_ms`** on that **first** plan | **≥ 10×** | Order-of-magnitude, not incremental (shared-parse class) |
| Full first plan with POI/barrier (Phase 2+) | **≤ 3.0 s** wall `plan_duration_ms` when **graph+POI+barrier** indexes were prebuilt offline | Closes remaining PBF scan cost |

“As fast as OsmAnd” is **not** the Phase 0 bar.

### Pre-committed go/no-go bars

| Phase | GO if | NO-GO if |
|---|---|---|
| **1a** (sanity) | Warm load of an **already-built** `.navigph` shows ≥10× vs cold PBF and ≤2 s `graph_build_ms` on the small corridor | Even repeat visits cannot clear the bar (would kill the approach) |
| **1b** (materializing index) | A **never-before-planned** bbox/region, loaded from a **prebuilt** index produced without that plan’s PBF scan at query time, clears ≥10× and ≤2 s vs cold PBF on the same OD | First-load of a new bbox still requires a full PBF scan (only warm cache is fast) **or** indexed load still fails the bars |
| **1c** (zero-copy) | Same first-load discipline as 1b, but graph is accessed **without** materializing an owned `RouteGraph` (mmap + archived layout), and clears the same ≤2 s / ≥10× bars | Zero-copy still fails absolute or speedup bars on SM-P613 |
| **2** | Full `plan_duration_ms` with prebuilt graph+POI+barrier ≤ 3.0 s on Espa corridor (first load of that pack), ≥10× vs cold full plan | Only graph improved; POI/barrier still multi-second on first load |
| **3** | Written format/migration plan reviewed; calendar estimate grounded in Phase 1–2 numbers | Design cannot meet Phase 0 targets without unacceptable RAM/size on 4 GB class |
| **4** | Explicit human approval of Phase 3 plan | No automatic slide from design → implementation |

### Tracking discipline

- This file is the **live** phase status (per [`status.md`](status.md)).
- Keep the row in [`future-proofing-audit-2026-07.md`](future-proofing-audit-2026-07.md) current.
- Do **not** start Phase 2 until a graph first-load path records **GO** here (now **1c**).
- Phase 1–2 code may be throwaway; do not merge a production format until Phase 4.

### Relationship to other mitigations

| Work | Role |
|---|---|
| Graph `.navigph` cache | **Per-trip-bbox** derived cache written **after** a normal PBF (or prior) build of that bbox. Speeds **repeat** identical OD/bbox only. Not a region-wide spatial index; not provision-time preprocessing. |
| Shared-parse | Interim I/O consolidation; folds into **converter** internals if Phase 4 ships |
| PMTiles / DEM | Unchanged (separate data plane) |

---

## Phase 1 — Honesty split (1a / 1b / 1c)

### What `.navigph` actually is

`.navigph` (`NAVIGPH7` + bincode via `load_or_build_reweighted_bbox`) is the
**existing graph cache** from the cache-hit audit:

- Built by scanning `.osm.pbf` (or loading a prior cache) for a **specific
  padded trip bbox**, then eco-reweighting and saving.
- Keyed by extract stem + profile + bbox rounded to 2 decimals.
- A **new** OD / new bbox still pays the **full cold PBF `graph_build`** once
  for that key.
- It is **not** OsmAnd/Navit-style preprocess-once spatial indexing for any
  never-visited corridor inside a downloaded region.

### Phase 1a — Warm-cache bar check (done; useful, not the full PoC)

**Claim tested:** “If a graph artifact is already on disk, can load+plan clear
the ≤2 s / ≥10× numbers?”

**Method:** Reused the existing `.navigph` warm path (same mechanism as the
cache-hit audit). **No new indexed-format prototype was built.**

**Host evidence (2026-08-06)** — Espa→Atnbrufossen, `espa-atnbrufossen-corridor.osm.pbf`:

| Pass | cache_hit | graph_build_ms | poi_barrier_ms | plan_duration_ms |
|---|---|---|---|---|
| Cold (empty cache) | false | **5729** | 3294 | 9652 |
| Warm (existing `.navigph`) | true | **213** | 3335 | 4097 |

- Speedup: **≈26.9×**; warm absolute **0.213 s** — Phase 0 numeric bar **achievable in principle**.
- This is the **same class of evidence** as the earlier cache-hit audit, not a
  new format PoC.

**Ostlandet scale check (same OD):** warm `graph_build_ms=371` after cold
`27077`; `poi_barrier` still ~24 s.

**Device:** README cold baseline `graph_build_ms=17571` (SM-P613); device
already stores multiple distinct trip-bbox `.navigph` files under
`graph-cache-ostlandet-latest.osm-car`.

**1a result:** **Met** — target numbers are not fantasy for “artifact already
on disk.”

### Phase 1b — First-load SQLite R*Tree PoC (done; NO-GO)

**Claim tested:** “Does a **prebuilt spatial / indexed** graph pack make
**first-time** loading of a **never-before-planned** region fast enough to clear
Phase 0 (≤2.0 s and ≥10× vs cold PBF)?”

**Method (throwaway tool):** `navi-ffi` binary `indexed-rtree-poc`
(`navi-ffi/src/bin/indexed_rtree_poc.rs`). Offline `build` parses a region PBF
once into SQLite nodes/edges + R*Tree; plan-time `bench` loads a trip bbox from
that DB into an in-memory `RouteGraph` (no `.navigph`, no cold PBF scan on the
indexed path).

**Region discipline:** Geofabrik/OSM.fr **`hedmark-latest.osm.pbf`** (~90 MiB).
Confirmed on SM-P613: app files had **no** `*hedmark*` cache / `.navigph`; only
`ostlandet-latest` trip-bbox caches. PoC ran under
`/data/local/tmp/navi_rtree_poc/` (not the app graph-cache). Test OD: Esso
Myklegård → Atnbrufossen.

**SM-P613 evidence (2026-08-07, authoritative for 1b):**

| Path | Time |
|---|---|
| Cold `build_from_pbf_bbox` | **16196 ms** |
| Prebuilt SQLite R*Tree → `RouteGraph` load | **2881 ms** |
| Speedup | **5.6×** |
| ≤2.0 s absolute | **Fail** (2.88 s) |
| ≥10× | **Fail** |
| **PHASE1B** | **NO-GO** |

Host sanity (same OD/DB): cold **5654 ms**, indexed **2126 ms**, **2.7×** — also
NO-GO. Routes agree (~162.5 km).

**Interpretation:** R*Tree query still **materializes ~90k nodes / ~195k edges**
into a full in-memory `RouteGraph`. That helps vs raw PBF (~5–6× on device) but
does **not** clear Phase 0. The missing piece was eliminating that materialization
step — tested next as Phase 1c.

**1b result:** **NO-GO** for materializing indexed loads.

### Phase 1c — rkyv + memmap2 zero-copy PoC (done; GO)

**Claim tested:** “Does eliminating owned-graph materialization via `rkyv`
(archived layout) + `memmap2` (mapped bytes) clear the Phase 0 bars that 1b
missed — and does a shared per-edge elevation Δh close the separate eco
reweight bottleneck?”

**Method (throwaway tool):** `navi-ffi` binary `rkyv-mmap-graph-poc`
(`navi-ffi/src/bin/rkyv_mmap_graph_poc.rs`).

- Offline `build`: parse trip-bbox graph from region PBF (preprocess cost),
  serialize flat CSR (`nodes` / `edges` / adjacency) with `rkyv`.
- Variant A: plain weights (no Δh payload).
- Variant B: same layout + `edge_delta_h_m` (metres; not energy).
- Plan-time `bench`: `memmap2` map the archive, `rkyv::access` the bytes, A\*
  **directly on archived fields** (no `RouteGraph`).

Same cold discipline as 1b: Hedmark extract, Esso Myklegård → Atnbrufossen,
no prior hedmark `.navigph`; PoC under `/data/local/tmp/navi_rkyv_poc/`.

**SM-P613 evidence (2026-08-07, authoritative for 1c):**

| Metric | Variant A | Variant B |
|---|---|---|
| Cold `build_from_pbf_bbox` | **16395 ms** | **16525 ms** |
| Indexed load (`mmap`+`rkyv::access`) | **2.9 ms** | **2.3 ms** |
| Touch-all edges (full page-in upper bound) | 12.6 ms | 19.5 ms |
| First A\* (length weights) | 123 ms | 126 ms |
| Archive size | 7.7 MiB | 8.5 MiB (+~0.8 MiB Δh) |
| Speedup vs cold (load) | **~5585×** | **~7123×** |
| ≤2.0 s absolute | **Pass** | **Pass** |
| ≥10× | **Pass** | **Pass** |
| **PHASE1C** | **GO** | **GO** |

Route distance matches 1b (~162.5 km; eco A\* ~162.8 km / 522 nodes).

**Variant B eco (same device run):**

| Step | Time |
|---|---|
| DEM `apply_eco_reweighting` (trip-bbox graph, Car) | **145.7 ms** |
| Δh→energy arithmetic over all archived edges (Car) | **1.69 ms** |
| Same arithmetic (Motorcycle, shared Δh index) | **1.74 ms** |
| Speedup arith vs DEM reweight | **~86×** |
| Eco A\* with live Δh→energy | **67 ms** |

Car vs Motorcycle energy sums from the **same** Δh array differ as expected
(distinct mass/Cd), confirming one shared elevation-delta index serves multiple
profiles without per-profile reprocessing.

Host sanity (same OD): load ~0.05 ms, first plan ~18 ms, DEM reweight ~38 ms vs
Δh arith ~0.87 ms — same GO class.

**Interpretation:** Zero-copy closes the 1b gap. Adding Δh does **not**
meaningfully change load time (still single-digit ms); it moves eco reweight from
DEM sampling to cheap arithmetic. Live Δh→energy in A\* stays cheap on device.

**rkyv / memmap2 constraints (flagged for Phase 3):**

| Topic | Finding |
|---|---|
| **mmap safety** | `Mmap::map` is `unsafe` w.r.t. file mutation: the mapped file must not be truncated/replaced underfoot. Region download/update must use write-temp + atomic rename **and** never mutate a file while any planner holds a map. Safe if pack lifetime is immutable-after-publish (same discipline as PMTiles). |
| **Schema evolution** | Archived `rkyv` layouts are brittle across struct changes. PoC uses magic `NVRK` + `variant` byte; production needs an explicit format version and a rebuild-on-mismatch path (do not attempt ad-hoc field migration of mapped bytes). |
| **Android** | Confirmed working on SM-P613 under `/data/local/tmp` (arm64 PIE, NDK linker). App-private files dirs should behave the same with read permission; keep packs on local filesystem (not content-provider streams). |

**1c result:** **GO** — first-load graph path clears Phase 0 without materializing
`RouteGraph`. Phase 2 may proceed on this load model.

### Phase 1 overall decision (2026-08-07)

| Sub-phase | Verdict |
|---|---|
| 1a warm-cache target check | **Met** — ≤2 s / ≥10× reachable when a trip graph is already on disk |
| 1b first-load SQLite R*Tree (materialize) | **NO-GO** on SM-P613 (5.6×, 2.88 s) |
| 1c first-load rkyv+memmap2 (zero-copy) ± Δh | **GO** on SM-P613 (~2–3 ms load, ≥5000×) |
| Phase 1 as a path to Phase 2 | **Open via 1c** — do not revive 1b materialization |

---

## Phase 2 — POI/barrier rkyv+memmap2 PoC (done; GO)

**Claim tested:** “Does the same `rkyv`+`memmap2` approach close the remaining
`poi_barrier_ms` cost (untouched by graph cache / 1c), and does
graph+POI/barrier together clear Phase 0’s full-plan bar (≤3.0 s, ≥10×)?”

**Method (throwaway tool):** `navi-ffi` binary `rkyv-mmap-poi-barrier-poc`
(`navi-ffi/src/bin/rkyv_mmap_poi_barrier_poc.rs`). Offline `build` extracts
trip-bbox POIs + danger barrier segments (highway-from-graph + PBF
railway/river/cliff/glacier) into a flat archived pack; `bench` compares cold
`PoiIndex::load_from_pbf_bbox` + `DangerBarrierIndex` vs mmap+access+materialize
of owned records (honest “ready for break planning”).

Same Hedmark / Esso Myklegård → Atnbrufossen discipline as 1b/1c; PoC under
`/data/local/tmp/navi_rkyv_poc/`.

**SM-P613 evidence (2026-08-07, authoritative for Phase 2):**

| Path | Time |
|---|---|
| Cold POI PBF load | **3982 ms** (841 POIs) |
| Cold barrier (from_graph + PBF) | **8287 ms** (~117k segs) |
| Cold `poi_barrier` combined | **12269 ms** |
| Indexed mmap + materialize | **1.8 ms** |
| Speedup vs cold `poi_barrier` | **~6710×** |
| Archive size | 3.7 MiB |

**Full-plan estimate (1c graph + Phase 2 POI/barrier on device):**

| Path | Time |
|---|---|
| Cold graph + cold poi_barrier | **~28.7 s** |
| Indexed: graph mmap (~0.04 ms) + 1c first-plan proxy (~150 ms) + POI/barrier load (1.8 ms) | **~152 ms** |
| Speedup | **~189×** |
| ≤3.0 s absolute | **Pass** |
| ≥10× | **Pass** |
| **PHASE2** | **GO** |

Host sanity: cold `poi_barrier` **4676 ms** → indexed **0.74 ms** (~6288×);
full-plan estimate ~151 ms vs ~10.6 s cold (~71×) — same GO class.

**Interpretation:** POI/barrier has a **different shape** than the routing graph
(far fewer POI records; barrier cost is a large segment list from a second PBF
pass), but under the same archive+mmap approach it improves by the same
order of magnitude as Variant A. No separate indexing strategy is required for
Phase 0 clearance. Note: plan-time still materializes owned `PoiRecord`s from
mapped bytes today (tags needed for break logic); that copy is cheap at this
scale (~ms). Production may later query tags zero-copy if needed — not required
to clear the bar.

**2 result:** **GO** — Phase 3 design may proceed.

---

## Phase 3 — Real format + migration design (ready for approval)

Status: **design complete**. Phase 4 must **not** start until this section is
**explicitly approved**.

### 3.1 On-disk format

#### Schema versioning

Every archive file begins with a fixed preamble (not free-form `rkyv` alone):

| Field | Type | Role |
|---|---|---|
| `magic` | `u32` | Category id (`NVRK` graph, `NVPB` poi/barrier, …) |
| `format_version` | `u32` | Monotonic schema version for that magic |
| `payload` | `rkyv` bytes | Archived body for **exactly** that version |

On load:

1. Read magic + version from the file header (plain bytes, before `rkyv::access`).
2. If magic unknown or `format_version` ≠ code’s supported version → **do not
   map/interpret payload**. Treat as missing pack.
3. Rebuild from on-disk source `.osm.pbf` (+ DEM for Δh) via the converter;
   never attempt partial / best-effort reads of a mismatched layout.

Bump `format_version` on any archived struct field change. Old packs are
invalidated and regenerated (acceptable: preprocess is offline / provision-time).

#### Contents and file layout — **separate archives per category**

| File (per region stem) | Contents | Regenerated when |
|---|---|---|
| `{stem}.navi-graph.rkyv` | Routing CSR + optional `edge_delta_h_m` (Variant B) | OSM extract changes; DEM coverage changes (if Δh present) |
| `{stem}.navi-poi-barrier.rkyv` | POIs + barrier segs + glacier rings | OSM extract changes |
| (unchanged) `{stem}.osm.pbf` | Source of truth | Download / Geofabrik update |
| (unchanged) place `place_index.db` | FTS search | Existing ensure-place-index step |
| (unchanged) PMTiles / DEM | Basemap / terrain | Separate pipelines |

**Why separate (not one combined blob):**

- OSM update without DEM redo can refresh graph topology + POI/barrier without
  re-sampling every edge Δh (or can refresh Δh alone when DEM tiles change).
- Smaller blast radius on version bumps (only one category’s version gate fires).
- Simpler pause/cancel: each file is an independent publish unit.

A thin `{stem}.navi-manifest.json` (plain JSON) lists expected filenames,
`format_version`s, source PBF fingerprint (size + mtime or content hash), and
optional DEM fingerprint for Δh — used only for presence/version checks, not
for routing.

#### Generation trigger — **transparent, inside existing Tools flow**

Extend **Tools → “Download region + build place index”** (and the equivalent
update path) to also run the archive converter after the PBF is durable on
disk. No separate user-facing “build indexed maps” step unless generation
fails (then surface error + retry in the existing download-progress UI).

Rationale: matches README “downloads happen through inbuilt tools”; users
already wait on place-index build; adding graph+POI/barrier preprocess there
is the natural one-time cost.

### 3.2 Immutability / write-safety

Generalize the Phase 1c invariant and the existing HTTP download pattern
(`stream_get_to_file` → `*.partial` → `rename`):

1. Write `{name}.rkyv.partial` (or under a job-private temp dir).
2. fsync file (and parent dir where practical).
3. Atomic `rename` to the final `{name}.rkyv` only after a successful full
   serialize + header write.
4. **Never** open a `.partial` for `mmap` / planning.
5. **Pause / resume / cancel** (same control channel as DEM/region jobs):
   - **Cancel or failed job:** delete `.partial`; leave any previous good
     `.rkyv` untouched.
   - **Pause:** stop writing; keep `.partial` for resume of **generation**
     only if the generator is checkpointable; otherwise delete `.partial` and
     restart generation on resume (simpler, recommended for v1 — generation is
     CPU-bound from local PBF, not a multi-GB network fetch).
   - **Resume after process death:** on next open, ignore/delete orphan
     `.partial`; if final `.rkyv` missing/wrong version → fallback path (§3.3).
6. Planner holds `Mmap` only for the duration of a plan (or an explicit
   region session). Region replace must wait until no live maps of that file
   (or use inode replace: rename-over means existing `Mmap` keeps old inode —
   acceptable if plans finish against the old snapshot).

### 3.3 Migration for already-downloaded regions

| Detection | Response |
|---|---|
| Manifest / archive missing | Fall back to today’s raw PBF `graph_build` + `poi_barrier` path for that plan |
| Magic/version mismatch | Same fallback; mark pack stale |
| PBF fingerprint mismatch vs manifest | Stale → regenerate |

**Regeneration:** prefer **local** `{stem}.osm.pbf` already on disk — **do not**
re-download solely to rebuild archives. Offer “Rebuild indexed maps” via the
**existing** Tools UI patterns. Convert progress uses the dedicated **Convert**
progress channel (`convert_progress_*`); the UI plan bar uses **Plan**
(`plan_progress_*`) so the two never clobber each other. Optional: auto-queue
rebuild after app update that bumps `format_version`, when the device is idle /
charging (nice-to-have; not required for v1).

Until rebuild completes, routing remains correct (slow path). No hard block.

### 3.4 Interaction with existing systems

| System | Decision |
|---|---|
| **`.navigph` graph cache** | **Recommend deprecate and remove** after the mmap pack load path ships. Phase 1c load (~2–3 ms) already beats warm `.navigph` (~213 ms host / similar device class). Keeping both adds keying, invalidation, and disk clutter for no win. Keep a short dual-read window only if needed for A/B; then delete write+read of `.navigph`. |
| **PMTiles / DEM basemap** | **Unaffected.** Separate visual/terrain plane. DEM tiles remain inputs to **offline Δh generation** only. |
| **Shared-parse mitigation** | **Fold into the converter**, not a parallel project. One provision-time pass (or tightly sequenced passes) over the PBF produces graph + POI/barrier (+ place index remains its own FTS build). Drops the interim “shared-parse at plan time” scope. |
| **Place index FTS** | Remains; not replaced by `navi-poi-barrier` (search ≠ break-POI spatial pack). |

### 3.5 Implementation milestones (Phase 4 — for approval)

Estimates assume **one** engineer familiar with this codebase; calendar weeks
include device regression. Throwaway PoC bins stay until M3 replaces them.

| # | Milestone | Effort | Exit criteria | Main risks |
|---|---|---|---|---|
| **M0** | Freeze header + manifest + version constants from this doc | **2–3 days** | Spec in-repo (`docs/` + Rust consts); mismatch→rebuild test | Over-designing multi-version readers (must not) |
| **M1** | Converter CLI/library: PBF(+DEM) → `.navi-graph` + `.navi-poi-barrier` with temp→rename | **1–1.5 weeks** | Host+device generate Hedmark/Ostlandet packs; cancel deletes `.partial` | Ostlandet wall time / thermal throttling on tablet; DEM gaps for Δh |
| **M2** | Wire converter into region download + place-index Tools flow + update path | **3–5 days** | Fresh region download produces packs; progress UI shows convert stage | Job control edge cases (kill mid-convert); storage full |
| **M3** | Plan load path: `plan_car_route` / hiking / truck use packs when valid; else PBF fallback | **1–1.5 weeks** | Hedmark OD first plan ≤3 s wall on SM-P613 with packs; fallback still works | API surface still assumes owned `RouteGraph`/`PoiIndex` — adapter layer; eco Δh wiring |
| **M4** | Migration: detect missing/stale; rebuild-from-local-PBF affordance in Tools | **3–5 days** | Old Ostlandet install without packs plans via fallback; rebuild produces packs without re-fetch | User confusion if rebuild is slow on large extracts |
| **M5** | Deprecate `.navigph`: stop writing; remove read path; clear docs | **3–5 days** | No new `.navigph`; disk cleanup optional; no perf regression | Hidden callers / tests still expecting cache_hit |
| **M6** | Full regression: six motor profiles + Hiking shared-path standard; SM-P613 spot checks | **1 week** | Route-level suite green; README cold/warm numbers updated | Profile-specific POI radii / HOS still scanning something unexpected |

**Total calendar estimate:** ~**6–9 weeks** to production-ready behind the
existing Tools flow, assuming no major Ostlandet-scale converter redesign.

**Non-goals for Phase 4 v1:** multi-version payload migration; shipping packs
from a server CDN (device-side convert from Geofabrik PBF is enough);
replacing place-index FTS; zero-copy POI tag queries (materialize stays OK).

### Phase 3 approval gate

Phase 4 starts only after an explicit “approve Phase 3 plan” decision (this
document’s §3). Until then: PoC tools remain investigation-only; no production
format merge.

---

## Phase 4 — Implementation (complete; plus 4b)

Phase 3 plan **approved** 2026-08-07. Status by milestone:

| # | Status | Evidence |
|---|---|---|
| **M0** | **Done** | `core/src/routing/indexed/` — 8-byte preamble (`magic`+`format_version`), manifest JSON, atomic `.partial`→rename. Unit tests: preamble roundtrip, atomic write, version gate. |
| **M1** | **Done** | `navi-indexed-convert` + `convert_region_packs`. SM-P613 Hedmark: convert **41.5 s** → 39 MiB graph + 8.1 MiB poi-barrier + manifest. Load+bbox materialize **~370 ms** graph / **~134 ms** poi; version mismatch → `VersionMismatch` (no payload interpret). Host convert ~15.4 s; load ~142 / 62 ms. |
| **M2** | **Done (SM-P613)** | `ensure_indexed_maps` / `indexed_maps_status` UniFFI; Tools download starts **non-blocking** background convert (`IndexedMapsBackground`) after place index — region usable immediately via bbox/PBF fallback; Tools shows passive status. Ostlandet full tiled convert ~10–11 min on SM-P613; see **Known limitation** below. |
| **M3** | **Done (SM-P613)** | Hedmark Esso Myklegård→Atnbrufossen via `navi-indexed-plan-bench`: **pack-hit** wall **1844 ms**, `build_s=0.54`, `pack_hit=true`/`poi_pack_hit=true`, 162.51 km; **genuine cold missing-pack** (wiped `.navigph`) wall **31159 ms**, `build_s=17.73`, both pack flags false, same 162.51 km. |
| **M4** | **Done (SM-P613)** | Rebuild-from-local-PBF in Tools (background); status via manifest; v2→v3 Ostlandet tiled regen without Geofabrik re-fetch; Friisvegen seasonal pack-hit verified on region tiles. |
| **M5** | **Done (SM-P613)** | `load_or_build_reweighted*` neither reads nor writes `.navigph`. M5 binary cold fallback: wall **30668 ms**, `cache_hit=false`, **cache dir empty** (no new `.navigph`); pack-hit still **1662 ms** / 162.51 km. Corridor instrumented test retargeted to packs. |
| **M6** | **Done (SM-P613 spot)** | All six motor profiles + Hiking pack-hit on Hedmark (see evidence). Car/Moto/MH/Truck ~1.5 s / 162.5 km; Bicycle/e-bike ~1.7 s / 160.7 km. Hiking wetland residual closed in **Phase 4b**. |

### Design refinements discovered in implementation (not silent)

1. **Graph filename is profile-suffixed** (`{stem}.navi-graph-car.rkyv`, `-truck`, `-foot`, `-bicycle`) listed in the manifest. Required because `RoutingProfile` filters differ; singular `{stem}.navi-graph.rkyv` could not serve Car+Truck+Foot+Bicycle.
2. **Converter uses `build_from_pbf_bbox` over the extract’s node extents**, not `osm4routing` full-file read — full read panics (`Missing node`) on Hedmark and is the path already documented as OOM-unsafe for Ostlandet.
3. **Plan-time materialize clips the region pack to the trip bbox** when building `RouteGraph`. Storing a region-wide pack matches Phase 3; loading the entire Ostlandet graph into RAM does not (existing `bbox_build.rs` comment). Zero-copy A\* over the map remains a future optimization; bbox materialize already clears Phase 0 on Hedmark (~0.5 s combined vs ~28 s cold).
4. **Wetland is a separate archive** (`{stem}.navi-wetland.rkyv` or tiled
   `{stem}.navi-wetland.t{r}_{c}.rkyv`, magic `NVWL`) — independent of POI/barrier;
   boardwalk carve-out remains on graph edges (`is_boardwalk_crossing`), not in
   the wetland pack. Region-scale converts emit **per-tile** wetland packs (shared
   way/coord extract, one tile materialized at a time) so 4GB-class devices avoid
   a full-region ring index. Monolith corridors still write a single wetland file.
   `PackStatus::Ready` requires wetland present (tiled or monolith) and POI/barrier
   **v2** (overnight building centroids for hiking corridor filter).
5. **Large regions use spatial graph tiles** (~1° cells, `graph_tiles` in the manifest) with way-first multi-tile PBF passes (not 3×N tiles×N profiles). POI/barrier extracted once after all profile graphs. Truck aliases car tiles.

### Known limitation — region-scale convert memory (open)

**Region-scale pack conversion has thin memory margin on lower-end 4GB-class
hardware.** Full wording and product status live in
[`../README.md` Known issues](../README.md#known-issues). Summary of the
measured fact (SM-P613, ~3.5GB RAM, MainActivity foreground, Østlandet
background tiled convert): completed with no crash / no LMK kill of the app;
lowest system `MemAvailable` ~**329 MiB**; ~**250 MiB** extra swap used;
`TRIM_MEMORY_RUNNING_CRITICAL` observed on other processes during the POI
phase. Survived pressure, not a comfortable margin — further mitigation not
yet implemented. Region **download and immediate use** are unaffected (PBF
fallback); risk is specific to background pack conversion.

### Module / tools

| Path | Role |
|---|---|
| `core/src/routing/indexed/` | Format, convert, load (graph / poi-barrier / wetland) |
| `navi-ffi` `ensure_indexed_maps` / `indexed_maps_status` | Tools + migration |
| Graph pack **v2** edge shape CSR (`edge_shape_offsets` + lon/lat) | Map overlay follows OSM curves on `pack_hit` (v1 was junction chords only) |
| Graph pack **v3** raw `motor_vehicle:conditional` / `access:conditional` / `maxspeed:conditional` strings | Plan-time seasonal closure eval (not a convert-time bool). **v1 limitation:** multi-day trips that cross a season boundary evaluate only the planned departure instant. |
| `navi-indexed-convert` | Offline converter CLI |
| `navi-indexed-bench` / `navi-wetland-bench` | Load + wetland PBF vs pack timing |
| `navi-indexed-plan-bench` | End-to-end plan pack-hit / fallback |

---

## Phase 4b — Wetland indexed pack (done; GO)

**Residual after M6:** Hiking graph pack-hit was fast (`build_s` ~0.3 s) but
`WetlandIndex::load_from_pbf_bbox` still did a multi-pass raw PBF scan —
same root cause as `graph_build` / `poi_barrier`.

### Baseline reconfirm (SM-P613, Hedmark long trip bbox)

| Path | Time | Rings |
|---|---|---|
| PBF wetland load (`navi-wetland-bench`) | **18616 ms** | 47271 |
| Indexed pack load (same bbox) | **93.5 ms** | 47271 |
| Speedup | **~199×** | identical class sample 64/64 |

Host sanity: PBF **7240 ms** → pack **65 ms** (~111×). Confirms raw-PBF-scan
shape before building the production path.

### Format / wiring

- New `{stem}.navi-wetland.rkyv` (`MAGIC_WETLAND` / `NVWL`, `format_version=1`)
- Manifest fields `wetland_file` + `wetland_format_version` (optional → PBF fallback)
- Converter emits wetland after POI/barrier; `ensure_indexed_maps` rebuilds if wetland missing/stale version
- `plan_hiking_route` prefers pack (`wetland_pack_hit=true`) else PBF
- Ring AABB culling in `WetlandIndex::class_at` (apply-time; correctness-preserving)

### Device plan evidence

| Case | Wall | Flags | Notes |
|---|---|---|---|
| Long hiking OD (previously aborted ~6 min) | **60978 ms** | `pack_hit=true`, `wetland_pack_hit=true` | Completes; 174.6 km |
| Short hiking + wetland pack | **13233 ms** | both pack hits | Was ~53 s pre-4b |
| Short hiking wetland-only fallback (graph pack, no wetland file) | **31594 ms** | `pack_hit=true`, `wetland_pack_hit=false` | Same 1.76 km / soft=212 |

### Boardwalk / apply identity

- Apply counters identical pack vs PBF wetlands on same foot graph
  (`soft=92`, `hard=0`, `boardwalk_kept=0` on Atnbrufossen-area bbox test).
- Boardwalk carve-out remains edge-tag based (`bridge=boardwalk` /
  `surface=wood` → `is_boardwalk_crossing` in the **graph** pack); wetland pack
  only stores Soft/Hard rings. Tag helpers + hard-over-soft precedence unit-tested.
- No in-repo Bråstein/Figgjo PBF fixture; identity of Soft/Hard + boardwalk
  counters is the regression gate used here.

**4b result:** **GO**

---

## Evidence log

| Date | What | Result |
|---|---|---|
| 2026-08-06 | Host corridor cold/warm `.navigph` | **1a met** (26.9×, 213 ms) — warm cache only |
| 2026-08-06 | Host Ostlandet cold/warm | Warm graph 371 ms; first-load still PBF-bound |
| 2026-08-06 | Device README baseline | Cold `graph_build_ms=17571` |
| 2026-08-06 | Status correction | Phase 1 **not** clean GO; **1b open** |
| 2026-08-07 | SM-P613 hedmark first-load SQLite R*Tree PoC | Cold **16196 ms** vs indexed **2881 ms** (5.6×); **1b NO-GO** |
| 2026-08-07 | Host hedmark same PoC | Cold **5654 ms** vs indexed **2126 ms** (2.7×); confirms NO-GO |
| 2026-08-07 | SM-P613 hedmark rkyv+memmap2 PoC (A/B) | Cold ~16.4 s vs mmap load **~2–3 ms** (≥5000×); Δh arith **~1.7 ms** vs DEM reweight **~146 ms**; **1c GO** |
| 2026-08-07 | Host hedmark rkyv+memmap2 sanity | Same GO class; A 7.7 MiB / B 8.5 MiB |
| 2026-08-07 | SM-P613 hedmark POI/barrier rkyv+mmap | Cold `poi_barrier` **12269 ms** vs indexed **1.8 ms** (~6710×); full-plan est **~152 ms** vs **~28.7 s** (~189×); **Phase 2 GO** |
| 2026-08-07 | Phase 3 design written | Ready for approval; Phase 4 not started |
| 2026-08-07 | Phase 3 approved → Phase 4 start | M0+M1 on SM-P613; design refinements recorded |
| 2026-08-07 | SM-P613 M3 pack-hit plan | Wall **1844 ms**, `build_s=0.54`, `pack_hit`+`poi_pack_hit`, 162.51 km |
| 2026-08-07 | SM-P613 M3 cold missing-pack fallback | Wall **31159 ms**, `build_s=17.73`, packs false, same 162.51 km (`.navigph` wiped) |
| 2026-08-07 | SM-P613 M5 no-write confirm | Cold fallback **30668 ms**; **no** `.navigph` created; pack-hit **1662 ms** |
| 2026-08-07 | SM-P613 M6 multi-profile | Car/Moto/MH/Truck ~1.5 s pack-hit; Bike/e-bike ~1.7 s; Hiking short pack graph 0.29 s (wetland residual ~53 s wall) |
| 2026-08-07 | SM-P613 Phase 4b wetland PBF vs pack | **18616 ms** → **93.5 ms** (~199×); long hike completes **61 s** with `wetland_pack_hit`; **4b GO** |
| 2026-08-10 | SM-P613 Ostlandet tiled v3 convert (UI fg) | ~657 s; 60 tiles; min `MemAvailable` **~329 MiB**; swap +~250 MiB; TRIM CRITICAL observed — **open memory margin** (README Known issues) |
| 2026-08-10 | SM-P613 Ostlandet pack-hit Friisvegen | summer `pack_hit=true` seasonal=0; winter `pack_hit=true` seasonal=36 |
| 2026-08-10 | SM-P613 tiled wetland + overnight buildings | Convert `peak_rss_mb=1737.4`, `wetland_rings=366222`, 20 wetland tiles; short Atnbrufossen hike **159477→3105 ms** (`wetland_pack_hit=true`, `overnight_buildings_pack_hit=true`) |
| 2026-08-24 | Pixel 9a pack-miss plan vs convert/cone | Foreground-plan yield for convert/place-index; cone/road-near bbox **skip** during plan; separate Plan/Convert/Cone progress channels; release Ostlandet Oslo OD ~**151 s** with GPS cone spam (was ~12–13 min) |
| 2026-08-27 | BRouter `.rd5` pack-miss speedup (Phase 1) | Official `segments4` cover Ostlandet and include elevation, but are **not** Navi packs (no wetland/overnight/conditionals). **Gate not cleared**; no Phase 2 spec. See [`brouter-pack-miss-investigation.md`](brouter-pack-miss-investigation.md). |
