# Bevensen → Vågå → Dalsøren: follow-up 2 (baseline, place index, packs, Fehmarn)

Follow-up 2. REPORT FIRST. No densify implementation. No commit.

Settings for the live plan: eco OFF, wild camping OFF, camping plugin OFF. Tunnels, highways, tolls, ferries allowed.

---

## Part 0 — restore working baseline

### What was isolated

Uncommitted `core/src/routing/plan_bbox.rs` densify patches **+508 / −29** were removed from this worktree and parked, not discarded:

| Location | Detail |
|---|---|
| Git branch | `wip/plan-bbox-densify-uncommitted` (not committed; ask before commit) |
| Worktree | `/mnt/2e9a1e9f-2097-408c-ab9a-a01b32f11d28/github-projects/Navi-wip-plan-bbox-densify` — dirty `plan_bbox.rs` still +508/−29 |
| Patch copies | `/tmp/plan_bbox-densify-uncommitted-508-29.patch` and `/mnt/2e9a1e9f-2097-408c-ab9a-a01b32f11d28/github-projects/plan_bbox-densify-uncommitted-508-29.patch` |

HEAD already contains densify (0.3.18-beta / 0.3.19-beta). Isolating +508/−29 does **not** remove committed densify; it only removes the extra uncommitted filters/anchors (Præstø→Farø, Staberdorf, Elbe, Ottadal extras, etc.).

Kept in this worktree: `MainActivity.kt` debug write of `files/route-polyline.txt` after a successful pending plan (debuggable builds only).

Build: `NAVI_ABIS=x86_64 ./scripts/build-android-native.sh x86_64 release` then `./gradlew :app:assembleDebug -PnaviAbis=x86_64`. Install: `adb install -r` (data kept; no `pm clear`, no index/SD wipe).

### SHA and APK

| Item | Value |
|---|---|
| HEAD | `4cb72de69539be5d57c913a6a55353573122996a` — Ship 0.3.19-beta |
| Built APK SHA256 | `bb6194e718bbaafaeddef9d730fb53301b723ccb39a6fae56b0e314f7fcd72ff` (`app/build/outputs/apk/debug/app-debug.apk`) |
| Installed `base.apk` SHA256 | same (`/data/app/~~7uqbXdKfI-y2fAFELjbT1g==/no.navi.app-CpmTBPxVo5YeMojKmUkpnQ==/base.apk`) |
| lastUpdateTime | 2026-10-06 03:35:25 |
| x86_64 `libnavi.so` | rebuilt 03:34 from HEAD `plan_bbox.rs` (no +508/−29) |
| App pid after install | 20488 (`am start` only as needed for install; data kept) |

---

## Part 1 — place index rule (audit, no code)

**Rule:** if a place index exists and is intact, never rebuild/discard/modify. Touch only: (1) missing for a downloaded region → build; (2) region updated → rebuild that region; (3) region deleted → remove that region's rows.

### Triggers (create / discard / reopen-write / rebuild / delete)

| Trigger | File:line | Class | Notes |
|---|---|---|---|
| `PlaceIndexBackground.ensureStarted` → `ensurePlaceIndex` | `PlaceIndexBackground.kt:84–126` | **1** | Skips if `PlaceIndexReady.isReady` or no indexable PBF. |
| Launch auto-index `LaunchedEffect` | `MainActivity.kt:2689–2701` | **1** | Starts background index when material exists and stamp not ready. |
| Region pipeline local-bake `ensurePlaceIndex` | `RegionDownloadBackground.kt:1838–1857` | **1** or **2** | After packs; cache-hit inside native if region complete. |
| `runOneRegion` `preparePipelineStart` | `RegionDownloadBackground.kt:1368–1379` | **2** (or resume) | Fresh pack download: `clearReady` + **delete region rows**. PLACE_INDEX resume with `complete=0`: stamp-only, keep rows. |
| `PlaceIndexReady.clearReady` | `PlaceIndexReady.kt:124–135` | **3** | Stamp remove + `clearRegionRows`. |
| `DownloadedRegionDelete` | `DownloadedRegionDelete.kt:201–203` | **3** | User/region delete. |
| Long-trip `enqueuePlaceIndexOnly` | `LongTripCoordinator.kt:384–425` | **1** | If packs ready and stamp not ready, starts PLACE_INDEX phase. |
| Tools Apply OSM update | `MainActivity.kt:8011–8028` | **2** (if update applied) | Always `ensurePlaceIndex` after successful apply. |
| Native `ensure_place_index` cache-hit | `navi-ffi/src/lib.rs:5788–5841` | **1 skip** | Size>10k, current schema, has rows, `region_index_complete`. |
| Native `ensure_place_index` `load_from_pbf_for_region` | `navi-ffi/src/lib.rs:5847–5853` + `search/mod.rs:483+` | **1/2** | Clears region rows unless resume (`complete=0`). |
| `build_place_index_from_pbf` / `ensure_place_index_after_pack_install` | `pack_server/place_index_after.rs:129–199` | **1/2** | `force_rebuild` = catalog generation change. **Not called from Kotlin app** (`ensurePackRegionPlaceIndex` is UniFFI-only). |
| `catalog_generation_requires_rebuild` | `osm_update.rs:114–124` | **2** | Remote generation non-empty and ≠ local. Identical re-download with same generation must not rebuild. |
| `clear_place_index_region_rows` FFI | `search/mod.rs:105–121`, `navi-ffi/src/lib.rs:5891` | **3** | Used by `PlaceIndexReady.clearRegionRows`. |
| `invalidate_derived` on OSM apply | `osm_update.rs:738–746` | does **not** delete DB | Graph-cache only; comments say apply callers rebuild via `ensure_place_index`. |
| Route planning (`plan_car_route` / densify) | `navi-ffi` plan path | **none** | Does not create/discard `place_index.db`. |

### OUTSIDE the 1/2/3 rule — LIST AND ASK (do not decide)

