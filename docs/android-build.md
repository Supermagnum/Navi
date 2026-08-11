# Building Android packages

Navi’s Android app (`app/`, application id `no.navi.app`) links a Rust CDYLIB
(`navi-ffi` → `libnavi.so`) via UniFFI/JNA. Build native code first, then Gradle
APKs. This works from **Linux**, **macOS**, and **Windows** hosts.

## Which document to open

| Step | Document |
|---|---|
| Install JDK 17, Android SDK/NDK, `adb`, set `ANDROID_HOME` | Host OS guide: [Linux](build-linux.md), [macOS](build-macos.md), [Windows](build-windows.md) |
| Build `libnavi.so`, assemble/install APK, emulator launch | **This page** |
| Short cheat-sheet in the README | [README — Building and installing](../README.md#building-and-installing) |

On **Windows**, run `./scripts/build-android-native.sh` from **Git Bash** (or WSL).
Gradle may use `.\gradlew.bat` from PowerShell. On **Linux/macOS**, use a normal
bash Terminal and `./gradlew`.

## Prerequisites

| Tool | Notes |
|---|---|
| **Rust** (rustup) | Stable toolchain |
| Android targets | `rustup target add x86_64-linux-android aarch64-linux-android` |
| **Android SDK** | API **36** (`compileSdk` / `targetSdk`); `minSdk` **26** |
| **Android NDK** | LLVM toolchain (example in-tree: NDK **27.3.x**) |
| **JDK 17** | For Gradle / Kotlin |
| **Gradle wrapper** | `./gradlew` / `gradlew.bat` at repo root (Gradle **8.11.1**; AGP **8.9.1**) |

Per-PR CI validates `gradle/wrapper/gradle-wrapper.jar` against the checksum list
shipped with [`gradle/actions/setup-gradle`](https://github.com/gradle/actions).
After bumping the Gradle version, regenerate the wrapper from the official
distribution (do not copy a JAR from another repo):

```bash
./gradlew wrapper --gradle-version=8.11.1 --distribution-type=bin
```

Commit `gradle-wrapper.jar`, `gradle-wrapper.properties`, `gradlew`, and
`gradlew.bat` together.

### Environment variables (all hosts)

```bash
# Typical SDK roots — pick the line for your OS:
#   Linux:   export ANDROID_HOME="$HOME/Android/Sdk"
#   macOS:   export ANDROID_HOME="$HOME/Library/Android/sdk"
#   Windows (Git Bash): export ANDROID_HOME="$LOCALAPPDATA/Android/Sdk"

export ANDROID_HOME=…   # required for Gradle / sdkmanager
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/<version>"   # required for native build
export PATH="$ANDROID_HOME/platform-tools:$PATH"      # so `adb` is found
```

`scripts/build-android-native.sh` requires `ANDROID_NDK_HOME` (or `ANDROID_HOME`
with an `ndk/<version>` folder). It prepends the correct NDK host prebuilt
(`linux-x86_64`, `darwin-arm64` / `darwin-x86_64`, or `windows-x86_64`) to
`PATH`.

### Point Cargo at your NDK

`.cargo/config.toml` currently contains **machine-local** linker paths. Update
both targets to your NDK’s prebuilt clang before the first native build:

**Linux**

```toml
[target.x86_64-linux-android]
linker = "<NDK>/toolchains/llvm/prebuilt/linux-x86_64/bin/x86_64-linux-android34-clang"
rustflags = ["-C", "link-arg=-lc++_shared"]

[target.aarch64-linux-android]
linker = "<NDK>/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android34-clang"
rustflags = ["-C", "link-arg=-lc++_shared"]
```

**macOS** — use `darwin-arm64` or `darwin-x86_64` instead of `linux-x86_64`
(see [`build-macos.md`](build-macos.md)).

**Windows** — use `windows-x86_64` and the `.cmd` clang wrappers (see
[`build-windows.md`](build-windows.md)).

Optional: `local.properties` in the repo root (not committed) for Gradle:

```properties
# Linux example:
sdk.dir=/home/you/Android/Sdk
# macOS:   sdk.dir=/Users/you/Library/Android/sdk
# Windows: sdk.dir=C:\\Users\\you\\AppData\\Local\\Android\\Sdk
```

---

## 1. Build the native library + UniFFI Kotlin

From the repository root (bash):

```bash
# Emulator (x86_64) — default
./scripts/build-android-native.sh x86_64-linux-android release

# Physical arm64 device / many Automotive head units
./scripts/build-android-native.sh aarch64-linux-android release

# Faster iterate (larger .so)
./scripts/build-android-native.sh x86_64-linux-android debug
```

What the script does:

1. `cargo build -p navi-ffi --target <triple> --release|--debug --lib`
2. Copies `target/<triple>/<profile>/libnavi.so` →
   `app/src/main/jniLibs/<abi>/libnavi.so`  
   (`x86_64` or `arm64-v8a`)
3. Regenerates Kotlin bindings under `app/src/main/java/` via
   `uniffi-bindgen`

Re-run this whenever Rust/`navi-ffi` exports change. Skipping it leaves a stale
`.so` or bindings and the APK will fail to link or call missing symbols.

---

## 2. Assemble / install APKs (Gradle)

```bash
# Debug APK only (output under app/build/outputs/apk/debug/)
./gradlew :app:assembleDebug

# Install debug on a connected device/emulator (preferred)
./gradlew :app:installDebug

# Release APK (minify currently off in app/build.gradle.kts)
./gradlew :app:assembleRelease
# → app/build/outputs/apk/release/app-release.apk
```

Windows PowerShell:

```powershell
.\gradlew.bat :app:assembleDebug
.\gradlew.bat :app:installDebug
```

Debug APK path (typical):

```text
app/build/outputs/apk/debug/app-debug.apk
```

Release APK / AAB:

```text
app/build/outputs/apk/release/app-release.apk
app/build/outputs/bundle/release/app-release.aab
```

### Signed release (local upload key)

```bash
# Once: gitignored keystore under app/keystore/ (not Play production)
./scripts/make-upload-keystore.sh

# Native libs for ABIs you ship
./scripts/build-android-native.sh aarch64-linux-android release
./scripts/build-android-native.sh x86_64-linux-android release

./gradlew :app:assembleRelease   # APK
./gradlew :app:bundleRelease     # AAB
```

If `app/keystore/navi-upload.jks` is present, the `release` build type signs with
it (override via `navi.upload.*` Gradle properties). Uninstall any debug-signed
`no.navi.app` before installing a release APK (`adb uninstall no.navi.app`).

Cheat-sheet also in the [README — Release build](../README.md#release-build-apk--aab).
See [`android-api36-plan.md`](android-api36-plan.md) for API 36 / Play AAB notes
and bundletool smoke. F-Droid Podman buildability:
[`tools/fdroid-check/README.md`](../tools/fdroid-check/README.md).

Signing a **production** Play release uses your Play App Signing / upload key —
the gitignored `app/keystore/navi-upload.jks` is for local `bundletool` checks
only.

### Manual `adb` install (optional)

Install the `adb` client itself from the host guide:
Linux [`build-linux.md` — adb](build-linux.md#android-debug-bridge-adb),
macOS [`build-macos.md` — adb](build-macos.md#android-debug-bridge-adb),
Windows [`build-windows.md` — adb](build-windows.md#android-debug-bridge-adb).
On the device, enable **Developer options** (tap **Build number** seven times
under About phone/tablet) and turn on **USB debugging** before connecting.

```bash
adb devices
adb install -r app/build/outputs/apk/debug/app-debug.apk
adb shell am start -n no.navi.app/.MainActivity
```

Confirm the APK embeds the expected ABI:

```bash
unzip -l app/build/outputs/apk/debug/app-debug.apk | grep 'lib/.*/libnavi.so'
```

On Android Automotive multi-user AVDs, Gradle install usually targets the
current user; if the package is missing for the driver profile, install with
`--user 10` (or run `:app:installDebug` while that user is foreground).

---

## 3. Launch on the Automotive emulator

```bash
adb devices
./scripts/launch-navi-emulator.sh
```

That script starts `no.navi.app/.MainActivity` and works around the yellow
display-compat “bordering activity” wrapper on some AAOS images.

MapLibre uses the **GLES** SDK artifact (`org.maplibre.gl:android-sdk:11.13.5`).
Vulkan (`android-sdk-vulkan`) was the prior default to avoid an older AAOS
emulator bearing SIGSEGV; GLES 11.13.5 re-validated on `emulator-5554` + SM-P613
(see README known issues / `docs/map-styles.md`).

---

## 4. Instrumented tests (optional)

```bash
./gradlew :app:installDebug :app:installDebugAndroidTest
./gradlew :app:connectedDebugAndroidTest
```

Single class:

```bash
./gradlew :app:connectedDebugAndroidTest \
  -Pandroid.testInstrumentationRunnerArguments.class=no.navi.app.CorridorInstrumentedTest
```

Corridor fixtures: see `scripts/prepare-android-fixtures.sh`,
`scripts/serve-android-fixtures.sh`, and [`android-test-results.md`](android-test-results.md).

---

## ABI cheat sheet

| Device | Rust target | `jniLibs` folder |
|---|---|---|
| Emulator (x86_64) | `x86_64-linux-android` | `app/src/main/jniLibs/x86_64/` |
| Most phones / AAOS arm64 | `aarch64-linux-android` | `app/src/main/jniLibs/arm64-v8a/` |

Ship both ABIs in one APK by building each target and leaving both `.so` files
under `jniLibs/` before `assemble*`.

---

## Troubleshooting

| Symptom | Likely fix |
|---|---|
| `ANDROID_NDK_HOME is not set` | Export NDK path (or `ANDROID_HOME` with `ndk/<version>`); see host OS guide |
| Linker / clang not found | Update `.cargo/config.toml` linker paths to your NDK **host** prebuilt |
| `UnsatisfiedLinkError` / missing `libnavi` | Re-run native script for the device ABI; confirm `jniLibs/<abi>/libnavi.so` exists |
| Kotlin UniFFI types missing | Re-run native script (bindgen step) |
| Map crash on rotate (historical GLES) | Prefer current `android-sdk:11.13.5` GLES; re-run `BearingCrashIsolationTest` on AAOS AVD if suspecting regression |
| Wrong app / yellow border UI | `./scripts/launch-navi-emulator.sh` |
| Windows: bash script not found | Use **Git Bash** or WSL for `scripts/*.sh` |

---

## Quick path (emulator debug)

For logcat tags, Android Studio attach, and instrumented-test harnesses, see
[`debugging.md`](debugging.md).

```bash
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/<version>"
./scripts/build-android-native.sh x86_64-linux-android debug
./gradlew :app:installDebug
./scripts/launch-navi-emulator.sh
```
