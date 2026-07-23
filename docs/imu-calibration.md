# IMU calibration for car-mounted units (deferred)

**Status: documentation only — not implemented.**  
Same treatment as early approach-instruction and voice-guidance specs: design
first, code later.

This feature improves **inclination (pitch/tilt)** accuracy for a
vehicle-mounted IMU used as an **accuracy boost** for elevation / gradient
inputs into the **eco-routing cost model**. It is **not** required for basic
Compass or Direction-of-travel map rotation (those modes need heading/course;
they work without this calibration).

Related: [`build-linux.md`](build-linux.md) (gpsd + IMU on Linux),
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
4. The app captures the IMU’s current reading as the **zero-reference offset**
   for pitch and roll and stores it against the **active vehicle profile**
   (so multiple vehicles / mounts can keep different offsets).

Until Calibrate is run, the stack may assume a default “already level” offset
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

Any upload requires **explicit opt-in**, consistent with Navi’s rule that every
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

## Implementation checklist (future)

1. Persist pitch/roll offset per vehicle profile in `app_config` / profile store.
2. Settings UI: Calibrate button + “level confirmed” copy.
3. Optional local logger with quality gating.
4. Opt-in upload client + server privacy review.
5. Correction tile format + apply path next to DEM cache.
6. Document defaults and that rotation modes do not depend on this feature.
