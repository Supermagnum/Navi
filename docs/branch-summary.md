# `dev` after merge of `wip/fu3-plan-diag-index-snapshot`

Merged in follow-up 58 with a merge commit. History is kept. Screenshot proof
sets (`docs/fu4*-map`, `docs/fu5*-map`, `docs/fu5*-tablet`), parked patches and
APK files stay on the wip branch only. The wip branch is not deleted.

## What changes for the user

**Routing.** Multi-region plans use the corridor (Stage B) and on-device skeletons. Waypoints attach by a detailed search. Stretch-split joints are hints, so Elsa no longer spikes at Bromma. A stretch whose clipped graph fits 900k nodes and the 1400 MB planning limit (12 % margin) is one detailed search; stretch repair tries Direct first, then merge, then slide. A short dense route keeps intervening tiles. A new plan cancels the old one, clears the plan log, and resets the corridor flag. Idle pack jobs pause while a plan runs. DATEX live documents are compacted before the first hop loads.

**Place index.** One shared database on the pack volume. Regions index from the pack server's place-source (labelled place-source, not own-extract). Search goes through the app. The ready state comes from the database, not a stamp file.

**Map.** Local vector archives mount as native MapLibre sources (no loopback HTTP). The world overview downloads once in the background; first start is not a blank screen. Several archives can draw in one view; the online map fills the rest when the network is on. Each mount set gets its own style file so MapLibre actually loads the new archives.

## Known failures and open issues

