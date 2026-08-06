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
| Current phase | **Phase 1 closed as NO-GO for this PoC class** — 1a met; **1b NO-GO** |
| Next | Redesign first-load path (tile/block loads without full in-memory graph rebuild); do **not** start Phase 2 on R*Tree→`RouteGraph` materialization |
| Last updated | 2026-08-07 |

## Phase status

| Phase | Intent | Status | Go/no-go |
|---|---|---|---|
| 0 | Decision framework + targets | **Done** | N/A (defines bars) |
| 1a | Confirm ≤2 s / ≥10× is achievable when a graph artifact is already on disk | **Done** | **Met** (warm `.navigph` — see honesty note) |
| 1b | Graph-only PoC: **first** plan of a never-before-indexed bbox/region via a prebuilt spatial index | **Done** | **NO-GO** — SQLite R*Tree PoC on SM-P613 (see evidence) |
| 2 | Extend PoC to POI/barrier | Not started | Blocked — needs a **different** first-load design that clears Phase 0, not this PoC |
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
does **not** clear Phase 0. OsmAnd/Navit-class first-load needs tile/block loads
**without** rebuilding the whole trip graph in RAM at plan time.

**1b result:** Phase 1’s open question is **answered** for this PoC class —
first-time region loads improve, but **not enough**. Do **not** treat this as a
GO to Phase 2; redesign the load model first (or revisit with a different
artifact).

### Phase 1 overall decision (2026-08-07)

| Sub-phase | Verdict |
|---|---|
| 1a warm-cache target check | **Met** — ≤2 s / ≥10× reachable when a trip graph is already on disk |
| 1b first-load SQLite R*Tree PoC | **NO-GO** on SM-P613 (5.6×, 2.88 s) |
| Phase 1 as a path to Phase 2 via this PoC | **Closed — do not advance** |

Optional interim: shared-parse remains available. Next indexed-format work must
target a load path that can clear ≤2 s / ≥10× on first use of a new region
without full trip-graph materialization from SQLite rows.

---

## Phase 2 — POI/barrier indexed PoC (blocked — redesign first-load first)

Pre-committed bar: full first-load `plan_duration_ms` ≤ 3.0 s with prebuilt
graph+POI+barrier on Espa corridor, ≥10× vs cold full plan. Not scheduled while
graph-only first-load still fails Phase 0 under the current PoC class.

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
| 2026-08-07 | SM-P613 hedmark first-load SQLite R*Tree PoC | Cold **16196 ms** vs indexed **2881 ms** (5.6×); **1b NO-GO** |
| 2026-08-07 | Host hedmark same PoC | Cold **5654 ms** vs indexed **2126 ms** (2.7×); confirms NO-GO |
