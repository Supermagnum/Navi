# IMU plugin: pitch / tilt / compass and mount calibration
**Status:** live display, mount calibration, reset, the configurable tilt
alarm and the Android product wiring are **implemented**: a wasmtime guest
(`plugins/imu`, engine in `plugins/imu/engine`) on the read-only `sensors_read`
HostApi capability, fed by an Android `SensorManager` backend through UniFFI, with
a settings panel under Map / Tools -> Plugins. The logging / upload / correction
pipeline below is still **deferred** (documentation only).

Issue: #150. Plugin README: [`plugins/imu/README.md`](../../plugins/imu/README.md).
This feature improves **inclination (pitch/tilt)** accuracy for a
vehicle-mounted IMU used as an **accuracy boost** for elevation / gradient
inputs into the **eco-routing cost model**. It is **not** required for basic
Compass or Direction-of-travel map rotation (those modes need heading/course;
they work without this calibration).
Related: [`build-linux.md`](../build-linux.md) (gpsd + IMU on Linux),
[`architecture.md`](../architecture.md) (sensor tier), eco costing in core.
---
## Purpose
| In scope | Out of scope |
|---|---|
| Zero-reference pitch/roll for a given vehicle mount | Replacing DEM elevation entirely |
| Optional logging of inclination + GPS for later correction | Silent background upload |
| Future DEM-adjacent correction downloads | Mandatory cloud account |
---
## User-facing calibration procedure

The settings panel shows this text verbatim next to the **Calibrate** button:

> Find a level concrete surface, like the floor of your garage, then tap Calibrate. The unit will then use the current values as level.

1. Park the vehicle on a level, smooth surface (a garage floor works well).
2. Open Map / Tools -> Plugins -> IMU pitch / tilt / compass and tap **Calibrate**.
3. Keep the vehicle still for about a second. The plugin captures the current
   gravity direction and stores the rotation that makes it level; if anything
   moves, the request is rejected and nothing is stored.
4. **Reset** removes the stored calibration. The status line and the
   "Calibration date" line show whether the unit is calibrated and when.

Until Calibrate is run, the unit reports its orientation as mounted (a
vehicle-mounted unit is almost never aligned with the vehicle).

Storing the calibration per vehicle profile (so several vehicles can keep
different mounts) is not done yet; there is one mount record per install.

---

## Deferred data collection and correction pipeline
Not built in this pass. Design intent:
```
calibrate → (optional) log inclination + GPS while driving
         → opt-in upload to public aggregation server
         → download correction alongside DEM tiles
         → apply gradient corrections ~every 50 m between good fixes
```
### Logging
After calibration, the app **may** optionally log inclination readings together
with GPS position during normal driving (user-visible toggle).
### Opt-in public server
Any upload requires **explicit opt-in**, consistent with Navi's rule that every
network call is user-visible / consented. No inclination trajectories are
collected or uploaded by default.
### Correction usage
Contributed data, once aggregated, would be downloaded with elevation (DEM)
packs and used to correct elevation/gradient estimates at roughly **50 m**
intervals between measurement points that have a GPS fix of **good quality** —
**Quality gate:** contribute / apply only when horizontal accuracy is **≤ 10 m**
(or better). Lower-accuracy fixes must not enter the correction set.
### Privacy
Inclination logs tied to GPS trajectories are **sensitive location data**.
Before any real server is implemented, the design needs a dedicated
**privacy / anonymization** treatment (retention limits, trajectory fuzzing or
aggregation, no re-identification), not merely an opt-in checkbox.
---
## Also in scope for this plugin (live sensor display)
Beyond the deferred calibration/correction pipeline above, the IMU plugin
itself should provide, once implemented:
- Live **pitch/tilt** and **compass course** display, useful for off-road driving.
- A **configurable alarm** that sounds when pitch/tilt approaches
  dangerous values (thresholds must be user-configurable, not hardcoded).
---
## Implemented in this pass

| Piece | Where |
|---|---|
| `sensors_read` capability, `sensor_availability` / `sensor_read` imports, `HostApi::sensor_*` (default: unavailable, never zeros) | `plugin-host`, `plugin-sdk` |
| `SensorHub`: availability detection, per-sensor rate limit (default 100 ms), lazy register, unregister after 3 s idle / on disable | `plugin-host/src/sensor_hub.rs` |
| Sensors: accelerometer, ambient temperature, gravity, gyroscope, light, linear acceleration, magnetic field, pressure, proximity, relative humidity, rotation vector | `SensorKind` |
| Android mapping (`SensorKind::android_sensor_types`): ambient temperature prefers `TYPE_AMBIENT_TEMPERATURE` (13) over deprecated `TYPE_TEMPERATURE` (7) | `plugin-host/src/abi.rs` |
| Pitch / roll / tilt from gravity (accelerometer fallback), tilt-compensated magnetic heading | `plugins/imu/engine` |
| Calibration, reset, calibration date and status text | `plugins/imu/engine` |
| Tilt alarm level with user thresholds and hysteresis (host plays the sound) | `plugins/imu/engine/src/alarm.rs` |

