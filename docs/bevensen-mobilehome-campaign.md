# Bad Bevensen to Dalsoren MobileHome campaign

## Latest — 2026-10-28 (route found; out of band)

Emulator **Navi_8c_4G_128G** ran with **6 cores** (guest `nproc` 6, CPUs online `0-5`) instead of 8. It was not relaunched and was not wiped. Tablet R52TB0JQEDE was not used.

Frozen app binary. No app or plugin source was edited during the run. The app package was not rebuilt and not reinstalled:

- Commit `44057d4ee6e2475b8314ef677e426f11a7c957d2`
- APK SHA-256 `7ad3297d20f5472d0230aef32120e06411c885c5769f829bd55674a4770b73ca`

Catalog `20261028T040556Z-2275792-d248bf27` (unchanged; Puttgarden still covered).

This plan is the one that finished after the virtual device was started again. Earlier the same morning: at 06:34:39 a second instrumentation killed a download before a route existed; at 06:38 a retry was not started because the then-installed test package opened CAT. Those attempts are superseded. The test package used for this plan does not open CAT. Update was not pressed.

### Sørlandet

`europe/norway/sorlandet` was Downloading when the emulator console reported `virtual device is stopped`. The sdcard qcow2 stopped growing at 07:03:15. On disk the fetch was a partial directory `.pack-fetch-sorlandet-latest.partial`, 204 MB, 13 of 21 pack files, no finished top-level `sorlandet-latest.osm.pbf` yet. The UI was frozen on `Fetching packs` for `europe_norway_sorlandet` (region 8 of 9).

`avd start` was sent. The device returned to `running`. The same app process continued the long-trip fetch. The partial directory was then replaced by installed files, including `sorlandet-latest.osm.pbf` at 74 MB. The campaign status then read **Installed**, then **Indexed**. Corridor skeleton for `sorlandet-latest` reported PASS. No files were deleted to free space. At resume, host data-drive free space was about 83 GB and the guest SD card had 495 GB free. This was not an ENOSPC failure.

### Re-plans

The app tried Plan four times at guest 07:06. Each failed with `terminate=skeleton_preparing` for Niedersachsen. The UI then said auto-retry had stopped. After the skeletons were ready, Plan was tapped once (button bounds only, no map swipe) at guest 07:16:19. That attempt produced the route below. No second `am instrument` was started.

### Route

| # | Field | Result |
|---|---|---|
| 1 | Cores | 6, not 8 |
| 2 | App commit / SHA-256 | `44057d4ee6e2475b8314ef677e426f11a7c957d2` / `7ad3297d20f5472d0230aef32120e06411c885c5769f829bd55674a4770b73ca` |
| 3 | Catalog | `20261028T040556Z-2275792-d248bf27` |
| 4 | Clean start | Tools UI deleted the corridor regions, then long-trip Plan downloaded them. Final status: all 9 Indexed, including Sørlandet |
| 5 | Distance 1461.3–1648.6 km | **1428.441 km. Out of band** (below 1461.3) |
| 6 | Duration 17–22 h | **981.4 min = 16.357 h. Out of band.** Below 17 is not reclassified |
| 7 | Instructions 55–100 | **32. Out of band** |
| 8 | Wild camping overnights | The plan log does not record a wild-camping overnight. It records two lodging overnights: Rasta Hallandsasen and Best Western Leto Arena. **Not shown as greater than 0** |
| 9 | Nearby attractions | Setting was on. The plan log does not list attraction names |
| 10 | Cover-gate Stage-B (`3ba01455c`) | Stage-B ran. No log line `demote ... densify hop end outside Ready tiles`. The ferry-free alternative (1495.8 km, 0 ferries) was ineligible beside the shorter ferry alternative (1327.6 km, 1 ferry) and was not picked. The picked corridor uses Rodby to Puttgarden |
| 11 | Re-plans | Four automatic attempts failed on Niedersachsen skeleton prep. One later Plan tap produced this route |
| 12 | DATEX | Six synthetic sits were in the cache. The finished plan log reports `datex_impacts=0`, `datex_block=0`, `datex_penalize=0` |
| 13 | Fuel stops | Calculated from 1428.441 km, 70 L full, 100 km margin. At 11 L/100 km the usable leg is 536 km and the trip needs 2 refuels. At 14 L/100 km the usable leg is 400 km and the trip needs 3 refuels. Not planned by the app |
| 14 | Ferries / eco / tolls | One ferry leg: Rodby (DK) to Puttgarden (D), 18.92 km. UI setting eco on; the router log for this plan says `eco=false`. Avoid tolls off. Avoid ferries off |
| 15 | Departure / vehicle | 2026-06-01T08:00:00. VW Transporter T6 2.0 BiTDi 4Motion camper. Soft daily budget 6.0 h, break 1.5 h / 15 min. Long trip on. Pack volume `uuid:BF1B-9F0E` |
| 16 | Overlay vs bad-luster.kml | Hop ends of this plan against `bad-luster.kml` (reference distance 1440.985 km in prior notes). Image: `docs/bevensen-mobilehome-overlay.png`. The drawn blue line is the 15 hop ends, not the full 719762-character polyline |
| 17 | Callsigns | Not available. Update was not pressed and the plan log has no repeater callsigns |

OSM and RadioID callsigns are not in this document because none were collected.

`git log --oneline CAT..origin/dev` is checked at commit time. `klm/` and `testdata/cat/local/` are not tracked.