1. **Schema mismatch: discard entire shared DB** — `NameIndex::discard_if_schema_stale` `search/mod.rs:390–413`, called from `ensure_place_index` `navi-ffi/src/lib.rs:5817` and `place_index_after.rs:145`. One region's schema bump deletes **all** regions. ASK: per-region migrate instead of file delete?
2. **`placeIndexLooksReady` deletes the ready stamp** when `user_version` is old — `RegionDownloadBackground.kt:780–784`. Then launch/long-trip treat every region as missing → rebuild. ASK: stamp-heal vs rebuild?
3. **`NameIndex::open` always migrates / may DROP+recreate FTS** — `search/mod.rs:150–171`, `ensure_search_doc_fts` `305–333`. An intact v5 DB can still be rewritten on open. ASK: read-only open for checks; never DDL on intact current schema?
4. **`is_current_schema` uses writable `Connection::open`** — `search/mod.rs:374–377`. Cheap readonly pragma is already used on Kotlin (`placeIndexSchemaCurrent` `808–822`). ASK: make Rust check readonly too?
5. **Failed open / corrupt DB** — `ensure_place_index` returns `FAIL: open index` (`lib.rs:5857–5860`) without deleting; next caller may retry forever or a human may delete the file. ASK: quarantine vs rebuild?
6. **`region_index_complete` trusts `complete=1` even when `name_entries` for that `region_id` are empty or tiny** — `search/mod.rs:428–452`. Live Halland: complete=1, expected=written=1520667, **14 rows**. Västra Götaland: complete=1, **0 rows**. Cache-hit then **skips** a real build. ASK: intact must include `COUNT(region_id)>0` (and maybe written≈count)?
7. **`name_index_build` missing** — treated as complete via `has_entries_for_region` fallback (`428–451`) and Kotlin `placeIndexBuildComplete` treats missing table as complete (`RegionDownloadBackground.kt:856` comment). ASK: missing flag = incomplete (rule 1) or legacy intact?
8. **Internal vs SD path** — place index always `files/place_index.db`; packs often `/storage/0000-0000/.../long-trip-packs`. Not a rebuild by itself, but stamp/UI can say Indexed while rows were never written for SD-only regions (Denmark, Skåne, Vestlandet, SH, Sørlandet). ASK: bind index to the same storage inventory as packs?
9. **Sweden län sharing `sweden-latest.osm.pbf`** — Halland/VG `expected` both 1520667. Indexing from the country extract under a leaf `region_id` overwrites/clears the wrong slice. ASK: index once under `europe/sweden` or require leaf extracts?
10. **App update / schema bump at launch** — `PLACE_INDEX_SCHEMA_VERSION = 5` (`place_context.rs:30`, Kotlin `806`). Next `ensure_place_index` hits (1). ASK: background migrate, never wipe on first frame?
11. **`healReadyFromDownloads` may rewrite `place-index-ready.json`** — `PlaceIndexReady.kt:70–107`. This session did **not** rewrite it (mtime 03:11). ASK: heal stamp without touching DB — allowed?
12. **Concurrent `am start` double plan** — two native threads (20555/20556) this run. Not an index rebuild, but contends SQLite. ASK: single-flight plans?

Route planning does not rebuild the place index.

### Cheap “intact” (must not cause rebuild, must not block main thread)

Off main thread, readonly SQLite, no WAL writer, no `ensure_place_index`:

1. `place_index.db` exists and `length >= 10_000`.
2. `PRAGMA user_version` (readonly) `>= PLACE_INDEX_SCHEMA_VERSION`.
3. `name_index_build.complete != 0` for that `region_id` **or** (legacy) no row and `COUNT(*)>0` for that id.
4. `SELECT COUNT(*) FROM name_entries WHERE region_id=?` **> 0** (cheap index on `region_id`).

If (1)–(4) hold, **never** call discard/open-for-write/load_from_pbf. Do not use `Connection::open` (read-write) for this check.

### How “region updated” is detected (identical re-download must not rebuild)

Already intended: `catalog_generation_requires_rebuild(local, remote)` (`osm_update.rs:114–124`) — rebuild only when remote generation is non-empty **and** differs from local install stamp (`*.navi-server-install.json` `generation`). Matching generations (including empty remote) must not `force_rebuild`. Incomplete indexes use `complete=0` resume, not generation.

Do **not** use PBF mtime/size alone: pack-server stubs vs real extracts and SD vs internal copies would false-trigger.

### Did this investigation rebuild or remove a place index?

**This Part 0 install + launch: no.** `place_index.db` still 1 349 812 224 bytes, mtime **03:11**; `place-index-ready.json` mtime **03:11**; WAL empty. No `PlaceIndexBg` / `discarded stale` lines after 03:35 install.

**Earlier session (honest):** Halland reindex from install + stamp + `am start` **did** run (campaign logs ~18:30 2026-10-05: `Place index: starting` for corridor regions; stamp marked Vestlandet/Sørlandet/Skåne Indexed). Live DB still shows Halland `complete=1` with **14** rows — that rebuild did not leave an intact Halland slice.

---

## Part 2 — device/SD inventory vs app belief

### App belief (this emulator, after Part 0, before/during plan)

- HUD: `camping_plugin_enabled=false`, `camping_professional_driver=false`, `long_trip_enabled=true`, pack volume `uuid:0000-0000`, **selected** `geofabrik_path=europe/germany/hamburg`.
- `plugin_enable.json`: `right_to_roam_camping: false`.
- `place-index-ready.json` (11 ids): denmark, hamburg, niedersachsen, schleswig-holstein, ostlandet, sorlandet, vestlandet, sweden, halland, skane, vastra_gotaland.

### Disk inventory (real files)

**Internal** `/data/user/0/no.navi.app/files` (place index lives here):

| Stem | PBF | Manifest | Graph car tiles | Ferry overlay | Notes |
|---|---|---|---|---|---|
| denmark | 495 245 679 | no | wetland leftovers | no | PBF only |
| halland | **16 384 stub** | yes | car+foot tiles | no | stub PBF |
| niedersachsen | 506 819 877 | yes | car+foot | car+truck overlay | |
| ostlandet | 455 330 648 | yes | car+foot | car+truck overlay | |
| schleswig-holstein | 158 623 177 | yes | car+foot | no overlay in internal | |
| sorlandet | 77 117 501 | no | no | no | PBF only |
| sweden | 857 127 942 | no | no | no | country extract |
| vastra_gotaland | **16 384 stub** | yes | car+foot | no | stub PBF |
| vestlandet | 257 415 041 | no | no | no | PBF only |
| skane | — | no | car tiles present | no | packs without PBF/manifest |
| hamburg | — | no | — | — | not in internal |

**SD** `/storage/0000-0000/Android/data/no.navi.app/files/long-trip-packs` (prefs volume): graph **format 9**, profiles **car+foot only** (truck aliases to car). Manifests: denmark, halland, hamburg, mecklenburg-vorpommern, niedersachsen, ostlandet, schleswig-holstein, skane, sorlandet, vastra_gotaland, vestlandet. Real PBFs as listed in Part 0 inventory (denmark 495 MB … vestlandet 257 MB). Ferry **overlay** sidecars (`.navi-ferry-overlay-{car,truck}.rkyv`): denmark, halland, MV, niedersachsen, ostlandet, SH, skane, sorlandet, vestlandet. **No** `*.ferry.osm.pbf` extracts. Hamburg: monolithic `car`/`foot` rkyv + wetland file.

