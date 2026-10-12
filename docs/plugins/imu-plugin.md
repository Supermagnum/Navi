# IMU plugin: pitch / tilt / compass and mount calibration
**Status:** live display, mount calibration, reset, and the configurable tilt
alarm are **implemented** as a wasmtime guest (`plugins/imu`, engine in
`plugins/imu/engine`) on the new read-only `sensors_read` HostApi capability.
The product APK does not yet bridge Android `SensorManager` into the host or show
the settings UI (follow-up). The logging / upload / correction pipeline below is
still **deferred** (documentation only).

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
1. Park the vehicle on a **smooth, flat, level** surface.
2. Verify level in **both X and Y** with a **physical bubble level**. The app
   cannot confirm true level by itself — this is an explicit user instruction.
3. In the app, open vehicle / IMU settings and tap **Calibrate** once level is
   confirmed.
4. The app captures the IMU's current reading as the **zero-reference offset**
   for pitch and roll and stores it against the **active vehicle profile**
   (so multiple vehicles / mounts can keep different offsets).
Until Calibrate is run, the stack may assume a default "already level" offset
of zero or refuse eco-inclination corrections — product choice at
implementation time.
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

## Implementation checklist (future)
1. ~~Persist mount rotation~~ (done in `plugin_kv`); key it per vehicle profile in
   `app_config` / profile store.
2. Settings UI: Calibrate / Reset buttons + the verbatim instruction above;
   Android `SensorManager` bridge feeding `SensorBackend`.
3. Optional local logger with quality gating.
4. Opt-in upload client + server privacy review.
5. Correction tile format + apply path next to DEM cache.
6. Document defaults and that rotation modes do not depend on this feature.
7. Live pitch/tilt/compass display UI (plugin state is ready).
8. Alarm sound playback and settings UI (plugin reports the level; thresholds and
   enable flag live in `imu_alarm`).
