# BRouter data for pack-miss speedup (Phase 1 investigation)

**Status:** investigation only — no production code, no Phase 2 spec.  
**Date:** 2026-08-27  
**Path:** `docs/brouter-pack-miss-investigation.md`

This is Part 2 Phase 1 of the pack-miss / BRouter question. Phase 2 (a
fetch-pipeline spec) is gated on these results and was **not** written.

Related live status: [`indexed-map-format-plan.md`](indexed-map-format-plan.md),
README known issues (pack-hit vs pack-miss planning latency).

---

## Question

A missing or stale indexed pack forces a local PBF rebuild (pack-miss),
measured at roughly **20–40×** slower than pack-hit (car ~54 s miss vs ~2.7 s
hit; hiking ~104 s miss vs ~25 s hit, release binaries, Ostlandet ODs). BRouter
publishes weekly planet `.rd5` tiles at
<https://brouter.de/brouter/segments4/>. Does fetching those tiles on pack-miss
produce a measurable speedup **for Navi's car / bicycle / hiking profiles**, or
is that only plausible on paper?

---

## Constraints honoured

- No production pipeline, app, or planner changes.
- BRouter was **not** wired into Navi's routing engine.
- Official `segments4` tiles (elevation baked in at weekly build) — not a
  no-elevation rebuild.
- Throwaway inspect script only: `/tmp/navi-brouter-probe/` (not in this repo).

---

## Coverage

BRouter tiles are 5°×5°, named from the south-west corner.

Ostlandet / Hedmark reference ODs:

| OD | Coordinates | Tiles |
|---|---|---|
| Espa → Atnbrufossen (car / bicycle baseline) | 60.5621914, 11.2561239 → 61.8512500, 10.2338420 | `E10_N60` (route bbox also reaches `E5_N60`, lon 9.95) |
| Skolla → Rondvassbu (hiking baseline) | 61.2430347, 10.8170385 → 61.8787483, 9.7963376 | `E10_N60` + `E5_N60` |
| Full Ostlandet extract (lon ≳ 7.5, includes Oslo ~59.91, 10.75) | region | `E5_N55`, `E10_N55`, `E5_N60`, `E10_N60` |

Official listing **2026-08-27 01:03** (weekly planet build, `segments4/`):

| Tile | Bytes | Covers |
|---|---:|---|
| `E5_N60.rd5` | 25 954 532 | lon 5–10, lat 60–65 (Rondane west, Rondvassbu) |
| `E10_N60.rd5` | 28 875 046 | lon 10–15, lat 60–65 (Espa, Skolla, Atnbrufossen) |
| `E5_N55.rd5` | 33 618 451 | lon 5–10, lat 55–60 |
| `E10_N55.rd5` | 64 235 504 | lon 10–15, lat 55–60 (Oslo) |

Corridor pair (`E5_N60` + `E10_N60`): **~54.8 MiB**. All four Ostlandet tiles:
**~152.7 MiB**. Compare Ostlandet Geofabrik PBF (~450 MiB class) plus Navi
region packs.