**Tiles:** `files/pmtiles/` has germany, hamburg, niedersachsen, ostlandet, vestlandet, halland, skane. **Rejected:** SH, sorlandet, vastra_gotaland, several Norrland. HUD `downloaded_pmtiles_regions` includes SH and VG and denmark — SH file on disk is `.rejected`.

**Place index DB (schema v5, 6 109 647 rows):**

| region_id | complete | expected / written | actual rows |
|---|---|---|---|
| europe/germany/hamburg | 1 | 1 520 917 / 1 720 908 | 1 720 871 |
| europe/germany/niedersachsen | 1 | 2 907 266 / 2 907 266 | 2 905 566 |
| europe/norway/ostlandet | 1 | 1 487 910 / 1 487 910 | 1 483 196 |
| europe/sweden/halland | 1 | 1 520 667 / 1 520 667 | **14** |
| europe/sweden/vastra_gotaland | 1 | 1 520 667 / 1 520 667 | **0** |
| denmark, SH, vestlandet, sorlandet, sweden, skane | stamped ready | — | **0** |

### Mismatch count: **18**

1. Stamp 11 Indexed vs DB rows for 4 ids only.
2. Halland complete vs 14 rows.
3. VG complete vs 0 rows.
4. Hamburg expected ≠ written.
5. Selected region hamburg vs plan PBF vestlandet.
6–10. Denmark, SH, Vestlandet, Sørlandet, Skåne, sweden: stamp and/or packs without index rows.
11. SH / VG / Sørlandet PMTiles rejected vs HUD downloaded set / packs present.
12. Halland + VG internal 16 KiB stubs vs SD real packs.
13. Duplicate stems on internal **and** SD.
14. `files/long-trip-packs` empty; real packs on UUID volume.
15. Skåne SD packs, no internal PBF, stamp ready, 0 index rows.
16. `localBakeReady` = “manifest file exists” (`PackRegionAvailability.kt:148–159`), not “tiles load for Truck”.
17. `resolvePlanPbf` picks **smallest** PBF that covers **any** waypoint (`RegionCoverage.kt:427–433`) → vestlandet (dest) not origin.
18. Mecklenburg packs on SD, not in ready stamp (not on this OD, still inventory drift).

Truck graphs absent is **not** counted: `manifest.rs:276–295` aliases Truck → car tiles. This session `pack_hit=true`.

### Why `pbf_stem=vestlandet-latest` on SH load

`planning_stem` is the filename of the **planning PBF** (`load.rs:224–234`). Long-trip plans call `RegionCoverage.resolvePlanPbf(dataDir, waypoints, longTripPackDir)` (`MainActivity.kt:2959–2960`), **not** `RouteReplan.resolvePbf` (which would prefer ostlandet).

No single extract covers Bevensen **and** Dalsøren. `partialCover` then takes the candidate covering **any** stop with **minimum file size**. Vestlandet’s 257 MB PBF (covers Dalsøren) is smaller than Ostlandet 455 MB (Vågå) and Niedersachsen 507 MB (Bevensen), so the plan PBF is `.../long-trip-packs/vestlandet-latest.osm.pbf`. Hop loads **re-home** via `pick_primary_manifest` (`load.rs:2097–2140`) to the Ready stem that PIP-covers the hop start (SH, Denmark, …). Log line `stem=schleswig-holstein-latest (pbf_stem=vestlandet-latest)` is that split, not a SH pack stored under Vestlandet.

### Why “PBF fallback” when packs exist

Kotlin hint fires on `pack_hit=false` (`MainActivity.kt:513–516`). **This eco-off plan: every logged hop had `pack_hit=true`.** Previous session’s “Slow plan: indexed maps not ready (PBF fallback)” with `eco=true` / camping on is **not** reproduced here.

If fallback happens on other hops, causes matching this device:

- `try_load_graph` Missing / Stale / VersionMismatch → PBF rebuild (`lib.rs:3495+`).
- Planning PBF is vestlandet: a miss rebuilds **Vestlandet extract**, not SH.
- `missing_ready_regions` fails closed (no PBF fallback).
- Empty `packDir` used to skip SD (`MainActivity.kt:2954–2956`); this run passed SD first in `dirs=`.

### Design: one source of truth (design only)

Scan **both** internal files and every mounted `.../long-trip-packs` (and tools pack roots). Record per Geofabrik id:

- pack home dir, manifest generation, graph format, profiles actually on disk (car tiles usable as truck),
- real PBF vs stub,
- ferry overlay sidecar present,
- PMTiles present vs `.rejected`,
- place-index intact check (Part 1 cheap probe),
- storage volume id.

Refresh on download complete, region delete, catalog generation change, and `StorageVolume` insert/remove — **without** process restart (existing `NaviStorageVolumes` watch + a single `InstalledMaps` snapshot). Router, place-index gate, and download UI all read that snapshot. `localBakeReady` must require loadable tiles for the active profile, not merely a JSON file. `resolvePlanPbf` should prefer the **origin hop leaf**, not the smallest dest PBF.

---

## Part 3 — baseline plan + full line

**Settings before start (logged; abort if wrong):** camping plugin false, plugin JSON camping off, professional driver false. `NaviDebugIntent` applied `eco=false`, `avoid_ferries=false`, `profile=mobile_home`, `restore=false`. `planning_start ... eco=false`.

**Result: FAIL** (waited until terminate).

| Item | Value |
|---|---|
| Terminal | `planning_failed` 04:22:47 |
| duration_ms | 2 415 522 (~40 min) |
| eco | false |
| terminate | `corridor_disconnected` |
| expansions | 2 914 097 |
| pads | `[0.35,0.35,0.35,0.35]` |
| UI reason | `Map data updated.` — **sanitizer lie**: `OsmUpdateUserCopy.sanitize` maps any technical report containing `pass` (e.g. `pack_hit` / `PASS` lines) to `UPDATED` (`OsmUpdateUserCopy.kt:70–76`) |
| `route-polyline.txt` | **absent** (no successful pending plan) |
| GeoJSON | **not written** |

**Densify (HEAD, no +508/−29), 19 hops:**

