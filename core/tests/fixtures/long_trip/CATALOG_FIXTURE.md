# long_trip fixtures provenance

## current.json
Real slim snapshot of the navigate-me pack catalog (`generation` /
`created_unix` preserved), filtered to regions needed for long-trip tests.
Pack `bytes` fields are from that live catalog pull (not invented).

## ors_*.geojson
**SYNTHETIC.** Hand-drawn densified polylines shaped like ORS GeoJSON so
parser / corridor / adjacency tests compile offline. They are **not**
recorded OpenRouteService responses. Route A (Redondo) and Route B
(Crescent City) deliberately do not match real I-15 / I-80 corridors —
see US region-list critique. Treat `CatalogCoverage::Complete` as
**unproven** until `live_us_long_trip_dry_run` with a real API key.

## us_endpoints.json
Nominatim-resolved coordinates for dry-run endpoints (real places).
