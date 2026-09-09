# Server disk for world-coverage indexed packs

Order-of-magnitude estimate of **how much server disk** would be needed to
host:

1. Prebuilt Navi indexed packs for every Geofabrik-style region (graph,
   poi/barrier, wetland), **with and without** elevation Δh (`edge_delta_h_m`)
2. An optional cache of **precomputed routes between major towns**

This document is a **server space estimate only**. It does not describe an
implementation plan or change runtime behaviour.

---

## What “server space” means here

| Layer | Role |
|---|---|
| **Published pack tree** | Per-region `.navi-graph-*`, `.navi-poi-barrier`, `.navi-wetland` (+ manifest) |
| **Town-to-town route cache** (optional) | Precomputed corridors between major places, versioned with the map/pack generation |
| **Source OSM extracts** | Planet and/or Geofabrik `.osm.pbf` for weekly convert |
| **Convert / bake scratch** | Temp space while rebuilding packs or route cache |
| **DEM / terrain** | Only if graph packs or route profiles include elevation — **not sized** here |

Packs and town routes are stored **per region** (or per region + cross-border
pair list), matching Tools download granularity — **not** one world blob.
Clients still download one region (plus any route-cache slice for that region)
at a time.

---

## Measured convert ratios (pack inputs)

Anchor convert (Hedmark):

| Input / output | Size | Notes |
|---|---|---|
| Source `hedmark-latest.osm.pbf` | **≈ 90 MiB** | Geofabrik / OSM.fr |
| Graph packs (all profiles) | **39 MiB** | Profile-suffixed `.navi-graph-*.rkyv` |
| POI + barrier pack | **8.1 MiB** | `.navi-poi-barrier.rkyv` |
| Wetland pack on disk | **not measured** | Rings known; archive MiB not logged |

| Pack type | Ratio | Arithmetic | Status |
|---|---|---|---|
| Graph (no Δh) | **0.433 MiB / MiB PBF** | 39 ÷ 90 | Measured |
| POI + barrier | **0.090 MiB / MiB PBF** | 8.1 ÷ 90 | Measured |
| Wetland | **0.090 MiB / MiB PBF** | same as poi-barrier | **Assumption / placeholder** |
| Graph Δh overhead | **× (8.5 ÷ 7.7) ≈ ×1.104** | trip-bbox 7.7 → 8.5 MiB with `edge_delta_h_m` | Measured on trip bbox; **assumption** it applies to region graphs |

