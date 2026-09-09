# Precomputed indexes, mirrors, and town-to-town routes

**Status:** product direction / architecture note — precomputed town-route
cache **not implemented**. Pack publish on navi-server **is** live.  
**Date:** 2026-09-09  
**Path:** `docs/precomputed-index-and-route-cache.md`

Related: [README Known issues](../README.md#known-issues),
[`indexed-map-format-plan.md`](indexed-map-format-plan.md) (measured ops-tree
space), [`pack-server-client.md`](pack-server-client.md).

---

## What Navi does today

After you download a regional OpenStreetMap extract, Navi can either:

1. **Install published packs** from navi-server (`current.json` →
   `/packs/<region_id>/<generation>/`), then still fetch a Geofabrik PBF for
   the on-device place index, or
2. **Fall back to local bake** when the region is missing / the host is down
   (Geofabrik PBF + on-device convert + place index).

Background convert on mid-range tablets is still minutes on region-scale data
when packs are not used. Pack-hit planning is typically a few seconds.

Navi’s APK does **not** ship with a ready-made national or continental routing
database. You download extracts and/or published packs per region.

---

## Measured disk: live published packs (globe catalog)

**Source:** `GET http://192.168.1.195/current.json` (same body as
`https://navigate-me.duckdns.org/current.json`), catalog generation
`20260909T014316Z-4181137-e437434e`. Sum of each region’s published `bytes`
field (pack-tree payload under DocumentRoot — **not** bake scratch /
blue-green ops tree).

| Metric | Value | Status |
|---|---|---|
| Ready regions in catalog | **540** | Measured |
| **Sum of published pack `bytes`** | **≈ 746 GiB** (**≈ 0.73 TiB**) | **Measured** (`800 687 272 113` bytes) |
| Average pack payload / region | **≈ 1.4 GiB** | Measured |

### Sample country / prefix pack payloads (same catalog)

| Prefix | Regions | Published pack bytes | Status |
|---|---|---|---|
| `north-america/us` (states / territories) | 54 | **≈ 93.5 GiB** | Measured |
| `europe/germany` | 29 | **≈ 24.0 GiB** | Measured |
| `europe/france` | 27 | **≈ 21.7 GiB** | Measured |
| `europe/united-kingdom` | 51 | **≈ 9.9 GiB** | Measured |
| `europe/sweden` (21 län) | 21 | **≈ 4.9 GiB** | Measured |
| `europe/norway` (landsdeler + Svalbard) | 6 | **≈ 4.8 GiB** | Measured |

These are **client-facing pack archives**. A bake host’s full `data/` tree
(scratch + generations + state) is larger per region; see the **measured
16-region ops tree (~337G `data/`)** and full-catalog projections in
[`indexed-map-format-plan.md`](indexed-map-format-plan.md).

**Globe headline for “how much published index to host”:** plan on the order of
**~0.75 TiB** of pack payload for today’s 540-leaf catalog, plus ops headroom
from the indexed-map plan if you bake on the same box.

---

## How many commercial units differ

Many commercial car/GPS products and head-unit navigation suites ship with
**pre-built routing databases** already on the device or on a card:

- Indexing / graph preparation happens **off-device** (vendor servers or factory
  imaging).
- The user experience is “maps are ready to route” after install or map update,
  not “wait while this tablet converts Østlandet.”
- Updates are usually whole map packages, not an on-device OSM→pack pipeline.

Navi is intentionally offline-first and open about OSM extracts. Packs from
navi-server already move most convert cost off-device when the region is
published; a full commercial-style factory image is still **not** shipped in
the APK.

---

## Direction: mirror of precomputed indexes

Preferred path (largely what pack-server clients do today):

```text
preferred:
  download published Navi packs for the region (from current.json)
  + Geofabrik PBF for place search
  plan → pack_hit on device

if server / mirror unreachable or packs missing:
  fall back to local database computing
    (background convert from the extract, or PBF plan until packs ready)
```

That keeps the planner and pack semantics on-device. The server’s job is
**compute once, distribute many times**.

**Natural fallback when the server is not reachable:** local convert from the
downloaded extract, and PBF graph build for plans until packs exist.

---

## Optional extra: precomputed town-to-town routes

Even with packs, the first plan between two distant places still runs A* (and
related stages). A further speedup — common in commercial products — is a
cache of **corridor or city-pair routes** prepared ahead of time.

### Popular OD pairs (general)

Examples: Haugesund → Bergen, Oslo → Fredrikstad, and other high-traffic town /
city pairs in a region. Hit / miss / stale rules:

| Role | Behaviour |
|---|---|
| Hit | Seed or short-circuit guidance when origin/destination snap near the cached pair (same profile / options). |
| Miss / mismatch | Full on-device plan (packs or PBF). |
| Stale map | Invalidate with pack / extract generation; never prefer a cached route over a fresher graph without a version check. |

This does **not** replace indexed packs. It is an optional layer on top.

### Highway-spine networks (US / Norway / Germany / peers)

A practical first bake set is **towns and junctions along numbered highway
systems**, not a complete graph of every place:

| Network class | Examples | Intent |
|---|---|---|
| United States | Interstate System, US Highways | City / interchange pairs along I‑* / US‑* spines |
| Norway | Europavei / riksvei (E6, E18, E39, …) | Landsdel / county hubs along the national road net |
| Germany | Autobahn (A*) and major Bundesstraßen | Stadt / Kreuz pairs on the motorway mesh |
| Other countries | Similar national motorway / trunk systems (e.g. France A*, UK motorways, Sweden E‑roads) | Same sparse-hub idea per country |

**Disk estimate assumptions** (not implemented; ~**8 KiB** per stored OD without
elev samples, ~**10 KiB** with coarse elev — same basis as
[`indexed-map-format-plan.md`](indexed-map-format-plan.md) §B). Pair counts are
**planning assumptions**, scaled loosely to how much published pack mass each
country already has in `current.json` (more road network → more corridor hubs).

| Scope | Assumed OD pairs | ≈ disk (no elev) | ≈ disk (with elev) | Status |
|---|---|---|---|---|
| Norway highway spine | ~5 000 | **~40 MiB** | **~50 MiB** | Assumption |
| Germany Autobahn / major B-roads | ~40 000 | **~0.3 GiB** | **~0.4 GiB** | Assumption |
| US Interstate / US Highway hubs | ~80 000 | **~0.6 GiB** | **~0.8 GiB** | Assumption |
| Similar systems worldwide (EU TEN‑T + peers) | ~250 000 | **~1.9 GiB** | **~2.4 GiB** | Assumption |
| **Highway-spine globe total (sum)** | **~375 000** | **~2.9 GiB** | **~3.6 GiB** | Assumption |
| Full regional town mesh (world, from indexed-map plan) | ~600 000 | **~4.6 GiB** | **~5.7 GiB** | Assumption |
| Dense world city mesh | ~2 000 000 | **~15 GiB** | **~19 GiB** | Assumption |

Two profiles (e.g. car + bicycle): multiply the chosen row by **~2**.

**Takeaway:** a globe-scale **highway-spine** town-to-town cache is small next
to the **~746 GiB** published pack catalog — single-digit GiB, not hundreds.
The expensive part remains hosting / baking the indexed packs themselves.

---

## Combined server ballpark (packs + optional route cache)

| Layer | Size | Status |
|---|---|---|
| Published pack payload (540 regions, live `current.json`) | **≈ 746 GiB** | **Measured** |
| Bake-host ops tree for a small catalog (historical 16-region `data/`) | **337G** | Measured (see indexed-map plan) |
| Highway-spine town-to-town cache (globe) | **~3–4 GiB** | Assumption |
| Regional-mesh town-to-town cache (globe) | **~5–6 GiB** | Assumption |
| Optional planet PBF hold | **~88 GiB** | Listed planet.openstreetmap.org size |

For a box that only **serves** today’s catalog: count **~0.75 TiB** pack
payload (+ web/server overhead). Adding highway town-route caches does not
materially change that headline.

---

## Summary

| Approach | Onboard convert | Offline when CDN down | In Navi today |
|---|---|---|---|
| Local extract + local pack convert | Yes (minutes on regions) | Yes | **Yes** |
| Server / mirror of precomputed Navi packs | Mostly no (download packs) | Fall back to local convert / PBF | **Yes** (navi-server publish) |
| Factory-bundled commercial routing DB | No | Yes (shipped data) | **No** |
| Precomputed town-to-town / highway corridor cache | N/A (plan shortcut) | Cache on device | **Not yet** |

Shipping precomputed packs is already the main path to commercial-like first
plan speed for published regions; optional town-to-town / highway corridor
caches remain the next layer when popular OD pairs justify the bake cost.
