# Branch `wip/fu3-plan-diag-index-snapshot` (for a later merge into `dev`)

Do not merge this follow-up. Product map code is still `535ab13b`. Later commits are harness, tablet page, proof screenshots, and this page. Part B is parked at `docs/fu51-parked/` and is not in the tree.

## What changes for the user

**Routing.** Multi-region plans use the corridor (Stage B) and on-device skeletons. Waypoints attach by a detailed search. Stretch-split joints are hints, so Elsa no longer spikes at Bromma. A short dense route keeps intervening tiles. A new plan cancels the old one, clears the plan log, and resets the corridor flag. Idle pack jobs pause while a plan runs.

**Place index.** One shared database on the pack volume. Regions index from the pack server's place-source. Search goes through the app. The ready state comes from the database, not a stamp file.

**Map.** Local vector archives mount as native MapLibre sources (no loopback HTTP). The world overview downloads once in the background; first start is not a blank screen. Several archives can draw in one view; the online map fills the rest when the network is on. Each mount set gets its own style file so MapLibre actually loads the new archives.

## Commits by topic (from `origin/dev`)

- **Corridor / Stage B:** `641b213e` … `dc0f9e12` (skeletons, attach, hop joints, Bromma slide, Elsa baseline).
- **Place index:** `820c5d9c`, `e621e401`, `1297ea56`, `10bc6a8e`, `12f3386d`, `b5d9c330`.
- **Map:** `ac075157`, `a5e64eeb` … `535ab13b` (native PMTiles, overview, style file per mount).
- **Gate / tablet / proof:** `4df71977` … `ea0b98e2`, then `d8eb5493` (settled map check), `83efd8da` (tablet page), `31429aa5` (proof set). This page is after those.

## Known failures and open issues

| Item | Issue | Notes |
|---|---|---|
| Neighbour country is flat fill at zoom 5 and below (Sweden from Oslo, Poland from Hamburg). The `earth` layer is what is visible. Online-alone or overview-alone did not change the picture. | [#152](https://github.com/Supermagnum/Navi/issues/152) | Open question: is this how the planet data looks at that zoom? Do not fix until a reference viewer and z5 tile contents are compared. |
| Elsa overview z15 (64.889, 19.516) is forest with almost nothing to draw. Umeå is the Swedish-town test and passes. Finland can still mount as a second source. | [#153](https://github.com/Supermagnum/Navi/issues/153) | |
| Elsa last hop: stretch repair can only merge or slide. | [#151](https://github.com/Supermagnum/Navi/issues/151) | |
| Oslo to Lillestrøm needs the corridor stage and about 1.1 GB for 22 km. | (tablet page; no issue) | |
| Elsa distance vs the 1944.2 km reference. | (known miss, not a gate fail) | Planned ~2253 km / 7 hops / 0 ferries. |

The gate still lists `low_zoom_mint_fill` and `elsa_overview_z15` as known tests outside the route cases.

## Tested only on the emulator

Map screenshots and visual verdicts (including `docs/fu51-map/head/`), the in-app search check, hop / border-pan / plan-must-not-blank, and the Elsa emulator plan wall and peak. The host gate is host-side planning only. No tablet hardware run is recorded here.

## Debug-only switches

None are on unless set. Release APKs ignore the trip extras. Listed on `docs/tablet-test.md`: trip seed, `navi_auto_plan`, `navi_long_trip`, profile, `navi_graph=pbf`, avoid/eco/DATEX, GPS inject, cabin filters, `navi_force_online_basemap`, `navi_force_basemap_source`, chrome hide, camera, search, extract, place-index harness, splash hold, DEM/hillshade test hooks.

Also used only by the harness: `navi_fu49_force_offline` and the other `navi_fu49_*` mount/style probes. In-memory hooks reset when the process dies. Do not leave `force_online` or a forced source on for a real session.

## Gate results

**Host** (last run on `535ab13b`, not repeated this follow-up): `HOST:0`, about 268 s. Cases a–j passed. The two known map tests were listed and were not treated as route failures. Elsa distance remains a known miss.

**Emulator** (this follow-up, APK `docs/fu51-map/head/app-debug.apk` / `535ab13b`, no force-stop; process 10003 for the run):

- Search: 8/8 (Oslo, Hamar, Lillehammer, Luleå, Kiruna, Piteå, Falun, Mora).
- Map off and on: every gate start/via/end settled with features, including Taastrup online (`visible=496`). The earlier 2.2 s blank count is gone.
- Display: hop 0.05 s, border pan 0 empty frames, plan-must-not-blank 0 % before and during.
- Elsa: `accepted=true`, 2253.3 km, 0 ferries, 7 hops, 109.6 s, planning peak 1116 MB (limit 1400), process peak 1402 MB. Place index `quick_check=ok`, 2.88 GB.

Tablet build on disk (not in git): `docs/fu51-map/head/app-debug.apk`, sha256 `bdb350c5f3c68bb301f331055a13d43f5b661c1f587a1be2f1815e1345027b8d`.
