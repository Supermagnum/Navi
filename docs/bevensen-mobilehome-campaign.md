# Bad Bevensen → Dalsøren MobileHome campaign

Date: **2026-09-30**. Branch: **`right-to-roam`**. Evidence SHA: **`9997e310`**
(westbound densify mid fix; corridor-leaf bias `be38eb11` was tried and
**reverted** as `e89b9d64` after Øresund `disconnected`).

**Canonical role:** one-shot campaign evidence for the long-trip MobileHome path
on the fixed Automotive AVD (`Navi_8c_4G_128G`). Real region packs on removable
SD; only DATEX closures are synthetic. Not wired into CI.

Instrumented runners:

- `LongTripMobileHomeBevensenDalsorenUiCampaignTest` — **primary** (this pass):
  plan via visible MainActivity Compose UI (From / Via / To / settings / Plan).
  Single natural via at Ottadal corridor; **no** forced corridor vias, land-bridge
  densify hops, ferries, or road segments.
- `LongTripMobileHomeBevensenDalsorenCampaignTest` — earlier FFI assist path with
  multi-via corridor (superseded for the natural-via requirement).

Build note: densify water-mid rejection is **geography-agnostic** (multi-country
spill by Ready **leaf** cover; foreign country AABB spill ignored when a leaf
covers the point). Overshoot-via skip remains general (imminent explicit via).
Westbound fjord approaches keep an even Chebyshev chord mid
(`prefer_north_then_east_mid` is northbound-only). Pack load pulls foreign Ready
leaves when a hop endpoint sits in them, and tile budget retention does not treat
country AABB spill as endpoint coverage.

---

## Verdict (UI natural-via run) — OUT OF EXPECTED BAND

| Metric | Result | EXPECTED |
|---|---|---|
| Distance | **1660.9 km** | 1461.3–1648.6 km |
| Driving time | **~21.4 h** | 17–~22 h |
| Instructions | **305** | 55–100 |
| Plan | **found** (`ui_planned=true`, 14 chunk legs PASS) | found |

**No release bump / no CI gate** for this pass (distance ~12 km over band upper;
maneuvers still far above). Duration is inside EXPECTED.

User fair counterexample (same via, tolls off, no wild camping): **1514.3 km /
~18.8 h**. Campaign excess vs user ≈ **+147 km / +2.6 h** after the mid fix
(was +254 km / +4.5 h at 1768.6 km before `9997e310`).

---

## Overshoot ranking (post-`9997e310`)

| Rank | Driver | Evidence | Status |
|---|---|---|---|
| 1 | Westbound NE-climb densify mid on Ottadal→Dalsøren | Campaign leg14 was **98.9 km / 18.3 km GC (5.41×)** between half-step mid ~8.78°E and next joint; `prefer_north_then_east_mid` applied 20%-of-dlon climb on negative `dlat`. Via→dest routed **238 → 131 km** after fix. | **Fixed** (`9997e310`) |
| 2 | Skåne raw AABB center (~13.53°E) east zigzag then Halland | Densify still anchors Skåne center then Halland 12.7°E; ~30–50 km corridor excess vs Öresund/E6-west. Corridor-centroid bias saved geometry on paper but **reintroduced Øresund `disconnected`** (`be38eb11` → reverted). | Open (no safe general bias yet) |
| 3 | Synthetic DATEX Blocks on hop midpoints | 2 legs with `datex_impacts=1` this pass (was 3 before mid-hop reshuffle); expected detour cost, not a planner bug. | Expected settings cost |
| 4 | MobileHome/Truck + VW T6 limits vs user path | Soft rests / wild camping do **not** change path distance (`poi_skipped=chunk_leg`; break POIs are fuel only). MH graph costs can still lengthen mountain/clearance legs vs a lighter profile. | Expected product cost |
| 5 | High-ratio early densify legs (e.g. leg3 ~1.70) | SH→Zealand land-bridge under chunk pad; leg2 still needs `trip_aabb` after corridor-band `disconnected`. | Residual densify/load cost |

---

## Test setup (2026-09-30 UI run)

| Item | Value |
|---|---|
| Emulator | `Navi_8c_4G_128G` (AVD; `adb` `emulator-5554`) — as-is |
| CPU / RAM / storage | 8 cores / 4 GB / 128 GB internal |
| SD card | ~64 GiB removable (`uuid:0000-0000`, label SDCARD) |
| Origin | Bad Bevensen Kurpark Stellplatz ≈ 53.079686, 10.587198 |
| Via (single, natural) | **61.8691419, 9.1055130** (Ottadal corridor) |
| Destination | Dalsøren Camping ≈ 61.4433766, 7.4614016 |
| Departure | `2026-06-01T08:00:00` local |
| Eco | requested on (Compose switch; see gap below) |
| Avoid toll roads | off |
| Ferries | use (allow if natural — not forced); route_uses_ferry=false |
| Soft daily budget | 6.0 h |
| Soft break spacing | 1.5 h interval, 15 min rest |
| Wild camping / long trip / DATEX / nearby attractions | on |
| Profile | MobileHome / Truck routing + VW T6 camper limits |
| Plan path | Compose `btn_plan_route` (not FFI-only) |

Packs download to
`/storage/0000-0000/Android/data/no.navi.app/files/long-trip-packs`
(corridor already Indexed/Installed from prior campaign downloads).

