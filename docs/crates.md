# Rust crates: created and unaltered

Which Cargo crates Navi **created** in this repository, and which third-party
crates it uses **unaltered** from [crates.io](https://crates.io).

**Summary:** There is no `vendor/` tree and no `[patch.crates-io]` forks of
navigation libraries. Upstream crates are used as published. Navigation logic,
UniFFI surface, plugin sandbox, and example WASM guests are original Navi code.

Also see [`architecture.md`](architecture.md) and
[`codebase-map.md`](codebase-map.md). A short pointer also lives at
[`rust-crates.md`](rust-crates.md).

---

## Created in this repository (first-party)

Workspace members are declared in the root [`Cargo.toml`](../Cargo.toml).
Package names may differ from directory names.

| Directory | Package name | Role |
|---|---|---|
| `core/` | `driver-break-core` | Trusted nav core: OSM graph / A*, eco reweight, elevation, POI, FTS search, rest/HOS, tracks, icons, SQLite, sensors helpers |
| `navi-ffi/` | `navi-ffi` (lib name `navi`) | UniFFI CDYLIB / staticlib bridging core ↔ Android (and other hosts); bindgen and plan/bench helper bins |
| `navi-linux/` | `navi-linux` | Linux gpsd + IMU sensor-feed demo binary (no map UI) |
| `navi-desktop/` | `navi-desktop` | Linux desktop map shell (WebKitGTK + MapLibre GL JS) |
| `plugin-host/` | `navi-plugin-host` | Wasmtime host: fuel / wall-clock limits, capability gate |
| `plugin-sdk/` | `navi-plugin-sdk` | Guest-side wrappers for the HostApi ABI |
| `plugins/log-hello/` | `navi-plugin-log-hello` | Reference WASM guest (example only) |
| `plugins/busy-loop/` | `navi-plugin-busy-loop` | Reference WASM guest for isolation / fuel stress (example only) |

Default workspace members for a plain `cargo build` / `cargo test`:
`driver-break-core`, `navi-plugin-host`.

Core modules under `core/src/` (`routing/`, `poi/`, `search/`, `storage/`,
`config/`, `tracks/`, `icons/`, `download/`, `pack_server/`, `sensors/`, `ecu/`,
`bus/`, `nav/`)
are original Navi code. Upstream crates supply primitives; Navi owns algorithms
and schemas.

---

## Unaltered third-party crates (crates.io)

Declared under `[workspace.dependencies]` in the root `Cargo.toml` and in
member `Cargo.toml` files. Versions are workspace pins (or local pins when not
shared). Transitive crates.io dependencies resolved by Cargo are also unaltered
and are not listed exhaustively.

### Navigation / geo / graph

| Crate | Pin | Used for |
|---|---|---|
| [`geo`](https://crates.io/crates/geo) | `0.29` | Geometry operations |
| [`geo-types`](https://crates.io/crates/geo-types) | `0.7` | Point / line types |
| [`rstar`](https://crates.io/crates/rstar) | `0.12` | R-tree spatial index (POI) |
| [`pathfinding`](https://crates.io/crates/pathfinding) | `4` | Graph search primitives |
| [`osmpbf`](https://crates.io/crates/osmpbf) | `0.3` | Read OSM PBF extracts |
| [`osm4routing`](https://crates.io/crates/osm4routing) | `0.8` | OSM → routing edges during graph build |
| [`opening-hours`](https://crates.io/crates/opening-hours) | `=1.4.0` | OSM opening_hours evaluation (e.g. speed cameras) |

### Elevation / DEM / basemap tiles

| Crate | Pin | Used for |
|---|---|---|
| [`srtm_reader`](https://crates.io/crates/srtm_reader) | `0.5` | SRTM HGT tiles |
| [`geotiff`](https://crates.io/crates/geotiff) | `0.1` | GeoTIFF DEM reads |
| [`pmtiles`](https://crates.io/crates/pmtiles) | `0.23` | Regional PMTiles basemap / DEM jobs |
| [`zip`](https://crates.io/crates/zip) | `2` | Archives (DEM / downloads) |
| [`tar`](https://crates.io/crates/tar) | `0.4` | Tar extracts where needed |

### Storage / serialization / concurrency

| Crate | Pin | Used for |
|---|---|---|
| [`rusqlite`](https://crates.io/crates/rusqlite) | `0.32` (bundled SQLite) | Config, FTS, job stores |
| [`serde`](https://crates.io/crates/serde) / [`serde_json`](https://crates.io/crates/serde_json) | `1` | Config, manifests, JSON APIs |
| [`bincode`](https://crates.io/crates/bincode) | `1` | Compact binary (e.g. graph cache) |
| [`rkyv`](https://crates.io/crates/rkyv) | `0.8` | Zero-copy indexed map packs |
| [`memmap2`](https://crates.io/crates/memmap2) | `0.9` | Memory-map pack files |
| [`tokio`](https://crates.io/crates/tokio) | `1` | Async runtime (downloads, I/O) |
| [`rayon`](https://crates.io/crates/rayon) | `1` | Parallel graph / worker pools |
| [`uuid`](https://crates.io/crates/uuid) | `1` | Ids (routes, jobs, …) |
| [`chrono`](https://crates.io/crates/chrono) | `0.4` | Local time / calendars |

### HTTP / download

| Crate | Pin | Used for |
|---|---|---|
| [`reqwest`](https://crates.io/crates/reqwest) | `0.12` (rustls-tls, stream, json) | Opt-in downloads |
| [`futures-util`](https://crates.io/crates/futures-util) | `0.3` | Stream helpers |
| [`bytes`](https://crates.io/crates/bytes) | `1` | Byte buffers |

### FFI / logging / icons

| Crate | Pin | Used for |
|---|---|---|
| [`uniffi`](https://crates.io/crates/uniffi) | `0.29` | Kotlin/Android (and other) bindings from `navi-ffi` |
| [`android_logger`](https://crates.io/crates/android_logger) | `0.14` (Android target) | Logcat from native code |
| [`log`](https://crates.io/crates/log) | `0.4` | Logging facade |
| [`env_logger`](https://crates.io/crates/env_logger) | (desktop/linux) | Host stderr logging |
| [`usvg`](https://crates.io/crates/usvg) / [`resvg`](https://crates.io/crates/resvg) | `0.44` | SVG → PNG for map overlays |
| [`flate2`](https://crates.io/crates/flate2) / [`png`](https://crates.io/crates/png) | `1` / `0.17` | Compression / PNG encode |

### Plugins (host + guests)

| Crate | Pin | Used for |
|---|---|---|
| [`wasmtime`](https://crates.io/crates/wasmtime) | (plugin-host) | Sandboxed WASM plugin host |
| [`wee_alloc`](https://crates.io/crates/wee_alloc) | `0.4.5` | Small allocator in example WASM guests |

### Desktop / Linux UI helpers

| Crate | Pin | Used for |
|---|---|---|
| [`wry`](https://crates.io/crates/wry) / [`tao`](https://crates.io/crates/tao) | (navi-desktop) | Embedded WebView window shell |
| [`axum`](https://crates.io/crates/axum) / [`tower-http`](https://crates.io/crates/tower-http) | (navi-desktop) | Local HTTP for map assets / APIs |
| [`include_dir`](https://crates.io/crates/include_dir) | (navi-desktop) | Embed static web assets |
| [`gpsd_proto`](https://crates.io/crates/gpsd_proto) | `1.0` (feature `gpsd` on core) | Pure-Rust gpsd TCP/JSON client |

### Error / OS / test helpers

| Crate | Pin | Used for |
|---|---|---|
| [`anyhow`](https://crates.io/crates/anyhow) / [`thiserror`](https://crates.io/crates/thiserror) | `1` / `2` | Error handling |
| [`libc`](https://crates.io/crates/libc) | `0.2` | Low-level OS bits where needed |
| [`tempfile`](https://crates.io/crates/tempfile) | `3` (dev) | Tests |

---

## What is not altered

| Kind | Policy |
|---|---|
| crates.io dependencies | Semver pins only; no silent forks |
| Vendored Rust source | None — no `vendor/` tree |
| Upstream algorithm crates | Used as libraries (`pathfinding`, `osm4routing`, `wasmtime`, …) |

If a fork is required later, document it here with an explicit `[patch]` entry.

---

## Related (not Cargo crates)

- SVG icons under `core/src/icons`: mostly Navit-derived (GPL-2.0) — see
  [`icons.md`](icons.md).
- Android UI (`app/`): MapLibre + Kotlin/Compose, outside the Cargo workspace.

## Planned but not yet in Cargo.toml

| Stack | Doc |
|---|---|
| [`rtl-sdr-rs`](https://crates.io/crates/rtl-sdr-rs) | [`APRS-SDR.md`](APRS-SDR.md) |
| [`rodio`](https://crates.io/crates/rodio) / [`cpal`](https://crates.io/crates/cpal) | [`voice-guidance.md`](voice-guidance.md) |
| [`meshtastic`](https://crates.io/crates/meshtastic) (`bluetooth-le`, `tokio`) | [`plugins/lora-convoy-spec.md`](plugins/lora-convoy-spec.md) — host-native BLE client to a Meshtastic node |

## How to refresh

1. Read root `Cargo.toml` `[workspace.dependencies]` and each member
   `Cargo.toml`.
2. Confirm there is still no `vendor/` tree and no unexpected `[patch]`.
3. Update the tables when adding a workspace member or direct dependency.
