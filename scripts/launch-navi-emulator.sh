#!/usr/bin/env bash
# Launch Navi on the Automotive emulator, bypassing the yellow
# "All views bordering activity should be visible" display-compat screen.
set -euo pipefail

ADB="${ADB:-adb}"
PKG=no.navi.app
ACT=no.navi.app/.MainActivity

echo "Stopping display-compat wrapper and Navi..."
"$ADB" shell am force-stop com.android.car.displaycompat.app || true
"$ADB" shell am force-stop "$PKG" || true

# Disable the yellow IgnoreDecorActivity wrapper if present (re-enable with: pm enable).
if "$ADB" shell pm path com.android.car.displaycompat.app >/dev/null 2>&1; then
  echo "Disabling com.android.car.displaycompat.app (yellow bordering test UI)..."
  "$ADB" shell pm disable-user --user 0 com.android.car.displaycompat.app 2>/dev/null || true
  "$ADB" shell pm disable-user --user 10 com.android.car.displaycompat.app 2>/dev/null || true
fi

# Prefer driver profile (user 10) on Automotive AVDs; else primary user.
USER=10
if ! "$ADB" shell pm path --user 10 "$PKG" >/dev/null 2>&1; then
  USER=0
fi

echo "Starting $ACT (user $USER)..."
"$ADB" shell am start --user "$USER" -n "$ACT" \
  -a android.intent.action.MAIN \
  -c android.intent.category.LAUNCHER \
  --activity-clear-task

# Also disable the stock maps placeholder that owns APP_MAPS on the center display.
"$ADB" shell am force-stop --user "$USER" com.android.car.mapsplaceholder 2>/dev/null || true

sleep 1
"$ADB" shell dumpsys activity activities 2>/dev/null | grep -E 'mFocusedApp|navi.app' | head -n 8 || true
echo "Done. You should see Navi (OpenFreeMap basemap), not the yellow bordering screen."
echo "Re-run anytime: ./scripts/launch-navi-emulator.sh"
