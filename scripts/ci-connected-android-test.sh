#!/usr/bin/env bash
# Run from repo root inside reactivecircus/android-emulator-runner.
# That action executes `script` line-by-line as separate `sh -c` invocations,
# so variables and `set +e` do not span lines — keep this as one script file.

set +e

EVIDENCE="${GITHUB_WORKSPACE:-.}/instrumented-evidence"
mkdir -p "${EVIDENCE}/tombstones"

{
  echo "api=33 target=android-automotive memory=4096"
  echo "cwd=$(pwd)"
  echo "GITHUB_WORKSPACE=${GITHUB_WORKSPACE:-}"
} > "${EVIDENCE}/env.txt"

# Default Automotive CI skin is ~320x640; override to a landscape HUD size so
# width-fraction asserts match real AAOS validation devices.
adb shell wm size 1920x1080 > "${EVIDENCE}/wm_size_set.txt" 2>&1 || true
adb shell wm density 160 > "${EVIDENCE}/wm_density_set.txt" 2>&1 || true

adb devices -l > "${EVIDENCE}/adb_devices_before.txt" 2>&1 || true
adb shell getprop ro.build.fingerprint > "${EVIDENCE}/fingerprint.txt" 2>&1 || true
adb shell wm size > "${EVIDENCE}/wm_size.txt" 2>&1 || true
adb shell wm density > "${EVIDENCE}/wm_density.txt" 2>&1 || true

adb logcat -c || true
adb logcat -v threadtime > "${EVIDENCE}/logcat.txt" 2>&1 &
LOGCAT_PID=$!

./gradlew :app:connectedDebugAndroidTest --no-daemon
status=$?

kill "${LOGCAT_PID}" 2>/dev/null || true
wait "${LOGCAT_PID}" 2>/dev/null || true

adb devices -l > "${EVIDENCE}/adb_devices_after.txt" 2>&1 || true
adb logcat -b crash -d > "${EVIDENCE}/logcat_crash.txt" 2>&1 || true
adb shell "ls -la /data/tombstones/" > "${EVIDENCE}/tombstones_ls.txt" 2>&1 || true
adb pull /data/tombstones "${EVIDENCE}/tombstones" > "${EVIDENCE}/tombstones_pull.txt" 2>&1 || true
cp -a app/build/reports/androidTests/connected "${EVIDENCE}/reports" 2>/dev/null || true
cp -a app/build/outputs/androidTest-results "${EVIDENCE}/androidTest-results" 2>/dev/null || true
echo "gradle_exit=${status}" > "${EVIDENCE}/status.txt"
ls -laR "${EVIDENCE}" > "${EVIDENCE}/listing.txt" 2>&1 || true

exit "${status}"
