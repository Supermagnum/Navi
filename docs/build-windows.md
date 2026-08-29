# Building and running on Windows

**There is currently no packaged end-user Navi install for Windows** (no Store /
MSI of the Automotive UI). What you can do on Windows:

| Goal | Support |
|---|---|
| Rust **core** + WASM plugins + Cargo tests | **Supported** with MSVC toolchain |
| Android APK (`app/`) via NDK + Gradle | **Supported** (use Git Bash or a Unix-like shell for `scripts/*.sh`) |
| `navi-desktop` map shell | **Experimental** — `wry` uses WebView2; less exercised than Linux |
| gpsd / Linux IMU SBC path | Prefer Linux; see [`build-linux.md`](build-linux.md) |

Android APK / UniFFI detail: [`android-build.md`](android-build.md).  
Linux desktop / gpsd: [`build-linux.md`](build-linux.md).  
macOS: [`build-macos.md`](build-macos.md).  
Debugging: [`debugging.md`](debugging.md).

---

## Getting the code

Install [Git for Windows](https://git-scm.com/download/win) (or use Git inside
**WSL2**, e.g. `sudo apt install git` — see [`build-linux.md`](build-linux.md#getting-the-code)).
Then in **Git Bash** or PowerShell:

```bash
git clone https://github.com/Supermagnum/Navi.git
cd Navi
git checkout dev
```

**Branch / tag:** Development happens on **`dev`** (newest features). A plain
`git clone` checks out **`main`** (GitHub default). There are no formal release
tags yet. One-step: `git clone -b dev https://github.com/Supermagnum/Navi.git`.

Prefer **Git Bash** (or WSL2) when running `./scripts/build-android-native.sh`
and other bash scripts. Gradle can run from PowerShell via `.\gradlew.bat`.

---

## Needed tools

| Tool | Required for | Notes |
|---|---|---|
| **Git for Windows** | Clone / bash scripts | Includes Git Bash |
| **Visual Studio Build Tools** (MSVC) | Rust `x86_64-pc-windows-msvc` | “Desktop development with C++” workload |
| **Rust (rustup)** | Everything Rust | Default host **MSVC**; channel pin **1.98** |
| **wasm32-unknown-unknown** | WASM plugins / isolation tests | `rustup target add …` |
| **JDK 17** | Android Gradle / Kotlin | Temurin or Android Studio’s JDK |
| **Android SDK** API **36** | APK build | `compileSdk` / `targetSdk` 36; `minSdk` 26 |
| **Android NDK** (e.g. **27.3.x**) | `libnavi.so` | LLVM clang under `windows-x86_64` |
| **adb** (platform-tools) | Install / logcat on device | See [adb](#android-debug-bridge-adb) |
| **Android Studio** (optional) | SDK Manager, emulator | Cmdline-tools alone also work |
| **WebView2 Runtime** | Embedded `navi-desktop` window | Usually preinstalled on Win10/11 |
| **WSL2** (optional) | Linux-like script/env | Follow [`build-linux.md`](build-linux.md) inside WSL for desktop/gpsd |

---

## Prerequisites

### MSVC C toolchain

Install [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
or Visual Studio, with the **Desktop development with C++** workload.

Confirm a developer shell can see the linker (`link.exe`) after installing
Rust’s MSVC target.

### Install Rust (rustup)

From [https://rustup.rs](https://rustup.rs) (or the rustup-init EXE):

```powershell
# After installer, in a new terminal:
rustc --version
cargo --version
```

Choose the default host **`x86_64-pc-windows-msvc`** (recommended). The GNU ABI
is possible but not what this project documents.

**Toolchain pin:** workspace channel **1.98** is for **reproducibility**, not a
proven MSRV matrix (same policy as Linux). Raised from 1.88 because wasmtime 48
requires Rust >= 1.95.

### WASM target (plugins)

```powershell
rustup target add wasm32-unknown-unknown
```

### Android SDK, NDK, and JDK

1. Install **Android Studio** or command-line tools.
2. SDK Manager / `sdkmanager`: Platform **36**, Build-Tools, **NDK**,
   **Platform-Tools**.
3. JDK **17** (Temurin or Studio embedded).

User environment variables (System Properties → Environment Variables), example:

```text
ANDROID_HOME=C:\Users\you\AppData\Local\Android\Sdk
ANDROID_NDK_HOME=%ANDROID_HOME%\ndk\<version>
JAVA_HOME=C:\Program Files\Eclipse Adoptium\jdk-17.x.x-hotspot
Path += %ANDROID_HOME%\platform-tools
Path += %ANDROID_HOME%\cmdline-tools\latest\bin
```

Repo-root `local.properties` (gitignored):

```properties
sdk.dir=C:\\Users\\you\\AppData\\Local\\Android\\Sdk
```

(Gradle accepts either doubled backslashes or forward slashes.)

### Point Cargo at the Windows NDK clang

`.cargo/config.toml` may contain Linux linker paths. On Windows use the
**windows-x86_64** NDK prebuilt (forward slashes are fine in TOML):

```toml
[target.x86_64-linux-android]
linker = "C:/Users/you/AppData/Local/Android/Sdk/ndk/<ver>/toolchains/llvm/prebuilt/windows-x86_64/bin/x86_64-linux-android34-clang.cmd"
rustflags = ["-C", "link-arg=-lc++_shared"]

[target.aarch64-linux-android]
linker = "C:/Users/you/AppData/Local/Android/Sdk/ndk/<ver>/toolchains/llvm/prebuilt/windows-x86_64/bin/aarch64-linux-android34-clang.cmd"
rustflags = ["-C", "link-arg=-lc++_shared"]
```

Export `ANDROID_NDK_HOME` and prefer
`scripts/build-android-native.sh` from **Git Bash** so PATH/clang discovery
matches [`android-build.md`](android-build.md).

### Android Debug Bridge (adb)

**Option A — SDK platform-tools**

```powershell
$env:ANDROID_HOME = "$env:LOCALAPPDATA\Android\Sdk"
$env:Path = "$env:ANDROID_HOME\platform-tools;$env:Path"
adb version
adb devices
```

Install/update **Android SDK Platform-Tools** in SDK Manager, or:

```powershell
sdkmanager "platform-tools"
```

**Option B — standalone zip**

Download [platform-tools for Windows](https://developer.android.com/tools/releases/platform-tools),
unzip, and add that folder to `Path`.

**USB device**

1. Enable **Developer options** on the Android device (hidden by default):
   - **Settings → About phone** / **About tablet**.
   - Tap **Build number** seven times (may be under **Software information**).
   - Open **Settings → System → Developer options** (Samsung often lists
     **Developer options** directly under Settings).
2. Turn on **USB debugging** (and optionally **Wireless debugging**).
3. Install an OEM USB driver if Windows does not enumerate the device
   (Google USB Driver via SDK Manager, or vendor driver).
4. Plug in USB; unlock the device; accept the **Allow USB debugging?** RSA
   fingerprint prompt.
5. `adb devices` → `<serial>    device`.

Wireless debugging: same `adb pair` / `adb connect` flow as other platforms.
Prefer USB for large pushes.

---

## Build the Rust core

**PowerShell or Git Bash:**

```powershell
cd C:\path\to\Navi
cargo build -p driver-break-core --release
cargo test -p driver-break-core
```

WASM plugin example:

```powershell
cargo build --release --target wasm32-unknown-unknown `
  --manifest-path plugins/log-hello/Cargo.toml
```

Fixtures / ignored integration tests: same directory layout as
[`build-linux.md`](build-linux.md#ignored-integration-tests-and-fixtures)
(`core/target/integration-fixtures/`). First downloads need network and roughly
**1 GB** free for common extracts.

---

## Build and install the Android app

Full shared recipe: [`android-build.md`](android-build.md). On Windows, use
**Git Bash** for the native script; Gradle/`adb` may run from Git Bash or
PowerShell.

### Native library (Git Bash)

```bash
cd /c/path/to/Navi
export ANDROID_HOME="${ANDROID_HOME:-$LOCALAPPDATA/Android/Sdk}"
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$ANDROID_HOME/ndk/<version>}"
export PATH="$ANDROID_HOME/platform-tools:$PATH"

rustup target add aarch64-linux-android   # once (phone/tablet)
./scripts/build-android-native.sh aarch64-linux-android release
```

Emulator ABI:

```bash
rustup target add x86_64-linux-android   # once
./scripts/build-android-native.sh x86_64-linux-android release
```

### Gradle APK (PowerShell or Git Bash)

```powershell
.\gradlew.bat :app:assembleDebug
.\gradlew.bat :app:installDebug
adb shell am start -n no.navi.app/.MainActivity
```

Or Git Bash: `./gradlew :app:assembleDebug` then `./gradlew :app:installDebug`.

Manual install:

```powershell
adb install -r app\build\outputs\apk\debug\app-debug.apk
adb shell am start -n no.navi.app/.MainActivity
```

Confirm ABI inside the APK (Git Bash / any unzip):

```bash
unzip -l app/build/outputs/apk/debug/app-debug.apk | grep 'lib/.*/libnavi.so'
```

Launch helper script (Git Bash / emulator):

```bash
./scripts/launch-navi-emulator.sh
```

---

## Desktop map shell (`navi-desktop`) on Windows

Defaults enable `gpsd`, `linux-imu`, and **embedded-webview** (`wry` →
**WebView2**). Ensure [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/)
is installed for the embedded window.

```powershell
cargo run -p navi-desktop -- --demo-imu
cargo run -p navi-desktop -- --demo-imu --no-webview
```

If linking or WebView2 fails, try browser-only without the embedded feature:

```powershell
cargo run -p navi-desktop --no-default-features --features gpsd,linux-imu -- `
  --demo-imu --no-webview
```

`navi-desktop` default data dir logic looks at `HOME`; on Windows set
explicitly:

```powershell
$env:NAVI_DATA_DIR = "$env:LOCALAPPDATA\navi"
# or:  --data-dir "$env:LOCALAPPDATA\navi"
```

Flags and fixtures: [`build-linux.md`](build-linux.md#desktop-map-shell-navi-desktop).

**WSL2 alternative:** clone inside WSL and follow [`build-linux.md`](build-linux.md)
for the desktop shell and gpsd; use Windows-side Android Studio/adb for the
phone if preferred (`adb.exe` from PowerShell).

---

## What does **not** replace Android

| Feature | Status on Windows |
|---|---|
| Full Compose Automotive HUD | Android only |
| MapLibre 3D / offline PMTiles product UI | Android app |
| Installable Windows desktop product | **Not yet** |

Use Windows as a **build and Rust development** host; ship UX via the Android
APK.

---

## Troubleshooting

| Symptom | Likely fix |
|---|---|
| `link.exe` not found / MSVC errors | Install VS Build Tools C++ workload; use `x86_64-pc-windows-msvc` |
| `bash: ./scripts/build-android-native.sh: No such file` | Run from **Git Bash**, or use WSL; do not rely on `cmd.exe` for bash scripts |
| NDK clang `.cmd` not found | Fix `.cargo/config.toml` to `windows-x86_64` prebuilt; check `ANDROID_NDK_HOME` |
| `adb` not recognized | Add `%ANDROID_HOME%\platform-tools` to User `Path`; reopen terminal |
| Device missing in `adb devices` | Enable Developer options (tap **Build number** seven times); USB debugging; OEM USB driver; accept RSA prompt |
| Gradle `sdk.dir` / SDK missing | `local.properties` with correct `sdk.dir` |
| `UnsatisfiedLinkError` / missing `libnavi` | Rebuild native for the correct ABI and reinstall APK |
| WebView2 / wry window fails | Install WebView2 Runtime, or use `--no-webview` |
| Long paths / weird cargo failures | Enable long paths in Windows, or keep the clone near `C:\src\Navi` |
