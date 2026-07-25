# Pictures

Emulator screenshots for Navi (MapLibre + OpenFreeMap liberty on Android
Automotive).

## GitHub allowlist (space)

**Only** the files listed below may be committed under `docs/images/`.
Extra instrumented-test captures stay on the device / local disk and must not be
added to the repo.

Do **not** use synthetic (hand-drawn / 2-point stub) routes in tests or in any
screenshot that documents routing — corridor geometry must come from a real
planner run (host `raufoss_approach_route`, in-app corridor pipeline, etc.).

Idle HUD bars and the Helgøya → Atnbrua route capture live in the
[README](../README.md#working-app-emulator-screenshots) only
(`docs/images/hud/hud_idle_both_bars.png`,
`docs/images/terrain/hike_eldabu_ramshogda_3d.png`).
Map tilt at 45° (3D off / 3D on) is also shown there
(`docs/images/tilt45_3d_off.png`, `docs/images/tilt45_3d_on.png`).
Current-street UTF-8 evidence (fixture names from Ostlandet place index):
`docs/images/hud/hud_current_street_mjosevegen.png` (ø),
`hud_current_street_trollaas.png` (å),
`hud_current_street_aevongsli.png` (Æ).

Norwegian gallery: [`bilder.md`](bilder.md).

## Map / routing

| Scene | Preview |
|---|---|
| Prekestolen base camp, POI's visible. | ![Prekestolen POIs](images/zoom_z16.png) |
| Helgøya → Atnbrua (eco + 3D on, breaks visible) | ![Route overlay](images/route_map.png) |
| Route from Gjendebu to Thonvollen, 3D map. | ![Gjendebu to Thonvollen 3D](images/gjendebu_thonvollen_3d.png) |
| Gjendebu to Thonvollen, flat map. | ![Gjendebu to Thonvollen flat](images/gjendebu_thonvollen_flat.png) |
| Hamar loop, 45° tilt, 3D off | ![45° tilt 3D off](images/tilt45_3d_off.png) |
| Hamar loop, 45° tilt, 3D on | ![45° tilt 3D on](images/tilt45_3d_on.png) |

## Current street (bottom HUD)

Real Østlandet fixture names via `CurrentStreetInstrumentedTest`. See
[`current-street.md`](current-street.md) /
[`unicode-road-names.md`](unicode-road-names.md).

| Scene | Preview |
|---|---|
| Currently on Mjøsvegen (ø) | ![Mjøsvegen](images/hud/hud_current_street_mjosevegen.png) |
| Currently on Trollåsveien (å) | ![Trollåsveien](images/hud/hud_current_street_trollaas.png) |
| Currently on Ævongsli (Æ) | ![Ævongsli](images/hud/hud_current_street_aevongsli.png) |

Prefer `images/terrain/` and recent HUD shots when comparing route visuals.
Older `route_map.png` copies were ~57% MapLibre empty background (`#f8f4f0`)
because tiles never loaded — that is the “beige outdated” failure mode.
Regenerated shots (allowlisted) show Liberty/Protomaps tiles plus start/end
labels. Pale land fill in Liberty/Protomaps light styles is normal and is not
the same as empty-map beige.

Start / via / end **place names** are drawn as a Compose map overlay (and a
MapLibre waypoints layer). Screenshot tests set
`NaviMapTestHooks.routeStartLabel` / `routeEndLabel` because search chrome is
often hidden with `hideSearchChrome=true`.

## Moving icons (APRS-style)

| Scene | Preview |
|---|---|
| Before move | ![Moving before](images/navi_moving_before.png) |
| After move | ![Moving after](images/navi_moving_after.png) |
| After move (2) | ![Moving after 2](images/navi_moving_after2.png) |

## Offline PMTiles basemap / 3D

Evidence for [`map-styles.md`](map-styles.md). Captured by
`BasemapPmtilesScreenshotTest` on the Automotive emulator (kind + terrain
attach + camera logged). Drive HUD bars kept visible (`hideUiChrome=false`).

| Scene | Preview |
|---|---|
| Offline Protomaps (Kløfta, 3D off) | ![Offline Protomaps](images/basemap/basemap_offline_protomaps.png) |
| Coverage boundary → live Liberty (Tromsø) | ![Boundary Liberty](images/basemap/basemap_coverage_boundary_tromso.png) |
| Online 3D (Mapterhorn DEM hillshade, Gjendebu, Jotunheimen) | ![3D hillshade](images/basemap/basemap_3d_mapterhorn_hillshade.png) |
| Flat map, Gjendebu, Jotunheimen | ![Flat map](images/basemap/basemap_3d_fallback_liberty.png) |

## Multi-day day cards

Captured by `MultiDayDayCardsScreenshotTest` (route tools sheet day list).
Live truck corridor (emulator GPS → Bodø) by `LiveMultiDayDayCardsInstrumentedTest`.

| Scene | Preview |
|---|---|
| Truck multi-day day cards | ![Truck day cards](images/multi_day_day_cards.png) |
| Hiking multi-day day cards | ![Hiking day cards](images/multi_day_day_cards_hiking.png) |
| Truck live multi-day day cards (GPS → Bodø) | ![Live truck day cards](images/multi_day_day_cards_live.png) |
