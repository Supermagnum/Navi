# Debug route simulation

Debug-only playback that walks a planned motor corridor and feeds the **same**
`applyFix` / `updateGpsFix` path as live `LocationManager` fixes. Failures here
are treated as bugs in the live GPS-driven guidance pipeline, not simulator
artefacts.

## Gating

- UI entry (**Simulate route** / **Stop simulation**) is shown only when the app
  is **debuggable** (`ApplicationInfo.FLAG_DEBUGGABLE`).
- A red **SIMULATING** banner (`testTag("simulating_banner")`) stays visible while
  playback or seek-driven fixes are active.
- **UiAutomator / accessibility caveat:** On the Automotive emulator, MapLibre’s
  `SurfaceView` and AAOS system chrome often omit Compose text from the
  UiAutomator accessibility tree, so `By.text("SIMULATING")` can report
  `banner_visible=false` even when the red label is on screen. Treat pixel
  screenshots (`adb exec-out screencap -p` or instrumentation
  `uiAutomation.takeScreenshot`) as the source of truth for banner visibility;
  do not re-flag a failed UiAutomator text query as a missing-banner bug without
  a visual check.
- Real `LocationManager` fixes are ignored while simulating so they cannot fight
  the playback.

## Speed

Each densified sample carries `speed_kmh` from the planned graph edge:

1. Posted OSM `maxspeed` when parseable
2. Else the highway-class fallback table used for pre-departure ETA
   (`core/src/routing/eta.rs` — motorway 100 … residential 40 … service 20,
   default 50)

Wall-clock playback is **1×** that speed in the UI. Instrumented tests may set
`NaviMapTestHooks.simulationTimeScale` to compress waits only; reported speed
still follows the table above.

## Guidance

`RouteProgressTracker` snaps each fix onto the sample chain and publishes:

- Approach-instruction box (750 m / 150 m / 25 m thresholds)
- Remaining trip ETA from remaining sample segment times
- Break countdown from **integrated** planned driving hours along the sample
  chain (same segment-speed method as ETA) — not `along_m / instantaneous_speed`,
  which under-counts elapsed hours and inflates minutes-to-break
- Direction-of-travel camera bearing from sample course
- Via geofence (~40 m) continue; end geofence (~40 m) stop / arrive

Plan results expose `simSamplesJson` and `maneuversJson` on
`CorridorRouteResult` (also merged across multi-via legs on the Android host).

## Related

- Live GPS / IMU hardware checks remain in [`real-hardware-testing.md`](real-hardware-testing.md)
  — this simulator does **not** replace those.
- Approach UX: [`approach-instructions.md`](approach-instructions.md)
