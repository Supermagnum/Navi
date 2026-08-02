# Pictures

Screenshots for Navi. **Real hardware only** (Samsung Galaxy Tab S6 Lite
SM-P613). Do **not** use the emulator for gallery captures.

Route-following / current-position scenes use the in-app **route simulator**
(red **SIMULATING** banner) along a **real planner-run** corridor. Gallery
frames must **not** reveal the tester’s live GPS position (no live GPS follow
at the physical device location; `disableGpsFollow` for static POI frames).

Coordinates for most captures below were entered via the Route search keyboard
as `lat, lon` (not map-tap). Peak frames that must show basemap amenity/peak
icons at zoom ≥ **16** may also use `NaviMapTestHooks.pendingCamera` /
`GalleryPeakPoiRetakeScreenshotTest` so framing is reliable on hardware.

Basemap amenity / peak icons (not [`poi.md`](poi.md) PoiIndex) need camera
zoom ≥ **16**. OpenFreeMap Liberty has no `mountain_peak` layer — some peaks
(e.g. Elgpiggen) show on offline Protomaps but not on Liberty online.


Idle HUD bars and the Skolla → Rondvassbu hike overview also appear in the
[README Screenshots](../README.md#screenshots) section
(`docs/images/hud/hud_idle_both_bars.png`,
`docs/images/terrain/hike_eldabu_ramshogda_3d.png`).
Cold-start splash (red open-app mark, Splash Screen API):
`docs/images/splash_open_app.png` (SM-P613; hold with
`am start … --ez navi_keep_splash true`).
Map tilt at 45° (3D off / 3D on) on the Ådalsbruk / Løten loop:
`docs/images/tilt45_3d_off.png`, `docs/images/tilt45_3d_on.png`.
GPS follow / Recenter / rotation evidence (all with **SIMULATING** on the Løten
loop): `docs/images/follow_gps/01_simulating_follow.png` …
`06_rotation_modes_ok.png`.
Current-street UTF-8 evidence:
`docs/images/hud/hud_current_street_mjosevegen.png` (ø),
`hud_current_street_trollaas.png` (å),
`hud_current_street_aevongsli.png` (Æ).

Norwegian gallery: [`bilder.md`](bilder.md).

Capture harness: `GalleryDocsKeyboardCaptureTest` (SM-P613). Pull:
`adb pull /data/local/tmp/navi_gallery_docs/ docs/images/`.

POI online/offline pairs under `docs/images/`:
`poi_jutulhogget.png` / `poi_jutulhogget_offline.png`,
`poi_galdhopiggen_3d.png` / `poi_galdhopiggen_online.png`,
`poi_elgpiggen.png` / `poi_elgpiggen_online.png`,
`poi_prekestolen.png` / `poi_prekestolen_offline.png`.

## Map / routing (documented corridors)

All route-overlay rows were captured on **SM-P613** with **SIMULATING** where a
position marker is shown (except idle HUD / static POI rows). Route-planner
chrome is closed for map-heavy shots (Løten loop, Espa corridor, hiking /
e-bike overlays) so the corridor is visible.

| Scene | Preview |
|---|---|
| Espa → Atnbrufossen (eco / primary corridor), simulating | ![Espa–Atnbrufossen](images/route_espa_atnbrufossen.png) |
| Åkersætra → Jammerdalsbu → Rondvassbu (DNT hiking), simulating | ![Hiking corridor](images/route_akersaetra_rondvassbu_hiking.png) |
| Ringebu / Venabygdsfjellet (electric cycle climb), simulating | ![E-cycle climb](images/route_venabygdsfjellet_ebike.png) |
| Ådalsbruk / Løten loop (simulator + turn instructions), simulating | ![Løten loop](images/route_adalsbruk_loten_loop.png) |
| Same Løten loop (legacy `route_map.png` slot) | ![Route overlay](images/route_map.png) |
| Finnstad → Søndre Ommang → Rosenlund, 3D on, simulating | ![Route 3D](images/finnstad_sondre_ommang_3d.png) |
| Finnstad → Søndre Ommang → Rosenlund, flat, simulating | ![Route flat](images/finnstad_sondre_ommang_flat.png) |
| 45° tilt, 3D off, simulating (Løten loop) | ![45° tilt 3D off](images/tilt45_3d_off.png) |
| 45° tilt, 3D on, simulating (Løten loop) | ![45° tilt 3D on](images/tilt45_3d_on.png) |

Tilt/3D demos. Older gallery PNGs may show a blue hydro soft-edge fringe;
that is a
[screenshot-capture artifact](map-styles.md#hydro-soft-edge-fringe-screenshot-artifact),
not what users see live.

## Single points (keyboard fly-to)

Static map frames after typing WGS84 coordinates into Route search (no live GPS
follow; Route chrome closed before screencap). Opt-in 3D with **45°** camera
tilt and Mapterhorn hillshade where available.

Each landmark has an **online** (Liberty + remote DEM) and **offline**
(Ostlandet Protomaps PMTiles + local DEM) pair when the point falls inside the
installed extract. Preikestolen is outside Ostlandet coverage: preferring
offline still resolves to Liberty 3D (documented fallback row).

Confirm tiles loaded (not beige empty-tile failure mode).

| Scene | Preview |
|---|---|
| Jutulhogget canyon, offline Protomaps 3D | ![Jutulhogget offline](images/poi_jutulhogget_offline.png) |
| Galdhøpiggen peak, offline Protomaps 3D (61.6364721, 8.3124426) | ![Galdhøpiggen](images/poi_galdhopiggen_3d.png) |
| Elgpiggen peak, offline Protomaps 3D (62.1592913, 11.3584086) | ![Elgpiggen](images/poi_elgpiggen.png) |
| Preikestolen / Pulpit Rock, online 3D (58.9870777, 6.1887732) | ![Preikestolen](images/poi_prekestolen.png) |
| Preikestolen with offline preferred (no Ostlandet PMTiles → Liberty 3D fallback) | ![Preikestolen offline](images/poi_prekestolen_offline.png) |

## Current street (bottom HUD)

Real Østlandet fixture names via `CurrentStreetInstrumentedTest`. See
[`current-street.md`](current-street.md) /
[`unicode-road-names.md`](unicode-road-names.md).

| Scene | Preview |
|---|---|
| Currently on Mjøsvegen (ø) | ![Mjøsvegen](images/hud/hud_current_street_mjosevegen.png) |
| Currently on Trollåsveien (å) | ![Trollåsveien](images/hud/hud_current_street_trollaas.png) |
| Currently on Ævongsli (Æ) | ![Ævongsli](images/hud/hud_current_street_aevongsli.png) |

## GPS follow / Recenter / rotation

Captured on **SM-P613** during **route simulation** on the Ådalsbruk / Løten
loop (`HardwareGallerySimScreenshotTest` / gallery keyboard harness). Every
frame below must show **SIMULATING** (not live GPS at the tester’s location).

| Scene | Preview |
|---|---|
| Simulating while GPS follow is on | ![Follow while simulating](images/follow_gps/01_simulating_follow.png) |
| After pan (follow paused) | ![After pan](images/follow_gps/02_after_pan.png) |
| After zoom in | ![After zoom in](images/follow_gps/03_after_zoom_in.png) |
| After zoom out | ![After zoom out](images/follow_gps/04_after_zoom_out.png) |
| After Recenter | ![After Recenter](images/follow_gps/05_after_recenter.png) |
| Rotation modes OK | ![Rotation modes](images/follow_gps/06_rotation_modes_ok.png) |

## Moving icons (APRS-style)

| Scene | Preview |
|---|---|
| Before move | ![Moving before](images/navi_moving_before.png) |
| After move | ![Moving after](images/navi_moving_after.png) |
| After move (2) | ![Moving after 2](images/navi_moving_after2.png) |

## Offline PMTiles basemap / 3D

Evidence for [`map-styles.md`](map-styles.md). Captured by
`BasemapPmtilesScreenshotTest` / SM-P613 offline 3D tests. Drive HUD bars kept
visible (`hideUiChrome=false`).

| Scene | Preview |
|---|---|
| Offline Protomaps (Kløfta, 3D off) | ![Offline Protomaps](images/basemap/basemap_offline_protomaps.png) |
| Coverage boundary → live Liberty (Tromsø) | ![Boundary Liberty](images/basemap/basemap_coverage_boundary_tromso.png) |
| Online 3D (Mapterhorn DEM hillshade, Gjendebu, Jotunheimen) | ![3D hillshade](images/basemap/basemap_3d_mapterhorn_hillshade.png) |
| Flat map, Gjendebu, Jotunheimen | ![Flat map](images/basemap/basemap_3d_fallback_liberty.png) |

## Multi-day day cards

Refreshed on **SM-P613**. Hiking cards come from a **keyboard-entered** real
plan Åkersætra → Jammerdalsbu → Rondvassbu (`GalleryDocsKeyboardCaptureTest`).
Truck cards come from a keyboard-entered Espa → Atnbrufossen truck plan with a
tight daily driving cap so the planner emits real `daysJson` cards (no live-GPS
Bodø start — privacy).

| Scene | Preview |
|---|---|
| Truck multi-day day cards (Espa → Atnbrufossen, finished plan) | ![Truck day cards](images/multi_day_day_cards.png) |
| Hiking multi-day day cards (Åkersætra → Rondvassbu) | ![Hiking day cards](images/multi_day_day_cards_hiking.png) |
