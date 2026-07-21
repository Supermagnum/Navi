# API surface

Primary host API: UniFFI bindings generated from `navi-ffi` into
`app/src/main/java/uniffi/navi/navi.kt`.

Notable exports:

- Region provision / corridor routing (`provisionRegionData`, `runCarCorridorPipeline`)
- Place index + FTS search (`ensurePlaceIndex`, `searchPlaces`)
- OSM updates (`checkOsmUpdates`, `applyOsmUpdate`, `bindGeofabrikRegion`, weekly reminder helpers)
- Icon rasterization (`rasterizeIconPng`, `rasterizeIconCheck`)
- Travel profile helpers (`ecoModeToggleable`, `ecoModeDefault`)
- Saved routes and vehicle limits (see UniFFI records in `navi-ffi`)

Plugin guest API: see [`plugins.md`](plugins.md).
POI categories: see [`poi.md`](poi.md).
OSM update cadence: see [`osm-updates.md`](osm-updates.md).

Internal Rust modules are documented at the crate root of `driver-break-core`.
