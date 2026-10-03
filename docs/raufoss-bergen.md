# Raufoss → Bergen busstasjon (tablet campaign)

Date: **2026-10-03**. Device: Samsung Galaxy Tab S6 Lite **SM-P613** serial
**R52TB0JQEDE**. Sideloaded debug APK from `origin/dev` **`eac4539b`**
(`versionName` **0.3.13-beta**, `versionCode` **18**,
`compiled/navi-debug.apk` sha256 `aff74cf5…`). **Not** added to CI. No app or
plugin code was changed during the run.

Host GPS mock (ADB test providers `gps`/`network`/`fused`) stayed at Raufoss
**60.7277483, 10.6109403** (elev target 311 m; HUD altitude often `--` because
`cmd location` has no altitude extra). DATEX inject was ADB-only
(`files/datex_cache/`). All other steps were tablet UI.

---

## Verdict

| Metric | Result | EXPECTED | OK |
|---|---|---|---|
| Distance | **459.6 km** | 0–500 km | yes |
| Driving time | **~6.9 h** (411 min; from 459.6 km at 67 km/h implied by 1.5 h / 100.5 km rest spacing) | 3–8 h | yes |
| Instructions | HUD showed at least **left** onto Nysethvegen (92 m). Full maneuver dump was not recovered (logcat ring). | 1–170 | partial |
| Plan | **found** (`PLAN_CAR_ROUTE`, `pack_hit=true`, `use_eco=true`, `long_trip_enabled=false`) | found | yes |
| DATEX | **5** synthetic Blocks written to `datex_cache` at 13:05. First plan and a 13:06 replan both logged `datex_impacts=0` | 3–6 applied | inject yes / plan apply no |

Corridor: Raufoss (Innlandet / Østlandet pack) west toward Vestlandet (map labels
included Hagafoss, Måbø gård). `route_ferry_legs=0`. `motorway_share_pct=0.00`.
One calendar driving day (`motor_multi_day: days=1`).

---

## Test setup

| Item | Value |
|---|---|
| Device | SM-P613 `R52TB0JQEDE` (no removable SD; emulated volume only) |
| Origin | Raufoss **60.7277483, 10.6109403**, elev target **311 m** |
| Destination | Bergen busstasjon, Marken / Skuteviken, Bergenhus, Bergen, Vestland 5008 ( Nominatim `amenity/bus_station`, 60.388114, 5.333857 ) |
| Departure | UI has **no** departure-datetime field. Plan wall-clock **2026-10-03 ~13:03** CEST. Requested `2026-06-01T08:00` local was not settable |
| Profile | Car (chip) |
| Eco | on (`use_eco=true`; Eco routing switch + leaf HUD) |
| Avoid toll / ferries / tunnels / motorways | off / off / off / off |
| Soft daily budget | **no UI field**; `CarRestSettings.maxHours` stays default **8.0**. Trip fitted in one day |
| Soft break | **1.5 h** interval, **15 min** rest (Drive settings saved 12:51) |
| Wild camping | off (`Right-to-roam camping` switch left off) |
| Long trip | off (`long_trip_enabled=false`) |
| DATEX overlay | on (map settings; fetch when a route is planned) |
| Nearby attractions | on (look-ahead HUD fired at origin) |

---

## Regions downloaded (user-driven UI)

All packs, PBFs, elevation, PMTiles, and `place_index.db` lived under **internal**
`/data/user/0/no.navi.app/files` (`packDirPath` same). Innlandet is covered by the
Østlandet extract (no separate Innlandet download).

Download order actually executed: **Østlandet** (active) then queued **Sørlandet**
then **Vestlandet**. Sørlandet is **not required** for Raufoss→Bergen; the user
queued it and it completed.

### Østlandet `europe/norway/ostlandet`

| Phase | Start | End | Duration |
|---|---|---|---|
| Packs (`pack_fetch.total`, 74 files, 2.62 GB) | 11:45:19 | 11:48:45 | 206.7 s (3.4 min) |
| PBF Geofabrik (455.3 MB) | 11:48:45 | 11:57:43 | 538.0 s (9.0 min) |
| Elevation (`dem.ensure_corridor`) | 11:57:43 | 11:57:43 | 20 ms (`dem_download_s=0`, already present) |
| Packs installed / Innlandet graphs | | 11:57:43 | |
| PMTiles (585173 tiles, 1.16 GB) | 11:57:45 | 12:03:54 | 369.1 s (6.2 min) |
| Place index | 12:06:52 | ~12:19 | ~12.1 min (waited behind Sørlandet) |

PBF: `files/ostlandet-latest.osm.pbf`. Ready stamp includes `europe/norway/ostlandet`.

### Sørlandet `europe/norway/sorlandet` (extra)

| Phase | Start | End | Duration |
|---|---|---|---|
| Packs (20 files) | 11:59:23 | 12:00:18 | 55.1 s |
| PBF (77.1 MB) | 12:00:18 | 12:00:33 | 14.6 s |
| PMTiles (163578 tiles, 331 MB) | 12:00:35 | 12:02:59 | 144.5 s |
| Place index | 12:02:59 | 12:06:52 | 232.2 s; **indexed=235091** |

