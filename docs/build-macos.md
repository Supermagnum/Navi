# Building and running on macOS

**There is currently no packaged end-user Navi install for macOS** (no App Store /
Homebrew cask of the Automotive UI). What you can do on a Mac:

| Goal | Support |
|---|---|
| Rust **core** + WASM plugins + Cargo tests | **Supported** (same as Linux) |
| Android APK (`app/`) via NDK + Gradle | **Supported** (primary product path) |
| `navi-desktop` map shell | **Experimental** — uses `wry`/WKWebView; gpsd/IMU bring-up is still Linux-oriented |
| gpsd / Linux IMU SBC path | Prefer a Linux host; see [`build-linux.md`](build-linux.md) |

Android APK / UniFFI detail: [`android-build.md`](android-build.md).  
Linux desktop / gpsd: [`build-linux.md`](build-linux.md).  
Windows: [`build-windows.md`](build-windows.md).  
Debugging: [`debugging.md`](debugging.md).

---

## Getting the code

Git usually arrives with **Xcode Command Line Tools** (`xcode-select --install`).
Otherwise: `brew install git`. Debian/Ubuntu users on a Mac-adjacent Linux box
can use `sudo apt install git` — full table in
[`build-linux.md`](build-linux.md#getting-the-code).

```bash
git clone https://github.com/Supermagnum/Navi.git
cd Navi
git checkout dev
```

**Branch / tag:** Development happens on **`dev`** (newest features). A plain
`git clone` checks out **`main`** (GitHub default). There are no formal release
tags yet. One-step: `git clone -b dev https://github.com/Supermagnum/Navi.git`.

---

## Needed tools

| Tool | Required for | Notes |
|---|---|---|
| **Xcode Command Line Tools** | Rust native builds | Provides `clang`, `git`, headers |
| **Rust (rustup)** | Everything Rust | Channel pin **1.98** (`rust-toolchain.toml`) |
| **wasm32-unknown-unknown** | WASM plugins / isolation tests | `rustup target add …` |
| **Homebrew** (recommended) | Optional package installs | [https://brew.sh](https://brew.sh) |
| **JDK 17** | Android Gradle / Kotlin | Temurin or Android Studio’s JDK |
| **Android SDK** API **36** | APK build | `compileSdk` / `targetSdk` 36; `minSdk` 26 |
| **Android NDK** (e.g. **27.3.x**) | `libnavi.so` | LLVM clang for Android targets |
| **adb** (platform-tools) | Install / logcat on device | See [adb](#android-debug-bridge-adb) |
| **Android Studio** (optional) | SDK Manager, emulator UI | Not required if you use cmdline-tools |

---

## Prerequisites

### Xcode Command Line Tools

```bash
xcode-select --install
clang --version
```

### Install Rust (rustup)

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
rustc --version
cargo --version
```

Distro/Homebrew Rust packages exist; **rustup** is recommended so Android and
WASM targets stay easy to add.

**Toolchain pin:** workspace channel **1.98** is for **reproducibility**, not a
proven MSRV matrix (same policy as Linux). Raised from 1.88 because wasmtime 48
requires Rust >= 1.95.

### WASM target (plugins)

```bash
rustup target add wasm32-unknown-unknown
```

### Android SDK, NDK, and JDK

1. Install **Android Studio** or [command-line tools](https://developer.android.com/studio#command-line-tools-only).
2. In SDK Manager (or `sdkmanager`), install at least:
   - Android SDK Platform **36**
   - Android SDK **Build-Tools**
   - **NDK** (match a version under `$ANDROID_HOME/ndk/`, e.g. 27.3.x)
   - **Android SDK Platform-Tools** (provides `adb`)
3. Install **JDK 17** if Studio did not already:

```bash
# Homebrew example
brew install --cask temurin@17
```

Shell profile (Apple Silicon paths are typical; adjust if needed):

```bash
export ANDROID_HOME="$HOME/Library/Android/sdk"
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/<version>"   # e.g. 27.3.13750724
export PATH="$ANDROID_HOME/platform-tools:$PATH"
export JAVA_HOME="$(/usr/libexec/java_home -v 17 2>/dev/null || true)"
```

Repo-root `local.properties` (gitignored) for Gradle:

```properties
sdk.dir=/Users/you/Library/Android/sdk
```

### Point Cargo at the macOS NDK clang

`.cargo/config.toml` often has **Linux** linker paths checked in for a single
developer machine. On macOS, set the **darwin** prebuilt folder before the first
Android native build:

```toml
[target.x86_64-linux-android]
linker = "<NDK>/toolchains/llvm/prebuilt/darwin-x86_64/bin/x86_64-linux-android34-clang"
rustflags = ["-C", "link-arg=-lc++_shared"]

[target.aarch64-linux-android]
linker = "<NDK>/toolchains/llvm/prebuilt/darwin-arm64/bin/aarch64-linux-android34-clang"
rustflags = ["-C", "link-arg=-lc++_shared"]
```

On Intel Macs both targets usually use `darwin-x86_64`. Prefer letting
`scripts/build-android-native.sh` prepend `$ANDROID_NDK_HOME/.../bin` to `PATH`
after you export `ANDROID_NDK_HOME` correctly. Details:
[`android-build.md`](android-build.md).

### Android Debug Bridge (adb)

**Option A — SDK platform-tools (recommended with Android Studio)**

```bash
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
export PATH="$ANDROID_HOME/platform-tools:$PATH"
adb version
```

Install/update via SDK Manager → SDK Tools → **Android SDK Platform-Tools**, or:

```bash
yes | "$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager" "platform-tools"
```

**Option B — Homebrew**

```bash
brew install android-platform-tools
adb version
```

**USB device**

1. Enable **Developer options** on the Android device (hidden by default):
   - **Settings → About phone** / **About tablet**.
   - Tap **Build number** seven times (may be under **Software information**).
   - Open **Settings → System → Developer options** (Samsung often lists
     **Developer options** directly under Settings).
2. Turn on **USB debugging** (and optionally **Wireless debugging**).
3. Plug in USB; unlock the device; accept the **Allow USB debugging?** RSA
   prompt.
4. `adb devices` should show `<serial>    device`.

Wireless debugging (Android 11+): **Developer options → Wireless debugging** →
`adb pair` / `adb connect`. Prefer USB for large `adb push` transfers.

---

## Build the Rust core

```bash
cd /path/to/Navi
cargo build -p driver-break-core --release
cargo test -p driver-break-core
```

Optional features (`gpsd`, `linux-imu`) compile on macOS for the shared sensor
bus helpers; a real gpsd daemon is uncommon on Mac — use Linux for live GNSS, or
`--demo-imu` with `navi-desktop` / `navi-linux` where applicable.

WASM plugin example:

```bash
cargo build --release --target wasm32-unknown-unknown \
  --manifest-path plugins/log-hello/Cargo.toml
```

Fixtures / ignored integration tests: same layout as
[`build-linux.md`](build-linux.md#ignored-integration-tests-and-fixtures)
(`core/target/integration-fixtures/`).

---

## Build and install the Android app

Full shared recipe: [`android-build.md`](android-build.md). On macOS, from the
repository root after SDK/NDK/`adb` are set up:

```bash
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$ANDROID_HOME/ndk/<version>}"
export PATH="$ANDROID_HOME/platform-tools:$PATH"

# Physical arm64 device / many tablets
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

Confirm the APK contains the expected ABI:

```bash
unzip -l app/build/outputs/apk/debug/app-debug.apk | grep 'lib/.*/libnavi.so'
```

Manual install:

```bash
adb install -r app/build/outputs/apk/debug/app-debug.apk
adb shell am start -n no.navi.app/.MainActivity
```

---

## Desktop map shell (`navi-desktop`) on macOS

`navi-desktop` defaults to `gpsd` + `linux-imu` + **embedded-webview** (`wry`).
On macOS, `wry` uses **WKWebView** (no WebKitGTK package). Bring-up is less
exercised than Linux:

```bash
# Embedded window + synthetic IMU (no gpsd required for IMU side)
cargo run -p navi-desktop -- --demo-imu

# Or open the system browser instead of the embedded window
cargo run -p navi-desktop -- --demo-imu --no-webview
```

If the default feature set fails to link on your macOS version, try:

```bash
cargo run -p navi-desktop --no-default-features --features gpsd,linux-imu -- \
  --demo-imu --no-webview
```

Flags and fixtures: see [`build-linux.md`](build-linux.md#desktop-map-shell-navi-desktop).
Data dir still defaults toward `~/.local/share/navi` unless you set
`--data-dir` or `NAVI_DATA_DIR`.

---

## What does **not** replace Android

| Feature | Status on macOS |
|---|---|
| Full Compose Automotive HUD | Android only |
| MapLibre 3D / offline PMTiles product UI | Android app |
| Installable Mac app | **Not yet** |

Use the Android app on a device or emulator for product UX. Use macOS as a
**build and Rust development** host.

---

## Troubleshooting

| Symptom | Likely fix |
|---|---|
| `xcrun: error: invalid active developer path` | `xcode-select --install` |
| NDK linker not found / wrong `linux-x86_64` path | Point `.cargo/config.toml` at `darwin-arm64` or `darwin-x86_64`; export `ANDROID_NDK_HOME` |
| `adb: command not found` | Install platform-tools; put `$ANDROID_HOME/platform-tools` on `PATH`, or `brew install android-platform-tools` |
| `adb devices` unauthorized / empty | Unlock phone; enable Developer options (tap **Build number** seven times); turn on USB debugging; accept RSA prompt; retry cable |
| Gradle cannot find SDK | Set `sdk.dir` in `local.properties` |
| `UnsatisfiedLinkError` / missing `libnavi` | Re-run `build-android-native.sh` for the device ABI |
| Odd errors after `git pull` | `cargo clean` then rebuild; `rustup update` |
