# IMU plugin (`imu`)

Live pitch, tilt and compass heading for a vehicle-mounted unit, plus a
configurable tilt-danger alarm level. It runs as a wasmtime guest and reads the
device sensors through the read-only `sensors_read` HostApi capability. Full
design and storage contract: [`docs/plugins/imu-plugin.md`](../../docs/plugins/imu-plugin.md).

## Why calibrate (vehicle mount)

A tablet or phone in a dash mount is almost never perfectly aligned with the
vehicle. Without calibration, a car parked on level ground reads a few degrees
of pitch and roll, which would corrupt the inclination used for the tilt display,
the alarm, and the gradient input to eco routing. Calibration measures that mount
offset once and stores it as a rotation, so the unit reports the **vehicle's**
pitch and roll instead of the unit's.

## Calibration instructions

Find a level concrete surface, like the floor of your garage, then tap Calibrate. The unit will then use the current values as level.

Keep the vehicle still while it samples (about one second). If anything moves,
the plugin rejects the request and stores nothing; tap Calibrate again. **Reset**
removes the stored calibration and the unit reports its orientation as mounted.
The status line shows whether the unit is calibrated and on which date.

## Layout

| Path | Role |
|---|---|
| `engine/` | Native-testable maths, calibration, alarm, and the per-tick state machine |
| `src/lib.rs` | wasm32 shim: implements the engine `Host` over `navi-plugin-sdk` |
| `plugin.json` | Capabilities: `log`, `sensors_read`, `plugin_kv`, `clock_read` |

## Build and test

```bash
cargo build --release --target wasm32-unknown-unknown --manifest-path plugins/imu/Cargo.toml
cargo test -p navi-plugin-imu-engine
cargo test -p navi-plugin-host --test imu_plugin_wasm
```

Sensor access is host-owned: availability is detected, reads are rate-limited,
and sensor listeners are unregistered when the plugin stops reading. The plugin
never plays sound; the host reads the alarm level from the plugin state.

In the Android app the guest is staged by `scripts/build-plugin-wasm.sh`, fed by
the `SensorManager` backend (`ImuSensorBackend.kt`) through `navi-ffi`
(`imu_plugin_*`), and controlled from Map / Tools -> Plugins -> IMU pitch / tilt /
compass. It is off by default.
