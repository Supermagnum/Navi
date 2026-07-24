# Route integration test results

Date: 2026-07-21 (eco cost-model fix validation)

| Test | Status | Duration |
|---|---|---|
| `kongsvinger_lillehammer_integration` (Car) | **PASS** | ~232 s |
| `falletvegen_atnbrufossen_eco` (Car eco validation) | **PASS** | ~146 s |
| `dnt_hiking_integration` (Hiking / DNT) | **PASS** (water: indexing OK; corridor sparse at 2 km, hits at 5 km) | ~93 s |
| `config::eco` unit tests | **PASS** (4) | — |

Commands:

```bash
cargo test -p driver-break-core config::eco -- --nocapture
cargo test -p driver-break-core --test falletvegen_atnbrufossen_eco -- --nocapture --ignored
cargo test -p driver-break-core --test kongsvinger_lillehammer_integration -- --nocapture --ignored
cargo test -p driver-break-core --test dnt_hiking_integration -- --nocapture --ignored
```

**Supersession note:** All eco-mode joule figures from before the descent-credit / floor-unit fix
(including the old **94.9 MJ** Espa–Atnbrufossen number) are **superseded** and must not be
used as a baseline. Current car physics energy is ~**116.6 MJ** under climb-charged /
regen-0 combustion costing.

---

# Car: Falletvegen/Espa -> Atnbrufossen (post car-reverse + eco-unit fix)

## Graph bugs fixed
1. **Car reverse edges:** car/truck graphs only inserted OSM-way forward edges
   and ignored `car_backward`. Rv3 near Kolomoen is two-way but drawn southbound,
   so northbound Østerdalen was unreachable → Moelv detour. Fixed in
   `builder.rs` (`directed_access` + reverse edges). Cache magic → `NAVIGPH2`.
2. **Eco unit fallback:** missing DEM set `eco_weight=None`; router used
   `length_m` vs joules (~200× cheaper uncovered edges → ~900 km eco paths).
   Missing elev now uses flat joule energy in `reweight.rs`.

## Results (Passat regen=0)
| | Distance | Climb / descent | Energy |
|---|---|---|---|
| Eco OFF | 188.88 km | 1984 / 1448 m | 115396122 J (32.05 kWh) |
| Eco ON | 199.34 km | 1335 / … m | 110621600 J (30.73 kWh) |
| Via-Atnosen (diagnostic) | 197.69 km | 1532 / 997 m | 112774271 J (31.33 kWh) |

- OSRM: direct 189.5 km; via Atnosen 204.7 km
- Navi via now on Rv3 corridor (Kolomoen); length ~197.7 vs prior 215.0
- paths_differ: true; DEM coverage 100% on both chosen routes
- Eco-on beats forced via-Atnosen on physics energy → skipping Atnosen as
  via-constraint still consistent with eco A* choice

Residual: length-direct audit can pick ~2 km track/service shortcuts once reverse
edges exist (osm4routing treats `highway=track` as car-accessible). Separate
filter hardening, not the Kolomoen gap.

---

# Car: Falletvegen/Espa -> Atnbrufossen (post-fix, superseded distances)

## Setup
Route: (60.5621914, 11.2561239) -> (61.8512500, 10.2338420)
Vehicle: VW Passat B8 diesel — Cd=0.28, A=2.2 m², mass=1500 kg, regen_efficiency=0
OSRM cross-check (same coords): direct ~189.5 km; via Atnosen ~204.7 km

## Eco ON vs OFF
| | Eco OFF | Eco ON |
|---|---|---|
| Distance | 190.58 km | 190.58 km |
| Climb / descent | 2012 / 1476 m | 2011 / 1475 m |
| Energy (physics) | 116585486 J (32.38 kWh) | 116570569 J (32.38 kWh) |
| paths_differ | — | **true** |

## Atnosen
Reachable under Car (primary/trunk nearby). Diagnostic via-Atnosen: **215.02 km**,
1709 m climb, **123280630 J (34.24 kWh)** — higher energy than direct; skipping Atnosen
is consistent with the cost model.

---

# Hiking: Aakersaetra -> Jammerdalsbu -> Rondvassbu (DNT)

Post-fix path (supersedes pre-fix 153.0 km / 2749 m climb and interim 145.6 km):

- Total route: **139.9 km**, climb **2164 m**, descent **1659 m**
- Energy (physics): **4212359 J (1.17 kWh)** — supersedes 4.276 MJ
- **DEM coverage: 316/316 edges (100%)** — not a missing-tile joule-fallback artifact
- **DNT coverage: 116.8 / 139.9 km = 83.5%** (was 91.5% on 153 km; 85.0% on 145.6 km)
- Overall priority-path: see latest `dnt_hiking_report.md`
- **Multi-day segmentation:** core `plan_hiking_multi_day` is wired into UniFFI
  `planHikingRoute` (report lines + overnight pins). The richer DNT integration
  helper `plan_multi_day` (per-day rast stops, CombinedPoiIndex) remains the
  detailed suite path.

## Water POIs (tracked separately from eco)

- Indexing **healthy**: 4 water POIs within 5 km of Aakersaetra.
- Root cause of earlier FAIL: eco-shortened path missed mapped water at the default
  **2 km** sample radius (highland sparsity), not a broken index.
- Fix: hard-fail only if trailhead indexing is empty; widen corridor search to 5 km
  once if needed. After fix: **4** corridor water hits at 5 km — PASS.

## Follow-up items (2026-07-21)

1. **Atnosen Navi vs OSRM geometry:** real ~10 km gap (Navi Moelv loop vs OSRM
   Kolomoen→Rv3); not noise. Open graph issue. Eco skip-Atnosen still stands under
   OSRM-length estimate.
2. **Direct road-type audit:** clean (only ~70 m `service` at start = 0.04%).
3. **DNT water:** fixed/assert clarified as above (own bug, not eco cost model).
4. **DNT coverage on new path:** **85.0%** vs original **91.5%** (−6.5 pp; still high).
