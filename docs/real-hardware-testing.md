# Real-hardware testing guide

**The app needs testing on real hardware.** Development and automated checks so
far have used the **Android Automotive emulator only** — no physical Android
Automotive / phone head-unit has been available. Emulator results are not a
shipping substitute for device GPS/IMU, GPU drivers, audio, USB accessories, or
4 GB RAM behaviour. Treat **8 cores (~2 GHz class) and 4 GB RAM** as the
**minimum required** hardware for this product class; see
[README minimum hardware and storage capacity](../README.md#minimum-hardware-and-storage-capacity)
(including country disk budgets for Norway / Sweden / Germany / Russia / USA).

If you have access to real hardware, this guide covers what to check and how to
log results via `adb` so findings can be compared against the emulator baseline.

The specific known open question: whether the MapLibre native
`CircleLayer`/`SymbolLayer` rendering issue (moving icons not painting via the
standard GL path — see `README.md` known issues) is **specific to the Android
Automotive emulator**, or whether it also occurs on real hardware. The current
moving-icons fix uses a screen-space overlay workaround; if native layers *do*
paint correctly on real hardware, that is useful to know, since it may mean the
overlay approach only needs to be the emulator-specific path rather than the
permanent production path.

**Related (fixed on emulator via Vulkan):** Map camera rotation under the OpenGL
MapLibre SDK crashed this Automotive AVD (`SIGSEGV` fault `0x30` in
`libGLESv2_enc.so` during `MapRenderer::render`) on any non-zero bearing, without
needing a screenshot. The app now depends on `android-sdk-vulkan`. Still confirm
Compass / DoT rotation on real hardware GPU drivers (item 5) — shipping risk is
low if device Vulkan/GLES is healthy, but real-device confirmation remains the
decider for whether residual GLES-only builds would be safe.

---

## General approach

For each area below, run the described scenario on real hardware and capture:

- `adb logcat` output for the relevant tag(s), full and untruncated (no
  `tail`/`head` — per this project's existing logging convention).
- A screenshot (`adb exec-out screencap -p > name.png`) at key states.
- A plain description of what was observed vs. expected.

Report findings as: **matches emulator behavior**, **differs from emulator**
(describe how), or **new issue not seen on emulator at all**.

---

## 1. Moving icons — native layer check (priority)

- Run the moving-icons scenario (4 tracked icons, position updates, per the
  existing `MovingIconInstrumentedTest`) on real hardware.
- **Specifically check**: do markers render via MapLibre's native
  `CircleLayer`/`SymbolLayer` on real hardware, or does the same "no markers"
  symptom occur that was found on the emulator?
- If native layers work correctly on real hardware, this confirms the issue is
  emulator-specific GL/rendering behavior — valuable, since it means the
  screen-space overlay workaround could potentially be limited to the emulator
  path rather than needed everywhere.
- If the same failure occurs on real hardware, the overlay approach is confirmed
  necessary in production, not just for testing — also valuable, but changes how
  that workaround should be treated going forward.
- Log: `adb logcat` filtered for MapLibre/GL-related tags during marker
  registration and the first position update.

## 2. Menu behavior

- Exercise each menu item from the HUD verification pass (profile menu, eco
  toggle, To/Via search, saved routes, rotation mode, ETA toggle,
  break-reminder toggle, zoom controls, and all bottom-bar settings: break
  interval, rest time, fuel tank capacity, fuel added).
- For each: does it open/apply/auto-close correctly? Any lag, visual glitch, or
  crash not seen on the emulator?
- Log: `adb logcat` during each menu interaction; screenshot before/after each
  apply action.

HUD verification screenshots from the emulator baseline live under
`docs/images/hud/`. The instrumented test is
`HudVerificationInstrumentedTest`.

## 3. Routing behavior

- Run at least one real route computation (any profile) and compare timing
  against the Part 1 performance-constraint estimates already documented — real
  hardware timing is the actual validation those estimates were waiting on,
  since the emulator does not reflect real CPU/RAM constraints.
- Check whether RAM constraints (4 GB target) actually bind on real hardware the
  way planned — does a regional-scale graph load without issue? Does a larger
  load degrade gracefully or crash?
- Log: `adb logcat` during graph parse/build/reweight; note wall-clock timing
  for each stage if visible in logs.

## 4. Settings persistence

- Set values in each settings area (vehicle limits, fuel config, rest config,
  rotation mode, unit preference) and confirm they persist correctly across an
  app restart on real hardware.
- Log: confirm via `adb logcat` or a settings-dump mechanism if available,
  rather than just visual confirmation, so persistence is verifiable in the log
  record.

## 5. Sensor input (GPS/IMU)

- With real GPS and IMU present (not simulated/fed data, unlike the emulator
  rotation test), confirm:
  - "Use GPS as from / via / to" — applies Android `LocationManager` last known /
  live fixes to the currently selected search target (From, Via, or To). Native
  `lastGpsFix()` mirrors those pushes via `updateGpsFix` (it is **not** a demo
  coordinate stub). Confirm the on-screen coordinates match
  `adb shell dumpsys location` for the current user. Typed `lat, lon` in search
  also sets Via or To when that chip is selected.
  - Compass and Direction-of-travel rotation modes respond correctly to real
    device movement/orientation, not just fed synthetic values.
- Log: `adb logcat` for location/sensor provider tags during a short real walk
  or drive test.
- Debug route simulation on a planned corridor (emulator / lab) exercises the
  same `applyFix` pipeline — see [`route-simulation.md`](route-simulation.md).
  It does **not** replace this real GPS/IMU checklist.

## 6. ECU / OBD (when a plugin or adapter is available)

Live ECU polling is not shipped yet (see [`ECU.md`](ECU.md)). If you are
testing a prototype OBD-II / J1939 / MegaSquirt adapter against the
`LiveEnergyProvider` hook:

- Confirm snapshots update while driving (fuel rate or SoC changes).
- Confirm eco reweight / live cost uses the snapshot (not stale zeros).
- Confirm ignition-off or adapter disconnect clears live energy rather than
  leaving a stuck L/h value.
- Log: full `adb logcat` for the ECU/plugin tag; do not log VIN by default.

---

## Output format

For each of the sections above, report: pass/fail/differs-from-emulator, the
relevant logcat excerpt (full, not truncated), and any screenshots taken. If
real hardware is Android Automotive specifically (not a phone/tablet running
the app), note the exact device/head-unit model, since automotive-class
hardware may have different GPU/driver behavior than typical phone hardware —
directly relevant to item 1.
