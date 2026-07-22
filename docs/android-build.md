# Building Android packages

Navi’s Android app (`app/`, application id `no.navi.app`) links a Rust CDYLIB
(`navi-ffi` → `libnavi.so`) via UniFFI/JNA. Build native code first, then Gradle
APKs.

## Prerequisites

| Tool | Notes |
|---|---|
| **Rust** (rustup) | Stable toolchain |
| Android targets | `rustup target add x86_64-linux-android aarch64-linux-android` |
| **Android SDK** | API **35** (`compileSdk` / `targetSdk`); `minSdk` **26** |
| **Android NDK** | LLVM toolchain (example in-tree: NDK **27.3.x**) |
| **JDK 17** | For Gradle / Kotlin |
| **Gradle wrapper** | `./gradlew` at repo root |

Set (or export in your shell profile):

```bash
export ANDROID_HOME="$HOME/Android/Sdk"          # or your SDK path
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/<version>"
```

### Point Cargo at your NDK

`.cargo/config.toml` currently contains **machine-local** linker paths. Update
both targets to your NDK’s prebuilt clang before the first native build:

```toml
[target.x86_64-linux-android]
linker = "<NDK>/toolchains/llvm/prebuilt/linux-x86_64/bin/x86_64-linux-android34-clang"
rustflags = ["-C", "link-arg=-lc++_shared"]

[target.aarch64-linux-android]
linker = "<NDK>/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android34-clang"
rustflags = ["-C", "link-arg=-lc++_shared"]
```

On macOS the prebuilt folder is typically `darwin-x86_64` or `darwin-arm64`
instead of `linux-x86_64`. `scripts/build-android-native.sh` also reads
`ANDROID_NDK_HOME` and prepends that toolchain `bin/` to `PATH`.

Optional: `local.properties` in the repo root (not committed) for Gradle:

```properties
sdk.dir=/home/you/Android/Sdk
```

---

## 1. Build the native library + UniFFI Kotlin

From the repository root:

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

# Install debug on a connected device/emulator
./gradlew :app:installDebug

# Release APK (minify currently off in app/build.gradle.kts)
./gradlew :app:assembleRelease
```

Debug APK path (typical):

```text
app/build/outputs/apk/debug/app-debug.apk
```

Release:

```text
app/build/outputs/apk/release/app-release-unsigned.apk
```

Signing a release for distribution is not configured in-repo yet — use your own
keystore / Play App Signing flow before shipping.

Install a built APK with adb:

```bash
adb install -r app/build/outputs/apk/debug/app-debug.apk
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

MapLibre uses the **Vulkan** SDK artifact (`android-sdk-vulkan`); prefer an AVD
with working Vulkan/ranchu graphics. See README known issues for the GLES
rotation crash that Vulkan avoids.

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
`scripts/serve-android-fixtures.sh`, and [`android-test-results.md`](../android-test-results.md).

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
| `ANDROID_NDK_HOME not found` | Export NDK path; fix default in `build-android-native.sh` if needed |
| Linker / clang not found | Update `.cargo/config.toml` linker paths to your NDK |
| `UnsatisfiedLinkError` / missing `libnavi` | Re-run native script for the device ABI; confirm `jniLibs/<abi>/libnavi.so` exists |
| Kotlin UniFFI types missing | Re-run native script (bindgen step) |
| Map crash on rotate (OpenGL) | Ensure dependency is `android-sdk-vulkan` (already in `app/build.gradle.kts`) |
| Wrong app / yellow border UI | `./scripts/launch-navi-emulator.sh` |

---

## Quick path (emulator debug)

For logcat tags, Android Studio attach, and instrumented-test harnesses, see
[`debugging.md`](debugging.md).

```bash
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/<version>"
./scripts/build-android-native.sh x86_64-linux-android release
./gradlew :app:installDebug
./scripts/launch-navi-emulator.sh
```
