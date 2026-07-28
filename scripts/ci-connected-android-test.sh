#!/usr/bin/env bash
# Run from repo root inside reactivecircus/android-emulator-runner.
# That action executes YAML `script:` line-by-line as separate `sh -c` invocations,
# so this must stay a single script file (variables / set +e / traps do not span lines).
#
# Critical: after the emulator dies mid-suite, bare `adb` / `wait` on logcat can
# hang forever — that looks like "BUILD FAILED then no further progress". Every
# post-failure adb call and logcat teardown must be bounded with `timeout`.

set +e

EVIDENCE="${GITHUB_WORKSPACE:-.}/instrumented-evidence"
mkdir -p "${EVIDENCE}/tombstones" "${EVIDENCE}/runner"

ADB_TIMEOUT_SEC="${ADB_TIMEOUT_SEC:-20}"

adb_try() {
  # usage: adb_try outfile adb args...
  local out="$1"
  shift
  timeout "${ADB_TIMEOUT_SEC}" adb "$@" >"${out}" 2>&1
  local rc=$?
  if [ "${rc}" -eq 124 ]; then
    echo "adb timed out after ${ADB_TIMEOUT_SEC}s: $*" >>"${out}"
  fi
  return 0
}

# Sentinel so upload-artifact always has at least one file even if everything else fails.
{
  echo "collection_started_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "api=33 target=android-automotive memory=4096"
  echo "cwd=$(pwd)"
  echo "GITHUB_WORKSPACE=${GITHUB_WORKSPACE:-}"
  echo "runner_name=${RUNNER_NAME:-}"
  echo "runner_os=${RUNNER_OS:-}"
  echo "adb_timeout_sec=${ADB_TIMEOUT_SEC}"
} | tee "${EVIDENCE}/env.txt" > "${EVIDENCE}/collection_started.txt"

LOGCAT_PID=""
GRADLE_STATUS=1
COLLECTED=0

stop_logcat() {
  if [ -z "${LOGCAT_PID}" ]; then
    return 0
  fi
  kill "${LOGCAT_PID}" 2>/dev/null || true
  # Bounded wait — a blocked adb logcat will not respond to SIGTERM alone.
  timeout 5 wait "${LOGCAT_PID}" 2>/dev/null || true
  kill -9 "${LOGCAT_PID}" 2>/dev/null || true
  timeout 3 wait "${LOGCAT_PID}" 2>/dev/null || true
  LOGCAT_PID=""
  sync 2>/dev/null || true
}

collect_evidence() {
  local reason="${1:-unspecified}"
  if [ "${COLLECTED}" -eq 1 ]; then
    echo "collect_evidence skipped (already ran) reason=${reason}" >>"${EVIDENCE}/collection_log.txt"
    return 0
  fi
  COLLECTED=1
  echo "collect_evidence reason=${reason} utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
    | tee -a "${EVIDENCE}/collection_log.txt"

  stop_logcat

  adb_try "${EVIDENCE}/adb_devices_after.txt" devices -l
  if ! grep -E 'emulator-.*device' "${EVIDENCE}/adb_devices_after.txt" >/dev/null 2>&1; then
    {
      echo "emulator was already gone by evidence-collection step"
      echo "reason=${reason}"
      echo "see runner/ for host-side free/df/dmesg and adb_devices_after.txt"
      echo "continuous logcat (if any) remains in logcat.txt from before device loss"
    } | tee "${EVIDENCE}/EMULATOR_GONE.txt"
  fi

  # Bounded — these hang indefinitely when the AVD is dead or adb is wedged.
  adb_try "${EVIDENCE}/logcat_crash.txt" logcat -b crash -d
  adb_try "${EVIDENCE}/tombstones_ls.txt" shell "ls -la /data/tombstones/"
  timeout "${ADB_TIMEOUT_SEC}" adb pull /data/tombstones "${EVIDENCE}/tombstones" \
    >"${EVIDENCE}/tombstones_pull.txt" 2>&1 || true
  if grep -q 'timed out' "${EVIDENCE}/tombstones_pull.txt" 2>/dev/null; then
    :
  elif [ ! -s "${EVIDENCE}/tombstones_pull.txt" ]; then
    echo "adb pull finished rc unknown" >>"${EVIDENCE}/tombstones_pull.txt"
  fi

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

# Always attempt evidence dump on any exit (gradle fail, signal, hang avoided via timeouts).
trap 'collect_evidence trap_exit' EXIT

# Default Automotive CI skin is ~320x640; override to a landscape HUD size so
# width-fraction asserts match real AAOS validation devices.
adb_try "${EVIDENCE}/wm_size_set.txt" shell wm size 1920x1080
adb_try "${EVIDENCE}/wm_density_set.txt" shell wm density 160

adb_try "${EVIDENCE}/adb_devices_before.txt" devices -l
adb_try "${EVIDENCE}/fingerprint.txt" shell getprop ro.build.fingerprint
adb_try "${EVIDENCE}/wm_size.txt" shell wm size
adb_try "${EVIDENCE}/wm_density.txt" shell wm density

timeout "${ADB_TIMEOUT_SEC}" adb logcat -c || true
# Continuous logcat survives emulator death better than a post-mortem dump alone.
adb logcat -v threadtime > "${EVIDENCE}/logcat.txt" 2>&1 &
LOGCAT_PID=$!

./gradlew :app:connectedDebugAndroidTest --no-daemon
GRADLE_STATUS=$?

collect_evidence "after_gradle"
trap - EXIT

exit "${GRADLE_STATUS}"