**Variant downloaded / inspected:** official `https://brouter.de/brouter/segments4/`
weekly files (not a local no-elevation rebuild). These are the files BRouter
documents as generated **with SRTM elevation** when the planet job has CGIAR
SRTM available ([build_segments.md](https://github.com/abrensch/brouter/blob/master/docs/developers/build_segments.md)).
There is no separate “with elevation” filename on that index — elevation is a
footer flag inside the rd5 (`PhysicalFile.elevationType`, default 3 =
3-arc-second class) plus per-node `selev` (`elev_m = selev / 4`). Nodes without
a valid elevation use `Short.MIN_VALUE`; hollow working nodes use `-12345`.

---

## Content check (way geometry, elevation, tags)

`.rd5` is BRouter's own compact routing graph (MicroCache2 cells inside a 5×5°
`PhysicalFile`), **not** OSM PBF and **not** Navi `.navi-graph` / wetland /
POI-barrier packs.

Each node carries integer lon/lat and `selev`. Each forward link carries a
description bitmap (way tags via `lookups.dat`) and packed geometry. That is
enough **geometry + elevation** to draw a line and to cost hills **inside
BRouter**.

Tag table vs what Navi actually uses (from upstream
`misc/profiles2/lookups.dat`, 2026-08):

| Need | In `lookups.dat` / rd5 | Navi consumer |
|---|---|---|
| `highway`, `oneway`, `maxspeed`, `bridge` (incl. boardwalk), `access`, `motor_vehicle`, `foot`, `bicycle` | Yes | Graph build / access |
| `surface`, `tracktype`, `smoothness`, `mtb:scale`, `mtb:scale:uphill` | Yes | Bicycle suitability (PBF pass at convert) |
| `sac_scale` | Yes | Hiking profiles in BRouter; Navi hiking is not SAC-scripted the same way |
| Node elevation (`selev`) | Yes (official tiles) | Navi eco Δh comes from DEM at convert, stored as `edge_delta_h_m` |
| `motor_vehicle:conditional` / `access:conditional` | **No** | Car/truck seasonal closures (Friisvegen way `361797686`, graph pack v3) |
| Wetland polygons (`natural=wetland`, etc.) | **No** | Hiking `wetland_pack` — pack-hit vs 18.6 s PBF |
| Overnight buildings / POI-barrier pack | **No** | Hiking overnight + POI |
| Street `name` / `ref` as Navi HUD fields | Partial / not the pack schema | Graph pack v5 CSR + names |

### Per-profile gaps

**Car.** Way network + elevation exist. **Seasonal motor closures are absent**
(`motor_vehicle:conditional` is not in the lookup table, so it cannot be in the
rd5 bitstream). A BRouter-derived car graph would not reproduce Navi's
Friisvegen winter exclusion. Access/highway/maxspeed are present enough for a
*plausible* highway route, not for Navi's conditional-closure contract.

**Bicycle.** Surface / tracktype / smoothness / `mtb:scale` **are** in the
lookup table, so a converter *could* apply Navi's `BikeCapability` exclusions
at convert time. That still is not Navi's bicycle pack: packs are
profile-filtered `FlatGraphPack` v5 with Navi's access/oneway/lanes/motorroad
fields. No automatic identity with `plan_bicycle` pack-hit.

**Hiking.** Footways, `foot=*`, `sac_scale`, and elevation exist. Navi hiking
pack-hit is **not** “foot graph only”: wetland pack + overnight buildings pack
dominate the miss penalty (README: Skolla→Rondvassbu ~104 s without packs vs
~25 s with `wetland_pack_hit` / `poi_pack_hit`; short Atnbrufossen hike 159 s
→ ~3.1 s with wetland + overnight). Those layers **cannot** be filled from
`.rd5`. A BRouter hiking line may look reasonable on a map and still be the
wrong product (no wetland hard-avoid, no 150 m building overnight filter, no
DNT multi-day hut logic).

---

## Conversion cost (rd5 is not Navi-consumable)

Navi's planner pack-hit path loads mmap `rkyv` packs
(`FlatGraphPack` / wetland / POI-barrier) via
`try_load_graph_for_plan`. An `.rd5` file will never satisfy that check.

To use BRouter tiles **inside Navi's engine** you would need a converter:

1. Decode MicroCache2 cells (`PhysicalFile` / `OsmFile` / `DirectWeaver`).
2. Decode description bitmaps with `lookups.dat`.
3. Map links → `GraphEdge` / `FlatGraphPack` (and invent missing fields).
4. Write temp→rename packs + a manifest the existing hit check accepts.

That converter does not exist (and was not added). A full-tile decode
(`PhysicalFile.checkFileIntegrity`) is the **lower bound** on conversion: it
already walks every microcache. Encoding a Navi pack, profile splits, and
DEM Δh would cost extra.

**Even a fast converter cannot emit wetland or overnight packs from rd5.**
Hiking would remain pack-miss (or a hybrid: graph from rd5, wetland still from
PBF — which is the slow part). Car would miss conditionals. The planner would
still fall through to PBF for those layers unless the hit check were lied to.

BRouter's own process is the opposite direction (PBF → rd5 via OsmCutter …
WayLinker). There is no supported rd5 → OSM or rd5 → Navi pack tool.

**BRouter-native plan** (Java `RoutingEngine`, `-Xmx128M` in the standalone
server) is a different product: it uses `.brf` profiles (`car-vario`,
`trekking`, `hiking-mountain`), not Navi costing. Measuring that time answers
“is BRouter fast on this corridor?” — yes, that is why rd5 exists — but it is
**not** “Navi pack-miss speedup.” Wiring that engine was explicitly out of
scope.

---

## End-to-end timing

### Existing Navi baselines (reused — same Ostlandet ODs / release class)

From README known issues (host re-check 2026-08-24, cited in the task as
Pixel 9a / SM-P613 release class):

| Profile | OD | Pack-miss | Pack-hit |
|---|---|---:|---:|
| Car | Espa → Atnbrufossen | ~**54 s** (`pack_hit=false`, graph~29 s + POI~25 s) | ~**2.7 s** |
| Hiking | Skolla → Rondvassbu | ~**104 s** | ~**25 s** (`wetland_pack_hit` / `poi_pack_hit`) |
| Bicycle | same motor corridor class as car | not separately published at 54/2.7; pack-hit Hedmark bike ~1.7 s vs car ~1.5 s (SM-P613 M6) | miss tracks motor PBF rebuild |

