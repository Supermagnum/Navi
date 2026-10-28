# Bad Bevensen to Dalsoren MobileHome campaign

## Latest — 2026-10-28 06:34 (stopped; no route)

Emulator **Navi_8c_4G_128G** ran with **6 cores** (guest `nproc` 6, CPUs online `0-5`, emulator maximum) instead of 8. It was not relaunched and was not wiped. Tablet R52TB0JQEDE was not used. `adb -s emulator-5554` only.

App binary frozen for the run:

- Commit `44057d4ee6e2475b8314ef677e426f11a7c957d2` (CAT). Repeater Update runs `catPluginUpdateRepeatersFromServer` on `Dispatchers.IO` and disables the Update control while that job runs. That commit is before the run.
- APK SHA-256 `7ad3297d20f5472d0230aef32120e06411c885c5769f829bd55674a4770b73ca` (debug, `x86_64`, versionName 0.3.26-beta). Installed 2026-10-28 06:15. The same hash was still on the device after the test APK install. The app was not rebuilt or reinstalled after that.
- Catalog generation `20261028T040556Z-2275792-d248bf27` (unchanged). Puttgarden remains inside published `europe/germany/schleswig-holstein` on that catalog. The catalog id was re-checked and matched, so tile coverage was not re-probed.

No app or plugin code changed during the run. The instrumented test was later adjusted so a stale status line containing "off" cannot click the long-trip switch when the saved pref is already on. That test change is not in the frozen app APK.

This was not a finished clean plan. Regions were partly deleted, long trip was on, one Plan click started corridor downloads, and the app process was killed before any route existed.

### Stop point

2026-10-28 06:34:39.855. ActivityManager:

`Killing 18880:no.navi.app/u0a209 (adj 0): stop no.navi.app due to start instr`

A second `am instrument` of `LongTripMobileHomeBevensenDalsorenUiCampaignTest` started at 06:34 and force-stopped the app. Last campaign progress, 06:34:17, pid 18880:

`Niedersachsen (1 of 9)=Indexed · Schleswig-Holstein (2 of 9)=Downloading · Denmark (3 of 9)=Downloading · Skåne (4 of 9)=Downloading · Halland (5 of 9)=Downloading · Västra Götaland (6 of 9)=Downloading · Ostlandet (7 of 9)=Downloading · Sorlandet (8 of 9)=Indexed · Vestlandet (9 of 9)=Downloading`

`corridor_ready=false`, `polyline_chars=0`, `distance_km=0`, `report_has_pass=false`.

That second instrument then opened the CAT sheet and clicked Update at 06:35:10. This run stopped that process. It was not a route re-plan and it produced no route. This agent did not press Update, did not open the CAT sheet, and did not connect a radio.

### Regions

Before any wipe this morning, SD `/storage/BF1B-9F0E/Android/data/no.navi.app/files/long-trip-packs` was **16G** (`du -sh`). File-byte sums from `ls -l` at that time:

| Stem | Bytes |
|---|---|
| niedersachsen | 3,523,203,885 |
| denmark | 3,464,017,696 |
| ostlandet | 3,136,425,019 |
| vestlandet | 1,501,566,363 |
| place_index.db | 1,437,216,768 |
| schleswig-holstein | 1,038,217,774 |
| vastra_gotaland | 1,011,594,923 |
| sweden | 942,926,646 |
| skane | 489,086,264 |
| sorlandet | 405,542,552 |
| halland | 192,215,922 |

Elevation directory about 122M. Internal `files/long-trip-packs` was empty (8.0K). Host free space stayed above 10 GB (`/` about 40G, data drive 31G down to 25G). GPS feeding was not paused.

The Tools UI wipe did not finish. Text input on the Geofabrik field failed after Schleswig-Holstein, so Sorlandet was not deleted and stayed Indexed (pbf still the 04:07 file, 74M). At the kill, Niedersachsen had been re-downloaded (pbf 485M, 06:32) and other corridor regions were still Downloading. Directory size then was about 6.9G to 7.5G, with those two `*.osm.pbf` files visible. Internal pack dir still empty.

### CAT

Update was not pressed as setup. Drop folder `Android/data/no.navi.app/files/repeaters` had 7 KML files plus README: Sachsenlink, denmark, funknetz, germany, kaxi, repeaters_2610270617, repeaters_sweden. `cat_drop_imported=true`. Prefs after the killed run: `cat_plugin_enabled=true`, `cat_network_choice=auto`, `cat_backend=hamlib`, `cat_rig_model=1`, `long_trip_enabled=true`, `long_trip_pack_volume_id=uuid:BF1B-9F0E`. No consensus key (still off). Nothing was connected.

### DATEX

Host inject of 6 synthetic MaintenanceWorks penalties (DE, DK, SE, NO) landed before Plan: `datex-GetSituation.xml` 15747 bytes, `datex-cache.json` 237 bytes, `apply_to_routing` 1 byte, under `/data/user/0/no.navi.app/files/datex_cache`. The plan never reached a route, so route impact counts are not available.

### Campaign fields

| Field | Result |
|---|---|
| Distance 1461.3–1648.6 km | not available: no route (`distance_km=0`) |
| Duration 17–22 h | not available: no route. Not reclassified. |
| Instructions 55–100 | not available: no maneuver list |
| Nearby attractions | not available: no route |
| Wild camping overnights | not available: no route. Requirement was > 0. |
| Cover-gate Stage-B (`3ba01455c`) | not triggered: planning never started a route search |
| Fuel stops | not available: no route distance to apply the 70 L / 100 km margin table |
| DATEX penalties on the route | not available: cache was injected, route was not computed |
| Ferries on the route | not available: no route |
| Eco | on in the UI before Plan (`eco_routing_switch` clicked ON) |
| Avoid tolls | off (row visible, default off) |
| Avoid ferries | off (harness ferries=use) |
| Departure | 2026-06-01T08:00:00 local, as in the harness |
| Vehicle | VW Transporter T6 2.0 BiTDi 4Motion camper, harness constants (H 2.477 m, W 2.297 m, L 5.304 m, total 3020.4 kg, axle 1661.2 kg, tank 70 L) |
| Soft daily budget / break | 6.0 h / 1.5 h / 15 min, harness seed |
| Re-plans | none. One Plan click at 06:27:15. No same-APK UI re-plan. |
| Overlay vs bad-luster.kml | not available: no route polyline, so no overlay map image |

OSM and RadioID callsigns along a route are not available: there is no route and Update was not used to refresh the repeater list for this run.

`git log --oneline CAT..origin/dev` was empty after fetch. No merge was required.

Prior in-band figures from earlier the same morning are in git history of this file (commit `2036d4cc4` and parents). They are not this run.
