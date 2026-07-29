# FMCSA truck Hours of Service (Navi coverage)

Navi’s **Truck** / **TruckElectric** profiles resolve a jurisdiction driving-hours
pack at the corridor start (`resolve_driving_hours_pack_at`). When the start
falls in the United States (offline ISO ring via `country_iso_at`), the planner
uses **US FMCSA** property-carrying Hours of Service (49 CFR 395.3) instead of
EU EC 561/2006.

Official summary:
[FMCSA Hours of Service](https://www.fmcsa.dot.gov/regulations/hours-service/summary-hours-service-regulations).

Related: EC 561 coverage — [`ec-561-truck-rest.md`](ec-561-truck-rest.md);
jurisdiction pack pattern — [`jurisdiction-rules.md`](jurisdiction-rules.md);
truck overnight POI tags — [`poi.md`](poi.md) (**RestArea**).

## Pack selection

| Pack | When | Planner |
|---|---|---|
| `ec561` | Start ISO in EC 561 family (EU rings present + NO/IS/LI/CH). **Not** GB — see below | Existing `TruckRestParams` / `plan_truck_multi_day` |
| `fmcsa` | Start ISO `us` | `FmcsaHosParams` / `plan_fmcsa_multi_day` |
| `unknown` | Unmatched coordinates, or detected ISO with no pack (including **`gb`**) | **Decline** commercial legal tracking — report clearly; **no** duty history commit |

**United Kingdom (`gb`):** UK applies *assimilated* EC 561 with national
derogations and a distinct AETR split for some journeys
([GOV.UK guidance](https://www.gov.uk/guidance/drivers-hours-goods-vehicles/1-assimilated-and-aetr-rules-on-drivers-hours)).
Navi does **not** silently apply the EU EC 561 pack; resolve → `unknown` until a
dedicated UK pack exists.

Report line: `hos_pack=ec561|fmcsa|unknown`.

## Property-carrying defaults (`FmcsaHosParams`)

| Rule | Value | Navi status |
|---|---|---|
| Max driving after 10 h off duty | 11 h | **Enforced** as day budget in `plan_fmcsa_multi_day` |
| On-duty window | 14 h | **Informational** in route planning (driving-hours budget used; full on-duty needs ELD / host status) |
| Min consecutive off duty | 10 h | **Enforced** as overnight between multi-day segments |
| Break after driving | 8 h → 30 min | **Enforced** for break spacing (`break_interval_km` from 8 h × planned speed) |
| 70 h / 8-day cycle | 70 h rolling | **Evaluated** on plan (`evaluate_fmcsa_trip`); restart 34 h inserted when multi-day would exceed cycle |
| 34 h restart | Optional cycle reset | **Implemented** as weekly-style overnight when cycle would otherwise be exceeded |

## Multi-day segmentation

When total driving exceeds remaining 11 h capacity on the start day,
`plan_car_route` runs `plan_fmcsa_multi_day`:

1. Split into day segments within the 11 h driving cap.
2. Insert 10 h off-duty overnight between days (RestArea / services POI match when available).
3. Prefer a 34 h restart overnight when the rolling 70 h / 8-day cycle would be exceeded.
4. Emit structured day cards on `CorridorRouteResult.days_json` and merge overnight pins into `break_pois_json`.

Duty evaluation notes are report-only for now (no separate FMCSA history commit blob).

## Wiring notes

- Params: `core/src/config/fmcsa_params.rs` (`FmcsaHosParams`).
- Pack enum: `JurisdictionDrivingHoursPack::{Ec561, Fmcsa, Unknown}`.
- Resolution: `resolve_driving_hours_pack_at` (offline ISO rings via
  `country_iso_at` / `core/src/routing/elevation/country_polys.rs`).
- Planner: `plan_fmcsa_multi_day` / `evaluate_fmcsa_trip` in `core/src/routing/rest/fmcsa_multi_day.rs`.
- FFI: `plan_car_route` branches on `hos_pack`; Android hosts render `daysJson` via `MultiDayPlanCards`.

## Not legal advice

Figures follow the FMCSA summary for property-carrying CMVs. Passenger-carrying
rules, sleeper-berth exceptions, and ELD clock detail are out of scope for this
route-planning pack. Always verify against current regulation and your ELD.