```
53.0797,10.5872; 53.5650,10.0400; 54.2100,11.0250; 54.3999,11.3643;
55.1125,12.0400; 55.9100,13.0520; 56.9350,12.4960; 57.5950,12.2428;
58.2550,11.9895; 58.4437,11.2087; 59.2762,10.8637; 59.9100,10.7500;
60.7950,11.0680; 61.1150,10.4660; 61.7720,9.4200; 61.8691,9.1055;
61.8380,8.5690; 61.6170,8.0440; 61.4434,7.4614
```

Via Vågå is `61.8691,9.1055`. Unlike the previous 21-hop eco=true tree, there is **no** mid-water `54.4990,11.2315`. Fehmarn densify is `54.2100,11.0250` → `54.3999,11.3643` (east Fehmarn / Lolland). Præstø `55.1125,12.0400` is present (the isolated +508/−29 Farø swap was not in this APK).

Two native plan threads (20555/20556) from a second `am start` on the already-running activity. Both used packs.

**Hop sequence (primary stem, all `pack_hit=true`):** niedersachsen → hamburg → schleswig-holstein → **denmark** → skane → halland → vastra_gotaland → ostlandet. **Never** `stem=vestlandet-latest`. Fail after ~40 min still on **ostlandet** with `need_extra=true extras=1`, last graphs **8678–9961 nodes** (tiny clip).

**Failing hop (best evidence):** last densify points in Ostlandet/Vestlandet after the via:

- A: `61.8380,8.5690` (west of Vågå / toward Lom)
- B: `61.6170,8.0440`
- Dest: `61.4434,7.4614` (Dalsøren, Vestlandet)

Primary component: **ostlandet-latest** Truck=car tiles. Extra: 1 neighbour (should be vestlandet; extra stem names were **not** in the surviving logcat buffer). Snap metres not logged. A* `corridor_disconnected` after 2.9e6 expansions. Dest pack was never primary; Ostlandet-only (or Ostlandet+thin extra) cannot hold Rv15 / Sognefjell / ferry toward Luster.

**Spikes from full polyline:** **cannot recompute** (no edge polyline).

---

## Part 4 — Fehmarn Belt (report only)

This eco-off plan **did load Denmark after SH** (`03:48:35` denmark `pack_hit=true` nodes=396045). The previous eco=true disconnect on the water hop **did not recur** with HEAD densify `54.3999,11.3643` and packs.

There is **no** full edge polyline, so there is **no** exact Fehmarn–Lolland edge list with `is_ferry` from this run.

Facts that **are** on device / in this plan:

| Fact | Evidence |
|---|---|
| No DE–DK fixed link | Unchanged geography; route must use ferry or a huge detour |
| SH pack | SD + internal, format 9, car/foot tiles, Ready enough to `pack_hit=true` |
| SH ferry overlay sidecar | `schleswig-holstein-latest.navi-ferry-overlay-truck.rkyv` + `.meta` `build=2;len=158623177` |
| DK ferry overlay sidecar | `denmark-latest.navi-ferry-overlay-{car,truck}.rkyv` |
| Compact `{stem}.ferry.osm.pbf` | **neither** SH nor DK |
| `route_ferry_legs` | **not** in surviving logs for this run (logcat rotated over 40 min) |
| Puttgarden–Rødby in pack vs overlay | **Cannot** list pack edges without a polyline or a pack dump. Overlay **exists** for SH+DK truck. Prior session logged `ferry_overlay stem=schleswig-holstein-latest ferry_edges=26` and still disconnected on a **different** densify mid (`54.4990,11.2315`). This session’s hop geometry is on-shore `54.3999,11.3643` and proceeded to Denmark. |

**ASK / unknown:** whether Puttgarden–Rødby `route=ferry` is **inside the SH/DK graph tiles** with `is_ferry`, only in the overlay sidecar, or neither. Needs a pack/overlay edge query or a **successful** polyline, not another densify coordinate.

---

## Part 5 — densify design (no implement)

Previous rec: **(a)** joints as **cuts**, not extra via-points; **(c)** path≫chord repair as safety net.

Update after Parts 2–4:

- Several “densify spikes” in older dumps were mixed with **wrong planning PBF** (smallest dest extract) and **eco/camping-on** sessions. This eco-off HEAD tree **crossed Fehmarn into Denmark on pack_hit**. Extra +508/−29 Farø/water mids are **not** required to explain the old water fail; the old mid-on-water hop was.
- **This** fail is **Ostlandet→Vestlandet after Vågå**: dest pack never became primary; last graphs are ~9k nodes. That is **pack merge / extra-stem / tile-band**, not a missing Præstø filter. Path≫chord repair would not run because the hop **never found a path**.
- (a) still stands: Vågå as a **cut** so the following hop’s primary is Vestlandet (or Ostlandet+Vestlandet extras with a real dest PIP), not another Ostlandet clip.
- (c) still stands as a safety net **after** a connected path exists.
- Do **not** add more place-specific densify anchors. Fix inventory (`resolvePlanPbf` origin leaf) and hop extras before more geometry.

---

## Return (requested)

- **Part 0:** HEAD `4cb72de69539be5d57c913a6a55353573122996a`; APK SHA256 `bb6194e718bbaafaeddef9d730fb53301b723ccb39a6fae56b0e314f7fcd72ff`.
- **Part 1 ASK:** schema wipe of whole DB; stamp delete on old schema; DDL/FTS on open; writable schema probe; failed-open policy; `complete=1` with 0/14 rows; missing build flag; internal vs SD index; Sweden-län shared PBF; launch schema bump; stamp heal; concurrent plans.
- **Part 2:** **18** mismatches; vestlandet stem = `resolvePlanPbf` min-size partial cover of dest.
- **Part 3:** **FAIL** `corridor_disconnected` after ~40 min; no polyline; spike count N/A; fail on Ostlandet hops toward Dalsøren (not Fehmarn).
- **Part 4:** SH→DK pack_hit this run; overlay sidecars present; no `.ferry.osm.pbf`; no exact ferry edge flags without polyline.
- **Part 5:** keep (a) cuts + (c) path≫chord; do not add anchors; fix PBF/pack primary for Vestlandet dest first.

---

# Follow-up 3 (A–C implemented; D–E report)

Branch: `wip/fu3-plan-diag-index-snapshot` from `origin/dev` `4cb72de6`. Densify +508/−29 not included. `route-polyline.txt` export kept. No commit.

Hamburg / Niedersachsen / Ostlandet place-index slices were not rebuilt (DB mtime still 2026-10-06 03:11, 1 349 812 224 bytes).

## Part B9 — Sweden län sharing one country extract (report only; no decision)

