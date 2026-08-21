# Current street (bottom HUD)

Shows **which road the vehicle is on now**, as a low-weight line on the
collapsed bottom drive bar — Navit’s “Currently on …” pattern. Distinct from
the approach-instruction box, which shows the **next** street after a turn
([`approach-instructions.md`](approach-instructions.md)).

**Status:** implemented.

---

## Where it appears

| Element | Role |
|---|---|
| Bottom drive HUD | `Currently on {label}` — `labelSmall`, one line, test tag `hud_current_street` |
| Bottom drive HUD | `{speed} / {limit} km/h` — same column, test tag `hud_current_speed` |
| Approach box | Next street only — unchanged |

Layout notes: [`hud-layout.md`](hud-layout.md).

---

## Label resolution

Same order as approach next-street for name/ref, then a **highway-class fallback**
(approach omits the name line when both are missing; current-street does not):

1. OSM way `name` (UTF-8)
2. Else OSM `ref`
3. Else human highway-class label from the **same** class table as maxspeed /
   ETA fallback (`motorway` → “Motorway”, `service` → “Service road”, `path` →
   “Path”, …) — never a raw `highway=*` tag

Core: `driver_break_core::current_road_label` /
`routing::eta::highway_class_display_label`.  
Kotlin mirror (HUD): `formatCurrentRoadLabel` / `highwayClassDisplayLabel` in
`RouteGuidanceModels.kt` (kept aligned with `highwayFallbackKmh`).

UniFFI: `formatCurrentRoadLabel`, `highwayClassDisplayLabel`, `roadLabelNear`,
`roadNearInfo`, `currentSpeedKmh`, `resolveSpeedLimitKmh`, `overspeedDeltaKmh`.

Live GPS speed comes from Android `Location.speed` (m/s → km/h) via
`updateGpsFix`; the applicable limit reuses sticky nearest-edge matching
(`RoadLabelSticky`) plus `maxspeed:conditional` evaluation and the ETA
highway-class fallback table.

Overspeed chrome uses [`OverspeedHud`](../app/src/main/java/no/navi/app/OverspeedHud.kt)
(hybrid margin, widened from an untuned `+0.5` float-epsilon; effective margin
is `max(limit × 0.05, speedAccuracyKmh, 3.0)` — 5% of the posted limit, optional
GNSS speed-accuracy widening, and a 3.0 km/h floor). Confirm outdoors with
`GpsSpeedNoiseInstrumentedTest` (not the route simulator).
Spoken escalating overspeed (percentage tiers, arm delay) is a **plugin spec
only** — [`plugins/adaptive-speed-warning-spec.md`](plugins/adaptive-speed-warning-spec.md) —
and must not fire unless this HUD would already paint overspeed.

---

## Live updates / no-route policy

| Situation | Behaviour |
|---|---|
| Active planned corridor (or debug simulation) | Label from snapped `sim_samples_json` sample (`street` + `highway`) on every fix |
| **No planned corridor** (idle GPS) | 1) Quick interim from place-index addresses (most-common street in ~200 m). 2) Then upgrade to the **nearest routing-graph edge** within ~80 m (`road_label_near` / sticky `RoadLabelSticky` over shape-aware distance) when a region PBF is available. Among edges within 8 m of the closest hit, prefer one with OSM `name`/`ref` over a class-only stub. **Stickiness:** once locked, require the alternate to be ≥ ~10 m closer for two consecutive polls (~3 s each) before switching — avoids Furnesvegen/E6-class flip-flop from GPS noise on ~25 m parallel corridors. Distance uses full edge shape, not start→end chords. Refresh every ~3 s or after ~30 m of movement (IO thread; in-process cell cache) |
| No region PBF on device | Place-index interim only (weaker at junctions — side-street houses can outvote the through-road) |
| Neither PBF nor place index | Line stays hidden |

**Why nearest edge (not addresses alone):** idle GPS must name the OSM **way** under the fix. At Peer Gyntvegen / Steinbrotvegen junctions, closer house numbers on a side street produced the wrong HUD label when only the place index was used. A small bbox-clipped graph (~0.05° cell + pad, cached under `graph-cache-*`) is enough; the full country graph is never loaded for this path. Corridor samples remain preferred when a route is active.

---

## Unicode (æ / å / ø / ä / ü / …)

Road names must survive OSM parse → graph cache → `sim_samples_json` → UniFFI /
Kotlin JSON → Compose. See [`unicode-road-names.md`](unicode-road-names.md).

---

## Code map

| Piece | Path |
|---|---|
| Sample `street` field | `core/src/routing/guidance_path.rs` |
| Nearest-edge idle label | `core/src/routing/graph/road_near.rs` (`nearest_road_label`) |
| UniFFI idle snap | `road_label_near` in `navi-ffi` |
| Class labels | `core/src/routing/eta.rs` |
| Bottom bar UI | `BottomDriveHud` in `DriveHud.kt` |
| Fix-path wiring | `MainActivity.kt` (`applyFix` + clear on delete route) |
