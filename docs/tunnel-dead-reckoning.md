# Tunnel / GPS-gap dead reckoning (IMU-assisted)

When GNSS is lost or badly degraded (tunnels, deep cuts), Navi keeps the map
position moving along the road instead of freezing, then hands back to GPS
smoothly when a good fix returns.

## Where the logic lives

**Host-owned**, not inside the IMU wasm plugin.

| Piece | Owner | Why |
| --- | --- | --- |
| Dead reckoning (enter/exit, speed carry, road snap, blend, give-up) | Android host (`DeadReckoningEngine` + `MainActivity` GPS path) | Position drives camera, guidance, and announcements. The host already owns `LocationManager` fixes via `applyFix`. |
| Sensors | Same `SensorManager` / SensorHub the IMU plugin uses (`sensors_read`) | Raw samples without Play Services. |
| Mount calibration (#162) | IMU plugin KV (`imu_mount`); host reads offsets for vehicle-frame heading | Calibration stays one place; DR reuses it. |
| Road / route geometry | Active route sample chain, else nearest graph edge (`is_tunnel` on edges) | Host already has `RouteProgressTracker` samples; graph edges come from installed packs. |

The IMU wasm guest stays pitch / tilt / compass. It does **not**
publish a map position. If a future design moved estimate generation into a
plugin, the guest would deliver an estimated fix only through a HostApi write
capability that the host sanity-checks (accuracy bounds, road snap, give-up)
before touching the map — that path is not used here.

## Sensors and calibration

1. **Heading change:** prefer `TYPE_ROTATION_VECTOR`; fall back to integrating
   `TYPE_GYROSCOPE`. Apply the #162 mount rotation so device yaw maps to vehicle
   yaw along the road.
2. **Speed change:** project `TYPE_LINEAR_ACCELERATION` onto the vehicle forward
   axis and integrate; clamp to sensible bounds.
3. **Speed carry:** if accel is missing or unusable, keep the **last good GPS
   speed** (and, on an active route, prefer the planned sample speed when the
   carried speed is wildly off).
4. **No calibration:** identity mount; heading still tracks relative gyro/RV
   change from the last GPS course. Absolute compass bias may be wrong; road
   constraint limits lateral drift.

Works without Google Play services (`LocationManager` + `SensorManager` only).

## Road constraint

- **Active route:** advance along the planned `RouteSimSample` chain by
  `speed * dt` (along-route metres). Curved tunnels stay on the polyline.
- **No route:** project onto the current road-graph edge (nearest edge at last
  good fix, including tunnel edges).
- **Tunnel tags in packs:** graph edges store `is_tunnel` from OSM `tunnel=*`
  at bake time. Serde cache on v9 packs includes `is_tunnel`, so **downloaded
  v9 packs carry the flag**. The raw tag string is not kept; the boolean is
  enough for tunnel legs (`path_tunnel_legs` / `route_tunnel_*`) and for
  treating tunnel edges as normal roads for snap. Locally built graphs use the
  same builder path.

## Behaviour

| Phase | Rule |
| --- | --- |
| Enter | No usable fix for `gps_loss_timeout_ms`, or horizontal accuracy worse than `gps_degrade_accuracy_m` for a sustained window. |
| While estimating | Feed estimated lat/lon through the same progress / announcement path as GPS (`provider=navi-dead-reckon`). Marker uses estimated colour. |
| GPS return | When accuracy is good again, blend along-route (or lerp) so the map does not jump. |
| Give up | After `max_dr_duration_s` or `max_dr_distance_m` without a good fix: show "Position lost", stop advancing, keep last estimate. |

Defaults (overridable in code / prefs): loss timeout ~2 s, degrade accuracy
~40 m, give-up ~120 s or ~3 km.

## Tests

Unit tests use injectable clock, location, and sensor sources:

- straight tunnel; curved tunnel
- speed change inside a tunnel
- GPS return with lateral/along offset (blend / error at exit)
- long gap hitting give-up
- no mount calibration (sensible fallback)

Instrumented / emulator route replay drops GPS inside every tunnel and feeds
motion-consistent sensor data; reports whether the marker kept moving, stayed
on road, exit error, and hand-back smoothness.

## Standstill gate

A reported GNSS speed of 0 stays 0: it is never raised to `minSpeedMps` and
never replaced by the planned route speed. Planned speed is used only when
speed is **missing** and the IMU shows motion (or no IMU is present).

After GNSS loss, speed is seeded from the last good GNSS value and then updated
by integrating longitudinal acceleration (device forward axis learned while GNSS
was available by correlating GNSS Δv/Δt with forwardAccel). Trust in the seed
decays with τ = 8 s; a positive GNSS speed is never left unchanged past the
2 s loss timeout. Planned route speed is only an upper bound, or a fallback
when no IMU is present.

Standstill is **braking-aware**: latch only after a braking event
(forward accel ≤ −0.75 m/s²) has brought estimated speed below 0.5 m/s, followed
by a quiet hold (accel 0.15 m/s² / yaw 2 deg/s, hold 1 s). Quiet cruise alone
never latches. Release on forward thrust ≥ 1.0 m/s² (or a sharp yaw); the speed
estimate then restarts from 0. A reported GNSS speed of 0 is still immediate 0.

While **Location diagnostics** is on, a shadow standstill evaluator mirrors the
braking-aware gate on every IMU sample in every DR mode. It never changes the
shown position, speed, or mode. With diagnostics off there is no extra cost.

## Validation

- **Stationary behaviour** is verified on a real tablet (SM-P613) with an
  anonymised indoor replay fixture and a live rest session after the fix.
- **Moving and tunnel behaviour** is verified in **simulation** (and emulator
  when an AVD is available). There has been **no real drive**. The suite uses a
  shortened Raufoss→Bergen slice with Lærdalstunnelen (straight) and
  Gudvangatunnelen (curved), generated GNSS + IMU, vibration RMS steps, and
  accelerometer biases ±0.02 / ±0.05 / ±0.1 m/s².
- **Vibration:** with the braking-aware gate, steady cruise must not false-latch
  at any step including 0.40 and near-zero RMS.
- Unit tests: `StationaryDeadReckoningTest`, `StandstillSimulatedDriveTest`.

## Drive verification checklist

Run with **Location diagnostics** on (Tools). Leave the overlay optional.

1. Steady-speed tunnel: enter a tunnel at constant speed. The standstill gate
   must **not** latch. Position must keep advancing along the route.
2. Stop inside or immediately after a tunnel: speed must go to 0 and along-track
   must stop advancing (position held).
3. Pull away from standstill: the gate must release within 1 s of motion, and
   displayed speed / along-track must resume.

Pull the session and print the timeline (no root required):

```bash
adb pull /storage/emulated/0/Android/data/no.navi.app/files/location-diag/ \
  ./local/location-diag-raw/
python3 tools/location_diag_timeline.py \
  ./local/location-diag-raw/location-diag-*.jsonl
```

Raw pulls stay under `local/location-diag-raw/` (gitignored). If a fixture is
needed for unit tests, anonymise first:

```bash
python3 tools/anonymise_location_diag.py \
  ./local/location-diag-raw/location-diag-YYYYMMDD-HHMMSS.jsonl \
  -o app/src/test/resources/location-diag-replay.jsonl
```

## Limits

- Not a substitute for GNSS outdoors; only bridges gaps while constrained to a
  known road or route.
- Without a route and without a nearby graph edge, DR cannot continue and
  falls through to "Position lost".
- Accel-based speed is approximate; long tunnels without calibration drift more
  — give-up bounds exist for that reason.