Planet input: `planet-latest.osm.pbf` listed **88G** on
[planet.openstreetmap.org/pbf/](https://planet.openstreetmap.org/pbf/)
(file dated **2026-08-27 11:00**). Arithmetic: **≈ 88 GiB = 90 112 MiB**
(**assumption:** directory “88G” means GiB).

---

## A — Indexed packs on the server (with vs without Δh)

### Superseded estimate — see Measured below

Previously extrapolated with Hedmark ratios × 90 112 MiB planet PBF
(directory “88G” on planet.openstreetmap.org, 2026-08-27). **Do not use these
as the current server budget** — they estimated pack payloads only from a
single-region ratio × whole-planet PBF size, before a real multi-region bake
existed.

| Pack set | ≈ MiB | ≈ GiB | Status |
|---|---|---|---|
| Graph **without** Δh | 90 112 × 0.433 = **39 049** | **~38.1** | Superseded estimate |
| Graph **with** Δh | 90 112 × 0.433 × (8.5÷7.7) = **43 107** | **~42.1** | Superseded estimate |
| POI + barrier | 90 112 × 0.090 = **8 110** | **~7.9** | Superseded estimate |
| Wetland (placeholder) | 90 112 × 0.090 = **8 110** | **~7.9** | Superseded estimate |

| Variant | Formula | **Old pack-only total** | Status |
|---|---|---|---|
| **Without elevation (no Δh)** | 38.1 + 7.9 + 7.9 | **≈ 54 GiB** | Superseded estimate |
| **With elevation (Δh on graph)** | 42.1 + 7.9 + 7.9 | **≈ 58 GiB** | Superseded estimate |
| **Δh delta** | with − without | **≈ +4 GiB** | Superseded estimate |

Cross-check only (still useful per region): Hedmark ratios remain
**0.433** graph / **0.090** poi+barrier / Δh **×1.104** (see
[Measured convert ratios](#measured-convert-ratios-pack-inputs)). Hedmark-sized
(~90 MiB PBF) → ~**55 MiB** packs without Δh / ~**59 MiB** with Δh
(**estimate**; graph+poi portion measured at 47.1 MiB). Wetland line in the
old table was a **placeholder**.

### Measured

For the live navi-server **16-region catalog** bake, use the
[Measured: 16-region catalog on disk](#measured-16-region-catalog-on-disk)
section below (**337G** under `data/`, **341G** tree total). That figure is the
authoritative server disk number for packs-on-server planning today — not the
planet-ratio arithmetic above.

---

## B — Precomputed routes between major towns (optional)

Not implemented; **no measured archive size**. All figures below are
**assumptions** for server planning.

### What would be stored

Per OD pair (example content — not a format spec): profile id, origin/dest
place ids, distance, duration, encoded shape (and optionally a coarse
elevation sample along the path if the bake used Δh/DEM). Versioned against
the same pack / extract generation so stale routes are dropped.

Hit = seed or short-circuit when the user plans near that pair; miss = normal
on-device plan (packs or PBF). Does **not** replace indexed packs.

### Pair-count models (**assumptions**)

Complete graphs among all “major” places explode; practical caches are sparse.

| Model | Assumed pair count (world sum) | How it is built |
|---|---|---|
| **Sparse hub** | **~50 000** | ~5 000 majors × ~10 directed neighbors (or ~25 000 undirected) — **assumption** |
| **Regional mesh** | **~600 000** | ~200 Geofabrik-class leaves × ~80 towns × 79/2 ≈ 632 000 undirected — **assumption** |
| **Dense city mesh** | **~2 000 000** | ~2 000 world cities, undirected complete graph — **assumption** |

### Bytes per stored route (**assumptions**)

| Payload | ≈ size per OD | Notes |
|---|---|---|
| Shape + metadata, **no** elev samples | **~8 KiB** | ~500–1000 shape points compressed/quantized + ids; **assumption** (order of a mid-length corridor) |
| Same **with** coarse elev along route | **~10 KiB** | **assumption** ≈ +25% for Δh samples / climb summary |
| Extra routing profile (e.g. car + bicycle) | **×2** | If both baked; **assumption** that bicycle is stored separately |

### Route-cache disk on the server (**estimate**)

One profile, world sum:

| Pair model | Without elev on route | With elev on route | Status |
|---|---|---|---|
| Sparse hub (~50k) | 50k × 8 KiB ≈ **0.4 GiB** | 50k × 10 KiB ≈ **0.5 GiB** | Assumption |
| Regional mesh (~600k) | 600k × 8 KiB ≈ **4.6 GiB** | 600k × 10 KiB ≈ **5.7 GiB** | Assumption |
| Dense city mesh (~2M) | 2M × 8 KiB ≈ **15.3 GiB** | 2M × 10 KiB ≈ **19.1 GiB** | Assumption |

Two profiles (car + bicycle): multiply the chosen row by **~2** (**assumption**).

**Planning default used in combined totals below:** regional mesh, one profile
→ **~5 GiB** without route elev / **~6 GiB** with route elev. Labelled
**assumption**, not measured.

---


## Measured: 16-region catalog on disk

Live navi-server checkout at `/media/navi/navi-server` (host **media**, ZFS
dataset `Mypool/navi`), after a bake covering the **16-region** catalog.
`du -sh --max-depth=1` (values as reported; not re-measured here):

| Path | Size | Notes |
|---|---|---|
| `/media/navi/navi-server` | **341G** | Tree total |
| `…/data` | **337G** | Published packs + scratch + generations + state — **not broken out further**; no further breakdown is available |
| `…/copernicus` | **3.0G** | Root-level DEM source tiles; known duplicate of `data/elevation/copernicus/`; flagged for cleanup — **not** part of what a rented server must host |
| `…/target` | **1.6G** | Rust build artifacts — **not** shipped/published |
| `.git` | 11M | |
| `pack-convert-core` | 3.1M | |
| `scripts` | 538K | |
| `plugins` | 170K | |
| `systemd` | 90K | |
| `docs` | 61K | |
| `.tile_locks` | 58K | |
| `http` | 32K | |
| `navi-indexed-convert` | 14K | |
| `.github` | 7.5K | |

### Per-region average (better extrapolation base)

**337G ÷ 16 ≈ 21.1 G per region** (ops footprint amortized over the catalog:
published + scratch + generations + state). Prefer this over the single-region
Hedmark × planet-PBF ratio when projecting a larger Geofabrik catalog.

### Revised full-Geofabrik-catalog projection

Using the doc’s existing **assumption** of ~**200** Geofabrik-class leaves
(same order as the regional-mesh pair model in section B):

| Projection | Arithmetic | ≈ total |
|---|---|---|
| Full Geofabrik-class catalog at measured average | 200 × 21.1 G | **≈ 4.2 TB** `data/`-class ops footprint |

That projects the **measured ops tree shape** (not pack-payload-only). The
superseded ~54–58 GiB planet-ratio pack totals remain a lower-bound
cross-check for *archive bytes of packs alone*, not for blue-green + scratch
+ state on a working bake host.

---

## Combined server publish disk (packs ± Δh ± town routes)

**Measured (16-region catalog, live navi-server):** **337G** under `data/`
(published packs + scratch + generations + state — see
[Measured](#measured-16-region-catalog-on-disk)). Town-route cache is still
**not implemented**; rows that add town routes keep the **assumption** sizes
from section B.

| Configuration | Packs / ops tree | Town routes | **Total** | Status |
|---|---|---|---|---|
| **16-region catalog (measured `data/`)** | **337G** | — | **337G** | **Measured** |
| Same + town cache (regional mesh, no route elev) | 337G | ~5 GiB | **~342G** | Measured + assumption |
| Same + town cache (regional mesh, with route elev) | 337G | ~6 GiB | **~343G** | Measured + assumption |
| Same + dense city mesh + route elev | 337G | ~19 GiB | **~356G** | Measured + assumption |

Superseded pack-only planet-ratio rows (~54 / ~58 GiB publish) are kept only
under [A — Superseded estimate](#superseded-estimate--see-measured-below).
Sparse hub town cache still adds only **~0.5 GiB** (**assumption**) — noise
next to the measured tree.

---

## Full server footprint (publish + planet + ops)

| Component | Size | Status |
|---|---|---|
| **`data/` ops tree (16-region catalog)** | **337G** | **Measured** — published packs, scratch, generations (incl. live/previous blue-green), and state bundled; not broken out further |
| Town-route cache (regional mesh default) | **~5 / ~6 GiB** | Assumption (not implemented) |
| Source planet PBF (optional hold) | **~88 GiB** | Listed size on planet.openstreetmap.org |
| Blue-green second pack tree | *(inside 337G)* | **Measured** — `data/generations/` + `data/live` / `data/previous` live under `data/` on the real box; no longer a separate assumption-sized line |
| Scratch (convert + route bake) | *(inside 337G)* | **Measured** (bundled; no separate breakdown available) |
| Root-level `copernicus/` DEM duplicate | **3.0G** | Measured; **exclude** from “disk needed to host” (duplicate of `data/elevation/copernicus/`; reclaimable) |
| `target/` Rust build artifacts | **1.6G** | Measured; **exclude** from host budget (not published) |
| DEM used to bake Δh | — | Needed at bake time; not sized beyond the duplicate note above |

### Headline server budgets

| Scenario | Size | Status |
|---|---|---|
| **16-region catalog tree (`du` total)** | **341G** | **Measured** (`337G` data/ + `3.0G` copernicus/ + `1.6G` target/ + small repo files) |
| Host budget for packs/ops (`data/` only) | **337G** | **Measured** |
| Same + town routes (regional mesh **assumption**) | **~342–343G** | Measured + assumption |
| Same + optional planet PBF hold | **~425–431G** | Measured + listed ~88 GiB |
| Superseded pack-only planet-ratio ballpark | ~54–210 GiB | See [A — Superseded estimate](#superseded-estimate--see-measured-below) |

For a rented box that only needs to **serve** the catalog, count **337G**
(`data/`). The **341G** tree total includes reclaimable non-host paths.

---

## Fallback (unchanged)

| Missing piece | Behaviour |
|---|---|
| No valid pack for region | On-device compute from local `.osm.pbf` (today) |
| No town-route cache hit | Full on-device plan (packs or PBF) |
| Server unreachable | Same local paths — no hard dependency on the mirror |

---

## Investigation note — weekly server-side bake

**How weekly updates work today** on
[Supermagnum/navi-server](https://github.com/Supermagnum/navi-server)
(`README` + `scripts/`): `fetch-extracts.sh` does a **conditional GET** per
region (`If-None-Match` / `If-Modified-Since` from `data/state/regions/<id>/`);
**HTTP 304** skips re-download of that extract. `publish-packs.sh` publishes
**per region** with blue-green **stage → validate → atomic swap** of
`data/live` (keeping `data/previous` for rollback) and syncs into
`data/published/packs/<geofabrik-path>/<generation>/` without wiping the whole
packs tree — untouched regions stay in `current.json`. Daily
`navi-pack-scrub.timer` runs `cleanup.sh` and prunes old generations, convert
scratch, extracts, and staging. So “weekly update” means **per-region
fetch-if-changed + atomic per-region publish**, not a full wipe-and-rebuild of
every published region’s tree every week.

Open product questions (unchanged): hosting/egress cost, staleness vs OSM
edits, trust/signing, unreachable-server fallback (same as today), CPU/RAM for
convert and optional multi-OD town-route bake, which place set counts as
“major town,” and whether eco/seasonal/via plans may use a cached geometry.
Town-route bake remains optional / not measured (section B).

---

## Fits within a 500 GB rented server

The real measured **341G** tree total for the **16-region** catalog
comfortably fits inside a **500 GB** rented server, with
**500 − 341 ≈ 159 GB** of headroom for future map-data growth (more regions,
Δh rollout, town-route cache). Some of the 341G is reclaimable (**3.0G**
root-level duplicate `copernicus/` and **1.6G** `target/` build artifacts), so
usable headroom is likely somewhat higher (~**163+ GB**) after cleanup.
Host-facing ops disk is the **337G** `data/` figure.

---

## Summary — server space

| Question | Figure | Status |
|---|---|---|
| **16-region catalog `data/`** | **337G** | **Measured** |
| **16-region tree total** | **341G** | **Measured** |
| GiB-per-region average (`data/`) | **≈ 21.1 G** | **Measured** (337 ÷ 16) |
| Full Geofabrik-class catalog (~200 leaves **assumption** × 21.1 G) | **≈ 4.2 TB** | Measured average × leaf-count assumption |
| 500 GB rented box headroom (341G used) | **≈ 159 GB** | **Measured** vs 500 GB |
| Town-to-town cache (regional mesh) | **~5–6 GiB** | Assumption (not implemented) |
| Superseded pack-only planet-ratio (no Δh / with Δh) | ~54 / ~58 GiB | Superseded estimate (Hedmark × 90 112 MiB) |

Town-route sizes remain **assumed** (pair counts × ~8–10 KiB). Hedmark
graph/poi/Δh ratios stay as a per-region **cross-check**. Planet **88G**
(2026-08-27) remains a listed optional source-hold size.
