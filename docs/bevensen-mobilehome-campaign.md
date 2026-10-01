# Bad Bevensen → Norway MobileHome campaign

Date: 2026-09-24 (corrected retest). Branch: **`dev`** (not yet merged to
**`main`** as of this writing).

**Canonical role:** campaign evidence for the long-trip MobileHome path on a
fixed Automotive AVD. Chronological Android log remains
[`android-test-results.md`](android-test-results.md) (Item 17 points here).
Product status stays in the README Features / Known issues tables — do not treat
this file as live status unless README is updated to match.

Instrumented classes:

- `LongTripMobileHomeBevensenLiveTest` — earlier live run (height/weight left
  null when inputs were missing).
- `LongTripMobileHomeBevensenResumePlanTest` — corrected resume + plan with
  VW Transporter T6 2.0 BiTDi 4Motion camper height/weight and 6 h/day soft
  budget wired through `FfiCarRestSettings.maxHours`.

---

## Test setup

| Item | Value |
|---|---|
| Emulator | Fixed Automotive AVD |
| CPU | 8 cores |
| Internal storage | 128 GB |
| RAM | 4 GB |
| SD card | 512 GB (host image under `.avd-sd/`; not committed) |
| Origin | Bad Bevensen Kurpark Stellplatz ≈ 53.079686, 10.587198 |
| Destination | 61.6170857, 8.0438639 |
| Departure | `2026-06-01T08:00:00` local |
| Eco | on |
| Tolls | `FfiTollPolicy.PENALIZE` (soft cost; not hard avoid) |
| Ferries | avoid |
| Soft daily budget | 6.0 h (`RestConfig.car.max_hours` via UniFFI) |
| Soft break spacing | 1.5 h interval, 15 min rest |

Packs download to the removable SD volume; plan `dataDir` / graph `cacheDir`
stay under app internal data so `navi.db` rest settings resolve (same layout as
MainActivity long-trip).

### Vehicle — VW Transporter T6 2.0 BiTDi 4Motion camper

Canonical MobileHome long-trip profile used by
`LongTripMobileHomeAvdInstrumentedTest`,
`LongTripMobileHomeBevensenLiveTest` (length/width/tank), and
`LongTripMobileHomeBevensenResumePlanTest` (corrected height/weight wiring).
Raised-roof conversion body height — **not** a stock VW California normal-roof
(~1.99 m).

| Spec | Value |
|---|---|
| Label | VW Transporter T6 2.0 BiTDi 4Motion camper |
| Engine / drive | 2.0 L BiTDi twin-turbo I4, 199 PS, 4Motion (4×4) |
| Wheelbase | 3.400 m (LWB) |
| Length | 5.304 m (LWB) |
| Width (excl. mirrors) | 1.904 m (on file; not passed to `FfiVehicleLimits`) |
| Width (incl. mirrors) | 2.297 m (wired as `widthM`) |
| Body height | 2.477 m (wired as `heightM` on corrected retest) |
| Loaded total weight | 3020.4 kg (wired as `totalWeightKg`; 2–3 people + multi-day kit) |
| Loaded rear axle | 1661.2 kg (wired as `axleWeightKg`) |
| Loaded front axle | 1359.2 kg (total − rear; not separately wired) |
| Fuel tank | 70 L (`FuelConfig` learning / HUD only — see Known gaps) |
| Conservative GVWR (reference) | 2800 kg (Avd report bound; loaded total is over) |
| Conservative GAWR front / rear | 1710 kg / 1625 kg (closest-variant bounds in Avd report; loaded rear is over) |

---

## Bug history and fixes (in order)

All four fixes landed on **`dev`** (and related long-trip work) before or with
this campaign. They are **not** on **`main`** as of 2026-09-24.

### 1. O(E) scan hang in `node_has_allowed_incident` (vehicle-filtered snap)

Vehicle-filtered waypoint snap scanned incident edges with an O(E) walk per
node, which hung on long-trip graphs. Fixed in the graph builder / snap path
(`node_has_allowed_incident` / unfiltered incident helper). Commit on `dev`:
`Fix vehicle-filtered snap hang from O(E) per-node edge scans.`

### 2. First-hop graph-load OOM (~3 GiB LMK → ~984 MiB after fix)

Loading the first long-trip hop materialization exceeded ~3 GiB and tripped the
Android low-memory killer on the 4 GB AVD. Corridor / hop graph loads were
clipped to fit Automotive RAM. Peak observed after the fix: about **984 MiB**.
Commit on `dev`: `Clip long-trip graph loads to fit 4 GB Automotive RAM.`

### 3. Corridor-band clip bug on leg 13 (pad widening vs materialization)

Widening the corridor pad did not expand the corridor-band edge materialization,
so leg 13 stayed disconnected under band clip. Retry path: fall back to
**trip-AABB** edge clip when the band stays disconnected (`edge_clip_fallback=trip_aabb`).
That fallback is **expected** on the corrected Bevensen run for leg 13.
Commit on `dev`: `Fall back to trip-AABB edge clip when corridor band stays disconnected.`

### 4. Day-budget wiring bug (`FfiCarRestSettings.max_hours` + wrong data dir)

Soft multi-day overnight splits read `RestConfig.car.max_hours`, but UniFFI
`FfiCarRestSettings` did not expose `max_hours`, and plan-time rest load used the
graph-cache parent (SD pack root) — creating/reading an empty `navi.db` that
shadowed the real soft budget (default 8 h instead of the saved 6 h).

Fix:

- Add `max_hours` to `FfiCarRestSettings` load/save.
- Prefer host settings `data_dir` when loading rest config for a plan
  (`load_rest_config_for_plan`).

---

## Final corrected retest

| Metric | Result |
|---|---|
| Distance | **1648.6 km** |
| Driving time | **~21.78 h** |
| Days at 6 h/day soft budget | **4** |
| Days 1–3 | **454.2 km** each |
| Day 4 (final) | **286.1 km** / **3.78 h** |
| Leg status | **13/13 PASS** |
| Leg 13 clip | trip-AABB fallback **expected** |
| Vehicle limits wired | T6 BiTDi 4Motion — **2.477 m** / **2.297 m** width / **5.304 m** / **3020.4 kg** / **1661.2 kg** rear axle |
| Break POIs | **16** (13 soft + 3 overnight) |

Engine excludes ways by `RouteOptions.vehicle` (`maxheight` / `maxweight` /
`maxwidth` / `maxlength`) when limits are saved. Toll policy on the plan was
`PENALIZE` with ferries avoided and eco on.

---

## Known gaps

- **Fuel-stop planning is unimplemented.** `FuelConfig` (`tankCapacityL` /
  `fuelAddedL`) is tank/fill learning input for HUD / consumption estimates only.
  There is no fuel-stop / range lookahead planner inside `planCarRouteAt`. Spec
  direction only: [`plugins/safety-resupply.md`](plugins/safety-resupply.md).

---

## Branch note

All campaign fixes and the corrected retest evidence above are on **`dev`**.
They are **not yet merged to `main`** as of this writing (2026-09-24). Update
this paragraph when that changes.
