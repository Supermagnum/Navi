# Rust crates: used, unaltered, and created

This document answers three questions for the Cargo workspace:

1. **Which first-party crates did Navi create?**
2. **Which third-party crates does it depend on?**
3. **Are any of those third-party crates forked or vendored?**

**Short answer:** All third-party Rust crates come from [crates.io](https://crates.io)
(via versions in the root `Cargo.toml` / package `Cargo.toml` files). There is
**no** `vendor/` tree and **no** path-patched forks of upstream crates. Upstream
crates are **used unaltered**. Navigation logic, UniFFI surface, plugin sandbox
wiring, and example WASM guests are **original Navi code** in this repository.

Pair with [`architecture.md`](architecture.md) (how crates wire together) and
[`codebase-map.md`](codebase-map.md) (where to edit modules).

---

## Created in this repository (first-party)

Workspace members are declared in the root `Cargo.toml`. Package names may
differ from directory names.

| Directory | Package name | Role | Status |
|---|---|---|---|
| `core/` | `driver-break-core` | Trusted nav core: OSM graph / A*, eco reweight, elevation, POI, FTS search, rest/HOS, tracks, icons raster, SQLite, sensors helpers | **Created** — all modules under `core/src/` |
| `navi-ffi/` | `navi-ffi` (lib name `navi`) | UniFFI CDYLIB / staticlib bridging core ↔ Android (and other hosts); bindgen helper bins | **Created** |
| `navi-linux/` | `navi-linux` | Linux gpsd + IMU sensor-feed demo binary (no desktop map UI) | **Created** |
| `plugin-host/` | `navi-plugin-host` | Wasmtime host: fuel / wall-clock limits, capability gate | **Created** |
| `plugin-sdk/` | `navi-plugin-sdk` | Guest-side wrappers for the HostApi ABI | **Created** |
| `plugins/log-hello/` | `navi-plugin-log-hello` | Reference WASM guest | **Created** (example only) |
| `plugins/busy-loop/` | `navi-plugin-busy-loop` | Reference WASM guest (isolation / fuel stress) | **Created** (example only) |

### Core modules that are original code

These live under `core/src/` and are **not** thin wrappers around a single
upstream “nav engine” crate. Upstream crates supply primitives (geometry,
SQLite, PBF parse, A* search helpers, SVG raster, HTTP, etc.); Navi owns the
algorithms and schemas.

| Module | Owns |
|---|---|
| `routing/` | Graph build/cache, eco reweight, A* options, OSM updates, elevation jobs, PMTiles basemap jobs, guidance path, ETA, workers |
| `routing/rest/` | Car / truck / hiking multi-day, EC 561 and FMCSA HOS packs |
| `routing/safety/` | Overnight / barrier filters |
| `nav/` | Approach thresholds, maneuver kinds, current-road label helpers |
| `poi/` | Categories, OSM tag classifier, R-tree index |
| `search/` | FTS5 name index + saved routes |
| `storage/` | SQLite schema, config / elevation / PMTiles job stores |
| `config/` | Profiles, rest/fuel/safety/vehicle defaults, driving-hours packs |
| `tracks/` | Moving-station store (APRS-style range/timeout) |
| `icons/` | Key resolution + `usvg`/`resvg` PNG raster (SVG assets: see note below) |
| `download/` | Shared pause / resume / cancel progress |
| `sensors/` | gpsd / Linux IMU helpers (feature-gated) |
| `ecu/` | Live-energy types (extension points; no live UniFFI poll yet) |
| `bus/` | `WorldSnapshot` (position + profile + energy) |

Default workspace members for a plain `cargo build` / `cargo test`:
`driver-break-core`, `navi-plugin-host`.

---

## Unaltered third-party crates (crates.io)

Declared primarily under `[workspace.dependencies]` in the root `Cargo.toml`,
plus a few crate-local deps. Versions below are the **workspace pins** (or the
local pin when not workspace-shared). Transitive dependencies of these crates
are also unaltered crates.io packages resolved by Cargo — they are not listed
exhaustively here.

### Navigation / geo / graph

| Crate | Pin | Used for |
|---|---|---|
| [`geo`](https://crates.io/crates/geo) | `0.29` | Geometry operations |
| [`geo-types`](https://crates.io/crates/geo-types) | `0.7` | Point / line types |
| [`rstar`](https://crates.io/crates/rstar) | `0.12` | R-tree spatial index (POI) |
| [`pathfinding`](https://crates.io/crates/pathfinding) | `4` | Graph search primitives used by corridor routing |
| [`osmpbf`](https://crates.io/crates/osmpbf) | `0.3` | Read OSM PBF extracts |
| [`osm4routing`](https://crates.io/crates/osm4routing) | `0.8` | OSM → routing edges during graph build |

### Elevation / DEM / basemap tiles

| Crate | Pin | Used for |
|---|---|---|
| [`srtm_reader`](https://crates.io/crates/srtm_reader) | `0.5` | SRTM HGT tiles |
| [`geotiff`](https://crates.io/crates/geotiff) | `0.1` | GeoTIFF DEM reads |
| [`pmtiles`](https://crates.io/crates/pmtiles) | `0.23` (core; write + mmap-async-tokio) | Regional PMTiles basemap / DEM jobs |
| [`zip`](https://crates.io/crates/zip) | `2` (deflate) | Archives (e.g. DEM / downloads) |
| [`tar`](https://crates.io/crates/tar) | `0.4` | Tar extracts where needed |

### Storage / serialization / concurrency

| Crate | Pin | Used for |
|---|---|---|
| [`rusqlite`](https://crates.io/crates/rusqlite) | `0.32` (bundled SQLite) | Config, FTS, job stores |
| [`serde`](https://crates.io/crates/serde) / [`serde_json`](https://crates.io/crates/serde_json) | `1` | Config, manifests, JSON APIs |
| [`bincode`](https://crates.io/crates/bincode) | `1` | Compact binary (e.g. graph cache) |
| [`tokio`](https://crates.io/crates/tokio) | `1` | Async runtime (downloads, I/O) |
| [`rayon`](https://crates.io/crates/rayon) | `1` | Parallel graph / worker pools |
| [`uuid`](https://crates.io/crates/uuid) | `1` | Ids (routes, jobs, …) |

### HTTP / download

| Crate | Pin | Used for |
|---|---|---|
| [`reqwest`](https://crates.io/crates/reqwest) | `0.12` (rustls-tls, stream, json) | Opt-in downloads (DEM, Geofabrik, PMTiles, …) |
| [`futures-util`](https://crates.io/crates/futures-util) | `0.3` (core) | Stream helpers |
| [`bytes`](https://crates.io/crates/bytes) | `1` (core) | Byte buffers |

### FFI / logging / icons

| Crate | Pin | Used for |
|---|---|---|
| [`uniffi`](https://crates.io/crates/uniffi) | `0.29` | Kotlin/Android (and other) bindings from `navi-ffi` |
| [`android_logger`](https://crates.io/crates/android_logger) | `0.14` (Android target only) | Logcat from native code |
| [`log`](https://crates.io/crates/log) | `0.4` | Logging facade |
| [`usvg`](https://crates.io/crates/usvg) / [`resvg`](https://crates.io/crates/resvg) | `0.44` | SVG → PNG for map overlays |
| [`flate2`](https://crates.io/crates/flate2) / [`png`](https://crates.io/crates/png) | `1` / `0.17` | Compression / PNG encode |

### Plugins (host + guests)

| Crate | Pin | Used for |
|---|---|---|
| [`wasmtime`](https://crates.io/crates/wasmtime) | `29` (cranelift, runtime, gc-drc; default features off) | Sandboxed WASM plugin host |
| [`wee_alloc`](https://crates.io/crates/wee_alloc) | `0.4.5` | Small allocator in example WASM guests |

### Optional / Linux sensors

| Crate | Pin | Used for |
|---|---|---|
| [`gpsd_proto`](https://crates.io/crates/gpsd_proto) | `1.0` (feature `gpsd` on core) | Pure-Rust gpsd TCP/JSON client (no `libgps`) |

### Error / OS / test helpers

| Crate | Pin | Used for |
|---|---|---|
| [`anyhow`](https://crates.io/crates/anyhow) / [`thiserror`](https://crates.io/crates/thiserror) | `1` / `2` | Error handling |
| [`libc`](https://crates.io/crates/libc) | `0.2` | Low-level OS bits where needed |
| [`tempfile`](https://crates.io/crates/tempfile) | `3` (dev) | Tests |

---

## What is *not* altered

| Kind | Policy in this repo |
|---|---|
| crates.io dependencies | Pulled by Cargo at declared semver pins; **no** `[patch.crates-io]` forks for nav libraries |
| Vendored Rust source | **None** — no `vendor/` directory of crate sources |
| Upstream algorithm crates | Used as libraries; Navi does not ship a modified copy of e.g. `pathfinding`, `osm4routing`, or `wasmtime` |

If a future need requires a fork, document it here and prefer an explicit
`[patch]` entry with a clear reason — do not silently vendor.

---

## Related assets (not Cargo crates)

SVG icons under `core/src/icons` are mostly **Navit-derived (GPL-2.0)**, with a
few Navi custom drop-ins. That is asset provenance, not a Rust crate fork — see
[`icons.md`](icons.md).

The Android UI (`app/`) uses **MapLibre** and Kotlin/Compose; those are outside
the Cargo workspace (see [`architecture.md`](architecture.md)).

---

## Planned / documented but not yet Cargo dependencies

These appear in specs only; they are **not** first-party crates and **not** in
`Cargo.toml` today:

| Crate / stack | Doc | Intent |
|---|---|---|
| [`rtl-sdr-rs`](https://crates.io/crates/rtl-sdr-rs) | [`APRS-SDR.md`](APRS-SDR.md) | USB RTL-SDR IQ for APRS |
| [`rodio`](https://crates.io/crates/rodio) / [`cpal`](https://crates.io/crates/cpal), optional Piper | [`voice-guidance.md`](voice-guidance.md) | Voice guidance plugin audio |

---

## How to refresh this list

1. Read root `Cargo.toml` `[workspace.dependencies]` and each member
   `Cargo.toml`.
2. Confirm there is still no `vendor/` tree and no unexpected `[patch]`
   section.
3. Update the tables above when adding a workspace member or a new direct
   dependency.
)
