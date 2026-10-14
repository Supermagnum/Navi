#!/usr/bin/env bash
# Run the IMU emulator virtual-sensor campaign on ANDROID_SERIAL=emulator-5554.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
export ANDROID_SERIAL="${ANDROID_SERIAL:-emulator-5554}"
if [[ "$ANDROID_SERIAL" != "emulator-5554" ]]; then
  echo "refusing non-emulator serial: $ANDROID_SERIAL" >&2
  exit 2
fi
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Android/Sdk}"
export PATH="$ANDROID_HOME/platform-tools:$PATH"

pkill -f imu-calibration-campaign-host.py || true
adb -s "$ANDROID_SERIAL" wait-for-device
adb -s "$ANDROID_SERIAL" install -r -t app/build/outputs/apk/debug/app-debug.apk
adb -s "$ANDROID_SERIAL" install -r -t app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk
adb -s "$ANDROID_SERIAL" shell mkdir -p /data/local/tmp/imu_campaign
adb -s "$ANDROID_SERIAL" shell rm -f /data/local/tmp/imu_campaign/*
adb -s "$ANDROID_SERIAL" logcat -c

python3 scripts/imu-calibration-campaign-host.py > /tmp/imu-campaign-host.log 2>&1 &
HOST_PID=$!
trap 'kill $HOST_PID 2>/dev/null || true' EXIT
sleep 2

adb -s "$ANDROID_SERIAL" shell am instrument -w -r \
  -e class no.navi.app.ImuCalibrationCampaignInstrumentedTest \
  no.navi.app.test/no.navi.app.NaviAndroidTestRunner

OUT=/tmp/imu_campaign_evidence
rm -rf "$OUT"
mkdir -p "$OUT"
adb -s "$ANDROID_SERIAL" pull /data/local/tmp/imu_campaign "$OUT/" || true
adb -s "$ANDROID_SERIAL" pull /sdcard/Android/data/no.navi.app/files/ "$OUT/app_files/" 2>/dev/null || true
echo "host log:"; cat /tmp/imu-campaign-host.log
echo "results:"; cat "$OUT/imu_campaign/results.txt" 2>/dev/null || true
ls -la "$OUT/imu_campaign" 2>/dev/null || true
