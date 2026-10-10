# Branch `wip/fu3-plan-diag-index-snapshot` (for a later merge into `dev`)

Do not merge this follow-up. Product map code is still `535ab13b`. Later commits are harness, tablet page, proof screenshots, and this page. Part B is parked at `docs/fu51-parked/` and is not in the tree.

## What changes for the user

**Routing.** Multi-region plans use the corridor (Stage B) and on-device skeletons. Waypoints attach by a detailed search. Stretch-split joints are hints, so Elsa no longer spikes at Bromma. A stretch whose clipped graph fits 900k nodes and the 1400 MB planning limit (12 % margin) is one detailed search; stretch repair tries Direct first, then merge, then slide. A short dense route keeps intervening tiles. A new plan cancels the old one, clears the plan log, and resets the corridor flag. Idle pack jobs pause while a plan runs.

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
| Elsa last hop: stretch repair can only merge or slide. | [#151](https://github.com/Supermagnum/Navi/issues/151) | Closed. Last stretch is one Direct search (136.448 km). Elsa 2159.8 km / 1531 min / 6 hops. |
| Oslo to Lillestrøm needs the corridor stage and about 1.1 GB for 22 km. | (tablet page; no issue) | Closed in the same change: 22.4 km direct search, corridor skipped, host 228 MB. |
| Elsa distance vs the 1944.2 km reference. | (known miss, not a gate fail) | Planned 2159.8 km / 6 hops / 0 ferries. |

The gate still lists `low_zoom_mint_fill` and `elsa_overview_z15` as known tests outside the route cases.

## Tested only on the emulator

Map screenshots and visual verdicts (including `docs/fu51-map/head/`), the in-app search check, hop / border-pan / plan-must-not-blank, and the Elsa emulator plan wall and peak. The host gate is host-side planning only. No tablet hardware run is recorded here.

## Debug-only switches

None are on unless set. Release APKs ignore the trip extras. Listed on `docs/tablet-test.md`: trip seed, `navi_auto_plan`, `navi_long_trip`, profile, `navi_graph=pbf`, avoid/eco/DATEX, GPS inject, cabin filters, `navi_force_online_basemap`, `navi_force_basemap_source`, chrome hide, camera, search, extract, place-index harness, splash hold, DEM/hillshade test hooks.

Also used only by the harness: `navi_fu49_force_offline` and the other `navi_fu49_*` mount/style probes. In-memory hooks reset when the process dies. Do not leave `force_online` or a forced source on for a real session.

## Gate results

**Host** (follow-up 54, unified pad and 1400 MB direct-search cap): all of a–k PASS. a–d identical to the previous figures (1440.7 / 375.3 / 528.7 / 1586.8 km). e 2159.8 km, 1531 min, 6 hops, 20.5 s, 903 MB (corridor ran; last stretch Direct). f–k corridor skipped. h 22.4 km / 19.1 min / 2.2 s / 228 MB. j 22.2 km / 19.0 min / 1.2 s / 278 MB. k 136.4 km / 123.4 min / 2.4 s / 606 MB. Elsa vs 1944.2 km remains a known miss.

**Emulator** (one APK install, no force-stop; process 21033):

- Search: 8/8 (Oslo, Hamar, Lillehammer, Luleå, Kiruna, Piteå, Falun, Mora). Place index `quick_check=ok`, 2.88 GB.
- Map off and on: 44/44 settled. Display: hop 0.05 s, border pan 0 empty frames, plan-must-not-blank 0 % before and during.
- e Elsa: 2159.8 km, 1531 min, 6 hops, corridor ran, 100.4 s, planning peak 1241 MB (limit 1400).
- h Oslo–Lillestrøm: 22.4 km, 19.1 min, corridor skipped, 9.2 s, 815 MB.
- k 60.27656,10.81650–Sjuvass: 136.4 km, 123.4 min, corridor skipped, 9.1 s, 901 MB.

Tablet build on disk (not in git): `docs/fu51-map/head/app-debug.apk`, sha256 `84a953d072831a2b248be4e2b7e2975ef1eae2d16f0348394e7b4a20b0781d1f`.
