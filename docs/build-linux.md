# Building and running on Linux

**There is currently no packaged end-user Navi install (`.deb` / Flatpak) for
Linux.** What *does* exist is a **desktop map shell** (`navi-desktop`) plus the
Rust **core**, `navi-linux` gpsd/IMU demo, and WASM plugins. The richest
Automotive UX remains the Android host (`app/`); the Linux shell is the
developer / bring-up map UI.

Navi’s navigation logic lives in Rust (`driver-break-core`). On Linux you compile
that core (and plugins) with Cargo, and optionally run **`navi-desktop`** for a
real MapLibre map with route planning and live position.

Android APK / UniFFI: [`android-build.md`](android-build.md).  
Debugging loops: [`debugging.md`](debugging.md).  
Plugin build details: [`plugins.md`](plugins.md).  
Map styles / offline PMTiles: [`map-styles.md`](map-styles.md).  
gpsd / IMU on Linux: see [Sensors](#sensors-on-linux-gpsd--imu) below and
[`imu-calibration.md`](imu-calibration.md).  
ADB / device install from Linux: [Android Debug Bridge (adb)](#android-debug-bridge-adb).  
macOS host: [`build-macos.md`](build-macos.md).  
Windows host: [`build-windows.md`](build-windows.md).

---

## Getting the code

Clone the repository:

```bash
git clone https://github.com/Supermagnum/Navi.git
cd Navi
```

**Branch / tag:** Development happens on `main`. There is **no** formal release
or stable-tag process yet — use `main` for the latest code. If tags appear later,
prefer a documented release tag for a frozen checkout; until then, treat `main`
as the only supported tip.

---

## Prerequisites

### Install Rust (rustup)

If you do not already have a Rust toolchain:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Follow the installer prompts, then reload your shell (or `source "$HOME/.cargo/env"`).
Confirm:

```bash
rustc --version
cargo --version
```

Distro packages of Rust exist, but **rustup** is recommended so you can add
Android and WASM targets cleanly (see [`android-build.md`](android-build.md) and
below).

**Toolchain pin:** Workspace `rust-version` / `rust-toolchain.toml` channel is
**1.88**. That pin is for **build reproducibility** (CI and local rustup install
the exact channel). It is **not** a formally verified minimum: there is no CI
matrix job building an older claimed MSRV. Do not lower the pin without proving
the dependency set still compiles; do not claim “MSRV verified” without adding
that matrix.

### WASM target (plugins)

Guest plugins compile to **`wasm32-unknown-unknown`** (not WASI — guests get no
WASI filesystem or network; see [`plugins.md`](plugins.md)). Add the target
before building example plugins or running host isolation tests that compile
them:

```bash
rustup target add wasm32-unknown-unknown
```

### System packages (C linker / pkg-config)

Native bits in the dependency tree need a C toolchain and often `pkg-config`.

**Debian / Ubuntu:**

```bash
sudo apt update
sudo apt install build-essential pkg-config
```

**Fedora:**

```bash
sudo dnf groupinstall "Development Tools"
sudo dnf install pkgconf-pkg-config
```

**Arch Linux:**

```bash
sudo pacman -S --needed base-devel pkgconf
```

### WebKitGTK (for the embedded `navi-desktop` window)

The desktop shell embeds MapLibre GL JS via **WebKitGTK** (`wry` on Linux).
Install the development package so `cargo build -p navi-desktop` can link:

**Debian / Ubuntu:**

```bash
sudo apt install libwebkit2gtk-4.1-dev
```

**Fedora:**

```bash
sudo dnf install webkit2gtk4.1-devel
```

**Arch Linux:**

```bash
sudo pacman -S webkit2gtk-4.1
```

Without those headers you can still build with
`cargo build -p navi-desktop --no-default-features --features gpsd,linux-imu`
and open the UI in a normal browser (`--no-webview` / `--browser`).

### Android Debug Bridge (adb)

`adb` is **not** required for `navi-desktop` or Cargo core tests. Install it
when you talk to a phone/tablet/emulator from this Linux host — install APKs,
`adb devices`, `adb logcat`, push fixtures, or run Gradle
`:app:installDebug`. Full Android/NDK build steps:
[`android-build.md`](android-build.md).

**Option A — distro package (fastest for adb alone)**

**Debian / Ubuntu:**

```bash
sudo apt update
sudo apt install adb android-sdk-platform-tools-common
```

(`android-sdk-platform-tools-common` ships udev rules so USB devices show up
without running adb as root.)

**Fedora:**

```bash
sudo dnf install android-tools
```

**Arch Linux:**

```bash
sudo pacman -S --needed android-tools
```

Confirm:

```bash
adb version
adb devices
```

**Option B — Google platform-tools (matches Android Studio / SDK)**

If you already use the Android SDK (needed to *build* the APK anyway):

```bash
# Typical SDK location after Android Studio or cmdline-tools:
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Android/Sdk}"
export PATH="$ANDROID_HOME/platform-tools:$PATH"
```

Install or refresh **platform-tools** with `sdkmanager` (or Android Studio →
SDK Manager → SDK Tools → Android SDK Platform-Tools):

```bash
# Example with cmdline-tools already under $ANDROID_HOME:
yes | "$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager" "platform-tools"
```

Or download the standalone zip from
[Google’s platform-tools page](https://developer.android.com/tools/releases/platform-tools),
unpack it, and put that `platform-tools/` directory on your `PATH`.

**USB device access**

1. On the Android device, enable **Developer options** (hidden by default):
   - Open **Settings → About phone** (or **About tablet** / **About device**).
   - Find **Build number** (sometimes under **Software information**).
   - Tap **Build number** seven times until the device reports that you are a
     developer (you may need to unlock with PIN/pattern).
   - Go back to **Settings → System → Developer options** (on some Samsung
     devices: **Settings → Developer options**).
2. In **Developer options**, turn on **USB debugging**.
   Optionally also enable **Wireless debugging** (Android 11+) if you will use
   `adb` over Wi‑Fi.
3. Plug in USB; unlock the device and accept the **Allow USB debugging?** RSA
   fingerprint prompt (check **Always allow from this computer** if you trust
   this host).
4. On Linux, if `adb devices` shows `unauthorized` or an empty list while the
   cable is connected, install the udev package above (Debian/Ubuntu) or copy
   rules from
   [android-udev-rules](https://github.com/M0Rf30/android-udev-rules), then
   replug and run `adb kill-server && adb start-server && adb devices`.

Expect a line like `<serial>    device` (not `offline` / `unauthorized`).

**Wireless debugging (optional)**

On Android 11+ you can pair over Wi‑Fi (**Developer options → Wireless
debugging**) and use `adb pair <ip>:<port>` then `adb connect <ip>:<port>`.
Prefer USB for first-time installs and large `adb push` transfers.

### System libraries

| Library | Required? | Notes |
|---|---|---|
| **SQLite** | No system package required | Workspace uses `rusqlite` with the **`bundled`** feature |
| **OpenSSL** | Usually not | `reqwest` uses **`rustls-tls`** |
| **GDAL / GEOS** | Not used | Elevation via SRTM/Copernicus readers + geotiff, not GDAL |
| **libgps** | Not used | Optional gpsd client is pure Rust (`gpsd_proto` over TCP/JSON) |
| **WebKitGTK 4.1** | For embedded desktop window | See above; runtime + `-dev` for the default `embedded-webview` feature |

SVG rasterization (`usvg` / `resvg`) and `flate2` / `png` are pure Rust crates —
no extra system SVG libraries.

---

## Build and install the Android app

Full shared recipe: [`android-build.md`](android-build.md). On Linux, from the
repository root after SDK/NDK/`adb` are set up ([Prerequisites](#prerequisites)
and [adb](#android-debug-bridge-adb) above):

```bash
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Android/Sdk}"
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$ANDROID_HOME/ndk/<version>}"
export PATH="$ANDROID_HOME/platform-tools:$PATH"

# Physical arm64 tablet / phone
rustup target add aarch64-linux-android   # once
./scripts/build-android-native.sh aarch64-linux-android release
./gradlew :app:assembleDebug
./gradlew :app:installDebug
adb shell am start -n no.navi.app/.MainActivity
```

Emulator (x86_64 image):

```bash
rustup target add x86_64-linux-android   # once
./scripts/build-android-native.sh x86_64-linux-android release
./gradlew :app:installDebug
./scripts/launch-navi-emulator.sh
```

Confirm the APK embeds `libnavi.so` for the ABI you built:

```bash
unzip -l app/build/outputs/apk/debug/app-debug.apk | grep 'lib/.*/libnavi.so'
```

Manual install (same as Gradle `:app:installDebug`):

```bash
adb install -r app/build/outputs/apk/debug/app-debug.apk
adb shell am start -n no.navi.app/.MainActivity
```

---

## Workspace crates (Linux-relevant)

| Crate | Role on Linux |
|---|---|
| `core` (`driver-break-core`) | Elevation, routing, POI, rest/safety, search, icons, tracks, sensors |
| `navi-ffi` | UniFFI CDYLIB — Android ABI; also linked by `navi-desktop` for plan/search helpers |
| `navi-desktop` | **Desktop map shell** (WebKitGTK + MapLibre GL JS) |
| `plugin-host` / `plugin-sdk` / `plugins/*` | WASM sandbox; build/test on Linux |
| `navi-linux` | gpsd + IMU console demo (no map UI) |

---

## Desktop map shell (`navi-desktop`)

### Rendering choice (spike result)

Two paths were considered:

| Option | Stack | Result |
|---|---|---|
| **A** | `eframe`/`egui` + [`maplibre-rs`](https://github.com/maplibre/maplibre-rs) | **Rejected for this pass.** Upstream still describes itself as a **proof-of-concept** (crates.io ~`0.0.3`); missing labels/symbols/raster; **no clear Protomaps PMTiles offline path** matching Navi’s existing Android basemap work. |
| **B** (chosen) | **WebKitGTK** (`wry`/`tao`) + **MapLibre GL JS** + local `axum` HTTP | **Selected.** Same MapLibre / OpenFreeMap / Protomaps ecosystem as Android; mature PMTiles protocol in JS; thin Rust server serves range requests against local `.pmtiles` and calls `driver-break-core` / `navi-ffi` for routing, search, and icons. Tradeoff: C WebKit dependency (documented above). |

Basemap resolution order mirrors Android `BasemapStyleResolver`: completed local
Protomaps PMTiles covering the camera when available, otherwise OpenFreeMap
**Liberty** online (`--force-online` forces Liberty). 3D / Mapterhorn hillshade
is out of scope for this shell. See [`map-styles.md`](map-styles.md).

Sensors reuse the same `SensorBus` path as `navi-linux` (gpsd + optional
`--demo-imu`). Routing uses `plan_car_route` from `navi-ffi`; icons use core
`usvg`/`resvg` rasterization.

### Build and run

```bash
# After WebKitGTK -dev is installed:
cargo run -p navi-desktop -- --demo-imu

# Or open the system browser instead of the embedded window:
cargo run -p navi-desktop -- --demo-imu --no-webview
```

Useful flags:

| Flag | Purpose |
|---|---|
| `--data-dir DIR` | Config / caches (default `~/.local/share/navi` or `$NAVI_DATA_DIR`) |
| `--pbf PATH` | OSM extract for planning (defaults to Ostlandet under data-dir or `core/target/integration-fixtures/`) |
| `--pmtiles PATH` | Force offline Protomaps basemap |
| `--force-online` | Always use OpenFreeMap Liberty |
| `--place-index PATH` | FTS5 DB for place search |
| `--gpsd HOST:PORT` | gpsd address (default `127.0.0.1:2947`) |
| `--demo-imu` | Synthetic IMU without hardware |
| `--listen HOST:PORT` | Bind for the local HTTP UI (default `127.0.0.1:0`) |
| `--no-webview` / `--browser` | Use `xdg-open` instead of embedded WebKit |

Example with fixtures already on disk (see [fixtures](#ignored-integration-tests-and-fixtures) and [`map-styles.md`](map-styles.md)):

```bash
cargo run -p navi-desktop -- --demo-imu --no-webview \
  --pbf core/target/integration-fixtures/oppland-latest.osm.pbf \
  --pmtiles core/target/integration-fixtures/europe_norway_ostlandet.pmtiles \
  --elev-dir core/target/integration-fixtures/elevation
```

Enter start/end as lat,lon (or use place search when `--place-index` is set).
First graph build from a large PBF is slow — prefer a regional extract when
possible.

Packaging (`.deb` / Flatpak) is **out of scope** for this pass — `cargo run` is
enough.

---

## Build the core

From the repository root (after [Getting the code](#getting-the-code)):

```bash
cd /path/to/Navi   # e.g. ~/Navi after git clone
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

### Build a reference WASM plugin

Requires `wasm32-unknown-unknown` (see [Prerequisites](#wasm-target-plugins)):

```bash
cargo build --release --target wasm32-unknown-unknown \
  --manifest-path plugins/log-hello/Cargo.toml
```

More detail: [`plugins.md`](plugins.md).

---

## Run tests on Linux

Unit / module tests (no large downloads):

```bash
cargo test -p driver-break-core
cargo test -p driver-break-core poi::
cargo test -p driver-break-core config::eco -- --nocapture
cargo test -p driver-break-core truck_multi_day -- --nocapture
cargo test -p driver-break-core motor_multi_day -- --nocapture
cargo test -p driver-break-core rest_area -- --nocapture
cargo test -p driver-break-core lodging -- --nocapture
cargo test -p driver-break-core --test truck_driving_history -- --nocapture
```

### Ignored integration tests and fixtures

Several `--ignored` tests live under `core/tests/` and write under
`core/target/integration-fixtures/`. On first run they **download** OSM extracts
(and DEM tiles where needed) over the network, then reuse the cache.

| Fixture | Source (approx. size) |
|---|---|
| `ostlandet-latest.osm.pbf` | [Geofabrik](https://download.geofabrik.de/europe/norway/ostlandet-latest.osm.pbf) (~430–450 MB) |
| `hedmark-latest.osm.pbf` | [OSM.fr](https://download.openstreetmap.fr/extracts/europe/norway/hedmark-latest.osm.pbf) (~90 MB) |
| `oppland-latest.osm.pbf` | [OSM.fr](https://download.openstreetmap.fr/extracts/europe/norway/oppland-latest.osm.pbf) (~90 MB) |
| DEM under `integration-fixtures/elevation/` | Downloaded by the test via core elevation jobs (corridor order ~100–200 MB once cached) |
| `europe_norway_ostlandet.pmtiles` | Regional Protomaps extract (~180 MB) — for `navi-desktop` offline basemap; see [`map-styles.md`](map-styles.md) |

**Disk space:** Plan on **about 1 GB free** for a first pass of the common
ignored routing/hiking tests (OSM extracts + DEM). The fixtures directory can
grow to **several GB** if you also keep optional artefacts there (e.g. full
region PMTiles / DEM PMTiles from map tooling — not required for these Cargo
tests). See also [`test-results.md`](test-results.md) for recent PASS timings.

You do **not** need to download fixtures by hand for
`kongsvinger_lillehammer_integration` / `dnt_hiking_integration`: those tests
call `download_if_missing` themselves. Manual prefetch (optional):

```bash
mkdir -p core/target/integration-fixtures
curl -L -o core/target/integration-fixtures/ostlandet-latest.osm.pbf \
  https://download.geofabrik.de/europe/norway/ostlandet-latest.osm.pbf
# hedmark / oppland URLs as in the table above, same directory
```

Some other ignored tests only assert that `ostlandet-latest.osm.pbf` (or a
smaller corridor cut) already exists — place it under
`core/target/integration-fixtures/` as above, or see comments in the specific
`core/tests/*.rs` file. Android corridor helpers:
[`android-test-results.md`](android-test-results.md) and
`scripts/prepare-android-fixtures.sh`.

```bash
cargo test -p driver-break-core --test kongsvinger_lillehammer_integration \
  -- --nocapture --ignored

cargo test -p driver-break-core --test dnt_hiking_integration \
  -- --nocapture --ignored
```

Plugin isolation (builds example guests for `wasm32-unknown-unknown` as needed):

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

### Install gpsd

**Debian / Ubuntu:**

```bash
sudo apt install gpsd gpsd-clients
```

**Fedora:**

```bash
sudo dnf install gpsd gpsd-clients
```

**Arch Linux:**

```bash
sudo pacman -S gpsd
```

(`gpsd-clients` provides helpers such as `cgps` / `gpspipe` on Debian-family
distros; package names vary slightly.)

### Point gpsd at a GPS device

Typical USB GNSS dongle or serial adapter (device node may differ — check
`dmesg` / `ls /dev/ttyUSB*` / `ls /dev/ttyACM*`):

```bash
# Example: USB serial GNSS on /dev/ttyUSB0
sudo gpsd -N -n /dev/ttyUSB0
```

Or, if your distro ships a systemd unit and device autodetection is configured:

```bash
sudo systemctl enable --now gpsd
```

Edit distro gpsd defaults (e.g. `/etc/default/gpsd` on Debian) so `DEVICES`
lists your port when using the service. Default TCP port for clients:
**`127.0.0.1:2947`**.

### Verify gpsd before `navi-linux` / `navi-desktop`

```bash
# JSON stream: a few TPV/SKY sentences then exit
gpspipe -w -n 5

# Interactive curses client (wait for a fix)
cgps -s
```

You want a listening gpsd and, outdoors or with a simulator, a valid fix in
`TPV`. Handshake used by Navi (via `gpsd_proto::ENABLE_WATCH_CMD`):

```text
?WATCH={"enable":true,"json":true};
```

Parse **TPV** (lat/lon/alt/speed/track) and optionally **SKY** (satellites / fix
quality). Feed samples into `sensors::PositionSample` / `SensorBus`.

### No GPS / IMU hardware yet (demo on-ramp)

`navi-linux` and `navi-desktop` enable `gpsd` + `linux-imu` by default. For
bring-up **without an IMU chip**, use **`--demo-imu`**: it publishes a rotating
synthetic heading on `SensorBus`.

```bash
# Console sensor demo
cargo run -p navi-linux -- --demo-imu

# Map shell with synthetic IMU
cargo run -p navi-desktop -- --demo-imu --no-webview
```

Without a receiver, start gpsd only when you have a device (or a gpsd-compatible
simulator such as `gpsfake`); otherwise the gpsd thread may exit with an error
while **`--demo-imu`** continues to publish IMU samples. That is the intended
**no-hardware** on-ramp for the IMU side.

IMU is **not** provided by gpsd. On a Linux SBC with a real chip, use a board IMU
(e.g. BMI160 / BNO055 over I²C/USB) plus software fusion (`linux-imu` feature’s
simple filter, or uf-ahrs / chip fusion in deployment). Compass /
Direction-of-travel HUD modes on Android were proven with **fed** headings; on
Linux they should consume **real** gpsd course and/or IMU heading through the
same rotation-mode wiring (`SensorBus` → host map bearing).

Vehicle mounting pitch/roll zeroing for eco elevation correction is a **deferred**
feature — see [`imu-calibration.md`](imu-calibration.md).

With a live gpsd and optional `--demo-imu`, `navi-linux` prints POS (course) and
IMU (heading) lines — the same bus `navi-desktop` polls for the position marker.

---

## What does **not** run on Linux desktop today

| Feature | Status |
|---|---|
| MapLibre map + route planning shell | **`navi-desktop`** (this doc) — not pixel-parity with Android HUD |
| UniFFI Kotlin bindings / Gradle APK | Android NDK build ([`android-build.md`](android-build.md)) |
| Android `LocationManager` / Automotive sensors | Android only (Linux uses gpsd / IMU) |
| Compose HUD / approach-box animations / 3D terrain | Android only for now |
| Full Automotive UX parity | Use Android emulator or device |
| Installable Linux package (`.deb` / Flatpak) | **Not yet** — `cargo run -p navi-desktop` |

Linux is the right place for **core algorithms, the desktop map shell,
integrations, gpsd/IMU bring-up, and WASM plugins** alongside Automotive UI work.

---

## Troubleshooting

| Symptom | Likely fix |
|---|---|
| `linker` / `cc` not found, or `error: linker \`cc\` not found` | Install the C toolchain ([System packages](#system-packages-c-linker--pkg-config): `build-essential` / `base-devel` / Development Tools). |
| `navi-desktop` fails to find `webkit2gtk-4.1` | `sudo apt install libwebkit2gtk-4.1-dev` (or distro equivalent), or build with `--no-default-features --features gpsd,linux-imu` and `--no-webview`. |
| Plugin or isolation test fails to compile for WASM | `rustup target add wasm32-unknown-unknown` (guests are **not** `wasm32-wasi`). See [`plugins.md`](plugins.md). |
| `navi-linux` / desktop: gpsd loop ends / no `POS` | Install and start gpsd; verify with `gpspipe -w -n 5`. Use `--demo-imu` for IMU without a chip. |
| Ignored integration test fails on missing PBF / HTTP error | Need network on first run, or prefetch extracts into `core/target/integration-fixtures/` ([fixtures](#ignored-integration-tests-and-fixtures)). Ensure ~1 GB free disk. |
| First `api/plan` is very slow | Cold graph build from a large PBF; prefer a regional extract (e.g. Oppland) or wait for cache under `--cache-dir`. |
| Odd compile errors after pulling `main` | `cargo clean` then rebuild; ensure rustup **stable** is current (`rustup update`). Do not hand-edit `Cargo.lock` unless resolving a deliberate pin — prefer `cargo update` / a clean tree from `main`. |
| `pkg-config` errors from a transitive crate | Install `pkg-config` / `pkgconf` for your distro (see Prerequisites). |
| `adb: command not found` | Install adb ([Android Debug Bridge](#android-debug-bridge-adb)): distro `adb` / `android-tools`, or `$ANDROID_HOME/platform-tools` on `PATH`. |
| `adb devices` empty / `unauthorized` | Enable Developer options (tap **Build number** seven times under About phone/tablet), turn on **USB debugging**, accept the RSA prompt; install udev rules (`android-sdk-platform-tools-common` or equivalent); `adb kill-server && adb start-server`. |
