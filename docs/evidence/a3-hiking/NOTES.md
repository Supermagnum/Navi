# A3 Hiking evidence: Gjendesheim -> Leirvassbu (via) -> Skjolden Hotel

Emulator runs via `scripts/emulator-hiking-a3.sh`. Profile: hiking. Waypoints: Gjendesheim, Leirvassbu, Skjolden Hotel.

## Variants

| Variant | Flags | Status | Distance | Via proximity | Densify |
| --- | --- | --- | --- | --- | --- |
| cabins-on | `--unlocked --networked` | planning_done | 102.865 km | 15.29 m | 4 waypoints added |
| cabins-off | (defaults off) | planning_done | 102.865 km | 15.29 m | 4 waypoints added |

Geometry is effectively identical between variants (same distance, same polyline length, same hybrid off-trail meters). Cabin flags change overnight / rest POI selection, not the path itself in this corridor.

## Trails vs roads

- Report: `route_mode=hybrid; off_trail_m=27359` (~27.4 km off mapped trail/foot graph).
- Off-trail advisory present (DEM/OSM limitations; right-to-roam does not imply safety).
- Hiking avoids motorways (locked). No per-highway-class share fields in this report dump.
- Terrain-gap densify: `terrain_gap_densify_added=4` so long disconnected foot-graph legs still plan (crow > 15 km gaps).

## Via / huts honored

- Explicit via Leirvassbu: closest polyline point ~15.3 m (log: waypoint pin warning prefer <= 12 m; corridor point used).
- Auto-via: `auto_vias=1; names=Memurubu` in both variants.
- Break POIs (12) include Memurubu, tent sites, Nøgla, Krossbu, Sognefjellshytta, Leirvassbu, and overnight huts.

## Unlocked + networked cabin effect

Overnight stops differ:

- **cabins-on** (unlocked+networked): Steinbu i Veslådalen (unlocked), Nøgla (unlocked), Trulsbu (networked / network_hut).
- **cabins-off**: Memurubu, Leirvassbu, Sognefjellshytta (reported cabin_class unlocked; no network hut overnight).

Rest-place lists also differ at the overnight positions (Trulsbu only when networked). Path distance unchanged.

## Script / artifact notes

- Boolean flags passed to Python as string compares (`== "true"`) so bare `true`/`false` cannot NameError.
- Waiter streams `adb logcat` through grep (does not hold the full dump in a bash variable); force-stops the app before start; recovers `status=done` from logcat if the wait loop races.
- Hiking planner does not emit car-style `hops.json` / `routing-plan.log`; stale car dumps are discarded when the route report is hiking.
- Primary evidence per variant: `result.json`, `route-result.json`, `route-polyline.txt`, `logcat.txt`, `plan-summary.txt`, `a3_hiking.png`.

## A3 completion

Both variants produced `planning_done` and the evidence files above. A3 complete for this corridor. Do not treat as A4 / commit gate.
