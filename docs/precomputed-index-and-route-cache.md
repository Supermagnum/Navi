# Precomputed indexes, mirrors, and town-to-town routes

**Status:** product direction / architecture note — not implemented.  
**Date:** 2026-08-27  
**Path:** `docs/precomputed-index-and-route-cache.md`

Related live status: [README Known issues](../README.md#known-issues),
[`indexed-map-format-plan.md`](indexed-map-format-plan.md).

---

## What Navi does today

After you download a regional OpenStreetMap extract, Navi **builds indexed
routing packs on the device** (graph tiles, POI/barrier, wetland, overnight).
That background convert is much faster than it was, but on region-scale data
it is still measured in minutes on mid-range tablets (Østlandet on SM-P613:
about **14.8 → 10.6 min**). Until packs exist, long-distance plans fall back
to scanning the raw `.osm.pbf` (tens of seconds to minutes). Once packs are
ready, pack-hit planning is typically a few seconds.

Navi’s APK does **not** ship with a ready-made national or continental routing
database. You download extracts and (today) convert them locally.

---

## How many commercial units differ

Many commercial car/GPS products and head-unit navigation suites ship with
**pre-built routing databases** already on the device or on a card:

- Indexing / graph preparation happens **off-device** (vendor servers or factory
  imaging).
- The user experience is “maps are ready to route” after install or map update,
  not “wait while this tablet converts Østlandet.”
- Updates are usually whole map packages, not an on-device OSM→pack pipeline.

Navi is intentionally offline-first and open about OSM extracts, but in its
**present state** it does **not** include that class of precomputed vendor
database. Local convert + optional PBF fallback is the current product shape.

---

## Direction: a server (example `navi.app`) with precomputed indexes

A natural way to remove most onboard convert cost is a **mirror or CDN** that
serves the **same indexed pack format** Navi already uses after convert
(graph / POI / wetland / overnight tiles — not a proprietary second planner):

```text
preferred:
  download region basemap + OSM extract
       and/or download precomputed Navi packs for that region
  plan → pack_hit on device

if server / mirror unreachable or packs missing:
  fall back to local database computing
    (background convert from the extract, or PBF plan until packs ready)
```

That keeps the planner and pack semantics on-device. The server’s job is
**compute once, distribute many times** — the same work Tools does today in
the background, done on infrastructure instead of every tablet.

Naming such as `navi.app` is an example only; any HTTPS mirror of signed pack
artifacts would fit. Detail of signing, catalog layout, and billing is out of
scope here; see also the pack-miss / engine notes in
[`brouter-pack-miss-investigation.md`](brouter-pack-miss-investigation.md) and
[`brouter-engine-substitution-investigation.md`](brouter-engine-substitution-investigation.md)
for adjacent fallback ideas (not substitutes for Navi packs).

**Natural fallback when the server is not reachable:** exactly what Navi does
now — local convert from the downloaded extract, and PBF graph build for plans
until packs exist. Offline must keep working without the mirror.

---

## Optional extra: precomputed town-to-town routes

Even with packs, the first plan between two distant places still runs A* (and
related stages). A further speedup — common in commercial products — is a
cache of **corridor or city-pair routes** prepared ahead of time, for example:

- Haugesund → Bergen  
- Oslo → Fredrikstad  
- other high-traffic town / city pairs in a region  

Used carefully:

| Role | Behaviour |
|---|---|
| Hit | Seed or short-circuit guidance when origin/destination snap near the cached pair (same profile / options). |
| Miss / mismatch | Full on-device plan (packs or PBF). |
| Stale map | Invalidate with pack / extract version; never prefer a cached route over a fresher graph without a version check. |

This does **not** replace indexed packs. It is an optional layer on top for
popular OD pairs. It also does not remove the need for seasonal / eco /
via-point logic when the cached answer would skip those rules.

---

## Summary

| Approach | Onboard convert | Offline when CDN down | In Navi today |
|---|---|---|---|
| Local extract + local pack convert | Yes (minutes on regions) | Yes | **Yes** |
| Server / mirror of precomputed Navi packs | Mostly no (download packs) | Fall back to local convert / PBF | **Not yet** |
| Factory-bundled commercial routing DB | No | Yes (shipped data) | **No** |
| Precomputed town-to-town route cache | N/A (plan shortcut) | Cache on device | **Not yet** |

Shipping precomputed packs (and optionally town-to-town corridors) is the
clearest path to “commercial-like” first-plan speed without giving up
offline-first behaviour when the network is gone.
