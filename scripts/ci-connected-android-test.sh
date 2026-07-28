#!/usr/bin/env bash
# Run from repo root inside reactivecircus/android-emulator-runner.
# That action executes YAML `script:` line-by-line as separate `sh -c` invocations,
# so this must stay a single script file (variables / set +e / traps do not span lines).

set +e

EVIDENCE="${GITHUB_WORKSPACE:-.}/instrumented-evidence"
mkdir -p "${EVIDENCE}/tombstones" "${EVIDENCE}/runner"

# Sentinel so upload-artifact always has at least one file even if everything else fails.
{
  echo "collection_started_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "api=33 target=android-automotive memory=4096"
  echo "cwd=$(pwd)"
  echo "GITHUB_WORKSPACE=${GITHUB_WORKSPACE:-}"
  echo "runner_name=${RUNNER_NAME:-}"
  echo "runner_os=${RUNNER_OS:-}"
} | tee "${EVIDENCE}/env.txt" > "${EVIDENCE}/collection_started.txt"

LOGCAT_PID=""
GRADLE_STATUS=1

collect_evidence() {
  local reason="${1:-unspecified}"
  echo "collect_evidence reason=${reason} utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
    | tee -a "${EVIDENCE}/collection_log.txt"

  # Stop background logcat so the file is flushed before we inspect sizes.
  if [ -n "${LOGCAT_PID}" ]; then
    kill "${LOGCAT_PID}" 2>/dev/null || true
    wait "${LOGCAT_PID}" 2>/dev/null || true
    LOGCAT_PID=""
  fi
  sync 2>/dev/null || true

  adb devices -l > "${EVIDENCE}/adb_devices_after.txt" 2>&1 || true
  if ! adb devices 2>/dev/null | grep -E 'emulator-.*device' >/dev/null; then
    {
      echo "emulator was already gone by evidence-collection step"
      echo "reason=${reason}"
      echo "see runner/ for host-side free/df/dmesg and adb_devices_after.txt"
      echo "continuous logcat (if any) remains in logcat.txt from before device loss"
    } | tee "${EVIDENCE}/EMULATOR_GONE.txt"
  fi

  # These no-op cleanly when the device is gone; still write stderr to files.
  adb logcat -b crash -d > "${EVIDENCE}/logcat_crash.txt" 2>&1 || true
  adb shell "ls -la /data/tombstones/" > "${EVIDENCE}/tombstones_ls.txt" 2>&1 || true
  adb pull /data/tombstones "${EVIDENCE}/tombstones" > "${EVIDENCE}/tombstones_pull.txt" 2>&1 || true
  cp -a app/build/reports/androidTests/connected "${EVIDENCE}/reports" 2>/dev/null || true
  cp -a app/build/outputs/androidTest-results "${EVIDENCE}/androidTest-results" 2>/dev/null || true

  {
    echo "gradle_exit=${GRADLE_STATUS}"
    echo "collect_reason=${reason}"
    echo "logcat_bytes=$(wc -c < "${EVIDENCE}/logcat.txt" 2>/dev/null || echo 0)"
    echo "adb_after=$(tr '\n' ' ' < "${EVIDENCE}/adb_devices_after.txt" 2>/dev/null)"
  } > "${EVIDENCE}/status.txt"

  ls -laR "${EVIDENCE}" > "${EVIDENCE}/listing.txt" 2>&1 || true
}

# Always attempt evidence dump on any exit (gradle fail, signal, set -e path, etc.).
trap 'collect_evidence trap_exit' EXIT

# Default Automotive CI skin is ~320x640; override to a landscape HUD size so
# width-fraction asserts match real AAOS validation devices.
adb shell wm size 1920x1080 > "${EVIDENCE}/wm_size_set.txt" 2>&1 || true
adb shell wm density 160 > "${EVIDENCE}/wm_density_set.txt" 2>&1 || true

adb devices -l > "${EVIDENCE}/adb_devices_before.txt" 2>&1 || true
adb shell getprop ro.build.fingerprint > "${EVIDENCE}/fingerprint.txt" 2>&1 || true
adb shell wm size > "${EVIDENCE}/wm_size.txt" 2>&1 || true
adb shell wm density > "${EVIDENCE}/wm_density.txt" 2>&1 || true

adb logcat -c || true
# Continuous logcat survives emulator death better than a post-mortem dump alone.
adb logcat -v threadtime > "${EVIDENCE}/logcat.txt" 2>&1 &
LOGCAT_PID=$!

./gradlew :app:connectedDebugAndroidTest --no-daemon
GRADLE_STATUS=$?

# Explicit collect before EXIT trap (trap will no-op second kill safely).
collect_evidence "after_gradle"
trap - EXIT

exit "${GRADLE_STATUS}"
