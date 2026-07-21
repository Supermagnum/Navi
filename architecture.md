# Navi architecture

## Layout

- `core/` — trusted Rust: elevation, routing, POI, search, rest/safety, icons
- `navi-ffi/` — UniFFI CDYLIB for Android (and other hosts)
- `app/` — Kotlin/Compose Android UI
- `plugin-host/` — wasmtime sandbox (fuel + wall-clock limits)
- `plugin-sdk/` — guest wrappers for the HostApi import ABI
- `plugins/` — reference `.wasm` plugins (build for `wasm32-unknown-unknown`)

## Network principle

Core routing, rest planning, and POI queries work fully offline once a region
extract is on disk. Network access is an **opt-in enhancement layer** (DEM
tiles, Geofabrik update checks, fixture downloads). Map data is never replaced
silently in the background — see [`docs/osm-updates.md`](docs/osm-updates.md).

## Thread priority tiers

| Tier | Role | Priority |
|---|---|---|
| T0 Sensor | GPS / IMU | Highest |
| T1 ECU | Live energy (WASM / native plugin) | High |
| T2 UI / audio | Compose UI; media must stay smooth | High |
| T3 Routing | Graph build, eco-reweight, A* | Medium (below audio) |
| T4 DB | SQLite | Lowest |

Routing-tier Rayon pools are sized from `std::thread::available_parallelism()` with
headroom reserved for T0–T2 so audio/UI are not starved. See
`driver_break_core::routing::workers::WorkerPoolPlan`.

Country-scale OSM loads are opt-in with a low-RAM warning; regional extracts are
the default working set on ~4 GB devices.

## Plugins

See [`docs/plugins.md`](docs/plugins.md) for HostApi, manifest format, and
capability gating. Plugins never receive WASI filesystem access; only the
declared HostApi imports are linked.
