# Building and running on Linux

Navi’s navigation logic lives in Rust (`driver-break-core`). The interactive map
UI today is the **Android Automotive** host (`app/`). On Linux you compile and
test the **core** (and plugins) with Cargo; there is **no** full desktop map
shell in this repository yet.

Android APK / UniFFI: [`android-build.md`](android-build.md).  
Debugging loops: [`debugging.md`](debugging.md).  
gpsd / IMU on Linux: see § Sensors below and [`imu-calibration.md`](imu-calibration.md).

---

## Prerequisites

| Tool | Notes |
|---|---|
| **Rust** (rustup) | Stable toolchain, edition 2021 (see workspace `Cargo.toml`) |
| **C linker / build-essential** | For crates that compile native bits |
| **pkg-config** | Often needed by transitive native deps |

### System libraries

| Library | Required? | Notes |
|---|---|---|
| **SQLite** | No system package required | Workspace uses `rusqlite` with the **`bundled`** feature |
| **OpenSSL** | Usually not | `reqwest` uses **`rustls-tls`** |
| **GDAL / GEOS** | Not used | Elevation via SRTM/Copernicus readers + geotiff, not GDAL |
| **libgps** | Not used | Optional gpsd client is pure Rust (`gpsd_proto` over TCP/JSON) |

SVG rasterization (`usvg` / `resvg`) and `flate2` / `png` are pure Rust crates —
no extra system SVG libraries.

---

## Workspace crates (Linux-relevant)

| Crate | Role on Linux |
|---|---|
| `core` (`driver-break-core`) | Elevation, routing, POI, rest/safety, search, icons, tracks, sensors |
| `navi-ffi` | UniFFI CDYLIB — mainly for Android; can still `cargo build -p navi-ffi` on Linux |
| `plugin-host` / `plugin-sdk` / `plugins/*` | WASM sandbox; build/test on Linux |
| `navi-linux` (optional binary) | gpsd + IMU demo / sensor feed for desktop/SBC testing |

There is **no** eframe/gtk MapLibre desktop UI crate. Map rendering remains Android
MapLibre for now.

---

## Build the core

```bash
cd /path/to/Navi
cargo build -p driver-break-core --release
```

Debug build:

```bash
cargo build -p driver-break-core
```

Optional features (see `core/Cargo.toml`):

| Feature | Purpose |
|---|---|
| `gpsd` | Enable `gpsd_proto` client helpers in `sensors` |
| `linux-imu` | Enable Linux IMU fusion helpers (board-dependent) |

```bash
cargo build -p driver-break-core --features gpsd,linux-imu
```

---

## Run tests on Linux

Unit / module tests:

```bash
cargo test -p driver-break-core
cargo test -p driver-break-core poi::
cargo test -p driver-break-core config::eco -- --nocapture
```

Ignored integration tests (need fixtures under `core/target/integration-fixtures/`
or as documented in each test):

```bash
cargo test -p driver-break-core --test kongsvinger_lillehammer_integration \
  -- --nocapture --ignored

cargo test -p driver-break-core --test dnt_hiking_integration \
  -- --nocapture --ignored
```

Plugin isolation:

```bash
cargo test -p navi-plugin-host --test isolation -- --nocapture
```

---

## Sensors on Linux (gpsd + IMU)

Architecture (`architecture.md`): GPS/IMU belong on the **highest-priority
sensor tier** — publish-only, non-blocking.

| Source | Role |
|---|---|
| **gpsd** | Position, speed, course via TCP JSON (`?WATCH=…`); crate **`gpsd_proto`** (chosen over `gpsd_client`: same TCP/JSON, no `libgps`, explicit handshake/`get_data` API) |
| **IMU** | Heading / attitude for Compass and Direction-of-travel map modes; separate from gpsd |

Default gpsd socket: `127.0.0.1:2947`. Handshake (via `gpsd_proto::ENABLE_WATCH_CMD`):

```text
?WATCH={"enable":true,"json":true};
```

Parse **TPV** (lat/lon/alt/speed/track) and optionally **SKY** (satellites / fix
quality). Feed samples into `sensors::PositionSample` / `SensorBus`.

IMU is **not** provided by gpsd. On a Linux SBC, use a board IMU (e.g. BMI160 /
BNO055 over I²C/USB) plus software fusion (`linux-imu` feature’s simple filter,
or uf-ahrs / chip fusion in deployment). Compass / Direction-of-travel HUD modes
on Android were proven with **fed** headings; on Linux they should consume
**real** gpsd course and/or IMU heading through the same rotation-mode wiring
(`SensorBus` → host map bearing).

Vehicle mounting pitch/roll zeroing for eco elevation correction is a **deferred**
feature — see [`imu-calibration.md`](imu-calibration.md).

Demo binary:

```bash
cargo run -p navi-linux -- --gpsd 127.0.0.1:2947 --demo-imu
```

With a live gpsd and optional `--demo-imu`, the console prints POS (course) and
IMU (heading) lines — the data path used for Travel / Compass rotation.

---

## What does **not** run on Linux desktop today

| Feature | Status |
|---|---|
| MapLibre map UI | Android only |
| UniFFI Kotlin bindings / Gradle APK | Android NDK build ([`android-build.md`](android-build.md)) |
| Android `LocationManager` / Automotive sensors | Android only |
| Compose HUD / approach box | Android only (logic/state can be shared via core) |
| Full offline nav “app” UX | Use Android emulator or device |

Linux is the right place for **core algorithms, integrations, gpsd/IMU bring-up,
and WASM plugins** before or alongside Automotive UI work.
