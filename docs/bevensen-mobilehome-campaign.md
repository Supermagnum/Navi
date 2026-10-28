# Bad Bevensen to Dalsoren MobileHome campaign

Emulator **Navi_8c_4G_128G** (`emulator-5554` only) ran with **6 cores** (guest `nproc` 6, CPUs online `0-5`). That is the emulator maximum, instead of the AVD's configured 8. Tablet R52TB0JQEDE was not used. AVD `Navi_8c_4G_128G_old` was not touched.

APK for this attempt: commit **`44057d4ee6e2475b8314ef677e426f11a7c957d2`**, SHA-256 **`7ad3297d20f5472d0230aef32120e06411c885c5769f829bd55674a4770b73ca`** (`app-debug.apk`, 43878485 bytes, x86_64). Device `pm path` hash matched that value at plan click. `lastUpdateTime=2026-10-28 06:17:42`. Catalog generation **`20261028T040556Z-2275792-d248bf27`** (unchanged; `europe/germany/schleswig-holstein` still published). Puttgarden coverage was not re-probed because the catalog id did not change.

This was started as a clean run from deleted regions. **No app or plugin code was changed after that APK was installed.** The run did not finish a route.

## Stop point

`2026-10-28 06:21:01` logcat `NaviRouting: planning_refused reason=region_missing message=53.07969, 10.58720 is not in a downloaded region`.

The campaign test then sat in its progress loop with `Long trip off (installed packs kept)`, `corridor_ready=false`, `polyline_chars=0`, `distance_km=0`. The instrumentation wait was stopped after that refusal so it would not sit until the 20 hour deadline. The app was force-stopped. No data wipe.

The UI showed a dialog: "Map data needed. Germany is not downloaded." Buttons: **Not now** and **Download Germany**. Download Germany was not pressed. Manual region download is not part of this run. That dialog is not a long-trip re-plan.

## Why long trip was off

The harness turned long trip off for the Tools delete, then clicked it on (`prefsOn` became true). Immediately before Plan the status line was still `Long trip off (installed packs kept)` while `long_trip_enabled` was already true. The harness treats any status line containing "off" as a reason to click the toggle. That click turned the pref **off**. After the run, `long_trip_enabled=false`. Plan was clicked in that state, so the app refused the origin instead of auto-downloading the corridor.

## CAT (not changed by this run)

Update was not pressed. The previous Update ANR is why the UI-thread hang was fixed in `44057d4ee` before this install; this run did not need Update. Drop folder `Android/data/no.navi.app/files/repeaters/` still has the seven source KML files plus `README.txt`: `Sachsenlink.kml`, `denmark.kml`, `funknetz.kml`, `germany.kml`, `kaxi.kml`, `repeaters_2610270617.kml`, `repeaters_sweden.kml`. The source `klm/` directory has those same seven KML files. `cat_drop_imported=true`. Prefs left as found: `cat_plugin_enabled=true`, `cat_network_choice=auto`, `cat_rig_model=1`, `cat_backend=hamlib`. No CAT sheet, Connect, Disconnect, Refresh, or `cat_*` writes.

## Regions before and after

Before any wipe, SD `/storage/BF1B-9F0E/Android/data/no.navi.app/files/long-trip-packs` was **16G** (`du -sh`). File-byte sums from `ls`:

| Stem | Bytes |
|---|---|
| niedersachsen | 3523203885 |
| denmark | 3464017696 |
| ostlandet | 3136425019 |
| vestlandet | 1501566363 |
| place_index.db | 1437216768 |
| schleswig-holstein | 1038217774 |
| vastra_gotaland | 1011594923 |
| sweden | 942926646 |
| skane | 489086264 |
| sorlandet | 405542552 |
| halland | 192215922 |
| elevation directory | about 122M (`du -sh`, not in the flat file sum) |

Internal `files/long-trip-packs` was empty (8.0K directory).

A UI delete pass that started on the previous binary had already removed Denmark and Niedersachsen before this APK's test began (directory about 10G, PBFs left: ostlandet, schleswig-holstein, sorlandet, sweden, vestlandet). The test on this APK then deleted through Tools. Sorlandet and Hedmark failed because `field_geofabrik_path` was gone (`Failed to perform text input`).

After the test, SD still had `sorlandet-latest.osm.pbf` and Sorlandet graph, ferry, wetland, and manifest files, plus `ostlandet-latest.navi-corridor-skeleton.bin` and `.meta`, plus `elevation`, `graph-cache`, `navi.db`, `place_index.db`, `place-index-ready.json`, `place_index_quick_check.txt`, `region_meta.json`. Internal `files/long-trip-packs` was empty. The UI did not show an empty installed-region list; it showed the Germany download dialog. The wipe was not complete.