Halland, Skåne, and Västra Götaland packs on this emulator do **not** ship a leaf `.osm.pbf`. The only real Sweden extract is `sweden-latest.osm.pbf` (internal 857 MB, SD 852 MB). `name_index_build` for Halland and VG both used **expected=written=1 520 667**, matching a full-country name pass, not a län clip.

Options (on-device cost; do not pick one here):

1. **Index once under `europe/sweden`.** One pass over ~851–857 MB. Order of **1–4 hours** on this AVD (same ballpark as Ostlandet 455 MB / Niedersachsen 507 MB). Search then filters by country, not leaf. Leaf HUD “Indexed” would be derived, not a second write.
2. **Download leaf extracts** (Halland/Skåne/VG Geofabrik files) and index each `region_id` from its own PBF. Extra download + **tens of minutes to hours per leaf**. Avoids tagging Sweden-wide rows as Halland.
3. **Clip the country extract to each län bbox on device**, then index the clip. Extra CPU, temp disk ≈ extract size, **hours** if done three times; one clipped Sweden pass is closer to (1).

**14 Halland rows:** `complete=1` with expected 1 520 667 but `COUNT(region_id)=14`. That is **not** an intact Halland index. It is a leftover sliver after a country-sized write was attributed to `europe/sweden/halland` and then almost entirely replaced or deleted. Intact gate now treats this as **missing**. Do not build this session.

**Hamburg slice:** `expected=1 520 917`, `written=1 720 908`, `COUNT=1 720 871`. Sweden expected was 1 520 667. Those magnitudes are a **country name table**, not a city extract. Internal still has **no** Hamburg PBF; SD now has `hamburg-latest.osm.pbf` **53 824 203** bytes (real). Intact check currently marks Hamburg **INTACT** because rows ≫ 0 and ≥10% of written. That answers “does the slice have rows?”, not “are they Hamburg OSM names?”. Do not decide a rebuild here.

## Missing place-index builds (listed, not started)

From `files/installed-maps-snapshot.txt` after FU3 launch. Intact left as-is: hamburg, niedersachsen, ostlandet.

| region_id | source file | note |
|---|---|---|
| europe/denmark | SD `denmark-latest.osm.pbf` 495 245 679 | ~1–4 h; 0 rows |
| europe/germany/schleswig-holstein | SD `schleswig-holstein-latest.osm.pbf` 158 623 177 | ~30–90 min; 0 rows; PMTiles rejected |
| europe/norway/vestlandet | SD `vestlandet-latest.osm.pbf` 257 415 041 | ~30–90 min; 0 rows |
| europe/norway/sorlandet | SD `sorlandet-latest.osm.pbf` 77 117 501 | under 30 min; 0 rows; PMTiles rejected |
| europe/sweden | internal `sweden-latest.osm.pbf` 857 127 942 | ~1–4 h; 0 rows; no graph pack |
| europe/sweden/halland | no PBF on pack volume (stub 16 KiB internal) | 14 rows; not intact; need leaf or country extract (B9) |
| europe/sweden/skane | no PBF on pack volume | 0 rows; SD graph pack; real names live in sweden extract |
| europe/sweden/vastra_gotaland | no PBF on pack volume (stub 16 KiB internal) | 0 rows; PMTiles rejected |
| europe/germany/mecklenburg-vorpommern | SD `mecklenburg-vorpommern-latest.osm.pbf` 128 227 738 | ~30–90 min; 0 rows; not on this OD stamp |

## Part C — emulator InstalledMaps snapshot (12 regions)

Volume for packs: `uuid:0000-0000` except `europe/sweden` (internal PBF only). Graph format 9. Profiles that load: **car,foot** (truck aliases to car). Ready = graph tiles loadable, not PMTiles.

| region | vol | pbf | ferry car/truck | pmtiles / rejected | index / rows |
|---|---|---|---|---|---|
| denmark | SD | REAL | yes/yes | no / no | MISSING / 0 |
| hamburg | SD | REAL 53 MB | no/no | yes / no | INTACT / 1 720 871 |
| mecklenburg-vorpommern | SD | REAL | no/yes | no / no | MISSING / 0 |
| niedersachsen | SD | REAL | no/yes | yes / no | INTACT / 2 905 566 |
| schleswig-holstein | SD | REAL | yes/yes | no / **rejected** | MISSING / 0 |
| ostlandet | SD | REAL | no/yes | yes / no | INTACT / 1 483 196 |
| sorlandet | SD | REAL | no/yes | no / **rejected** | MISSING / 0 |
| vestlandet | SD | REAL | no/yes | yes / no | MISSING / 0 |
| sweden | internal | REAL | no/no | no / no | MISSING / 0 |
| halland | SD | MISSING on volume | no/yes | yes / no | MISSING / 14 |
| skane | SD | MISSING on volume | no/yes | yes / no | MISSING / 0 |
| vastra_gotaland | SD | MISSING on volume | no/no | no / **rejected** | MISSING / 0 |

### FU2 “18 mismatches” vs this snapshot

| # | FU2 item | Now |
|---|---|---|
| 1–3, 6–10, 15 | Stamp Indexed vs empty/tiny rows | Snapshot `index=MISSING`; stamp file not rewritten |
| 4 | Hamburg expected ≠ written | Still true; content likely Sweden-scale (B9). Counted intact by row test |
| 5 | Selected hamburg vs plan PBF vestlandet | Plan PBF stem this run: **ostlandet-latest** (origin-leaf ranking; not smallest dest). Selected catalog region still hamburg in HUD if unchanged |
| 11 | PMTiles rejected vs HUD | Snapshot `rejected=true` for SH, Sørlandet, VG |
| 12 | Internal 16 KiB stubs | Snapshot pbf=MISSING on SD for Halland/VG/Skåne; stubs remain internal |
| 13–14 | Duplicate stems / empty `files/long-trip-packs` | Snapshot prefers SD packs when tiles load |
| 16 | Ready = manifest only | `tilesLoadFor(car)` from real `navi-graph-car` tiles |
| 17 | `resolvePlanPbf` smallest dest | Unit test: Niedersachsen wins. Device log still `pbf_stem=ostlandet-latest` (covering via/dest), **not** vestlandet |
| 18 | Mecklenburg on SD not in stamp | Snapshot lists it `index=MISSING` explicitly |

## Part D — Bevensen → Vågåvegen 80 → Dalsøren

**Settings (abort gate):** `plan_settings eco=false camping_plugin=false professional_driver=false avoid_motorways=false avoid_tolls=false avoid_ferries=false avoid_tunnels=false`. `plugin_enable.json` camping false.

