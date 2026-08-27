# Indexed pack storage at world scale

Order-of-magnitude estimate of how much disk the Navi indexed pack set would
need if every Geofabrik-style region a user can download in Tools carried
prebuilt archives (graph, POI/barrier, wetland), with and without elevation
(Δh) payload.

This document is a **space estimate only**. It does not describe an
implementation plan or change runtime behaviour.

---

## Measured convert ratios

Anchor convert (Hedmark region):

| Input / output | Size | Notes |
|---|---|---|
| Source `hedmark-latest.osm.pbf` | **≈ 90 MiB** | Geofabrik / OSM.fr extract used in indexed-convert work |
| Graph packs (all profiles) | **39 MiB** | Profile-suffixed `.navi-graph-*.rkyv` set from one convert |
| POI + barrier pack | **8.1 MiB** | `.navi-poi-barrier.rkyv` from the same convert |
| Wetland pack on disk | **not measured** | Convert work logged ring counts and load times, not archive MiB |

### Per-MiB-of-source-PBF ratios

| Pack type | Ratio | Arithmetic | Status |
|---|---|---|---|
| Graph (all profiles, no Δh) | **0.433 MiB pack / MiB PBF** | 39 ÷ 90 | Measured on Hedmark |
| POI + barrier | **0.090 MiB pack / MiB PBF** | 8.1 ÷ 90 | Measured on Hedmark |
| Graph + POI/barrier | **0.523** | (39 + 8.1) ÷ 90 | Measured on Hedmark |
| Wetland | **0.090 MiB pack / MiB PBF** | same number as poi-barrier | **Assumption / placeholder** — wetland archive MiB was never recorded; using poi-barrier as an order-of-magnitude stand-in until a real size is logged |
| Δh overhead on graph | **× (8.5 ÷ 7.7) ≈ ×1.104** | trip-bbox archives 7.7 MiB without Δh → 8.5 MiB with `edge_delta_h_m` | Measured relative overhead on a trip-bbox archive; **assumption** that the same relative overhead applies to region-scale graph packs |

Linear scaling from Hedmark density to other regions and to the full planet is
an **estimate** (urban extracts can be denser; rural ones sparser).

Illustrative cross-check (**estimate**): Ostlandet PBF is often treated as
**~450 MiB** class → predicted graph ≈ 450 × 0.433 ≈ **195 MiB**, poi-barrier ≈
450 × 0.090 ≈ **40.5 MiB**, wetland (placeholder) ≈ **40.5 MiB**.

---

## Planet source size

| Field | Value |
|---|---|
| File | `planet-latest.osm.pbf` |
| Listed size | **88G** |
| Source | [planet.openstreetmap.org/pbf/](https://planet.openstreetmap.org/pbf/) directory listing |
| Listing date | File dated **2026-08-27 11:00** (same size class as `planet-260824.osm.pbf`) |
| Unit used below | **≈ 88 GiB = 90 112 MiB** — **assumption** that the directory’s “88G” means GiB (1024³) |

Current planet dump only (not full-history).

---

## Download granularity

Navi already downloads **per region** (Geofabrik-style country / sub-region),
not one world blob. Packs follow that same unit: separate archives per category
for the region the user picks in Tools.

Full planet coverage in the tables below means the **sum** of those per-region
packs over a non-overlapping partition of the planet (about one planet-sized
PBF of source data), **not** a single combined world file.

---

## World totals (extrapolated)

Using Hedmark ratios × 90 112 MiB planet input:

| Pack set | Formula | ≈ MiB | ≈ GiB |
|---|---|---|---|
| Graph (no Δh) | 90 112 × 0.433 | **39 049** | **~38.1** |
| POI + barrier | 90 112 × 0.090 | **8 110** | **~7.9** |
| Wetland (placeholder ratio) | 90 112 × 0.090 | **8 110** | **~7.9** |
| Graph with Δh | 90 112 × 0.433 × (8.5 ÷ 7.7) | **43 107** | **~42.1** |

### Without elevation

Graph + poi/barrier + wetland (placeholder), no `edge_delta_h_m`:

| | |
|---|---|
| **Total packs** | **≈ 55 269 MiB ≈ 54 GiB** |
| Breakdown | ~38.1 GiB graph + ~7.9 GiB poi/barrier + ~7.9 GiB wetland |

Source `.osm.pbf` files remain separate (still downloaded / kept as source of
truth).

### With elevation

Same set, graph includes Δh payload (DEM needed at convert time; DEM tiles are
a separate data plane and are **not** included in these pack totals):

| | |
|---|---|
| **Total packs** | **≈ 59 327 MiB ≈ 58 GiB** |
| **Delta vs without elevation** | **≈ +4 058 MiB ≈ +4 GiB** (extrapolated Δh overhead on graph only) |

### Per-region scale (same ratios)

| Region class | Source PBF | Packs without Δh (**estimate**) | Packs with Δh (**estimate**) |
|---|---|---|---|
| Hedmark-sized | ~90 MiB | ~47 MiB graph+poi (**measured**) + ~8 MiB wetland (**assumption**) ≈ **55 MiB** | ~43 MiB graph + 8.1 + ~8 ≈ **59 MiB** |
| Ostlandet-sized | ~450 MiB | ~5× Hedmark class ≈ **~275 MiB** | ≈ **~295 MiB** |

If real wetland packs are **0.5×** or **2×** the placeholder ratio, only the
wetland line (and world totals) move by that factor; graph and poi-barrier
lines stay as measured/extrapolated above.

---

## Fallback (unchanged)

Any region **without** a locally present, valid prebuilt pack is handled the
same way Navi already does today: compute on the device from that region’s
local `.osm.pbf`. This estimate does not change that behaviour.

---

## Investigation note — weekly server-side convert

Not a request to implement. Sketch only:

A **weekly cron on a server** (not on-device) could:

1. Pull planet / Geofabrik regional extracts on a weekly cadence.
2. Run the same indexed convert pipeline used on-device to build per-region
   graph, poi-barrier, and wetland archives.
3. Stage those archives so a Tools region download can fetch them with (or
   instead of waiting on local convert for) that region’s PBF.

That would move **where** convert runs (ahead of time, on a server). It would
not by itself change the pack format or the on-device fallback above.

### Open questions (unresolved)

| Topic | Question |
|---|---|
| Hosting / bandwidth | Cost of storing and serving ~50–60 GiB of packs (Variant class above), refreshed weekly, plus per-region fan-out |
| Staleness | Gap between weekly builds and live OSM edits vs user expectations on “update region” |
| Trust | How clients should verify server-built packs (checksums, signatures, etc.) |
| Server unreachable | Must stay safe: same on-device convert / PBF plan path as today |
| Farm sizing | Large-region convert is CPU- and RAM-heavy; server capacity is separate from tablet limits |

---

## Summary

| Variant | World pack storage (**estimate**) |
|---|---|
| Without elevation | **~54 GiB** (sum of per-region packs) |
| With elevation | **~58 GiB** |
| Difference | **~+4 GiB** |

Inputs: Hedmark convert **39 + 8.1 MiB** packs from **~90 MiB** PBF; planet
**88G** (2026-08-27); wetland ratio **assumed** equal to poi-barrier until
measured.
