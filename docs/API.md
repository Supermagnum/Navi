# API surface

Primary host API: UniFFI bindings generated from `navi-ffi` into
`app/src/main/java/uniffi/navi/navi.kt`.

Notable exports:

- Region provision / corridor routing (`provisionRegionData`, `runCarCorridorPipeline`)
- Offline basemap PMTiles (`pmtilesQueueRegion`, `pmtilesRunJob`, pause/resume/cancel; see [`map-styles.md`](map-styles.md))
- Offline terrain DEM (`pmtilesQueueDemRegion` → `{region}_dem.pmtiles` from Mapterhorn; same run/pause/cancel path)
- Hiking multi-waypoint plan (`planHikingRoute`) with rast-interval hut pauses
  and tent-site fallback (day-by-day `plan_multi_day` is integration-test only,
  not exported via UniFFI)
- Car / motorcycle / cycle / truck corridor plan (`planCarRoute`) — soft motor
  multi-day overnight report lines / break POIs; Truck / TruckElectric also run
  EC 561 duty + multi-day daily/weekly rest (see [`ec-561-truck-rest.md`](ec-561-truck-rest.md))
- Truck rest settings helpers (`loadTruckRestSettings` / `saveTruckRestSettings` /
  exceptional-extension arming — see EC 561 doc)
- Place index + FTS search (`ensurePlaceIndex`, `searchPlaces`)
- OSM updates (`checkOsmUpdates`, `applyOsmUpdate`, `bindGeofabrikRegion`, weekly reminder helpers)
- Icon rasterization (`rasterizeIconPng`, `rasterizeIconCheck`)
- Travel profile helpers (`ecoModeToggleable`, `ecoModeDefault`)
- Saved routes and vehicle limits (see UniFFI records in `navi-ffi`)

Plugin guest API: see [`plugins.md`](plugins.md) (includes planned APRS, weather,
road info, CAT, ECU/EV plugins).
POI categories: see [`poi.md`](poi.md).
Truck rest / jurisdiction pattern: [`ec-561-truck-rest.md`](ec-561-truck-rest.md),
[`jurisdiction-rules.md`](jurisdiction-rules.md).
OSM update cadence: see [`osm-updates.md`](osm-updates.md).
ECU / OBD energy extension: see [`ECU.md`](ECU.md) (core types in
`driver_break_core::ecu`; no live polling API exported via UniFFI yet).
APRS tracks: UniFFI `FfiTrackStore` / `haversineKm` (see [`APRS.md`](APRS.md));
SDR ingest planned with [`rtl-sdr-rs`](https://crates.io/crates/rtl-sdr-rs)
([`APRS-SDR.md`](APRS-SDR.md)).
CAT / repeater auto-tune: see [`CAT.md`](CAT.md).
Crate wiring and databases: see [`architecture.md`](architecture.md).
Wire protocol index: see [`PROTOCOLS.md`](PROTOCOLS.md).

Internal Rust modules are documented at the crate root of `driver-break-core`.