### Android product wiring

| Piece | Where |
|---|---|
| `SensorPlatform` (UniFFI foreign trait) + `imu_plugin_*` session API (configure, install guest, enable, tick, calibrate / reset, alarm settings, release sensors) | `navi-ffi/src/imu_plugin.rs` |
| `SensorManager` backend: availability detection (`TYPE_AMBIENT_TEMPERATURE` first, `TYPE_TEMPERATURE` fallback via the shared `SensorKind` table), per-sensor rate limit, listener unregistered when the hub releases it | `ImuSensorBackend.kt`, `AndroidSensorSource` |
| Controller: ticks the guest (~8 Hz) only while the plugin is enabled and the app is started, publishes state (no alarm audio) | `ImuPluginController.kt`, `ImuPluginRuntime.kt` |
| Settings panel: enable switch, verbatim calibration instruction, Calibrate / Reset, date and status, live pitch / roll / tilt / heading, tilt warning thresholds | `ImuPluginPanel.kt` (inside `PluginSettingsSection`) |
| Tilt warning levels are display-only; host does not play IMU alarm audio | (removed `ImuAlarmAudio.kt`) |
| Guest staging: `scripts/build-plugin-wasm.sh` builds `plugins/imu` into `assets/plugins/imu/`; the app copies it to `filesDir/plugins/imu/` and reinstalls on a version change | same pattern as the camping guest |

The plugin is **off by default** (`plugin_enable.json`, name `imu`). Turning it
off unregisters every sensor listener and drops the live display state; the saved
calibration stays until **Reset**. `imu_plugin_tick` disables the plugin for the
session on a fuel / timeout / memory / trap failure and shows the reason.

Tests: `cargo test -p navi-ffi --lib imu_plugin`, `cargo test -p navi-ffi --test
imu_plugin_ffi` (staged wasm guest through the public FFI surface with a fake
`SensorPlatform`), and the JVM tests `ImuSensorBackendTest`,
`ImuReadoutTest`, `ImuPluginControllerTest`. On a device or emulator,
`ImuPluginInstrumentedTest` exercises the real `SensorManager`.

### Plugin storage contract (`plugin_kv`)

| Key | Persisted | Writer | Content |
|---|---|---|---|
| `imu_mount` | yes | plugin | rotation matrix, `unix_secs`, `date` |
| `imu_alarm` | yes | host UI | `{"enabled","pitch_warn_deg","pitch_danger_deg","roll_warn_deg","roll_danger_deg","hysteresis_deg"}` (sanitised on read) |
| `live.imu_cmd` | no | host UI | `{"cmd":"calibrate"}` or `{"cmd":"reset"}`; consumed by the plugin |
| `live.imu_state` | no | plugin | status, notice, instruction, pitch / roll / tilt / heading, alarm level, calibration date, sensor presence |
| `live.imu_hist` | no | plugin | recent samples for the stationary check |

Keys prefixed `live.` stay in memory in `FilePluginKv` so a 10 Hz tick does not
rewrite flash.

Conventions: vehicle frame +X right, +Y forward, +Z up; pitch nose-up positive,
roll right-side-down positive; heading is magnetic (no declination).

### Testing

`cargo test -p navi-plugin-imu-engine` (synthetic 15 deg pitch / 10 deg roll mount
levels after calibrate, rejection while moving, persistence, reset, missing
sensors, alarm, heading) and `cargo test -p navi-plugin-host --test
imu_plugin_wasm` (same flows through wasmtime with `FilePluginKv`, plus hub
register / release). No tablet is required. On an emulator, use Extended controls
> Virtual sensors to rock or hold the device for the calibrate / reject paths;
real mount accuracy still needs a vehicle-mounted device.

## Implementation checklist

1. ~~Persist mount rotation~~ (done in `plugin_kv`); keying it per vehicle profile
   in `app_config` / the profile store is still open.
2. ~~Settings UI: Calibrate / Reset buttons + the verbatim instruction;
   Android `SensorManager` bridge feeding `SensorBackend`~~ (done).
3. Optional local logger with quality gating.
4. Opt-in upload client + server privacy review.
5. Correction tile format + apply path next to DEM cache.
6. Document defaults and that rotation modes do not depend on this feature.
7. ~~Live pitch/tilt/compass display~~ (done in the settings panel); a map HUD
   chip is still open.
8. ~~Alarm sound playback and settings UI~~ (host alarm audio removed; thresholds/UI remain;
   custom alert sounds remain the separate
   [custom alert sounds spec](custom-alert-sounds-spec.md)).
