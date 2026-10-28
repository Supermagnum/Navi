# Bad Bevensen to Dalsoren MobileHome campaign

## Latest — 2026-10-28 06:38 (retry not started; no route)

Emulator **Navi_8c_4G_128G** is running with **6 cores** (guest `nproc` 6, CPUs online `0-5`, emulator maximum) instead of 8. It was not relaunched. Tablet R52TB0JQEDE was not used.

Frozen app binary, not rebuilt and not reinstalled for this retry:

- Commit `44057d4ee6e2475b8314ef677e426f11a7c957d2`
- APK SHA-256 `7ad3297d20f5472d0230aef32120e06411c885c5769f829bd55674a4770b73ca` (confirmed on device at 06:38)
- Catalog `20261028T040556Z-2275792-d248bf27` (unchanged; Puttgarden still covered). Not re-fetched beyond confirming the id.

No app or plugin code was changed. No instrumentation was started. No route exists, so this is not a clean completed run from deleted regions.

### Stop point

2026-10-28 06:38. The already-installed `no.navi.app.test` package still contains `updateRepeatersViaCatUi`: it opens the CAT sheet and clicks Update. The committed test source does not. Installing a CAT-free test APK was refused, because this retry must use the already installed frozen app and must not reinstall. Starting the installed test would press Update, which is forbidden. Exactly one instrumentation was required, and none was launched.

Read-only prefs at that moment: `cat_plugin_enabled=true`, `cat_network_choice=auto`, `cat_rig_model=1`, `cat_backend=hamlib`, `cat_drop_imported=true`, `long_trip_enabled=true`, `long_trip_pack_volume_id=uuid:BF1B-9F0E`. CAT was not opened and no cat_* pref was written.

### Previous attempt (superseded)

Report `26a6fa3d0` is superseded. That attempt was killed at 2026-10-28 06:34:39.855 by a second `am instrument` (`stop no.navi.app`) before a route existed. Last progress: Niedersachsen Indexed, Sorlandet Indexed, the other corridor regions still Downloading, `distance_km=0`. That second process then opened the CAT sheet and clicked Update at 06:35:10. It was stopped. It was not a route re-plan.

### Regions at the moment the retry stopped

SD `/storage/BF1B-9F0E/Android/data/no.navi.app/files/long-trip-packs` was **10G**. Visible PBF files: niedersachsen 485M (06:32), schleswig-holstein 152M (06:36), sorlandet 74M (04:07), plus ostlandet fragments, elevation, graph-cache, and place index. Internal `files/long-trip-packs` was empty (8.0K). No UI wipe was started on this retry, and nothing was deleted with adb. Host free space: `/` about 40G, data drive about 22G. Both above 10 GB.

Earlier the same morning, before any wipe, that SD directory was **16G** (denmark, halland, niedersachsen, ostlandet, schleswig-holstein, skane, sorlandet, sweden, vastra_gotaland, vestlandet, elevation, place index).

### Campaign fields

| Field | Result |
|---|---|
| Distance 1461.3–1648.6 km | not available: retry not started, no route |
| Duration 17–22 h | not available: no route. Not reclassified. |
| Instructions 55–100 | not available: no maneuver list |
| Nearby attractions | not available: no route |
| Wild camping overnights | not available: no route |
| Cover-gate Stage-B (`3ba01455c`) | not triggered: no route search |
| Fuel stops | not available: no route distance |
| DATEX on the route | not available: no route. A prior synthetic cache may still be on disk; it was not applied |
| Ferries | not available: no route |
| Eco / tolls / ferries setting | not available for this retry: the campaign UI was not driven |
| Departure / vehicle / soft budget | not applied on this retry. Harness constants remain 2026-06-01T08:00:00, VW Transporter T6 camper, 6.0 h / 1.5 h / 15 min |
| Re-plans | none |
| Overlay vs bad-luster.kml | not available: no polyline, so no overlay image |

OSM and RadioID callsigns are not available: there is no route, and Update was not pressed.

`git log --oneline CAT..origin/dev` was empty. `klm/` and `testdata/cat/local/` are not tracked.