Install `-r` did not auto-start the process; one launcher `am start` without extras after each install (pending-debug-trip.json). First APK hung in `corridorReadyForPlanning` waiting for place-index; second APK uses pack-ready as the plan gate (no index build).

**Hop log** (`files/routing-plan.log`): all listed hops `pack_hit=true`, `weak_ok=true`, `directed_ok=true`. Named extras as logged.

| hop | primary | extras | snap_a_m | snap_b_m | nodes |
|---|---|---|---|---|---|
| Bevensen → 53.565,10.040 | niedersachsen-latest | hamburg, mecklenburg-vorpommern | 21.6 | 57.6 | 250046 |
| → 54.210,11.025 | hamburg-latest | denmark, schleswig-holstein | 57.6 | 365.8 | 338801 |
| → 54.400,11.364 | schleswig-holstein-latest | (none) | 365.8 | 3513.9 | 52590 |
| → 55.113,12.040 | schleswig-holstein-latest | denmark | 3513.9 | 481.2 | 137793 |
| → 55.910,13.052 | denmark-latest | mecklenburg-vorpommern, skane | 481.2 | 424.7 | 396045 |
| → 56.935,12.496 | skane-latest | halland, vastra_gotaland | 424.7 | 128.2 | 151217 |
| → 57.595,12.243 | halland-latest | (none) | 128.2 | 988.4 | 63107 |
| → 58.255,11.990 | halland-latest | ostlandet, vastra_gotaland | 988.4 | 209.9 | 246850 |
| → 58.444,11.209 | vastra_gotaland-latest | denmark, ostlandet | 209.9 | 2923.5 | 110832 |
| → 59.276,10.864 | vastra_gotaland-latest | ostlandet | 2923.5 | 90.4 | 157304 |
| Ostlandet hops through Vågå | ostlandet-latest | (none) until last | | | 10k–304k |
| **61.617,8.044 → Dalsøren 61.443,7.461** | **ostlandet-latest** | **vestlandet-latest** | 31.9 | 17.0 | **9961** |

**“Destination pack never becomes primary”:** **confirmed.** Dest hop primary=`ostlandet-latest`, extra=`vestlandet-latest` only, 9961 nodes.

**Fail-fast:** did **not** fire. Same weak component (`comp_a=comp_b=9858452808`), `directed_ok=true`, then A* found a path. Success is not logged as `hop_result=` (file stayed 5735 bytes).

**Plan status: PASS** (finished 2026-10-06 06:42). HUD: Multi-day plan (4 days), next maneuver 38 m. Polyline was in **external** `long-trip-ui-report/route-polyline.txt` (1 034 846 chars), not `files/route-polyline.txt`. Earlier “still in A*” was that path miss plus no success line in the hop log.

**Settings still:** hop log `eco=false camping_plugin=false`; `plugin_enable.json` camping false; `navi.db` `rest_config` car/truck `eco_mode_enabled=false`; enumerations `wild_camping_site_count=0`. Profile in FFI report: **Car** with `use_eco=false`.

| Item | Value |
|---|---|
| Distance | 1618.42 km (app) / 1616.62 km (polyline) |
| Days | 4 (overnights Quality Hotel View, Hotel Caprifol, Vertshuset Sinclair) |
| `route_ferry_legs` enum | 0 (undercount) |
| Belt geometry | 13 919 m edge `54.50709,11.23183` → `54.62456,11.30643`; closest Puttgarden 268 m, Rødby 281 m |
| Via / dest snap | 6.6 m / 16.7 m |
| Fehmarn hop snap | 3514 m at `54.40,11.36` (largest snap) |

**GeoJSON:** `geojson-routes/bevensen-vagaa-dalsoren-fu3.geojson` (also copied to sibling `Navi/geojson-routes/`). Hop sidecar: `geojson-routes/bevensen-vagaa-dalsoren-fu3-hops.json`.

**Spikes/hairs/loops from polyline** (path/chord ≥ 2.5; hair turn ≥ 150° with 40–2500 m legs; loop close < 120 m after 8–80 km):

- Hairs (2): 180° out-and-back at Halland densify `56.93600,12.49705` (km 669) and Ostlandet densify `60.79531,11.06810` (km 1286).
- Loops (4): Fehmarn 21.5 km loop close 35 m at `54.443,11.181` (km 232); VG coast 13.0 km / 8.5 m at `58.421,11.291`; VG 14.5 km; Otta/Vågå 9.1 km at `61.792,9.43`.
- Worst path/chord: **19.7** on a 20 km window at Fehmarn (`54.443,11.183`, km 232) — same island loop. Other ≥6× windows at Præstø densify `55.12,12.02`, Uddevalla `58.41,11.27`, Otta `61.78,9.42`.

**place_index.db** mtime 06:50, size 1 866 371 072 (was 1 349 812 224 at 03:11). Stamp rewritten 06:50. **Denmark was indexed** (`complete=1`, 2 851 971 rows) — that is the size jump; it was on the missing-build list and should not have been auto-built this campaign. Hamburg / Niedersachsen / Ostlandet still have large slices (1 719 762 / 2 903 125 / 1 483 135). This D-only follow-up did not open the DB for write.

## Part E — Puttgarden / Rødby ferry (no route)

Host `navi-ferry-probe` on pulled SH+DK **car** clip tiles + car/truck overlay sidecars. Clip [54.35,11.05]–[54.80,11.55], radius 8 km. Manifest format 9; **tiles_truck=0** (truck overlay still loads; pack truck uses car tiles).

**Schleswig-Holstein**

- Overlay car/truck: 24 ferry edges; 8 near Puttgarden, 4 near Rødby.
- Pack car clip: 22 ferry in clip; 8 / 4 near the two ports.
- Crossing (overlay): `54.50282,11.22822` → `54.65356,11.35156` (~18.9 km) and a twin ~18.9 km to `54.65431,11.35081`, both directions, `is_ferry`.
- Short 33 m ferry stubs at Puttgarden. Outgoing non-ferry at overlay nodes: one end of each crossing has `src_road_out=1` or `tgt_road_out=1`; the opposite port often 0 outgoing (incoming-only on a directed graph).

**Denmark**

- Overlay: 26 ferry edges; 8 near Puttgarden, 6 near Rødby.
- Pack car clip: 12 ferry; 8 / 4 near ports.
- Crossing: `54.50282,11.22822` → `54.65339,11.35122` (~18.9 km) plus 29 m stubs at Rødby. Puttgarden 33 m stubs show `src_road_out=0 tgt_road_out=0` on those overlay nodes (roads may sit on the SH pack, not the DK clip).

