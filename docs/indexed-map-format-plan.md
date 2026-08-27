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

Extrapolated with Hedmark ratios × 90 112 MiB:

| Pack set | ≈ MiB | ≈ GiB |
|---|---|---|
| Graph **without** Δh | 90 112 × 0.433 = **39 049** | **~38.1** |
| Graph **with** Δh | 90 112 × 0.433 × (8.5÷7.7) = **43 107** | **~42.1** |
| POI + barrier | 90 112 × 0.090 = **8 110** | **~7.9** |
| Wetland (placeholder) | 90 112 × 0.090 = **8 110** | **~7.9** |

### Pack publish totals

| Variant | Formula | **Server disk for packs** |
|---|---|---|
| **Without elevation (no Δh)** | 38.1 + 7.9 + 7.9 | **≈ 55 269 MiB ≈ 54 GiB** |
| **With elevation (Δh on graph)** | 42.1 + 7.9 + 7.9 | **≈ 59 327 MiB ≈ 58 GiB** |
| **Δh delta** | with − without | **≈ +4 058 MiB ≈ +4 GiB** |

DEM tiles used to *build* Δh are **not** included. Wetland line is a
**placeholder**; if real wetland MiB is 0.5× or 2× this, only that line moves.

Per-region illustration: Hedmark-sized (~90 MiB PBF) → ~**55 MiB** packs without
Δh / ~**59 MiB** with Δh (**estimate**; graph+poi portion measured at 47.1 MiB).

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

## Combined server publish disk (packs ± Δh ± town routes)

| Configuration | Packs | Town routes | **Publish total** |
|---|---|---|---|
| Packs **without** Δh, **no** town cache | ~54 GiB | — | **~54 GiB** |
| Packs **with** Δh, **no** town cache | ~58 GiB | — | **~58 GiB** (~**+4 GiB** vs no Δh) |
| Packs **without** Δh + town cache (no route elev) | ~54 GiB | ~5 GiB | **~59 GiB** |
| Packs **with** Δh + town cache (with route elev) | ~58 GiB | ~6 GiB | **~64 GiB** |
| Same as row above, but dense city mesh + route elev | ~58 GiB | ~19 GiB | **~77 GiB** |

Sparse hub town cache only adds **~0.5 GiB** — noise next to packs.

---

## Full server footprint (publish + planet + ops)

| Component | Packs no Δh | Packs with Δh | Status |
|---|---|---|---|
| Published packs | **~54 GiB** | **~58 GiB** | Extrapolated |
| Town-route cache (regional mesh default) | **~5 / ~6 GiB** | same | Assumption |
| Source planet PBF | **~88 GiB** | **~88 GiB** | Listed size |
| Blue-green second pack tree | **~54 GiB** | **~58 GiB** | Ops assumption |
| Scratch (convert + route bake) | **~20–50 GiB** | **~20–50 GiB** | Assumption |
| DEM | — | **not estimated** | Needed to bake Δh |

### Headline server budgets (**estimate**)

| Scenario | Without pack Δh | With pack Δh | Δh delta |
|---|---|---|---|
| Publish packs only | **~54 GiB** | **~58 GiB** | **+4 GiB** |
| Publish packs + town routes (regional mesh) | **~59 GiB** | **~64 GiB** | **+5 GiB** (pack Δh + route elev) |
| Above + one planet PBF | **~147 GiB** | **~152 GiB** | **~+5 GiB** |
| Above + blue-green second pack tree | **~201 GiB** | **~210 GiB** | **~+9 GiB** |

Rough ballparks: **~60 GiB** publish with packs+towns, **~150 GiB** with planet
kept for weekly rebuild, **~200 GiB** with blue-green pack trees. Scratch and
DEM on top.

---

## Fallback (unchanged)

| Missing piece | Behaviour |
|---|---|
| No valid pack for region | On-device compute from local `.osm.pbf` (today) |
| No town-route cache hit | Full on-device plan (packs or PBF) |
| Server unreachable | Same local paths — no hard dependency on the mirror |

---

## Investigation note — weekly server-side bake

Not a request to implement. A weekly cron could:

1. Pull planet / Geofabrik extracts  
2. Convert per-region indexed packs (with or without Δh)  
3. Optionally bake major-town OD routes for each region (and selected
   cross-border pairs)  
4. Publish packs + route-cache slices for Tools downloads  

Disk cost is the tables above. Open questions: hosting/egress cost, weekly
staleness vs OSM edits, trust/signing, unreachable-server fallback (same as
today), CPU/RAM for convert and multi-OD bake, which place set counts as
“major town,” and whether eco/seasonal/via plans may use a cached geometry.

---

## Summary — server space

| Question | Without elevation (no Δh) | With elevation (Δh) |
|---|---|---|
| Host full-planet **packs** | **~54 GiB** | **~58 GiB** (**+4 GiB**) |
| Host packs + **town-to-town** cache (regional mesh **assumption**) | **~59 GiB** | **~64 GiB** |
| Packs + towns + planet PBF | **~147 GiB** | **~152 GiB** |
| Packs + towns + planet + blue-green packs | **~201 GiB** | **~210 GiB** |

Town-route sizes are **assumed** (pair counts × ~8–10 KiB). Wetland pack MiB is
a **placeholder**. Pack graph/poi ratios and planet **88G** (2026-08-27) are
the measured/listed anchors.