SM-P613 convert of full Østlandet remains ~10–11 min and is the memory-pressure
path (see below), **not** the 54 s plan-time miss.

### BRouter path (download + convert + Navi plan)

| Step | Result |
|---|---|
| Download corridor tiles (~55 MiB) | Network-bound. At 10 Mbit/s ~44 s; at 50 Mbit/s ~9 s. Not measured on SM-P613 in this pass. |
| Convert rd5 → Navi packs | **Not available.** Faithful packs cannot be produced from rd5 (wetland / overnight / conditionals). |
| Navi plan once “usable” | Would equal pack-hit **only if** the hit check accepted a complete pack. A graph-only fake hit would still miss wetland/POI on hiking and conditionals on car. |

There is therefore **no complete “BRouter path” wall time** for Navi's three
profiles: the middle step does not yield data the existing planner can treat
as a hit without silently dropping required layers.

### BRouter-native plan (out of scope for integration, recorded for honesty)

Standalone BRouter is designed to route these distances in **seconds** on
phone-class heaps (`-Xmx128M`). That does **not** count as clearing the gate
for Navi: different engine, different profiles, and forbidden to wire in.

Live BRouter HTTP plans on this host / SM-P613 were not completed in this
documentation pass (throwaway harness lives under `/tmp/navi-brouter-probe/`).
That absence does not reopen the gate: native BRouter speed would not make
rd5 a Navi pack.

### Route plausibility

Not a full accuracy audit.

- Car via BRouter `car-vario` on Espa→Atnbrufossen would be expected to follow
  the trunk/primary network (E6 / Rv3 class) — **reasonable** as a driving
  line, **not** equivalent to Navi eco + seasonal filters.
- Bicycle `trekking` uses surface/tracktype in-profile — **reasonable** as a
  BRouter trek, not Navi `BikeCapability` packs.
- Hiking `hiking-mountain` uses SAC/surface/hills — **reasonable** as a BRouter
  walk, **wrong** vs Navi wetland/overnight/hut multi-day.

---

## Memory (SM-P613)

Navi's known pressure is **background pack conversion from PBF** on ~3.5 GB
RAM: convert completed, but MemAvailable ~329 MiB, swap ~250 MiB,
`TRIM_MEMORY_RUNNING_CRITICAL` (README). That is the behaviour BRouter was
hoped to avoid.

`.rd5` corridor data is **~55 MiB on disk**. BRouter's published server opts
are **128 MiB heap**. Download + mmap of two tiles does not resemble the
Ostlandet PBF convert RSS (~1.7 GB class on tiled convert evidence).

A hypothetical Navi converter that materializes `FlatGraphPack` for a 5° tile
could still spike RAM (Navi already refuses loading a full Ostlandet graph
into RAM — bbox materialize is required). The investigation did **not** run a
converter on SM-P613, so it does **not** claim a new LMK; it also does **not**
claim the convert-pressure problem is solved, because the convert-to-Navi-pack
path was not buildable from rd5.

---

## Plain statement of what was found

Official BRouter `segments4` tiles **do** cover Ostlandet/Hedmark, **do**
include elevation (SRTM baked into weekly rd5, per-node `selev`), and **do**
carry way geometry plus the lookup tags needed for generic car/bike/foot
costing **inside BRouter**. They **do not** contain Navi's wetland rings,
overnight buildings, POI/barrier packs, or `motor_vehicle:conditional`
strings.

`.rd5` is **not** consumable by Navi's pack-hit path. Turning it into Navi
packs is a new converter, and even then hiking and seasonal car cannot be
faithful. The pack-miss penalty Navi actually pays on hiking is largely
wetland/POI PBF — which BRouter tiles do not replace. Fetching BRouter data
on pack-miss is therefore **not** a supported speedup for Navi's profiles;
it is a fast **different router**, which this task was not allowed to
integrate.

---

## Gate (Phase 2)

Proceed to a fetch-pipeline spec only if, for at least car and bicycle:

1. BRouter path total time is meaningfully faster than pack-miss (ratio).
2. Route is not obviously worse for the profile.
3. No new LMK / convert-class memory pressure on SM-P613.

**Gate not cleared.** There is no Navi-usable BRouter path to ratio against
pack-miss. Car is missing conditionals; bicycle would need a converter that
does not exist; hiking fails the data-model bar even more clearly. A spec
that put BRouter as tier 1, navi.app/mirror as tier 2, and local PBF as
tier 3 would describe a pipeline that still fell through to PBF for the
layers that make pack-hit fast — or would swap Navi's router for BRouter,
which was out of scope.

Phase 2 spec: **not written**.

---

## Open (if someone later proposes BRouter as a sidecar engine)

That would be a different task: optional external router, profile mapping,
legal/offline UX, and an explicit non-Navi cost model. It is not a pack-miss
tile fetch.