**Answer:** the belt crossing is in **both** pack tiles and overlay sidecars, with `is_ferry=true`. It is not missing from both. Road connection is **asymmetric per pack**: SH overlay shows a road out on one end of the crossing; DK overlay shows a road out on the Rødby stub. A merged SH+DK load is what the hop used (`extra=denmark` on the Fehmarn hop, `directed_ok=true`).

## FU3 return

- Branch: `wip/fu3-plan-diag-index-snapshot`
- Local commits: see Follow-up 4. Do not push.

---

# Follow-up 4 (report from existing GeoJSON; densify design only)

Sources: `geojson-routes/bevensen-vagaa-dalsoren-fu3.geojson`, `…-fu3-hops.json`, `…-fu3-polyline.txt`, device `files/routing-plan.log` (5735 bytes, no `hop_result=success`), external `long-trip-ui-report/route-polyline.txt` + `route-result.json`. **No new plan.** ORS / bad-luster reference **1440.985 km**.

Canonical artifacts after Step 2a (not used by the FU3 run): `{filesDir}/long-trip-ui-report/` (`routing-plan.log`, `route-polyline.txt`, `hops.json`, `route-result.json`), mirrored to `getExternalFilesDir()/long-trip-ui-report/` for `adb pull`.

## a) Spike / hair / loop table (full polyline)

Polyline **1616.62 km**, app **1618.42 km**. Gap to ORS **177.44 km**. Sliding windows with path/chord ≥ 2.5 overlap; **non-overlapping extra** from the worst windows is **~91 km**. Hairs are joint out-and-backs (~0.3 km). Loops are the same island/coast/Otta circuits as the large windows (do not add their path length on top of the window extra).

| kind | km | location | roads (campaign) | path km | chord km | extra km | densify hop |
|---|---|---|---|---|---|---|---|
| spike+loop | 232 | Fehmarn island `54.443,11.181` | B 207 / local Fehmarn, not the 18.9 km belt ferry | 20.0 (loop 21.5, close 35 m) | 1.01 | **19.0** | hop 3 SH `54.210,11.025 → 54.400,11.364` |
| spike | 360 | Præstø `55.12,12.02` | Næstvedvej (265) / town vs Farø E47 | 8.2 | 1.31 | **6.9** | hop 4 SH+DK `54.400 → 55.113` |
| spike+loop | 752–764 | Halland densify `57.57,12.25` | E6 / local Halland | 20.0 | 6.00 | **14.0** | hop 7 Halland `56.935 → 57.595` |
| spike+loop | 861 | VG/Halland coast `58.231,11.917` | E6 approach / Uddevalla coast | 20.3 | 5.94 | **14.4** | hop 8 Halland+VG `57.595 → 58.255` |
| spike+loop | 935 | Uddevalla `58.421,11.291` | local VG coast | 20.1 (loop 13.0, close 8.5 m) | 6.11 | **14.0** | hop 9 VG `58.255 → 58.444` |
| hair | 669.5 | Halland joint `56.936,12.497` | densify vertex out-and-back | 0.18 | ~0 | **0.18** | hop 6/7 Skåne→Halland |
| hair | 1286.2 | Ostlandet joint `60.795,11.068` | densify vertex out-and-back | 0.11 | ~0 | **0.11** | hop 12/13 |
| spike+loop | 1450–1461 | Otta `61.78–61.79,9.42` | E6 / Ottadalsvegen junction | 20.0 (loop 9.1, close 99 m) | 7.72 | **12.3** | hop 14 ostlandet `61.115 → 61.772` |
| spike | 1158 | Oslo densify `59.90,10.76` | local / E6 Oslo | 3.13 | 0.85 | **2.3** | hop 11 |
| spike | 1587–1596 | Sognefjell `61.50,7.7–7.8` | Rv 55 wiggle | 3.0+3.0 | 0.91+1.08 | **4.0** | hop 18 ostlandet+vestlandet extra |
| spike | 670 | Halland hair neighbourhood | E6 | 3.24 | 0.79 | (covered by hair) | hop 6 |

**~91 km** of the **177 km** ORS gap is geometric extra (loops/hairs/path≫chord), dominated by Fehmarn island, Zealand Præstø, Halland/VG coast, and Otta.

**Remainder ~86 km** is not a single missed road:

- **Ferry vs land:** Navi crosses the belt on a **13.919 km** unflagged water edge (`54.50709,11.23183 → 54.62456,11.30643` at km 262, hop 4). Overlay ferry is **~18.9 km**. That is not +5 km of extra vs ORS; ORS is the ferry. Island loop + wrong water edge together vs a clean A1/ferry/E47 line is a large share of the German–Danish gap.
- **Ottadal vs ORS Sjoa:** Otta→Vågå path **29.6 km** vs chord **19.4 km** (Otta loop). Vågå→Lom **29.8 vs 28.3 km** (almost the Rv 15 chord). Staying on Ottadalsvegen instead of a Sjoa/E6-south shortcut is a legal route-choice delta, not a spike table leftover of the same 91 km.
- **Graph / costing:** DATEX 0 this run; motorway share varies by hop (leg 1 `motorway_share_pct=22.94`); truck/car alias tiles; snap up to **3514 m** on Fehmarn densify.
- **Polyline vs app:** 1.8 km (measurement vs edge sum).

## b) Otta → Vågå → Lom / Rv 15

Yes: from Otta (`61.775,9.415`, km 1464) through Vågå via (`61.869,9.104`, km 1494) to Lom (`61.838,8.569`, km 1524) the polyline **never drops south of 61.77°** (Sjoa is ~61.70°). Vågå→Lom tracks Ottadalsvegen.

That is **not** an accident of the graph. HEAD densify **hard-codes** Otta `(61.772, 9.420)`, the Vågå via, and Lom `(61.838, 8.569)` in `norway_ottadal_westbound_anchors` (`plan_bbox.rs`) so the hop does not even-split south of Ottadalsvegen. Without those anchors the engine comment states A* leaves Rv 15. The Otta **loop** (9 km) is densify-joint geometry, not a Sjoa excursion.

## c) Ferry count: `is_ferry` on the belt vs `route_ferry_legs=0`

**Geometry:** one polyline edge **13.919 km** on the water (Puttgarden 268 m / Rødby 281 m in FU3 D). Overlay/pack probe (FU3 E): **~18.9 km** `is_ferry=true` crossings in **both** SH and DK **pack tiles and overlay sidecars**.