### Vestlandet `europe/norway/vestlandet`

| Phase | Start | End | Duration |
|---|---|---|---|
| Packs | 12:02:55 | 12:04:29 | 94.2 s |
| PBF (257.4 MB) | ~12:04:29 | 12:10:29 | 359.6 s |
| PMTiles (563173 tiles, 738 MB) | 12:10:31 | 12:14:40 | 249.3 s |
| Place index | ~12:19:03 | **12:25:14** | 371.0 s; **indexed=791493** |

PBF: `files/vestlandet-latest.osm.pbf`.

---

## Results summary

### Ferries

**0** (`route_ferry_legs=0`). Graph still had `graph_ferry_edges=26` in the
corridor bbox; none used.

### Attractions

Nearby attractions on. At origin the HUD showed look-ahead chips, including
**Pizzabakeren Raufoss** (260 m, hours unknown) and **Digg Pizza**. Types seen:
amenity/fast_food (pizza). Count of distinct look-ahead hits during the short
preview: **at least 2** (plus a `+1` overflow on the chip). Full `by_type`
histogram was not written to `route-result.json`.

### Rest places (name + coordinates)

Plan reported `break_poi_count=4` with `motor_break_interval_km=100.5` (1.5 h
soft interval at trip speed). `NaviRouting` `planning_pois` lines were not in
the surviving logcat buffer, so **names and coordinates were not captured**.
Map chrome along the polyline included Hagafoss, Muggedalen, Måbø gård,
Furnestreet; those are map labels, not confirmed rest-POI records.

### Wild camping

**0** sites used. Wild-camping / right-to-roam switch stayed off; long trip off.

### Tunnels

`avoid_tunnels=false`. `PLAN_CAR_ROUTE` text does not enumerate tunnel count.
Corridor toward Måbø / Hardanger typically includes tunnels; **count not
measured** this pass.

### Length / km per day / instructions

- Total **459.6 km**, **1 day**, **~411 min** (~6.9 h) estimated from rest spacing.
- Eco climb **1017 m**, descent **1167 m**.
- Snap start 25 m / end 33 m.
- First guidance: **left**, 92 m, **Nysethvegen**. Other kinds not dumped.

### Estimated fuel stops (report-only)

Full tank, 100 km margin, 500–600 mile range class (no onboard fuel planner):

- 500 mi = 804.7 km usable 704.7 km after margin → **stops_at_500mi=0**
- 600 mi = 965.6 km usable 865.6 km after margin → **stops_at_600mi=0**

### DATEX (timing)

Five synthetic **Block** situations (DATEX3 MaintenanceWorks) written via ADB to
`/data/user/0/no.navi.app/files/datex_cache/datex-GetSituation.xml` plus
`datex-cache.json` and `apply_to_routing`, **2026-10-03 13:05** CEST, after the
first plan (~13:03). Overlay was already enabled in map settings.

| Id | Lat | Lon | Intended vs first-plan arrival | Notes |
|---|---|---|---|---|
| syn-no-rv4-raufoss | 60.7050 | 10.4200 | ~12 min at 67 km/h | in 30–5 min band if driving starts at inject |
| syn-no-rv33-dokka | 60.6800 | 10.2200 | ~22 min | in band |
| syn-no-e16-ton | 60.6550 | 10.0500 | ~28 min | in band |
| syn-no-rv7-hagafoss | 60.5510 | 8.8470 | well over 30 min | later corridor; left in cache |
| syn-no-rv7-mabo | 60.4280 | 7.2150 | well over 30 min | later corridor; left in cache |

A second Plan (~13:06) updated `route-result.json` (`t_ms` 108980445 → 109154836)
but still `datex_impacts=0; datex_block=0; datex_penalize=0`. Live NPRA refresh
on plan can overwrite synthetic XML; host re-pin after refresh was incomplete
(`run-as` stamp path failed once). **Reroutes were not confirmed in the plan
report.**

---

## Plan engine excerpt (`route-result.json`)

```
TEST_KIND=PLAN_CAR_ROUTE
DATA_SOURCE=real_pbf
profile=Car; use_eco=true; long_trip_enabled=false
datex_impacts=0
pack_hit=true; primary_stem=ostlandet-latest; extra vestlandet-latest
route_ferry_legs=0
motor_multi_day: days=1
distance_km=459.61
```

Storage path for that file:
`/storage/emulated/0/Android/data/no.navi.app/files/long-trip-ui-report/route-result.json`.

---

## Gaps / notes

- Soft daily **6.0 h** has no Compose field; unused because the trip is one day.
- App `Use GPS as from` returned **GPS unavailable** while the OS mock was live
  (`ignoreLiveGpsFixes` leftover from an earlier debug extra). From was set as
  WGS84 `60.7277483, 10.6109403` in the search field instead.
- Do not send `KEYCODE_BACK` on this tablet (drops to Samsung launcher).
- Geofabrik path `EditText` swallows swipe digits; Tools swipes must stay off
  that field.
- Campaign is client/device only; not an instrumented test class.
