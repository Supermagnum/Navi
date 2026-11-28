#!/usr/bin/env bash
# One nearby-services metric run on a connected device.
# Usage:
#   tools/nearby-services-metric-run.sh <run_id> <lat> <lon> <cold|warm>
set -euo pipefail
RUN_ID="${1:?run_id}"
LAT="${2:?lat}"
LON="${3:?lon}"
MODE="${4:?cold|warm}"
PKG=no.navi.app
ACT=no.navi.app/.MainActivity
OUT_DIR="${NEARBY_METRIC_OUT:-/tmp/nearby-metrics}"
mkdir -p "$OUT_DIR"
LOG_FILE="$OUT_DIR/${RUN_ID}.logcat.txt"
REPORT="$OUT_DIR/${RUN_ID}.report.txt"

pss_kb() {
  local pid="$1"
  adb shell dumpsys meminfo "$pid" 2>/dev/null | awk '/TOTAL PSS:/{print $3; exit}'
}

cpu_ticks() {
  local pid="$1"
  adb shell "tr ' ' '\\n' </proc/$pid/stat" 2>/dev/null | sed -n '14,15p' | tr '\n' ' '
}

dump_nearby() {
  adb logcat -d 2>/dev/null | rg "NearbyServices" || true
}

CLK_TCK="$(adb shell getconf CLK_TCK | tr -d '\r')"

if [[ "$MODE" == "cold" ]]; then
  adb shell am force-stop "$PKG"
  sleep 1
fi

adb logcat -c
: >"$LOG_FILE"

adb shell am start -n "$ACT" \
  --ez navi_nearby_services_test true \
  --es navi_nearby_run_id "$RUN_ID" \
  --ed navi_gps_lat "$LAT" \
  --ed navi_gps_lon "$LON" >/dev/null

PID=""
for _ in $(seq 1 40); do
  PID="$(adb shell pidof "$PKG" | tr -d '\r' || true)"
  [[ -n "$PID" ]] && break
  sleep 0.25
done
if [[ -z "$PID" ]]; then
  echo "HARNESS_FAIL run_id=$RUN_ID reason=no_pid" | tee "$REPORT"
  exit 1
fi

PSS_BEFORE="$(pss_kb "$PID")"
read -r UT0 ST0 _ <<<"$(cpu_ticks "$PID")"
UT0="${UT0:-0}"; ST0="${ST0:-0}"
T0_MS="$(date +%s%3N)"
PEAK_PSS="${PSS_BEFORE:-0}"

got=0
for i in $(seq 1 120); do
  sleep 0.25
  dump_nearby >"$LOG_FILE"
  CUR="$(pss_kb "$PID" || true)"
  if [[ -n "${CUR:-}" ]]; then
    if [[ "$CUR" -gt "$PEAK_PSS" ]]; then
      PEAK_PSS="$CUR"
    fi
  fi
  if rg -q "RESULT run_id=$RUN_ID " "$LOG_FILE"; then
    got=1
    break
  fi
  if ! adb shell pidof "$PKG" | grep -q .; then
    echo "HARNESS_FAIL run_id=$RUN_ID reason=process_died" | tee "$REPORT"
    exit 1
  fi
done

T1_MS="$(date +%s%3N)"
read -r UT1 ST1 _ <<<"$(cpu_ticks "$PID")"
UT1="${UT1:-0}"; ST1="${ST1:-0}"
WALL_MS=$((T1_MS - T0_MS))
CPU_SEC="$(python3 - <<PY
ut=int("$UT1")-int("$UT0"); st=int("$ST1")-int("$ST0"); clk=float("$CLK_TCK")
print(f"{(ut+st)/clk:.3f}")
PY
)"
CPU_PCT="$(python3 - <<PY
wall=max(int("$WALL_MS")/1000.0, 0.001); cpu=float("$CPU_SEC")
print(f"{cpu/wall*100:.1f}")
PY
)"

# Allow TOP5/GAP flush
sleep 0.5
dump_nearby >"$LOG_FILE"
PSS_AFTER="$(pss_kb "$PID")"

RESULT_LINE="$(rg "RESULT run_id=$RUN_ID " "$LOG_FILE" | head -1 || true)"
PHASE_LINE="$(rg "PHASE run_id=$RUN_ID " "$LOG_FILE" | head -1 || true)"
GAP_LINE="$(rg "GAP run_id=$RUN_ID " "$LOG_FILE" | head -1 || true)"
TOP5="$(rg "TOP5 run_id=$RUN_ID " "$LOG_FILE" || true)"
START_LINE="$(rg "search start run_id=$RUN_ID " "$LOG_FILE" | head -1 || true)"

if [[ "$got" -ne 1 || -z "$RESULT_LINE" ]]; then
  {
    echo "HARNESS_FAIL run_id=$RUN_ID mode=$MODE reason=no_result"
    echo "pss_before_kb=$PSS_BEFORE peak_pss_kb=$PEAK_PSS pss_after_kb=$PSS_AFTER"
    echo "wall_ms=$WALL_MS cpu_sec=$CPU_SEC cpu_pct=$CPU_PCT"
    echo "log=$LOG_FILE"
    echo "--- log tail ---"
    tail -30 "$LOG_FILE" || true
  } | tee "$REPORT"
  exit 1
fi

# 30s retention sample (same process / same run id)
sleep 30
PSS_30S="$(pss_kb "$PID" || echo "")"
# Refresh log once more in case of late lines (should be same RESULT)
dump_nearby >"$LOG_FILE"
RESULT_LINE="$(rg "RESULT run_id=$RUN_ID " "$LOG_FILE" | head -1 || true)"
PHASE_LINE="$(rg "PHASE run_id=$RUN_ID " "$LOG_FILE" | head -1 || true)"
GAP_LINE="$(rg "GAP run_id=$RUN_ID " "$LOG_FILE" | head -1 || true)"
TOP5="$(rg "TOP5 run_id=$RUN_ID " "$LOG_FILE" || true)"

{
  echo "PASS run_id=$RUN_ID mode=$MODE lat=$LAT lon=$LON pid=$PID"
  [[ -n "$START_LINE" ]] && echo "$START_LINE"
  echo "$RESULT_LINE"
  echo "$PHASE_LINE"
  [[ -n "$GAP_LINE" ]] && echo "$GAP_LINE" || echo "GAP run_id=$RUN_ID (none)"
  echo "METRICS run_id=$RUN_ID pss_before_kb=$PSS_BEFORE peak_pss_kb=$PEAK_PSS pss_after_kb=$PSS_AFTER pss_30s_kb=$PSS_30S script_wall_ms=$WALL_MS cpu_sec=$CPU_SEC cpu_pct=$CPU_PCT"
  echo "$TOP5"
} | tee "$REPORT"

exit 0