**Where the count is lost:** not per-chunk summing (each hop report already has `route_ferry_legs=0`; the chunked planner only adds those tokens). Not a missing SH/DK extra on hop 4 (`primary=schleswig-holstein extra=denmark`, `directed_ok=true`). The **chosen path used an unflagged duplicate** (~13.9 km) instead of the flagged ~18.9 km ferry. `path_ferry_legs` / `path_uses_ferries` only look at `edge.is_ferry` (`builder.rs`). Enumerations therefore stay 0.

**13.9 vs 18.9:** different edges (shorter water chord vs named ferry). Overlay ferry was loaded (`graph_ferry_edges` non-zero on other hops; belt overlay listed in Part E) but A* did not ride it.

**ETA / boarding:** ferry duration and boarding cost apply only to `is_ferry` edges. This crossing was costed as ordinary length, **not** as a ferry.

## d) HUD “4 days” vs 19.82 h

`days_json` (device `route-result.json`):

| day | distance_km | driving_hours | overnight |
|---|---|---|---|
| 1 | 478.4 | **6.0** | Quality Hotel View |
| 2 | 478.4 | **6.0** | Hotel Caprifol |
| 3 | 478.4 | **6.0** | Vertshuset Sinclair |
| 4 | 183.2 | **2.30** | (final) |

Driving time **20.3 h** (same order as the earlier finished plan **19.82 h** / 1187 min). HUD “4 days” is **car multi-day lodging splits** (`MotorDailyBudget` ~6 h/day), not 96 h of driving. `rest_hours=0` on those overnight rows (lodging, not a 45 min break). Soft-rest POIs are listed separately (`rest_place_count` enum 0 with named Autohof / rest / hotels in `rest_place_names`). **No ferry time** in the ETA (c).

## e) Dest-hop and plan wall-clock (no new plan)

Hop log has **no** `pack_load_ms` / `search_ms` / `hop_result=success` (that is the PASS skip fixed in Step 2a). `route-result.json` truncates `report` to 2000 chars (leg 1 only): `pack_load_ms=15376`, `snap_ms=2261`, `search_ms=271`, `expansions=84278`, `nodes=250046`, `pad_attempts=[0.35]`, `edge_clip=CorridorBand`, `tile_budget=14`.

Dest hop (sidecar): `61.617,8.044 → 61.443,7.461`, **9961 nodes**, `pack_hit=true`, `weak_ok/directed_ok=true`, primary ostlandet, extra vestlandet, snaps 32 m / 17 m. FU2 logcat on the **same 9961-node clip** (disconnected retries) showed `pack_load_ms=1391–1799` **repeated** (~6 loads). FU3 hop log has **one** dest-hop endpoints line (no pad-retry storm). A* on ~10k nodes should be **well under 1 s**; dest-hop wall-clock is **pack load + extra-stem merge + clip (~1.5–2 s) + snap + sub-second search**, not a 40 min search. The 40 min FU2 fail was expansions on a disconnected clip, not 10k A*.

Whole FU3 PASS: hop sequence 18 hops, one endpoints line each; no `hop_fail`. Phase mix from surviving leg-1 + FU2 analogue: **pack load dominates** (2–8 s typical, 15 s on the first 250k-node hop); **search is small** when connected; **no evidence of pad widening** on the successful dest hop.

## f) “Destination pack never becomes primary”

Still **true** on the successful dest hop: primary=`ostlandet-latest`, extra=`vestlandet-latest` only. It **did not** prevent PASS: same weak component, `directed_ok=true`, path found.

FU2 `corridor_disconnected` after Vågå was **not** “vestlandet never primary” as a hard invariant. It was a **too-thin ostlandet clip** (9961 nodes, extra missing or unused, 2.9e6 expansions) plus a **double `am start`**. FU3 with vestlandet extra on that hop connected the component. Preferring dest as primary remains a robustness improvement (cleaner tile band), not the explanation of FU2 vs FU3.

## Step 4 — densify recommendation (do not implement)

Keep **(a) joints as cuts** and **(c) path≫chord repair**. Do **not** add more place-specific anchors.

| spike / hair / loop | (a) cut at joint | (c) path≫chord repair |
|---|---|---|
| Fehmarn island 19× / 21.5 km loop | Cut **after** Fehmarn east / before water so hop 3 cannot circuit the island | **Primary:** replace island tour with chord along B 207 / ferry approach |
| Præstø 6.9 km | Cut on Farø / E47 (HEAD already wants this; this polyline still parked at 55.11) | Repair Næstvedvej loop if a joint still lands in town |
| Halland hair 56.936 | **Primary:** treat densify vertex as a cut, do not reverse | Tiny |
| Halland 14 km E6 window | Cut at 57.595 | Repair local E6 service/loop |
| VG coast 14.4 + Uddevalla 14.0 / 13 km loop | Cut at 58.255 / 58.444 | **Primary:** coast loop vs E6 chord |
| Ostlandet hair 60.795 | **Primary:** cut | Tiny |
| Otta 12.3 / 9.1 km loop | **Primary:** cut at Otta so hop 14 cannot loop the E6/Rv15 junction | Repair leftover out-and-back |
| Oslo 2.3 km | Cut at 59.91 | Optional |
| Sognefjell 4 km | Leave Rv 55; optional (c) on 3 km windows | Optional |

### Implementation plan (wait for approval)

1. **Cut semantics:** a densify joint is a hop terminal, not a vertex the path may reverse through. Test: FU3 hairs at `56.93600,12.49705` and `60.79531,11.06810` absent on this polyline; same for `elsa-sjuvass.geojson` / `breneriroa-aga.geojson` joint-like 180° turns if present.
2. **Fehmarn (c):** if path/chord ≥ 2.5 on a 20 km window and the window sits on Fehmarn AABB, replace with the coarse highway/ferry skeleton. Test: FU3 window at km 232 ratio drops below 2.5; `bad-luster.json` ferry geometry unchanged.
3. **Coast/Otta (c):** generic repair when path ≥ 2.5 × chord on 8–20 km windows after a connected path exists. Test: Uddevalla km 935 and Otta km 1450 extras fall; `roa-florø.json` must not lose the fjord ferry if flagged `is_ferry`.
4. **No new named anchors.** Test: `densify_bevensen_vagaa_*` still pass; hop list must not gain Præstø-class points.
5. **Regression pack:** rebuild polylines only after approval; compare extra-km vs this FU3 table and vs ORS 1440.985 (gap should fall by ~the repaired extras, not by forcing Sjoa).
