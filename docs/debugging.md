# Debugging Navi

Practical loops for host (Rust) and Android Automotive (Kotlin + `libnavi.so`).
Build/install details: [`android-build.md`](android-build.md). Hardware checklist:
[`real-hardware-testing.md`](real-hardware-testing.md).

Debug route simulation (debuggable builds only; feeds the live `applyFix`
pipeline at maxspeed / highway-fallback pace): [`route-simulation.md`](route-simulation.md).

## Quick orientation

| Layer | What breaks | First tool |
|---|---|---|
| Rust core (`driver-break-core`) | Routing, POI, eco, config | `cargo test` / `RUST_LOG` |
| UniFFI (`navi-ffi` → `libnavi.so`) | Missing symbols, wrong ABI | Rebuild native + reinstall APK |
| Android host (`app/`) | HUD, MapLibre, sheets | `adb logcat`, Android Studio |
| Emulator / AAOS | Yellow border UI, wrong user | `./scripts/launch-navi-emulator.sh` |

Application id: `no.navi.app`. Main activity: `no.navi.app/.MainActivity`.

---

## 1. Host (Rust) debugging

### Unit / integration tests

```bash
# Fast focused modules
cargo test -p driver-break-core poi::
cargo test -p driver-break-core config::eco -- --nocapture

# Ignored (large) integrations — need fixtures under core/target/…
cargo test -p driver-break-core --test kongsvinger_lillehammer_integration \
  -- --nocapture --ignored
cargo test -p driver-break-core --test dnt_hiking_integration \
  -- --nocapture --ignored
```

Use `--nocapture` so `println!` / logging from the test binary stays on the
terminal. Notes from past runs: [`test-results.md`](test-results.md).

### IDE / debugger

- Open the workspace in **VS Code** (rust-analyzer) or **RustRover**.
- Set breakpoints in `core/src/…` and debug a single `cargo test` target
  (same filters as above).
- For UniFFI boundary issues, prefer a failing `cargo test -p navi-ffi` or an
  Android instrumented call that hits the same FFI function, then bisect in
  Rust.

### Logging

The core does not require a special env for most tests. When adding temporary
diagnostics, prefer `eprintln!` in tests or structured logs you already use in
that crate — remove noise before committing.

---

## 2. Android: install a debuggable build

Always rebuild the native library when Rust changed, then install the debug APK:

```bash
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Android/Sdk}"
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$ANDROID_HOME/ndk/<version>}"

# Emulator ABI
./scripts/build-android-native.sh x86_64-linux-android debug   # or release
./gradlew :app:installDebug
./scripts/launch-navi-emulator.sh
```

Physical arm64 head unit: use `aarch64-linux-android` instead of `x86_64-…`.

Stale `libnavi.so` is a common “debug red herring”: Kotlin looks fine, FFI
returns wrong data or crashes. If behaviour and code disagree, re-run
`build-android-native.sh` and reinstall.

---

## 3. Logcat (primary on-device tool)

```bash
# Clear, then follow Navi-related tags
adb logcat -c
adb logcat -v time \
  HudVerification:V NaviTracks:V NaviMapTest:V BearingCrash:V \
  AndroidRuntime:E libc:F DEBUG:F *:S
```

Useful tags seen in-tree:

| Tag | Area |
|---|---|
| `HudVerification` | HUD / settings hooks, screenshots, camera errors |
| `NaviTracks` | Moving-icon / APRS overlay load |
| `NaviMapTest` | Corridor / map instrumented tests |
| `BearingCrash` | MapLibre bearing isolation tests |
| `AndroidRuntime` | Java/Kotlin uncaught exceptions |
| `DEBUG` / tombstones | Native crashes (`libnavi.so`, MapLibre) |

Broader MapLibre / GL noise (when investigating map crashes):

```bash
adb logcat -v time | grep -iE 'maplibre|libEGL|GLES|vulkan|navi|UnsatisfiedLink'
```

On Automotive multi-user AVDs, confirm you are looking at the **driver** user
(often user **10**):

```bash
adb shell am get-current-user
adb shell ps --user 10 | grep navi || true
```

---

## 4. Android Studio / breakpoints

1. Open the repo (or the `app` module) in **Android Studio**.
2. Run → **Attach Debugger to Android Process** → `no.navi.app`
   (pick the correct user process if several appear).
3. Set breakpoints in Kotlin under `app/src/main/java/no/navi/app/`
   (`MainActivity`, `DriveHud`, tests under `androidTest`).

Native (Rust / JNI) breakpoints need an NDK debug build of `libnavi.so` and an
lldb attach workflow; day-to-day Navi debugging is usually **logcat + Kotlin
debugger + Rust unit tests**, not full JNI stepping.

Compose UI: use **Layout Inspector** and test tags (`top_drive_hud`,
`bottom_drive_hud`, `map_settings_sheet`, `status_toast`, …) with
instrumented tests or `adb shell uiautomator dump`.

---

## 5. Instrumented tests as a debug harness

```bash
./gradlew :app:installDebug :app:installDebugAndroidTest

# One class
./gradlew :app:connectedDebugAndroidTest \
  -Pandroid.testInstrumentationRunnerArguments.class=no.navi.app.HudVerificationInstrumentedTest

# One method
./gradlew :app:connectedDebugAndroidTest \
  -Pandroid.testInstrumentationRunnerArguments.class=no.navi.app.HudVerificationInstrumentedTest#hud_map_tap_does_not_affect_settings_sheets
```

Watch the same logcat tags while the suite runs. HUD screenshots are written
under the app external files dir (and sometimes mirrored under
`/data/local/tmp/`); see [`android-test-results.md`](android-test-results.md).

`NaviMapTestHooks` injects camera, headings, altitude, and sheet open requests
for tests — useful when reproducing HUD/map issues without manual UI.

---

## 6. Emulator pitfalls

| Symptom | What to try |
|---|---|
| Yellow “bordering activity” screen | `./scripts/launch-navi-emulator.sh` (disables display-compat wrapper) |
| App missing / wrong UI on AAOS | Launch with `--user 10` (driver); reinstall for that user |
| MapLibre SIGSEGV on bearing (old GLES SDK) | App should use **Vulkan** MapLibre; see README known issues |
| Gestures / pan seem dead | Overlay must forward touches (see map Canvas `pointerInteropFilter`); confirm in logcat / HUD map-tap test |
| Icons show fallback / no eco leaf | Clear app `files/icons` and relaunch so `ensureIconsCopied` refreshes `leaf.svg` |

---

## 7. Crash dumps

```bash
# Java stack
adb logcat -b crash -d

# Native tombstone (path varies by device)
adb shell ls /data/tombstones/ 2>/dev/null
adb bugreport bugreport.zip   # heavy; use when filing hardware issues
```

For hardware runs, attach full untruncated logcat as required by
[`real-hardware-testing.md`](real-hardware-testing.md).

---

## 8. Minimal “something is wrong” loop

1. Reproduce on emulator with `installDebug` + `launch-navi-emulator.sh`.
2. Capture `adb logcat` for the tags above from cold start through the failure.
3. If FFI/routing related: reproduce with a Rust test or re-run
   `build-android-native.sh` and reinstall.
4. If UI/HUD related: run the relevant `*InstrumentedTest` method; pull
   screenshots from the device files dir.
5. On a real head unit: follow [`real-hardware-testing.md`](real-hardware-testing.md)
   and keep logs + screenshots together.
