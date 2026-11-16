# Releasetest build variant

Sideloadable build with **release** native/Rust optimisations that installs
**next to** the debug app (no uninstall). Used for all Phase 5/6 device
measurements on Samsung `R52TB0JQEDE`.

| | |
|--|--|
| Gradle type | `releasetest` (`initWith(release)`) |
| applicationId | `no.navi.app.releasetest` |
| Signing | `app/keystore/navi-releasetest.jks` (gitignored) |
| R8 / minify | Same as production `release` (currently off) |
| Debuggable | false |

## One-time keystore

```bash
./scripts/make-releasetest-keystore.sh
```

## Build and install (device only)

```bash
export ANDROID_SERIAL=R52TB0JQEDE
./scripts/build-android-native.sh   # arm64 libnavi + liborganicmaps as usual
./gradlew :app:installReleasetest
```

## Instrumented tests against releasetest

```bash
./gradlew :app:installReleasetestAndroidTest -Pnavi.testBuildType=releasetest
# First run: copy MWMs from the debug app sandbox (or push leaves yourself)
adb -s "$ANDROID_SERIAL" shell \
  "mkdir -p /sdcard/Android/data/no.navi.app.releasetest/files && \
   cp -a /sdcard/Android/data/no.navi.app/files/navi-mwm-test \
        /sdcard/Android/data/no.navi.app.releasetest/files/"
adb -s "$ANDROID_SERIAL" shell am instrument -w -r \
  -e class 'no.navi.app.ComapsPhase5SearchInstrumentedTest,no.navi.app.ComapsPhase4SuiteInstrumentedTest' \
  no.navi.app.releasetest.test/no.navi.app.NaviAndroidTestRunner
```

Omit `-Pnavi.testBuildType=releasetest` to keep testing the debug app
(`no.navi.app` / `no.navi.app.test`).

## Test hooks in releasetest

`NaviMapTestHooks.replayHooksEnabled()` is true for `debug` **and** `releasetest`
(never for the shipped `release`). Route/camera/simulation hooks used by the Phase 6
acceptance (`pendingRoute`, `requestStartRouteSimulation`, `requestRecenterGps`, ...)
therefore work against the release-optimised native code. The debug-only intent
extras (`NaviDebugIntent`, debug source set) are still absent.

## CoMaps drape (Phase 6b)

When the CoMaps checkout exists (`third_party/comaps`, `NAVI_COMAPS_SRC` or
`../Navi-local-artifacts/comaps-<sha8>`), `settings.gradle.kts` includes `:comaps-sdk`
and the app packages the CoMaps Java SDK; with the MWM flag ON the drape is the only map
renderer. Measured releasetest APK on this branch (`-PnaviAbis=arm64-v8a`): **94.3 MB**
(breakdown in `docs/mwm-phase6b-drape.md` section 12). The symbol atlases come from
`scripts/generate-comaps-symbols.py` (needs `rsvg-convert` and Pillow).
`docs/mwm-phase6b-drape.md` has the device acceptance procedure
(`scripts/phase6-device-accept.sh install | clean | run <test>... | pull`). Espa-start tests
hold Espa as the GPS fix with Navi's own debug GPS hold; no third-party mock-location app.
