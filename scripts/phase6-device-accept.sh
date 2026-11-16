#!/usr/bin/env bash
# Phase 6b on-device acceptance for the CoMaps drape (physical device only).
#
#   scripts/phase6-device-accept.sh install            build + install releasetest app and test APK
#   scripts/phase6-device-accept.sh clean              pm clear the app (empty mwm dir: start of step a_)
#   scripts/phase6-device-accept.sh run <method>...    run ComapsPhase6bAcceptanceInstrumentedTest methods
#   scripts/phase6-device-accept.sh pull               pull screenshots + logs to test-artifacts/phase6b/
#
# Map data is NOT pushed: step a_download_from_scratch downloads the Oestlandet leaf set (MWMs,
# World + WorldCoasts, sidecars) with the app's own downloader after `clean`. For the CAT steps a
# repeater DB must exist at /data/local/tmp/phase6b/cat_repeaters.sqlite.
# Never uses an emulator: the serial must be a real device id.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SERIAL="${ANDROID_SERIAL:-R52TB0JQEDE}"
case "$SERIAL" in emulator-*) echo "error: emulators are not allowed" >&2; exit 1 ;; esac
export ANDROID_SERIAL="$SERIAL"
ADB=(adb -s "$SERIAL")
RUNNER=no.navi.app.releasetest.test/no.navi.app.NaviAndroidTestRunner
CLASS=no.navi.app.ComapsPhase6bAcceptanceInstrumentedTest
# PHASE_OUT=phase7 keeps a re-run apart from the Phase 6b artifacts (device dir /data/local/tmp/$PHASE_OUT).
PHASE_OUT="${PHASE_OUT:-phase6b}"
OUT="$ROOT/test-artifacts/$PHASE_OUT"
DEVICE_OUT="/data/local/tmp/$PHASE_OUT"
# Extra instrumentation arguments for every run, e.g. EXTRA_ARGS="-e perf_minutes 5 -e fps_cap 0".
EXTRA_ARGS="${EXTRA_ARGS:-}"

cmd="${1:-}"
shift || true
case "$cmd" in
  install)
    cd "$ROOT"
    ./gradlew :app:installReleasetest :app:installReleasetestAndroidTest -Pnavi.testBuildType=releasetest
    for p in ACCESS_FINE_LOCATION ACCESS_COARSE_LOCATION; do
      "${ADB[@]}" shell pm grant no.navi.app.releasetest "android.permission.$p"
    done
    ;;
  clean)
    "${ADB[@]}" shell pm clear no.navi.app.releasetest
    for p in ACCESS_FINE_LOCATION ACCESS_COARSE_LOCATION; do
      "${ADB[@]}" shell pm grant no.navi.app.releasetest "android.permission.$p"
    done
    ;;
  run)
    mkdir -p "$OUT/logs"
    "${ADB[@]}" shell "mkdir -p $DEVICE_OUT/screenshots $DEVICE_OUT/logs; chmod -R 777 $DEVICE_OUT"
    for m in "$@"; do
      "${ADB[@]}" logcat -c
      "${ADB[@]}" logcat -v time NaviCat:I NaviNative:I Phase6:I ComapsDrape:I NaviPlan:I \
        ComapsSession:I ComapsDrapeMap:I MwmMapDownloader:I NaviPlan:I AndroidRuntime:E DEBUG:F libc:F '*:S' >"$OUT/logs/logcat-$m.txt" 2>&1 &
      lc=$!
      "${ADB[@]}" shell am instrument -w -r -e out_dir "$DEVICE_OUT" $EXTRA_ARGS -e class "$CLASS#$m" "$RUNNER" | tee "$OUT/logs/instrument-$m.txt" \
        | grep -E "INSTRUMENTATION_STATUS: (test|stack)|^OK|FAILURES|Tests run" || true
      kill "$lc" 2>/dev/null || true
    done
    ;;
  pull)
    mkdir -p "$OUT"
    "${ADB[@]}" pull "$DEVICE_OUT/screenshots" "$OUT/"
    "${ADB[@]}" pull "$DEVICE_OUT/logs" "$OUT/"
    ;;
  *)
    sed -n '2,13p' "${BASH_SOURCE[0]}"
    exit 2
    ;;
esac
