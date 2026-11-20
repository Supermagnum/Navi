# Bad Bevensen to Dalsøren MobileHome campaign

## Latest — 2026-11-20 (Ottadal via; PASS)

Emulator **emulator-5554** (`sdk_gphone64_x86_64`), guest `nproc` **6**. Navi only. Long trip ON. Regions downloaded/indexed by the app before the corridor route was used for GPS replay. ADB used only for GPS feed + synthetic DATEX soft **Penalize** (never hard Block). No force-stop mid-run after start.

### Route

**Bad Bevensen → Via Ottadal (61.86914, 9.10551) → Dalsøren Camping, Luster**

Proven in log: `WAYPOINTS_WITH_VIA From: 53.07969, 10.58720 | To: 61.44338, 7.46140 | Via: 61.86914, 9.10551`  
Plan log tunnel fingerprint includes **Ottadalsvegen**.

| # | Field | Result |
|---|---|---|
| 1 | Distance 1461.3–1648.6 km | **1553.273 km. PASS** |
| 2 | Duration 17–22 h | **1178.7 min = 19.645 h. PASS** |
| 3 | Instructions 55–100 | **69. PASS** |
| 4 | Via Ottadal | **Applied** (`chip_via` / `via_ottadal`) |
| 5 | Germany MWM | **Alias path** for Niedersachsen + Schleswig-Holstein (not Geofabrik PBF) |
| 6 | DATEX | Soft Penalize; plan `datex_impacts=5; datex_block=0; datex_penalize=5` |
| 7 | Ferries | 1 leg (`Sydmotorvejen@19.17`, Rødby–Puttgarden corridor) |
| 8 | Departure / vehicle | 2026-06-01T08:00:00 local. VW Transporter T6 MobileHome limits; eco ON; soft daily 6.0 h |
| 9 | CAT / Dummy radio | Full DE+DK+SE+NO corridor list: **341** repeaters (callsign + name) |
| 10 | Town screenshots | Bad_Bevensen, Puttgarden, Rodby, Copenhagen, Helsingborg, Halmstad, Gothenburg, Oslo, Lillehammer, Dale_Luster |

Artifacts: `docs/evidence/bevensen-mobilehome-via-ottadal-20261120/`  
Repeater list: `cat_repeaters_DE_DK_SE_NO.json` / `.txt` (country, callsign, name).

### KML comparison vs `geojson-routes/bad-luster.kml`

| Metric | Value |
|---|---|
| Navi corridor points | 30131 |
| Navi length | ~1551.6 km (polyline haversine) / 1553.273 km (plan) |
| KML points parsed | 13682 |
| KML length (parsed) | inflated (~3469 km; multi-geom / parser note — not used as pass band) |
| Notable divergence | ~199.6 km leg near Ottadal via (start ~1298 km → end ~1498 km along Navi) — expected with via |

Detail JSON: `docs/evidence/bevensen-mobilehome-via-ottadal-20261120/kml_compare.json`.

### Harness notes

Prior direct run without via measured **1342.87 km** (`Via: none`). This run required a fresh androidTest APK that calls `chip_via`, plus `skipComapsRouteRender` so organicmaps `GenerateJoinNormals` does not SIGABRT on the ~30k-point via corridor while GPS/CAT finish.

### Catalog / binary

App commit at campaign plan: see `git log` around `d4c124d64` / `9c3246c24` (via harness + skip-render). Version after release bump recorded in `app/build.gradle.kts`.
