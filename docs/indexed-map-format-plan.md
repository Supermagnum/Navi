# Indexed map format evaluation (phased)

**Live status doc** — update the phase table and evidence sections as work
proceeds. Investigation canvases and timing harnesses are evidence, not status.

Related:

- [`status.md`](status.md) (canonical doc map)
- [`future-proofing-audit-2026-07.md`](future-proofing-audit-2026-07.md) (tracked open item)
- README known issues (planning latency)
- Route-start vs OsmAnd/Navit investigation (canvas)
- Graph-cache audit (canvas)

| Field | Value |
|---|---|
| Opened | 2026-08-06 |
| Owner track | Routing / plan-time I/O |
| Current phase | **Phase 1 open** — 1a done; **1b untested** |
| Next | Phase **1b**: first-load PoC with a real prebuilt index (not warm `.navigph`) |
| Last updated | 2026-08-06 |

## Phase status

| Phase | Intent | Status | Go/no-go |
|---|---|---|---|
| 0 | Decision framework + targets | **Done** | N/A (defines bars) |
| 1a | Confirm ≤2 s / ≥10× is achievable when a graph artifact is already on disk | **Done** | **Met** (warm `.navigph` — see honesty note) |
| 1b | Graph-only PoC: **first** plan of a never-before-indexed bbox/region via a prebuilt spatial index | **Not done** | **Open** — this is the real Phase 1 question |
| 2 | Extend PoC to POI/barrier | Not started | Requires **Phase 1b GO** |
| 3 | Real format + migration design | Not started | Requires Phase 2 GO |
| 4 | Full implementation | Not started | Requires Phase 3 plan **approval** |

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
| **1b** (real Phase 1) | A **never-before-planned** bbox/region, loaded from a **prebuilt** index produced without that plan’s PBF scan at query time, clears ≥10× and ≤2 s vs cold PBF on the same OD | First-load of a new bbox still requires a full PBF scan (only warm cache is fast) |
| **2** | Full `plan_duration_ms` with prebuilt graph+POI+barrier ≤ 3.0 s on Espa corridor (first load of that pack), ≥10× vs cold full plan | Only graph improved; POI/barrier still multi-second on first load |
| **3** | Written format/migration plan reviewed; calendar estimate grounded in Phase 1b–2 numbers | Design cannot meet Phase 0 targets without unacceptable RAM/size on 4 GB class |
| **4** | Explicit human approval of Phase 3 plan | No automatic slide from design → implementation |

### Tracking discipline

- This file is the **live** phase status (per [`status.md`](status.md)).
- Keep the row in [`future-proofing-audit-2026-07.md`](future-proofing-audit-2026-07.md) current.
- Do **not** start Phase 2 until **Phase 1b GO** is recorded here with dated evidence.
- Phase 1–2 code may be throwaway; do not merge a production format until Phase 4.

### Relationship to other mitigations

| Work | Role |
|---|---|
| Graph `.navigph` cache | **Per-trip-bbox** derived cache written **after** a normal PBF (or prior) build of that bbox. Speeds **repeat** identical OD/bbox only. Not a region-wide spatial index; not provision-time preprocessing. |
| Shared-parse | Interim I/O consolidation; folds into **converter** internals if Phase 4 ships |
| PMTiles / DEM | Unchanged (separate data plane) |

---

## Phase 1 — Honesty split (1a vs 1b)

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

### Phase 1b — Real open question (untested)

**Claim still untested:** “Does a **prebuilt spatial / indexed** graph pack
make **first-time** loading of a **never-before-planned** bbox (or new region)
fast — i.e. avoid the cold PBF scan entirely at plan time?”

That is the OsmAnd/Navit comparison point. Warm `.navigph` does **not** answer
it: a fresh bbox still costs ~5.7 s (corridor) to ~17–27 s (Ostlandet-class)
on first visit.

**What a genuine 1b PoC must do (throwaway OK):**

1. Offline (or provision-time): build an index covering the test extract
   **without** counting that work as plan-time `graph_build_ms`.
2. Plan an OD whose trip bbox has **no** existing `.navigph` (or wipe all
   trip-bbox caches first).
3. Plan path must load from the **new** index, not call
   `build_from_pbf_bbox`.
4. Compare `graph_build_ms` (or renamed load stage) to cold PBF on the same OD.
5. GO only if ≥10× and ≤2 s on that **first** plan.

Minimal approaches for 1b (pick one when scheduled): region-wide `.navigph`
built once for the whole extract + load subset; or a throwaway `rstar`-keyed
edge pack; or a Navit-like tile set. Do not mark 1b GO on warm-cache numbers.

### Phase 1 overall decision (2026-08-06)

**Not a clean Phase 1 GO.**

| Sub-phase | Verdict |
|---|---|
| 1a warm-cache target check | **Met** — proceed knowing the ≤2 s bar is reachable when data is pre-materialized |
| 1b first-load indexed PoC | **Open** — still required before Phase 2 |
| Phase 1 as written in the original plan | **Incomplete** |

Do **not** schedule Phase 2 until 1b GO. Optional interim: shared-parse remains
available if indexed-format work stalls.

---

## Phase 2 — POI/barrier indexed PoC (blocked on 1b GO)

Pre-committed bar: full first-load `plan_duration_ms` ≤ 3.0 s with prebuilt
graph+POI+barrier on Espa corridor, ≥10× vs cold full plan.

## Phase 3 — Real format + migration (blocked on Phase 2 GO)

Versioning, provision-time generation, coexist-vs-replace raw PBF,
reprocess UX, interaction with `.navigph` / PMTiles / shared-parse, calendar
milestones from 1b–2 evidence.

## Phase 4 — Implementation

Starts only after **explicit approval** of the Phase 3 plan.

---

## Evidence log

| Date | What | Result |
|---|---|---|
| 2026-08-06 | Host corridor cold/warm `.navigph` | **1a met** (26.9×, 213 ms) — warm cache only |
| 2026-08-06 | Host Ostlandet cold/warm | Warm graph 371 ms; first-load still PBF-bound |
| 2026-08-06 | Device README baseline | Cold `graph_build_ms=17571` |
| 2026-08-06 | Status correction | Phase 1 **not** clean GO; **1b open** |
