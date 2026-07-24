# EC 561/2006 truck rest (Navi coverage)

Navi’s **Truck** and **TruckElectric** profiles use `TruckRestParams`
(defaults in `core/src/config/defaults.rs`) for EU Regulation EC 561/2006
driving-time and break rules. This page states what is **enforced**, what is
**tracked / informational**, and what is **deferred**.

Official summary:
[EU driving time and rest periods](https://transport.ec.europa.eu/transport-modes/road/social-provisions/driving-time-and-rest-periods_en).

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
| EC 561 daily / weekly / fortnightly caps | **Not applied** — not a legal compliance tracker for leisure motorhomes |

A private motorhome driver is generally **not** bound by commercial HGV
driving-hours regulation the way a truck driver is. Showing EC 561 “duty”
language for MobileHome would imply a legal obligation the app is not entitled
to assert. Soft car-style rest reminders remain available as **wellbeing /
fatigue suggestions** only.

| Rule | Regulation | Navi status (Truck / TruckElectric only) |
|---|---|---|
| Break after 4.5 h driving (45 min continuous) | Art. 7 | **Enforced** in motor route break spacing (`mandatory_break_after_hours` / `break_duration_minutes` → km interval via planned ETA). HUD minutes-to-break uses truck settings when Truck is selected. |
| Split break 15 + 30 min | Art. 7 | **Implemented** as selectable alternative (`prefer_split_break`). Spacing still follows 4.5 h driving; duration metadata is 15+30 instead of 45. |
| Daily driving 9 h | Art. 6(1) | **Enforced** as the default daily cap in duty evaluation on plan (`max_daily_driving_hours`). |
| Daily driving 10 h twice/week | Art. 6(1) | **Enforced** via `TruckDrivingHistory.extensions_used_this_week` (max `max_daily_extensions_per_week` = 2). |
| Weekly driving 56 h | Art. 6(2) | **Enforced** against persisted day history (`max_weekly_driving_hours`). |
| Fortnightly 90 h | Art. 6(3) | **Enforced** against rolling history in `app_config` key `truck_driving_history` (`max_fortnightly_driving_hours`). |
| Exceptional +1 h to reach a suitable stop | Art. 12 (as amended) | **Enforced** only when the user explicitly arms it (`exceptional_extension_armed` / Drive settings toggle). Disarmed after use on commit. |
| Daily rest 11 h / reduced 9 h ≤3× / split 3+9 | Art. 8 | **Tracked / informational** on plan reports (`daily_rest_*` fields). Hard multi-day segmentation for truck is **deferred** (no multi-day truck trip planner yet; hiking already segments by day). |
| Weekly rest 45 h / reduced 24 h, ≤6 consecutive working days, no in-cab for regular 45 h | Art. 8 | **Tracked / informational** (`weekly_rest_*`, `max_consecutive_working_days`, `regular_weekly_rest_not_in_cab`, `weekly_rest_due` flag). Multi-day truck overnight / rest-stop planning is **deferred** for the same reason. |

## Wiring notes

- Persistence: `RestConfig.truck` + `truck_driving_history` via `ConfigStore` / `navi.db`.
- FFI: `loadTruckRestSettings` / `saveTruckRestSettings` / `setTruckExceptionalExtensionArmed`.
- Planner: `plan_car_route` for Truck / TruckElectric loads truck rest from the app data directory beside the graph cache, uses `motor_break_interval_km` (truck path), evaluates duty caps, and **commits** driving hours into `TruckDrivingHistory` so weekly / fortnightly caps accumulate across plans.
- HUD: `usesTruckRestSettings(profile)` is true only for Truck / TruckElectric.
- History prune: day rows older than ~21 days are dropped from the persisted blob; evaluation sums only a rolling 7-/14-day calendar window.

## Deferred (stated reason)

Multi-day truck trip modeling (day splits, overnight rest placement, in-cab accommodation checks) is not implemented. Until it exists, daily/weekly rest rules are stored with regulation-correct defaults and written into plan `truck_duty_note` lines so hosts can display them, but they do not reshape a single-leg A* path.