| Item | Issue | Notes |
|---|---|---|
| Neighbour country is flat fill at zoom 5 and below (Sweden from Oslo, Poland from Hamburg). The `earth` layer is what is visible. Online-alone or overview-alone did not change the picture. | [#152](https://github.com/Supermagnum/Navi/issues/152) | Open question: is this how the planet data looks at that zoom? Do not fix until a reference viewer and z5 tile contents are compared. Seen again on the tablet at Hamar z3/z5. |
| Elsa overview z15 (64.889, 19.516) is forest with almost nothing to draw. Umeå is the Swedish-town test and passes. Finland can still mount as a second source. | [#153](https://github.com/Supermagnum/Navi/issues/153) | |
| Place search always queries OpenStreetMap Nominatim when the network is up. | [#154](https://github.com/Supermagnum/Navi/issues/154) | Fixed in follow-up 61: opt-in setting (off by default), submit-only, limiter, attribution, configurable URL. |
| Place-index compact after a schema migration, when idle. The v5 to v6 migrate left `place_index.db.bak-pk-v6` plus dead pages. | [#155](https://github.com/Supermagnum/Navi/issues/155) | Implemented: an idle job writes a `VACUUM INTO` copy beside the live file, verifies it, swaps it in, and deletes the backup only when per-region row counts match. Needs Android 11+ for the rewrite. Not yet measured on the tablet (follow-up 56 baseline: 4.0 GB live file vs 2.2 GB v4 backup). |
| Place index keeps `name_entries` for regions that are not installed. Search does not query them (`PlaceIndexReady` is downloaded ∪ ready only). | [#156](https://github.com/Supermagnum/Navi/issues/156) | Tablet extras: denmark 2,842,083; niedersachsen 2,889,457; detmold 673,511; schleswig-holstein 834,504; halland 1,513,764. The halland id holds the whole of Sweden, not Halland county (about 31,000 rows). |
| Elsa last hop: stretch repair can only merge or slide. | [#151](https://github.com/Supermagnum/Navi/issues/151) | Closed. Last stretch is one Direct search (136.448 km). Elsa 2159.8 km / 1531 min / 6 hops. |
| Oslo to Lillestrøm needs the corridor stage and about 1.1 GB for 22 km. | (closed) | 22.4 km direct search, corridor skipped, host 228 MB. |
| Elsa distance vs the 1944.2 km reference. | (known miss, not a gate fail) | Planned 2159.8 km / 6 hops / 0 ferries. |

The gate still lists `low_zoom_mint_fill` and `elsa_overview_z15` as known tests outside the route cases.

## Debug-only switches

None are on unless set. Release APKs ignore the trip extras (`NaviDebugIntent` returns null when the APK is not debuggable). Defaults for `NaviMapTestHooks` and `Fu49MapDiag` are off. Listed on `docs/tablet-test.md`: trip seed, `navi_auto_plan`, `navi_long_trip`, profile, `navi_graph=pbf`, avoid/eco/DATEX, GPS inject, cabin filters, `navi_force_online_basemap`, `navi_force_basemap_source`, chrome hide, camera, search, extract, place-index harness, splash hold, DEM/hillshade test hooks.

Also used only by the harness: `navi_fu49_force_offline` and the other `navi_fu49_*` mount/style probes. In-memory hooks reset when the process dies. Do not leave `force_online` or a forced source on for a real session.

## Tablet (SM-P613, R52TB0JQEDE)

Install was `adb install -r` of the current debug APK. App data, packs and the place index were not cleared. After the follow-up 57 map pass, sqlite3 and the other files placed under the app data dir and `/data/local/tmp` were removed. The app stayed running; `quick_check=ok`; row counts per region unchanged.

Installed pack regions on that device: Østlandet, Sørlandet, Vestlandet, Trøndelag. Search uses those only.

Visual table: `docs/fu57-tablet/table.md`. Screenshots stay on the wip branch.

DATEX-none Raufoss → Bergen station (debug extras 60.727748,10.610940 → 60.388114,5.333857): 428.494 km / 344 min, same polyline as the host. DATEX-live wall 1045 s vs none 1128 s, both under the 1400 MB planning peak.

## Gate results (follow-up 58)

Each host case runs in its own process and records that case's own peak (follow-up 44). Case l is 428.494 km / 344 min / 666.4 MB stored; a later isolated run at 714 MB is inside the 25 % limit.

**Host** (isolated a–o, all PASS):

| Case | km | min | wall s | peak MB |
|---|---:|---:|---:|---:|
| a Bevensen–Vågå–Dalsøren | 1440.7 | 982 | 31.1 | 875 |
| b Brenneriroa–Aga | 375.3 | 346 | 5.0 | 565 |
| c Brenneriroa–Grotli–Florø | 528.7 | 420 | 4.9 | 654 |
| d Bevensen avoid ferries | 1586.8 | 955 | 46.2 | 974 |
| e Elsa–Sjuvass | 2159.8 | 1531 | 21.0 | 846 |
| f Hamar–Stange | 16.2 | 13 | 0.3 | 160 |
| g Hamar–Lillehammer | 60.8 | 44 | 1.1 | 262 |
| h Oslo–Lillestrøm | 22.4 | 19 | 2.1 | 228 |
| i Hamburg–Bergedorf | 19.5 | 20 | 0.9 | 144 |
| j Copenhagen–Taastrup | 22.2 | 19 | 1.1 | 278 |
| k 60.27656,10.81650–Sjuvass | 136.4 | 123 | 2.7 | 603 |
| l Raufoss–Bergen station | 428.5 | 344 | 5.1 | 714 |
| m Oslo–Lillehammer | 168.9 | 135 | 6.8 | 956 |
| n Hamar–Oslo | 130.1 | 81 | 5.8 | 823 |
| o Lillehammer–Gjøvik–Hønefoss | 137.9 | 108 | 1.5 | 208 |

**Emulator** (one debug install, process 4661 throughout; place index `quick_check=ok`, 3.09 GB):

- Search includes Oslo, Hamar, Lillehammer (top hit). Display: hop 0.042 s, border pan 0 empty frames, plan-must-not-blank 0 % before and during (visible 6925).
- e Elsa: 2159.825 km, 1531 min, 6 hops, 94.1 s, planning peak 1259 MB (limit 1400; 1259/1241 = 1.015).
- h Oslo–Lillestrøm: 22.427 km, 19.1 min, 0 hops, 9.1 s, 870 MB.
- k 60.27656,10.81650–Sjuvass: 136.448 km, 123.4 min, 0 hops, 9.1 s, 765 MB.
- m Oslo–Lillehammer: 168.936 km, 134.9 min, 0 hops, 24.3 s, 1182 MB.

Emulator kilometres match the host for e, h, k and m.

## CI

Automatic checks (push and pull request, [`.github/workflows/ci.yml`](../.github/workflows/ci.yml); skips docs, markdown, geojson-routes, and gate refs): `rust-fmt`, `rust-lib-tests`, `android-native-compile`, `kotlin-fast`, `android-app-compile`. Jobs are independent. Warm-cache wall is about 3 minutes (run 38094560779 rerun). Budget is 10 minutes. Cold first-green times were `rust-lib-tests` 435 s, `kotlin-fast` 175 s, `android-app-compile` 266 s, `android-native-compile` 268 s.

Manual (Actions → **CI manual** → Run workflow, [`.github/workflows/ci-manual.yml`](../.github/workflows/ci-manual.yml)): Clippy, workspace tests, plugin-host isolation, deny, audit, Linux workspace build, detekt, plugin-host Android aarch64 smoke, and the old regression-guards Rust integration tests (mini PBF). Not a required merge check. The instrumented suite stays on `android-instrumented.yml` (dispatch only).

Weekly (Monday 06:00 UTC, same **CI manual** workflow, `schedule` only): Clippy, `cargo deny`, and `cargo audit`. Does not run on push or pull request, does not block merges or the automatic `CI` workflow, and does not run the regression gate, the emulator, or the workspace build. A red weekly run is visible in the Actions tab as **CI manual**.

The [regression gate](regression-gate.md) is local and required before merging routing or map changes.
