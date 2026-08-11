# Real-hardware testing guide

**The app needs testing on real hardware.** Emulator results are not a shipping
substitute for device GPS/IMU, GPU drivers, audio, USB accessories, or
4 GB RAM behaviour. Treat **8 cores (~2 GHz class) and 4 GB RAM** as the
**minimum required** hardware for this product class; see
[README minimum hardware and storage capacity](../README.md#minimum-hardware-and-storage-capacity)
(including country disk budgets for Norway / Sweden / Germany / Russia / USA).

Two **confirmed** phone/tablet targets are documented below (**SM-P613** and
**Pixel 9a**). Keep their findings in **separate** sections — several rendering
and memory results have been **device-specific**, so do not merge Pixel results
into the SM-P613 baseline or assume one transfers to the other.

If you have access to real hardware, this guide covers what to check and how to
capture evidence so findings can be compared against the emulator baseline and
against these two devices.

## Diagnostic logging (preferred — no adb required)

You do **not** need `adb logcat` for most hardware reports. Prefer the in-app
**Tools → Diagnostic logging** toggle:

1. Open **Tools** in the planning panel.
2. Turn on **Diagnostic logging**.
3. Reproduce the scenario (plan, drive/simulate, change settings, etc.).
4. Turn logging off when finished (optional), or leave it on for the whole
   session.
5. Copy the session file from the device over a plain USB cable (MTP / file
   browser) — **developer options and adb are not required** for this path.

**Where the files are**

| Location | Path |
|---|---|
| Preferred (USB/MTP) | **Internal storage → Documents → debug** |
| On-device filesystem | `/storage/emulated/0/Documents/debug/navi_session_YYYY-MM-DD_HH-mm-ss.log` |
| Fallback if Documents is not writable | `Download/debug/` (same `navi_session_*.log` names) |
| Last resort | App-private storage (harder to browse without adb) |

**Tools → Export diagnostic log** opens Android’s share sheet if that is easier
than copying via MTP.

**What the session log contains** (pipe-delimited lines, categories from
`DiagnosticLog`):

| Category | Typical content |
|---|---|
| `GPS` | Position / accuracy / satellites (rate-limited; not a raw firehose) |
| `CAMERA` | Map camera zoom / idle changes |
| `TOGGLE` | UI toggles (eco, avoidances, follow, rotation, breaks, 3D, …) |
| `SETTING_SAVED` | Persisted settings (vehicle limits, rest fields, paths, …) |
| `ROUTE_PLAN` | Plan outcome summary |
| `ROUTE_PLAN_STAGES` | Per-stage plan timings (graph build, POI/barrier, A*, …) |
| `ECO_CALC` | Eco / energy-related plan notes |
| `POI_FOUND` | Break / overnight POI hits |
| `PAUSE_PLANNED` | Planned break / rest / overnight stops |
| `INSTRUCTION` | Maneuver progress / completed instructions |
| `FUEL_ADDED` | Fuel fill events when recorded |
| `SYSTEM` | Occasional resource snapshots while logging is on |

More detail: [`debugging.md`](debugging.md#3b-diagnostic-session-log-on-device-file)
and the end-user note in [`how-to-use.md`](how-to-use.md).

`adb logcat` and `adb screencap` remain useful for crashes, native GLES/Vulkan
faults, and when you already have a developer cable set up — they are
**optional**, not the default evidence path for testers.

The specific known open questions include: whether the MapLibre native
`CircleLayer`/`SymbolLayer` rendering issue (moving icons not painting via the
standard GL path — see `README.md` known issues) is **specific to the Android
Automotive emulator**, or whether it also occurs on real hardware. The hydro
soft-edge fringe previously listed here is **not** a live rendering open
question anymore — on the Automotive emulator it appears only in instrumented
screenshots, not during interactive use (see
[`map-styles.md`](map-styles.md#hydro-soft-edge-fringe-screenshot-artifact);
item 7 is only a capture-vs-live spot-check on device). The current
moving-icons fix uses a screen-space overlay workaround; if native layers *do*
paint correctly on real hardware, that is useful to know, since it may mean the
overlay approach only needs to be the emulator-specific path rather than the
permanent production path.

**Related (historical emulator GLES SIGSEGV; default now GLES again):** Map camera
rotation under older OpenGL MapLibre SDK crashed this Automotive AVD (`SIGSEGV`
fault `0x30` in `libGLESv2_enc.so` during `MapRenderer::render`) on any non-zero
bearing, without needing a screenshot. The app briefly depended on
`android-sdk-vulkan`; as of 2026-07-31 it **finalizes** `android-sdk:11.13.5`
GLES after `BearingCrashIsolationTest` **PASS** on `emulator-5554` (no MapLibre
SIGSEGV; SM-P613 wash also cleared under GLES). Pixel 9a (Mali-G715) later
confirmed the same GLES default: no bearing SIGSEGV and no Adreno-class
hillshade wash (see **Pixel 9a** findings). Still confirm Compass / DoT
rotation on real hardware GPU drivers (item 5).

---

## Confirmed devices (do not merge findings)

### Samsung Galaxy Tab S6 Lite (SM-P613)

Primary early tablet target: Samsung / Adreno 618, ~3.5–4 GB RAM class. Authoritative
detail for indexed packs, hillshade wash under Vulkan vs GLES, and Ostlandet
convert memory lives in [`indexed-map-format-plan.md`](indexed-map-format-plan.md)
and [`map-styles.md`](map-styles.md). Short recall for comparison only:

| Topic | SM-P613 note |
|---|---|
| Renderer | Vulkan produced olive hillshade wash (`washFrac`≈0.999); GLES 11.13.5 clears it (`washFrac`≈0.09 online / ≈0.002 offline) |
| Bearing | GLES bearing stress OK on this GPU (AAOS SIGSEGV was emulator/GL-enc) |
| Ostlandet tiled convert | ~657 s; min system `MemAvailable` ~**329 MiB**; swap +~250 MiB; thin margin / TRIM CRITICAL observed |

### Pixel 9a (`tegu`) — onboard 2026-08-11

Second confirmed real-hardware target: **stock Android** on Google Tensor
silicon — meaningfully different SoC/GPU/RAM/OEM skin from SM-P613. Serial used
in this pass: `58091JEBF00012`. App: `no.navi.app` debug install,
`minSdk=26` / `targetSdk=36`.

| Field | Value |
|---|---|
| Model / codename | Pixel 9a / `tegu` |
| Android / API | **17 / 37** (newer than typical SM-P613 tablet trains; relevant to API 36 target work) |
| GPU | **ARM Mali-G715** (`ro.hardware.egl=mali`; SurfaceFlinger: `GLES: ARM, Mali-G715, OpenGL ES 3.2 …`) — **not** Adreno 618 |
| RAM | `MemTotal` ~**7.5 GB** (`7498068` kB) |
| MapLibre default under test | GLES `android-sdk:11.13.5` (global finalized default) |

#### 1. Install / launch

- `adb devices -l`: `device` (not offline).
- `:app:installDebug` + `MainActivity` launch OK; planning UI navigable.

#### 2. Renderer (GLES vs Vulkan question — new GPU data)

Tested under the **current GLES default** only (no Vulkan A/B on this pass).

| Check | Result on Pixel 9a (Mali-G715) | vs SM-P613 / AAOS |
|---|---|---|
| `BearingCrashIsolationTest` (bearing + screenshot; bearing-alone) | **PASS** — no MapLibre / RenderThread SIGSEGV | Matches AAOS GLES re-check and SM-P613 GLES stability; **does not** reproduce the old AAOS GLES SIGSEGV |
| Online Gjendebu 3D (`OnlineGjendebu3dHillshadeDiagnosticTest`) | **PASS** — default `washFrac`≈**0.118**, `lum_std`≈41.7, `creamFrac`≈0.098; exag 0.3 `washFrac`≈**0.017** | **Not** the SM-P613 Vulkan wash (`washFrac`≈0.999). Same ballpark as SM-P613 **GLES** online (~0.09) |
| Offline downloaded 3D (`OfflineDownloaded3dScreenshotTest`) | **PASS** — `washFrac`≈**0.0001**, `creamFrac`≈0.54, `demHitsOk`≥10, elev/tile sanity OK | Matches SM-P613 GLES offline clear (SM-P613 `washFrac`≈0.002); wash bug **does not** reproduce on Mali under GLES |

**Divergence takeaway:** On Mali-G715 + stock Android 17, GLES looks **healthy** (no wash, no bearing crash). That supports “SM-P613 Vulkan wash was **device/GPU-backend-specific**,” not a universal MapLibre hillshade defect. It does **not** by itself prove Vulkan would be safe on Pixel — Vulkan was not re-enabled here. Per-device renderer selection remains an open product question; this is a second GPU data point under the global GLES default.

#### 3. Region download / indexed-pack conversion (MemAvailable)

Methodology: system `/proc/meminfo` `MemAvailable` sampled ~every 2 s during
`OstlandetV3TiledRebuildInstrumentedTest` (fallback plan + full tiled
`ensureIndexedMaps` + Friisvegen seasonal pack-hit), not process RSS alone.

| Metric | Pixel 9a (this pass) | SM-P613 baseline |
|---|---|---|
| Ostlandet tiled convert wall | `ensureIndexedMaps elapsed_ms`≈**978553** (~16.3 min); `graph_tiles=60`; report **PASS** | ~**657 s** (~11 min) |
| Min `MemAvailable` during run | ~**43 MiB** (brief dips; 51 samples &lt;200 MiB) | ~**329 MiB** |
| Swap context | Device already heavily swapped at start (`SwapUsed`≈**3660 MiB** of ~3661 MiB); little further swap headroom | ~+250 MiB swap during convert |

**Divergence takeaway:** Higher `MemTotal` did **not** make this convert “comfortable” under a busy system — absolute `MemAvailable` bottoms were **worse** than the SM-P613 controlled baseline. Treat Ostlandet convert memory pressure as **still a real risk** on Pixel-class hardware when the rest of the system is memory-busy; do not assume the SM-P613 thin-margin issue is tablet-only. Re-measure on a quieter Pixel (lower baseline swap) before claiming a permanent margin improvement.

Friisvegen seasonal (same test class): summer `pack_hit=true`,
`seasonal_closure_excluded_edges=0`; winter `pack_hit=true`,
`seasonal_closure_excluded_edges=36`, route `FAIL` / longer alt as expected.

#### 4. Regression corridors and recent features

Follow-up pass **2026-08-11/12** after Espresso **3.7.0** / androidx.test **1.7**
(API 37 `InputManager.getInstance` fix — see below).

| Scenario | Result |
|---|---|
| DNT Skolla→Rondvassbu keyboard hike (`HikingSearchRouteScreenshotTest`) | **PASS** |
| Espa→Atnbrufossen eco plan + sim (`DiagnosticLogOnDeviceInstrumentedTest`) | **PASS** |
| Full `CorridorInstrumentedTest` (smoke, map overlay, **realPipeline**, icons) | **PASS** (after wiping stale corridor indexed packs that caused a false `pack_hit` + snap fail — **not** a tooling issue) |
| Friisvegen seasonal (Ostlandet v3 rebuild class) | **PASS** |
| Rena leir military + Gjende glacier (`MilitaryGlacierLanduseScreenshotTest`) | **PASS** |
| Hellstugubrean glacier name ladder (`GlacierNameLabelScreenshotTest`) | **PASS** |
| Speed HUD FFI / Compose (`SpeedHudFfiInstrumentedTest`) | **PASS** |
| Speed-camera lean-pack icon (`SpeedCameraIconScreenshotTest`) | **PASS** |
| Avoid-motorways priority share (`AvoidMotorwaysShareInstrumentedTest`) | **PASS** |
| Bearing isolation (`BearingCrashIsolationTest`) | **PASS** |
| Diagnostic logging path | **PASS** — `/storage/emulated/0/Documents/debug/` on stock Pixel |

`realPipeline` snap failure with `pack_hit=true` was **separate from** the Espresso
gap: leftover/partial `espa-atnbrufossen-corridor` indexed packs made the pack
loader succeed while Atnbrufossen sat ~4.6 km off the pack graph. Clearing those
packs (and skipping `icons/aprs` asset dirs in corridor `setUp`) restored
**PASS**. Instrumented runner also marks the first-run speed-camera opt-in prompt
as already shown so that modal cannot block Compose UI after `pm clear`.

#### 4b. Instrumented test tooling for API 37

Android 17 removes reflective `InputManager.getInstance()`. Espresso **3.6.1**
still called it from `Espresso.onIdle` (Compose UI test path). Official fix:
[Espresso 3.7.0](https://developer.android.com/jetpack/androidx/releases/test)
(2025-07-30) — *“Use getSystemService instead of reflective InputManager.getInstance”*.

Pinned in `app/build.gradle.kts` (app `compileSdk`/`targetSdk` remain **36**):

| Artifact | Version |
|---|---|
| `androidx.test.espresso:espresso-core` | **3.7.0** |
| `androidx.test:runner` | **1.7.0** |
| `androidx.test:rules` | **1.7.0** |
| `androidx.test.ext:junit` | **1.3.0** |
| `androidx.test.uiautomator:uiautomator` | 2.3.0 (unchanged) |

Compose `ui-test-junit4` stays on the existing Compose BOM; it resolves Espresso
**3.5.0 → 3.7.0** via the direct espresso-core pin.

SM-P613 re-check (**Android 14 / API 34**, serial `R52TB0JQEDE`, 2026-08-12):
`SpeedHudFfiInstrumentedTest` + `BearingCrashIsolationTest` +
`AvoidMotorwaysShareInstrumentedTest` — **PASS** (6/6) with the same Espresso
3.7.0 / androidx.test 1.7 pins. No `InputManager` regression on the older API.

#### 5. Why this device matters

Pixel 9a is the first **non-Samsung / non-Adreno** confirmed target in this
project. Renderer and memory results above are **new data points** for the
recurring “universal fix vs device-specific” question — especially GLES
hillshade (looks good on Mali) and Ostlandet convert `MemAvailable` (still
thin when the system is busy).

---

## GitHub-hosted instrumented CI

The GitHub-hosted instrumented Android test workflow
(`android-instrumented.yml`) runs on manual dispatch only, not automatically.
Across repeated investigation, GitHub's runner renders through SwiftShader
(software Vulkan/GL), which has produced multiple environment-specific artifacts
(AVD profile mismatches, emulator memory overcommit under 3D rendering) rather
than genuine app-level bugs. Local emulator testing against the project's
validated Automotive AVD profile, and real-hardware testing once available, are
the trusted sources for instrumented test results. This workflow remains
available for deliberate manual runs (e.g. pre-release sanity checks) but is not
a blocking gate and does not run unattended.

Evidence collection, Automotive API 33 targeting, and the Basemap CI/local 3D
split stay in place for those manual runs — they are not discarded just because
the nightly schedule was removed.

---

## General approach

For each area below, run the described scenario on real hardware and capture:

- **Preferred:** a **Tools → Diagnostic logging** session file from
  **Documents/debug** (see above) — no adb required.
- A screenshot at key states (device screenshot share, or optional
  `adb exec-out screencap -p > name.png` if you already use adb).
- A plain description of what was observed vs. expected.
- **Optional (developers):** `adb logcat` for the relevant tag(s), full and
  untruncated (no `tail`/`head` — per this project's existing logging
  convention), especially for native crashes.

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
- Log: prefer Diagnostic logging (`GPS` / `CAMERA` / `TOGGLE` lines). Optional:
  `adb logcat` filtered for MapLibre/GL-related tags during marker registration
  and the first position update.

## 2. Menu behavior

- Exercise each menu item from the HUD verification pass (profile menu, eco
  toggle, To/Via search, saved routes, rotation mode, ETA toggle,
  break-reminder toggle, zoom controls, and all bottom-bar settings: break
  interval, rest time, fuel tank capacity, fuel added).
- For each: does it open/apply/auto-close correctly? Any lag, visual glitch, or
  crash not seen on the emulator?
- Log: prefer **Tools → Diagnostic logging** session file under Documents/debug
  (TOGGLE / SETTING_SAVED lines cover menu applies). Optional: `adb logcat`
  during each menu interaction; screenshot before/after each apply action.

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
- Log: prefer Diagnostic logging (`ROUTE_PLAN` / `ROUTE_PLAN_STAGES` for
  wall-clock stages). Optional: `adb logcat` during graph parse/build/reweight.

## 4. Settings persistence

- Set values in each settings area (vehicle limits, fuel config, rest config,
  rotation mode, unit preference) and confirm they persist correctly across an
  app restart on real hardware.
- Log: confirm via Diagnostic logging (`SETTING_SAVED` / `TOGGLE`) or optional
  `adb logcat` / a settings-dump mechanism if available, rather than just visual
  confirmation, so persistence is verifiable in the log record.

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
- Log: prefer Diagnostic logging (`GPS` lines while walking/driving). Optional:
  `adb logcat` for location/sensor provider tags during a short real walk
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
- Log: prefer Diagnostic logging when the plugin writes under
  Documents/debug; optional full `adb logcat` for the ECU/plugin tag; do not
  log VIN by default.

## 7. Hydro soft-edge fringe (capture vs live)

Background:
[`map-styles.md` — Hydro soft-edge fringe](map-styles.md#hydro-soft-edge-fringe-screenshot-artifact).
On the Automotive emulator the blue rim is a **screenshot-capture artifact**
(not visible live). Spot-check on real hardware that **live** shorelines stay
clean, and that any instrumented/`adb screencap` rim (if still present) is not
misread as a device GPU defect. Hillshade must still sit under water (no
darkened lake fill when 3D is on).

- **Live (primary):** with 3D off and on, look at a lake shoreline, a wide
  river, and a creek without taking a screenshot first — report whether any
  soft blue rim is visible to the eye.
- **Capture (secondary):** then take `adb exec-out screencap` at the same
  views; note whether fringe appears only in the PNG (matches emulator capture
  quirk) or also live.
- **Roads contrast:** confirm road edges stay sharp in both live and capture.
- Report: **live clean / capture-only fringe**, **live fringe on device**
  (unexpected — escalate), or **capture clean after settle wait**.
- Log: optional `adb logcat` around style load / 3D toggle; live observation
  matters more than metrics here.

## 8. Map long-press and saved places

Product how-to: [`map-marking-saved-places.md`](map-marking-saved-places.md).

- Hold **~4 s** on a clear map area (not the planning chrome): confirm the
  fill ring, then the **Marked location** sheet.
- Short press (~1–2 s) and pan/drag must **not** open the sheet.
- **Set as From / Via / To** fills the corresponding field; **Save this place**
  appears under **Saved places** (distinct from **Saved routes**).
- Select a saved place later as From / Via / To; Rename / Delete work.
- Screenshot evidence may live under `docs/images/map-long-press/`.

---

## Output format

For each of the sections above, report: pass/fail/differs-from-emulator, the
relevant logcat excerpt (full, not truncated), and any screenshots taken. If
real hardware is Android Automotive specifically (not a phone/tablet running
the app), note the exact device/head-unit model, since automotive-class
hardware may have different GPU/driver behavior than typical phone hardware —
directly relevant to items 1 and 7.
