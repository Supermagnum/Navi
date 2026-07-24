# EC 561/2006 truck rest (Navi coverage)

Navi’s **Truck** and **TruckElectric** profiles use `TruckRestParams`
(defaults in `core/src/config/defaults.rs`) for EU Regulation EC 561/2006
driving-time and break rules. This page states what is **enforced**, what is
**tracked / informational**, and what remains **deferred**.

Official summary:
[EU driving time and rest periods](https://transport.ec.europa.eu/transport-modes/road/social-provisions/driving-time-and-rest-periods_en).

Related: jurisdiction pack pattern — [`jurisdiction-rules.md`](jurisdiction-rules.md);
US FMCSA pack — [`fmcsa-truck-rest.md`](fmcsa-truck-rest.md);
truck overnight POI tags — [`poi.md`](poi.md) (**RestArea**).

## MobileHome (private motorhome) — deliberate separation

**Decision:** MobileHome does **not** use EC 561/2006 legal tracking.

When truck rest was first wired, MobileHome briefly shared the same
`TruckRestParams` path as Truck because it already shared the truck *routing*
profile for vehicle clearance. That was an **unexamined default**, not a
product decision that private motorhome drivers are under commercial HGV law.

**Current behaviour (intentional):**

| Concern | MobileHome |
|---|---|
| Road clearance (height / width / axle …) | Same physical limits as truck (routing) |
| Break reminders / rest settings | **Car-style** soft cadence (driver-chosen hours between breaks) |
| Soft multi-day overnight | Same as car: when driving time exceeds the soft daily budget (`plan_motor_multi_day`), lodging/camping/rest-area suggestions — **not** EC 561 daily/weekly rest law |
| EC 561 daily / weekly / fortnightly caps | **Not applied** — not a legal compliance tracker for leisure motorhomes |

A private motorhome driver is generally **not** bound by commercial HGV
driving-hours regulation the way a truck driver is. Showing EC 561 “duty”
language for MobileHome would imply a legal obligation the app is not entitled
to assert. Soft car-style rest reminders and soft multi-day overnight remain
available as **wellbeing / fatigue suggestions** only (see README “Rest and
overnight” and [`poi.md`](poi.md) **Lodging** / **RestArea**).

| Rule | Regulation | Navi status (Truck / TruckElectric only) |
|---|---|---|
| Break after 4.5 h driving (45 min continuous) | Art. 7 | **Enforced** in motor route break spacing (`mandatory_break_after_hours` / `break_duration_minutes` → km interval via planned ETA). HUD minutes-to-break uses truck settings when Truck is selected. |
| Split break 15 + 30 min | Art. 7 | **Implemented** as selectable alternative (`prefer_split_break`). Spacing still follows 4.5 h driving; duration metadata is 15+30 instead of 45. |
| Daily driving 9 h | Art. 6(1) | **Enforced** as the default daily cap in duty evaluation on plan (`max_daily_driving_hours`). |
| Daily driving 10 h twice/week | Art. 6(1) | **Enforced** via `TruckDrivingHistory.extensions_used_this_week` (max `max_daily_extensions_per_week` = 2). |
| Weekly driving 56 h | Art. 6(2) | **Enforced** against persisted day history (`max_weekly_driving_hours`). |
| Fortnightly 90 h | Art. 6(3) | **Enforced** against rolling history in `app_config` key `truck_driving_history` (`max_fortnightly_driving_hours`). |
| Exceptional +1 h to reach a suitable stop | Art. 12 (as amended) | **Enforced** only when the user explicitly arms it (`exceptional_extension_armed` / Drive settings toggle). Disarmed after use on commit. |
| Daily rest 11 h / reduced 9 h ≤3× / split 3+9 | Art. 8 | **Implemented** in multi-day segmentation (`plan_truck_multi_day`): trips that exceed the remaining daily driving budget are split into days with an overnight daily rest (regular 11 h by default; reduced 9 h when preferred and slots remain; split 3+9 when `prefer_split_daily_rest`). Reduced-rest count is stored on `TruckDrivingHistory.reduced_daily_rests_since_weekly`. |
| Weekly rest 45 h / reduced 24 h, ≤6 consecutive working days, no in-cab for regular 45 h | Art. 8 | **Implemented** in multi-day segmentation: after `max_consecutive_working_days` (6) working days, the overnight is a weekly rest (45 h regular with `not_in_cab=true`, or 24 h reduced when the previous weekly rest was regular and no unpaid compensation debt remains). |
| Compensation after reduced weekly rest | Art. 8 | **Implemented** as a tracked ledger on `TruckDrivingHistory.weekly_rest_compensations`: each reduced weekly rest records shortfall hours (typically 21) and a deadline (end of the third ISO week following the week of the reduction). Plan report surfaces outstanding debts (`truck_compensation:`). A later rest of ≥ 9 h that is long enough to carry the shortfall en bloc (or a full 45 h regular weekly rest) marks the oldest debt repaid on commit. **Planning auto-factoring:** when unpaid debt exists, the planner prefers a regular 45 h weekly rest over stacking another reduced weekly rest; it does **not** yet invent extra mid-week compensation blocks solely to repay debt. |

## Multi-day segmentation (implemented)

When a Truck / TruckElectric plan’s total driving time does not fit in the
remaining capacity of the start calendar day (9 h, or 10 h when a daily
extension is still available), `plan_car_route` runs `plan_truck_multi_day`:

1. Split the corridor into day segments, each within the single-day driving cap
   (including extension tracking against history).
2. Insert a daily or weekly overnight between days (see table above).
3. Match a **RestArea** POI near the day-boundary kilometre when one exists
   (tag rules: [`poi.md`](poi.md) **RestArea** — `highway=rest_area` OR
   `highway=services` OR `amenity=parking` + HGV access tags). Candidates are
   scored by **detour distance** from the corridor sample plus **facility tier**
   (`highway=services` preferred over bare `highway=rest_area` / HGV parking
   within ~8 km detour slack). Missing POIs do **not** hard-fail the plan
   (informational notes only).
4. Commit each driving day into `TruckDrivingHistory` (same store as
   single-trip duty), updating consecutive working days, reduced-daily count,
   last weekly rest kind, and the reduced-weekly **compensation ledger** as
   applicable.

Report lines: `truck_multi_day:`, `truck_day:`, `truck_overnight:`,
`truck_overnight_note:`, `truck_compensation:` / `truck_compensation_summary:`.

## Wiring notes

- Persistence: `RestConfig.truck` + `truck_driving_history` via `ConfigStore` / `navi.db`.
- FFI: `loadTruckRestSettings` / `saveTruckRestSettings` / `setTruckExceptionalExtensionArmed`.
- Planner: `plan_car_route` for Truck / TruckElectric loads truck rest beside the
  graph cache, uses `motor_break_interval_km` (truck path), evaluates duty caps,
  runs multi-day segmentation when needed, and commits into `TruckDrivingHistory`.
- HUD: `usesTruckRestSettings(profile)` is true only for Truck / TruckElectric.
- History prune: day rows older than ~21 days are dropped from the persisted blob;
  evaluation sums only a rolling 7-/14-day calendar window. Compensation ledger
  entries are retained independently of day-row prune (serde field on the same blob).

## Deferred / incomplete (stated accurately)

- **Dedicated mid-trip compensation rest blocks** invented solely to repay ledger
  debt (beyond preferring regular weekly rest when debt is outstanding) — not
  implemented; ledger track + surface + repay-on-sufficient-rest is the delivered
  scope.

Multi-day day cards and overnight map pins are delivered via
`CorridorRouteResult.days_json` / `break_pois_json` and Android
`MultiDayPlanCards` (search chrome). Multi-jurisdiction packs (EC 561 vs FMCSA
vs decline-unknown) are wired — see [`fmcsa-truck-rest.md`](fmcsa-truck-rest.md)
and [`jurisdiction-rules.md`](jurisdiction-rules.md).
