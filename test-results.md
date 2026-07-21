# Route integration test results

Date: 2026-07-21

| Test | Status | Duration |
|---|---|---|
| `kongsvinger_lillehammer_integration` (Car) | **PASS** | 270.22 s |
| `dnt_hiking_integration` (Hiking / DNT) | **PASS** | 112.60 s |

Commands:

```bash
cargo test --test kongsvinger_lillehammer_integration -- --nocapture --ignored
cargo test --test dnt_hiking_integration -- --nocapture --ignored
```

---

# Car: Kongsvinger -> Lillehammer

## Setup
Fixture dir: `/mnt/2e9a1e9f-2097-408c-ab9a-a01b32f11d28/github-projects/Navi/core/target/integration-fixtures`
Route: (60.5621914, 11.2561239) -> (61.8512500, 10.2338420)
Vehicle: VW Passat B8 diesel — Cd=0.28, A=2.2 m², mass=1500 kg
Fuel baseline (adaptive-learning seed only, NOT eco physics): 6.67 L/100km
Eco-mode uses physics model (Cd, A, mass, delta_h) — independent of the 6.67 L/100km figure.
Routing graph: ostlandet (1477831 nodes, 1263535 edges)
Start snap: node 7858041438 at 30 m; goal snap: node 8310601158 at 10 m

## Test 1 — With elevation awareness
Edges on route: 788, eco_weight Some: 788 (100.0%)
Elevation-aware:
- Distance: 190.73 km
- Total climb: 2011 m
- Total descent: 1476 m
- Energy (physics): 94938589 J
- Path cost (router): 100836869

## Test 2 — Without elevation awareness
Flat-weight:
- Distance: 190.58 km
- Total climb: 2012 m
- Total descent: 1476 m
- Energy (physics): 0 J
- Path cost (router): 190576

Path identical: false (distance delta 155.4 m, cost delta 100646293)
Energy cost (physics eco sum): 94938589 J vs flat base_weight sum 190576 J
Estimated fuel at 6.67 L/100km baseline: 12.72 L for 190.7 km
Estimated duration at 90 km/h: 2.12 h (127 min)

## Test 3 — POI awareness
POI index size: 2358 records
POIs found along corridor: 419

## Rest-stop parameter sanity (Car defaults)
Car break interval: 4-4.5 h, driving time 2.12 h -> 0 required breaks

## Car summary
- Test 1 distance: 190.7 km, climb 2011 m, descent 1476 m, energy 94938589 J
- Test 2 distance: 190.6 km, flat cost 190576
- Paths differ: true
- POI hits: 419
- Elapsed: 268.6 s

---

# Hiking: Aakersaetra -> Jammerdalsbu -> Rondvassbu (DNT)

## Setup
Route: (61.1553669, 10.9174631) -> (61.5857799, 10.3536473) -> (61.8787483, 9.7963376)
Profile: Hiking (foot), eco-mode on (locked default)
Path preference: DNT network soft penalty on non-DNT foot edges
RestConfig hiking: main=11.295 km, alt=2.275 km, max daily=40.0 km
SafetyConfig: cabin radius=5000 m, network hut radius=25000 m, hut preference=11000 m
POI index (oppland+hedmark): 2358 records
Edge tag map: 1796955 tagged edges, 19294 DNT relation ways
Routing graph (ostlandet, foot): 1477831 nodes, 3499770 edges
Start Aakersaetra snap: 5 m; Via Jammerdalsbu snap: 11 m; End Rondvassbu snap: 11 m
Total route: 153.0 km, climb 2749 m, descent 2245 m

## 1. DNT coverage and route validation
Total: 153.0 km | DNT-tagged: 140.0 km (91.5%) | other priority footpaths: 11.9 km (7.8%) | overall priority-path: 99.3%
DNT summary: 140.0 km of 153.0 km on DNT network, 91.5%

## 2. Jammerdalsbu POI resolution
POI id=845742951 name=Some("Jammerdalsbu") categories=[Cabin, NetworkHut, OvernightFacility] 0 m from via coords

## 3. Day-by-day plan
| Day | Start km | End km | Distance km | Rest stops | Overnight | Hut dist m |
|-----|----------|--------|-------------|------------|-----------|------------|
| 1 | 0.0 | 26.1 | 26.1 | 2 | ? | 1135 |
| 2 | 26.1 | 65.8 | 39.7 | 3 | Vetåbua | 481 |
| 3 | 65.8 | 104.2 | 38.5 | 3 | Veslefjellbua [network] | 3835 |
| 4 | 104.2 | 142.2 | 38.0 | 3 | Rondvassbu [network] | 11 |

Day details:
- Day 1 start (61.15541, 10.91744) -> end (61.20912, 10.64421); rest @ 11.29 km (main), 22.59 km (main)
- Day 2 start (61.20912, 10.64421) -> end (61.46577, 10.50974); rest @ 37.39 / 48.69 / 59.98 km (alt fallbacks)
- Day 3 start (61.46577, 10.50974) -> end (61.70155, 10.16210); rest @ 77.06 km (alt), 88.35 / 99.65 km (main)
- Day 4 start (61.70155, 10.16210) -> end (61.87870, 9.79652); rest @ 108.79 / 126.83 / 138.13 km (alt)

## 4. Overnight candidates
- Day 1: unnamed hut id=9969928992 network=false dist=1135 m
- Day 2: Vetåbua id=845742891 network=false dist=481 m
- Day 3: Veslefjellbua id=7627263964 network=true dist=3835 m
- Day 4: Rondvassbu id=291057219 network=true dist=11 m

## 5. Water POIs along corridor
Water POIs found: 5
Note: natural/untreated sources should be treated before drinking (informational).
- id=2760231771 (61.13526, 10.88475)
- id=1385170158 (61.14156, 10.87820)
- id=1347332195 (61.14516, 10.87546)
- id=853815905 (61.16841, 10.70970)
- id=853903045 (61.20561, 10.66224)

## 6. Flags
- Day segments: 4
- Overnight gap days: 0
- Alternative rest fallbacks: 7
- Forbidden segments: 0
- Low priority-path warnings: 0

## Hiking summary
- Total distance: 153.0 km across 4 days
- DNT coverage: 140.0/153.0 km (91.5%)
- Water POIs: 5
- Jammerdalsbu POI matches: 1
- Elapsed: 110.1 s