### Vehicle — VW Transporter T6 2.0 BiTDi 4Motion camper

| Spec | Value |
|---|---|
| Length | 5.304 m |
| Width (incl. mirrors) | 2.297 m |
| Body height | 2.477 m |
| Loaded total weight | 3020.4 kg |
| Loaded rear axle | 1661.2 kg |
| Fuel tank | 70 L (`FuelConfig` HUD only) |

---

## Corridor and packs (real downloads)

Regions in route order (status at plan time):

1. `europe/germany/niedersachsen` — Indexed
2. `europe/germany/schleswig-holstein` — Indexed
3. `europe/denmark` — Indexed
4. `europe/sweden/skane` — Indexed
5. `europe/sweden/halland` — Indexed
6. `europe/sweden/vastra_gotaland` — Installed
7. `europe/norway/ostlandet` — Indexed
8. `europe/norway/vestlandet` — Installed

| Metric | Result |
|---|---|
| Pack on removable | **true** |
| Download elapsed | cached (corridor ready immediately) |
| Corridor ready | **true** |
| Planner densify hops | **14** (`chunk_deg=1.15`) |
| Plan elapsed | **~same order as prior (~1 min plan)** |

---

## Plan metrics (full route)

| Metric | Result |
|---|---|
| Distance | **1660.9 km** (~12 km above EXPECTED upper) |
| ETA | **1282.8 min (~21.4 h)** (inside EXPECTED) |
| Maneuvers | **305** (above EXPECTED) |
| Ferries used | **false** |
| Via→dest densify | single even mid ≈ `(61.656, 8.283)` (was half-step `8.777` + micro-hop) |
| Fuel-stop estimate | report-only; planning unimplemented |

### Eco gap (UI)

Assist seed + drive-settings save set `ecoModeEnabled=true`, but chunk leg
reports still show `use_eco=false` when the Compose Eco switch is not toggled
in search-chip mode. No app feature code was changed for the run.

---

## Synthetic DATEX (only synthetic inputs)

Six Block situations seeded under `{dataDir}/datex_cache` with
`apply_to_routing=1` on densify hop midpoints (DE/DK/SE×2/NO×2):

| ID | Country | Lat/Lon | Label |
|---|---|---|---|
| syn-de-a7 | DE | 53.64484, 10.21610 | A7/E45 Lower Saxony |
| syn-dk-e45 | DK | 55.23188, 10.91563 | E45 Jutland |
| syn-se-e6 | SE | 56.59437, 12.34313 | Halland spine |
| syn-se-gbg | SE | 59.25687, 11.49000 | toward Ostlandet |
| syn-no-e6 | NO | 60.95479, 10.36055 | Ostlandet |
| syn-no-otta | NO | 61.76270, 8.94110 | Ottadal approach |

**DATEX proof:** two chunk legs reported `datex_impacts=1; datex_block=1` on the
`9997e310` pass (hop midpoints shifted after the via→dest mid fix). Campaign
helper `datex_ok` only requires any positive impact. Target floor 3–6 is soft.

---

## How to re-run (not CI)

```bash
./scripts/build-android-native.sh x86_64-linux-android release
./gradlew :app:connectedDebugAndroidTest \
  -Pandroid.testInstrumentationRunnerArguments.class=no.navi.app.LongTripMobileHomeBevensenDalsorenUiCampaignTest
```

Evidence JSON: app
`files/long-trip-bevensen-dalsoren/report.json` (mirrored under SD app files).

---

## Prior notes (not this pass)

- **2026-09-30 UI after densify spill fixes (`24f4c1c2`)**: **1768.6 km / ~23.3 h /
  317 maneuvers** — plan succeeded; Vestlandet west-loop gone; remaining +254 km
  vs user dominated by westbound mid bug + Skåne zigzag.
- **2026-09-30 corridor-centroid bias (`be38eb11`)**: densify pulled Skåne/Halland
  west; plan failed `chunk_leg6` Øresund-class `disconnected` / `bbox_exhausted`
  (19 hops). Reverted (`e89b9d64`).
- **2026-09-30 earlier UI attempts** failed on Baltic water mids / Skåne
  `snap_failed` / Øresund `disconnected` before densify+load fixes.
- **2026-09-30 FFI multi-via** (`Landskrona → Ängelholm → Gothenburg → Sognefjell`):
  completed at **2035.8 km / ~28.4 h / 380 maneuvers** — outside EXPECTED band;
  used forced corridor vias (superseded by natural Ottadal via requirement).
- **2026-09-24** dest = Sognefjell itself (ferries avoided): **1648.6 km / ~21.78 h**
  on `dev` — upper edge of today’s EXPECTED distance band.

---

## Known gaps

- Distance still ~12 km above EXPECTED upper and ~147 km above the user 1514 km
  fair path; primary remaining planner-shaped cost is Skåne AABB-center densify
  (safe corridor bias not yet found).
- Maneuver count remains ~3× EXPECTED (long-trip chunk legs + mountain guidance
  splits); not addressed by the mid fix.
- Eco Compose switch not reliably toggled from the UI campaign helper when the
  route sheet is in search-chip mode.
- Fuel-stop planning unimplemented (`FuelConfig` HUD only).
