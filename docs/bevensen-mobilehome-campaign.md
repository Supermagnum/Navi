# Bad Bevensen → Dalsøren MobileHome campaign

Date: **2026-09-30**. Branch: **`right-to-roam`**.

**Canonical role:** one-shot campaign evidence for the long-trip MobileHome path
on the fixed Automotive AVD (`Navi_8c_4G_128G`). Real region packs on removable
SD; only DATEX closures are synthetic. Not wired into CI.

Instrumented runner:

- `LongTripMobileHomeBevensenDalsorenCampaignTest` — Bevensen → Dalsøren via
  Sognefjell corridor vias (Landskrona, Ängelholm, Gothenburg, Sognefjell).

Related (older dest = Sognefjell itself, ferries avoided):

- `LongTripMobileHomeBevensenLiveTest`
- `LongTripMobileHomeBevensenResumePlanTest` (2026-09-24 corrected retest)

---

## Test setup (2026-09-30 run)

| Item | Value |
|---|---|
| Emulator | `Navi_8c_4G_128G` (AVD name; `adb` `emulator-5554`) |
| CPU | 8 cores |
| Internal storage | 128 GB |
| RAM | 4 GB |
| SD card | ~64 GiB removable (`uuid:0000-0000`, label SDCARD) |
| Origin | Bad Bevensen Kurpark Stellplatz ≈ 53.079686, 10.587198 |
| Via (named) | Sognefjellsvegen ≈ 61.6170857, 8.0438639 |
| Destination | Dalsøren Camping ≈ 61.4433766, 7.4614016 |
| Plan vias (progress_t-safe) | Landskrona; Ängelholm; Gothenburg; Sognefjell |
| Departure | `2026-06-01T08:00:00` local |
| Eco | on |
| Tolls | `FfiTollPolicy.PENALIZE` |
| Ferries | use (`avoidFerries=false`) |
| Soft daily budget | 6.0 h |
| Soft break spacing | 1.5 h interval, 15 min rest |
| Profile | MobileHome / Truck routing + vehicle limits |

Packs download to the removable SD volume
(`/storage/0000-0000/Android/data/no.navi.app/files/long-trip-packs`); plan
`dataDir` / graph `cacheDir` stay under app internal data.

### Vehicle — VW Transporter T6 2.0 BiTDi 4Motion camper

| Spec | Value |
|---|---|
| Label | VW Transporter T6 2.0 BiTDi 4Motion camper |
| Length | 5.304 m |
| Width (incl. mirrors) | 2.297 m |
| Body height | 2.477 m |
| Loaded total weight | 3020.4 kg |
| Loaded rear axle | 1661.2 kg |
| Fuel tank | 70 L (`FuelConfig` HUD only) |

---

## Corridor and packs (real downloads)

Regions in route order (all **Indexed** / **Installed** on SD before plan):

1. `europe/germany/niedersachsen`
2. `europe/germany/schleswig-holstein`
3. `europe/denmark`
4. `europe/sweden/skane`
5. `europe/sweden/halland`
6. `europe/sweden/vastra_gotaland`
7. `europe/norway/ostlandet`
8. `europe/norway/vestlandet`

| Metric | Result |
|---|---|
| Pack on removable | **true** |
| Download elapsed (cached re-run) | **25 ms** |
| Corridor ready | **true** |

---

## Plan result (campaign9, BUILD SUCCESSFUL)

| Metric | Result |
|---|---|
| Search terminate | **found** |
| Distance | **2035.8 km** |
| Driving time | **~28.39 h** (1703.4 min) |
| Chunk hops | **19** (all PASS) |
| Maneuvers | **380** |
| Break POIs | **17** |
| Soft multi-day days | **5** (4× ~6 h + final ~4.39 h) |
| Ferries used | **false** (`route_uses_ferry=false` on all hops) |
| Plan elapsed | **403.2 s** |
| RAM before / post-plan / final | **112 / 341 / 559 MiB** PSS |

### Soft day splits

| Day | Distance | Driving hours | Overnight |
|---|---|---|---|
| 1 | 430.2 km | 6.0 h | (none named) |
| 2 | 430.2 km | 6.0 h | Hotell Halland |
| 3 | 430.2 km | 6.0 h | Lygnasæter hotell |
| 4 | 430.2 km | 6.0 h | Brandseth Fjellstove |
| 5 (final) | 314.8 km | 4.39 h | — |

### Via note (densify)

Densify reorders vias by `progress_t` on the OD vector (NW), not list order.
Eastern Øresund endpoints (Malmö / Lernacken / Helsingborg-as-endpoint) sort
wrong or snap to a northbound-dead MH pier. Working corridor vias:

`Landskrona → Ängelholm → Gothenburg → Sognefjell`.

---

## Synthetic DATEX (only synthetic inputs)

Six Block situations seeded under `{dataDir}/datex_cache` with
`apply_to_routing=1` (DE, DK, SE×2, NO×2), inland of the E6 spine so a Block
still leaves a coastal alternate:

| ID | Country | Lat/Lon | Label |
|---|---|---|---|
| syn-de-a7 | DE | 53.25, 10.05 | A7/E45 Lower Saxony |
| syn-dk-e45 | DK | 55.40, 9.48 | E45 Jutland |
| syn-se-e6 | SE | 56.25, 13.15 | inland Skåne |
| syn-se-gbg | SE | 57.72, 12.15 | Gothenburg east |
| syn-no-e6 | NO | 60.80, 10.90 | Innlandet east |
| syn-no-fv55 | NO | 61.55, 7.95 | Fv55 spur |

**Observation:** every chunk leg reported `datex_impacts=0` /
`datex_block=0` / `datex_penalize=0`. Situations were on disk with a fresh
`fetched_unix`, but plan-time corridor apply did not attach impacts on this
run. Documented as a campaign gap (no app feature change in this pass).

---

## Fuel / attractions / wild camping

| Item | Result |
|---|---|
| Fuel-stop planning | **unimplemented** in `planCarRouteAt` |
| Report-only fuel estimate | 500–600 mi range + 100 km margin → **2** stops at either bound |
| Nearby attractions samples | load_ok; **2** hits (general) near origin samples |
| Wild camping (right-to-roam guest) | kind=OK; **0** accepted sites (2 probes rejected `too_close_to_building`) |

---

## How to re-run (not CI)

```bash
./gradlew :app:connectedDebugAndroidTest \
  -Pandroid.testInstrumentationRunnerArguments.class=no.navi.app.LongTripMobileHomeBevensenDalsorenCampaignTest
```

Evidence JSON: app
`files/long-trip-bevensen-dalsoren/report.json` (also mirrored under SD app
files when present).

---

## Prior corrected retest (2026-09-24, dest = Sognefjell)

| Metric | Result |
|---|---|
| Distance | **1648.6 km** |
| Driving time | **~21.78 h** |
| Days at 6 h/day | **4** |
| Branch then | **`dev`** |
| Ferries | avoid |

Bug fixes from that campaign (snap hang, graph-load OOM, trip-AABB fallback,
`max_hours` wiring) remain upstream of this Dalsøren run.

---

## Known gaps

- **Fuel-stop planning** unimplemented (`FuelConfig` HUD only).
- **DATEX plan-time impacts** were 0 despite synthetic seed (see above).
- Expected-band checks in the runner still quote the older Sognefjell bands;
  assert gate is `distanceKm > 100` + `found`.
