# Tablet test (debug APK)

Build from this branch: `app/build/outputs/apk/debug/app-debug.apk` (64 MB,
arm64 + x86_64). sha256
`b55c93dcfcfe11da202625240dcb6b934ce7781ca582bca556ff5e09206d9b43`.

Install this debug APK over an existing **debug-signed** `no.navi.app` with
`adb install -r app-debug.apk` (or the file manager “update”). Same application
id; packs, maps, place index and settings stay on disk. A **release / Play**
install cannot be overwritten by this APK (different signing key). Do not
uninstall. Copy `Documents/debug/`, `Android/data/no.navi.app/` and the SD pack
volume first if you must switch keys.

## Regions for a first test

Download **Østlandet**, **Vestlandet** and **Värmland**. Figures are from the
emulator after a real pack-server install (not estimates). The place index is
one shared file for every region.

| Region | Packs (graph + POI + wetland) | Map archive (.pmtiles) | Place-source / extract | Index | Skeleton | Ferry sidecar |
|---|---|---|---|---|---|---|
| Østlandet | 2.4 GB graph/POI/wetland (2.9 GB folder with extract) | 1.16 GB | own extract 434 MB still on disk | shared `place_index.db` 2.88 GB (1.49 M rows here) | 13 MB | 51 MB |
| Vestlandet | 1.1 GB graph/POI/wetland (1.4 GB folder with extract) | 704 MB | own extract 246 MB | same DB (0.79 M rows) | 8.4 MB | 42 MB |
| Värmland | 257 MB graph/POI/wetland (262 MB folder) | 203 MB | place-source used then discarded; listed extract 37.5 MB | same DB (53 k rows) | 0.9 MB | 4.7 MB |

Budget about **8 GB** for these three plus the shared index. A full Norway +
several Swedish län on the emulator used ~16 GB of map archives alone.

## What the app shows while jobs run

Status chip / toast, in order:

1. `Downloading europe/norway/ostlandet…` then percent, or `Resuming download…`
2. `Downloading extract for place index…` (only when the pack has no place-source)
3. `Downloading basemap (PMTiles)…` — extract can take **15–40 min** for Østlandet
4. Idle queue: `Running ferry sidecar (car) for …`, then truck, then
   `corridor skeleton`, then `place index`. Typical **2–10 min** per sidecar or
   skeleton; Østlandet place index **tens of minutes**. Jobs pause with
   `Idle pack jobs paused (planning)…` if you plan during a job.

Leave the app in the foreground on Wi‑Fi. Planning is allowed before tiles
finish; search needs the place index.

## Three routes to try

Pick the named place (city/town), not a farm of the same name.

1. **Short (one region):** Oslo (`place:city` 59.91333, 10.73897) to Lillestrøm
   (`place:town` 59.95592, 11.04911). About 22 km on the urban Østlandet tiles.
2. **Within Norway:** Hamar (`place:city` 60.79472, 11.06806) to Lillehammer
   (`place:town` 61.11455, 10.46701). About 60 km on the E6.
3. **Across a border:** Kongsvinger (`place:town` 60.19093, 11.99868, Østlandet)
   to Arvika (`place:town` 59.65436, 12.59162, Värmland).

Look at: the route on the **roads next to those places** (not a snap onto a
distant highway); no unexplained out-and-back; the map filled under the camera
(one offline archive is mounted, chosen by the camera centre — a view that
spans two regions can be half empty); tiles still visible while a plan runs.

## If something goes wrong

**Plan log (no adb):** Tools → Diagnostic logging on, plan, then copy
`Internal storage / Documents / debug / navi_session_*.log` over USB. Share
sheet: Tools → Export diagnostic log.

**InstalledMaps summary:** after opening the app, the file
`Android/data/no.navi.app/files/installed-maps-snapshot.txt` (app-private; USB
file transfer or `adb pull`). Same text is in logcat under `InstalledMaps`.

## Debug-only switches

None are on unless you set them. Release APKs ignore the trip extras.

| Extra / hook | Default | What it does |
|---|---|---|
| `navi_from_*` / `navi_to_*` / `navi_via{1-4}_*` | unset | Debug trip seed |
| `navi_auto_plan` | true if a trip is sent | Plan immediately |
| `navi_long_trip` | true if a trip is sent | Allow download-along-route |
| `navi_profile` / `navi_bike_capability` | unset | Override profile |
| `navi_graph=pbf` | pack | Force a cold PBF graph |
| `navi_avoid_ferries` / `navi_eco` / `navi_datex` | unset / live | Plan flags |
| `navi_inject_gps` | true if a trip is sent | Pin GPS at from |
| `navi_use_networked_cabins` / `navi_use_unlocked_cabins` | defaults | Cabin filters |
| `navi_restore_settings` | true | Restore after the debug plan |
| `navi_force_online_basemap` | false | Skip local PMTiles |
| `navi_force_basemap_source` | unset | Pin one archive |
| `navi_clear_basemap_test_hooks` | unset | Clear the two above |
| `navi_hide_chrome` | false | Hide chrome (cleared on normal launch) |
| `navi_camera_*` | unset | Frame the map (also disables GPS follow) |
| `navi_search_q` | unset | Run one search |
| `navi_extract_pmtiles` | unset | Queue a map extract |
| `navi_place_index_pbf` / `_region` / `_db` / `_clear_region` | unset | Harness index jobs |
| `navi_keep_splash` | false | Hold the splash for capture |
| `NaviMapTestHooks.localDemMapboxConversion` | false | Legacy DEM encoding |
| `NaviMapTestHooks.hillshadeExaggerationOverride` | unset | Test hillshade |

In-memory hooks reset when the process dies. Do not leave `force_online` or a
forced source on for a real tablet session.