Host disk at start and at stop: `/` 40G free, data drive 31G free. Neither dropped below 10G. No files were deleted to make room.

GPS: `adb -s emulator-5554 emu geo fix 10.587198 53.079686` (longitude, latitude). Map camera at refusal was about 53.07969 N, 10.58720 E. No map swipe, drag, pinch, or fling.

DATEX: six synthetic MaintenanceWorks penalties (DE, DK, SE, SE, NO, NO) were written to `files/datex_cache/datex-GetSituation.xml` (15747 bytes) after the harness marker `BEVENSEN_AWAIT_DATEX`. The first copy was 0 bytes because `run-as` cannot read `/sdcard`. A second copy via `/data/local/tmp` landed before the refusal was final, but the planner had already refused for `region_missing` and did not apply those penalties. The harness had also seen a pre-existing 28116457-byte XML and continued; that file was then replaced.

## Report fields

1. Regions downloaded, in order, with download, processing, and indexing times, size, and SD confirmation: **not available**. No corridor download started. Plan refused because the origin was not in a downloaded region and long trip was off. Expected order was not executed. Leftover Sorlandet files are not a download from this run.

2. Total length, and kilometres per day: **not available**. No route. Distance band 1461.3-1648.6 km was not measured. **Out of band is not claimed; there is no distance.**

3. Total duration: **not available**. No route. The 17-22 h band was not measured. Below 17 h was not observed and is not reclassified.

4. Navigation instructions and breakdown: **not available**. No maneuver list. The 55-100 band was not measured.

5. Ferries, including whether Rodby-Puttgarden was used: **not available**. No route.

6. Tunnels: **not available**. No route.

7. Nearby attractions: **not available**. No route. `poi_lookahead covering load failed ... Missing` at the origin.

8. Wild camping overnights: **not available**. No route. Count of sites used is not greater than 0 because none were selected. Candidates accepted or rejected were not produced.

9. Rest places: **not available**. No route.

10. Estimated fuel stops: **not available**. No route distance to apply the 70 L full tank, 100 km margin, or the consumption table (calm 8.5-9.5, mixed 9.5-11, motorway/mountains 11-14 L/100 km).

11. DATEX reroutes and resulting route changes: **not available** as route changes. Six synthetic situations were in the cache file (DE A7 chord, DK Great Belt approach, SE Halland E6, SE Bohuslan coast, NO E6 Ostlandet, NO toward Ottadal) with a 900 s delay penalty in the XML. The planner did not apply them.

12. RAM: campaign-window peak TOTAL PSS **327434 KB** at `2026-10-28T06:18:47+01:00` (monitor every 60 s). The monitor file also has an earlier peak of 480490 KB at 06:03:46 from the pre-run Update ANR, not this plan. Planner `vm_hwm_mb`: **not available**. No corridor plan ran.

13. Host disk: start and end both `/` 40G free and data drive 31G free (of 801G and 855G).

14. Repeaters along the route: **not available**. No route, and Update was not pressed. Drop-folder file count is 7 KML files. `cat_drop_imported=true`. No OSM or RadioID callsign is listed because none were selected along a route.

15. Comparison with `bad-luster.kml` and overlay map image: **not available**. No polyline, so no divergence, distance delta, time delta, ferry or tunnel difference, or overlay image.

16. Cover-gate Stage-B (`3ba01455c`): **did not trigger**. Planning stopped at `planning_refused reason=region_missing` before a hop end was tested. No hop end was outside Ready coverage on this attempt.

17. Re-plans and retries:
    - `06:17:52` instrumentation of `LongTripMobileHomeBevensenDalsorenUiCampaignTest` crashed before Plan: `UiAutomationService already registered` (a previous `uiautomator` session still held the connection). No route. Same APK was not rebuilt.
    - `06:18:24` the same test was started again on the same APK and reached Plan.
    - `06:21:01` Plan refused (`region_missing`). The Germany download dialog was not used. No same-APK UI re-plan.

## Bands

| Band | Result |
|---|---|
| Distance 1461.3-1648.6 km | not measured |
| Duration 17-22 h | not measured |
| Instructions 55-100 | not measured |
| Wild camping overnights > 0 | not met (no route) |

## Monitors

`/tmp/clean-bevensen-run/logcat.txt`, `meminfo.txt` (60 s), and `df.txt` (10 min) were already running and were left running. They are read-only.
