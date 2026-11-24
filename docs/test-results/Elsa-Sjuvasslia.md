# Elsa's caravan & galleri (Bugøynes) → Sjuvasslia Camping

Date: **2026-11-23** (Europe/Oslo). Device: Samsung **SM-P613** serial
**`R52TB0JQEDE`** (arm64-v8a). App `no.navi.app` **0.5.1-beta**
(`versionCode` **36**). Branch for this report: **`tests`**. No app or
plugin code was changed for this run.

Instrumentation driver
`LongTripMobileHomeElsaSjuvassliaUiCampaignTest#elsa_sjuvasslia_ui_plan_campaign`
drove the real UI. Host/artifacts:
`/tmp/elsa-sjuvass-campaign/attempt_20261123_122404/`. Device report
preserved at ART
`report/device_report_final_pre_overwrite.json` (full post-plan pull) and
live copies under `files/long-trip-elsa-sjuvasslia/` /
`files/long-trip-ui-report/` at plan time.

GPS town crawl was **waived by the user** after Samsung position-change
beeps; GPS injection was stopped mid-replay and was **not** restarted for
this write-up. Town screenshots are **not** claimed as campaign
deliverables.

### GPS HARD RULE (start / planning only)

**GPS ONLY at start/planning.** Mock / test-provider location may be set once
to place the device at the origin for plan setup. **No** mid-route GPS
replay, town crawl, or repeated `set-test-provider-location` injects for
Navi campaigns on this device (each update beeps on SM-P613). Prefer
waiving the crawl over restarting injection.

---

## Verdict

| Metric | Result | EXPECTED | OK |
|---|---|---|---|
| Distance | **2038.523 km** | 1600–2300 km | yes |
| Driving time | **1587.3 min ≈ 26.46 h** | 15–31 h | yes |
| Instructions | **81** maneuvers | 1–170 | yes |
| Plan | **found** (`ui_planned=true`, `ELSA_PLAN_OK`) | found | yes |
| Rest places / soft breaks | **17** (`rest_place_count=17`, `break_poi_count=17`, chunked soft) | rests found | yes |
| Wild camping | **64** accepted on-foot (`kind=OK`); route-result enum also lists **80** by country | > 0 | yes |
| Attractions | **4** total — by type: `tourism-attraction` **3**, `tourism-viewpoint` **1** | several | yes |
| Ferries | **0** legs | reported | yes |
| Tunnels (app enum) | **5** (road-name fp; includes bridge label) | reported | yes |
| Tunnels (OSM along corridor) | **25** named/lokket on Østlandet (~21.1 km) | report required | yes (geojson) |
| Soft DATEX injects (timed replay) | **4** of 6 staged soft Penalize sits fired before abort | soft DATEX count | yes (partial crawl) |
| DATEX on finished plan report | `datex_impacts=0` (`block=0`, `penalize=0`); enrich `datex_lines=3`; XML parse warning | honest | see DATEX |
| GPS town crawl / town screenshots | **waived** (device noise) | not required | n/a |
| KML compare vs `geojson-routes/elsa-sjuvass.kml` | same endpoints; plan ~4.7% longer; coarse median offset ~1.2 km | overlap note | yes |

**Overall:** distance / duration / maneuver bands **PASS**. Rests, wild
camping, and attractions **PASS** on plan report. Soft DATEX timed injects
**4** recorded before GPS abort. Full geometric mid-drive auto-reroute
(`rerouted=true` / distance delta) was **not** captured — GPS crawl stopped
early. GPS town screenshots **waived**.

---

## Test setup

| Item | Value |
|---|---|
| Device | Samsung SM-P613 / `R52TB0JQEDE` |
| Origin | Elsa's caravan & galleri, Bugøynes **69.9741435, 29.6337571**, elev **4 m** |
| Destination | Sjuvasslia Camping **59.803175, 9.397871** |
| Via | none |
| Departure ISO (assist) | `2026-06-01T08:00:00` |
| Profile | Mobile home / car long-trip UI (`compose_ui_btn_plan_route`) |
| Eco | on (settings); plan log also shows `eco=false` on the kept MWM pass |
| Avoid toll / ferries | off / ferries=use |
| Soft daily budget | **6.0 h** (settings) |
| Soft break | settings **4.0 h** / 15 min; plan chunked soft used **1.50 h** / **115.6 km** |
| Wild camping | on |
| Long trip | on |
| DATEX overlay | on |
| Nearby attractions | on |
| Vehicle | VW Transporter T6 camper — H **2.477 m**, W **2.297 m**, L **5.304 m**, axle **1661.2 kg**, total **3020.4 kg**, tank **70 L** |
| Pack / maps | Internal storage. Corridor Ready for 8 leaves. `files/mwm` ~**5.2 GiB**; `files/long-trip-packs` present (report `pack_dir=.../long-trip-packs`) |

---

## Corridor (long trip)

All eight Ready at plan time:

1. europe/norway/nord-norge  
2. europe/finland  
3. europe/sweden/norrbotten  
4. europe/sweden/vasterbotten  
5. europe/sweden/vasternorrland  
6. europe/sweden/jamtland  
7. europe/sweden/dalarna  
8. europe/norway/ostlandet  

Placement: **internal** (no removable SD on this tablet).

---

## Plan results

Kept plan (`ELSA_PLAN_OK` 13:18:40):

- Distance **2038.523 km**
- ETA **1587.3 min** (**26.455 h**)
- Maneuvers **81** (kinds: roundabout 47, left 9, right 8, straight 7, slight_left 3, exit_left 2, slight_right 2, exit_right 2, destination 1)
- Polyline **1 298 787** chars / **35 426** points
- Build ~**90.7 s**; `comaps_route_code=OK`; `terminate=ok`
- Ascent **17757.5 m** / descent **17156.5 m**
- Vehicle limits: `vehicle_limits_ok=false`, partial note `Finland_Northern Finland`
- Ferries **0**; app tunnels **5** (`Perämerentie`, `Hamarvegen`, `Drammensbrua`, `E 134`, `Gomsrudveien`) — see **Tunnels** for OSM re-derivation

### Rest places (chunked soft)

`chunked_soft_breaks=true`; `break_interval_km=115.6`; `break_interval_h=1.50`;
`rest_place_count=17`; `chunked_rest_pauses=17`; `break_pois_total=17`.

Seventeen named “Rest stop” pins with coordinates along the corridor
(≈115.6 km spacing from ~115.6 km to ~1964.9 km).

### Wild camping

Report `wild_camping`: **kind=OK**, **64** on-foot accepted, **0** vehicle.
Route-result enumerations also report **80** sites by country
(`no` 17, `fi` 23, `se` 40). Both figures are > 0; bands used **64**.

### Attractions

Recovered from campaign ART
`report/device_report_final_pre_overwrite.json` / plan enumerations
(`attempt_20261123_122404`; host ART dir no longer on `/tmp`). Verify note
`internal/elsa-sjuvasslia-doc-verify-commit.md`: attractions **4**, by_type
**3+1**.

| Field | Value | Result |
|---|---|---|
| `attraction_count` | **4** | PASS (several) |
| By type | `tourism-attraction` **3**, `tourism-viewpoint` **1** | complete |
| Pref | Nearby attractions **on** | — |

#### By type

| Type | Count |
|---|---:|
| tourism-attraction | 3 |
| tourism-viewpoint | 1 |
| **Total** | **4** |

#### Named hits (complete set from campaign report)

All four hits documented on the kept plan (near-origin cone samples):

| # | Name | Type (inferred from by_type mix) |
|---|---|---|
| 1 | Elsa's caravan & galleri | tourism-attraction |
| 2 | Harald R. | tourism-attraction |
| 3 | Bugøynes plantelivsfredning | tourism-attraction |
| 4 | Utsiktspunkt | tourism-viewpoint |

Full per-hit `hits_snip` JSON was not preserved after ART `/tmp` eviction;
totals and per-type counts above are the complete enumerated numbers from the
pre-overwrite device report.

### Tunnels

| Source | Value |
|---|---|
| App / plan enum `route_tunnel_count` | **5** |
| App `route_tunnel_fp` labels | `Perämerentie`, `Hamarvegen`, `Drammensbrua`, `E 134`, `Gomsrudveien` |
| **OSM-along-corridor (post-ART)** | **25** named/lokket tunnels (~**21.1 km**) on Østlandet — table below |

**App fp caveat:** labels are road/bridge names from the MWM report path
(`Drammensbrua` is a bridge, not an OSM tunnel). Same class of MWM
instrumentation quirk as Bevensen (sibling owns tunnel-token fix). Do **not**
treat the five labels as a complete OSM tunnel inventory.

**Independent re-derivation (docs only, no app code change):** Campaign ART
polyline (**35 426** pts) is gone from `/tmp`. Used
`geojson-routes/elsa-sjuvass.geojson` (**33 911** pts, **1944.2 km** — same
Bugøynes → Sjuvasslia corridor the report compared to the kept plan). Matched
Geofabrik PBFs under `navi-gate/packs-fu38-nogav`
(`nord-norge`, `finland`, `sweden`, `ostlandet`) for ways with `tunnel=*` and
`highway` in motorway/trunk/primary (+ links). Named tunnels / lokket kept if
sampled nodes lie along the corridor (mean distance ≤150 m after merge);
dual-carriageway length ≈ summed way lengths / 2. FI / SE / nord-norge
returned **0** hits on this coarse geojson chord (corridor inland samples
often far from carriageways); Østlandet returned the inventory below. Live
plan was ~4.7% longer than this geojson — OSM count is a lower bound for the
Østlandet spine, not a claim that FI/SE had zero tunnels on the live MWM
polyline.

**OSM tunnel count: 25** (approx. total length **~21.1 km**).

| along_km | ~length_km | Name | Approx. location |
|---:|---:|---|---|
| 1692.8 | 0.04 | Leirettunnelen | south of Elverum |
| 1714.5 | 0.09 | Byrotunnelen | toward Hamar |
| 1739.5 | 0.68 | Espatunnelen | E 6 Espa |
| 1750.6 | 2.33 | Morskogstunnelen | E 6 |
| 1755.9 | 0.63 | Korslundtunnelen | E 6 |
| 1766.7 | 1.17 | Eidsvolltunnelen | E 6 Eidsvoll |
| 1783.1 | 0.04 | Elstadmolokket | E 6 |
| 1786.9 | 0.06 | Hauerseterlokket | E 6 |
| 1830.3 | 0.83 | Vålerengtunnelen | Oslo |
| 1830.7 | 1.74 | Operatunnelen / Svartdalstunnelen | Oslo |
| 1831.4 | 1.98 | Operatunnelen / Ekebergtunnelen | Oslo |
| 1832.8 | 1.74 | Operatunnelen / Bjørvikatunnelen | Oslo |
| 1833.9 | 0.16 | Bjørvikatunnelen | Oslo |
| 1834.1 | 1.91 | Operatunnelen / Festningstunnelen | Oslo |
| 1835.3 | 0.65 | Operatunnelen | Oslo |
| 1836.3 | 0.12 | Framnestunnelen | Oslo west |
| 1838.4 | 0.18 | Sjølysttunnelen | Oslo west |
| 1841.0 | 0.32 | Lysakerlokket | Lysaker |
| 1841.7 | 0.25 | Teleplanlokket | Bærum |
| 1842.4 | 0.45 | Stabekklokket | Stabekk |
| 1847.6 | 0.17 | Sandviksåstunnelen | Sandvika |
| 1848.1 | 1.68 | Bjørnegårdtunnelen | E 18 |
| 1867.3 | 0.81 | Fosskolltunnelen | toward Drammen |
| 1874.1 | 1.22 | Bragernestunnelen | Drammen |
| 1876.2 | 1.88 | Strømsåstunnelen | Drammen / E 18 |

### Fuel (report-only)

Tank 70 L. Formula estimate for D=2038.523 km: **2** stops at 500 mi range
and **2** at 600 mi. Not a live HUD fuel list.

### RAM

Before ~**327 MiB** PSS; post-plan ~**1046 MiB**; final sample ~**1034 MiB**.
App left open (no force-stop for this campaign close-out).

---

## DATEX

Six synthetic **MaintenanceWorks** soft **Penalize** situations were staged
for the campaign (`datex_synthetic_reroutes` / host inject path):

| Id | Country | Lat | Lon | Label |
|---|---|---|---|---|
| syn-se-inari | FI | 69.44041 | 28.41524 | Inari–Kautokeino land |
| syn-se-pajala | SE | 67.79923 | 24.93191 | near Pajala |
| syn-se-skelleftea | SE | 66.00533 | 22.56261 | Skellefteå class |
| syn-se-ornskoldsvik | SE | 63.57672 | 19.64146 | Örnsköldsvik class |
| syn-se-sveg | SE | 62.29125 | 15.13323 | Sveg/Jämtland class |
| syn-no-elverum | NO | 60.62109 | 11.26339 | Elverum/Østlandet |

**On the finished plan report:** `datex_impacts=0; datex_block=0; datex_penalize=0`.
Logcat: `mwm datex avoid: no Block on first-pass corridor`; enrich
`datex_lines=3`. Post-plan refresh warned
`parse_failed:datex xml: expected 'severity' tag, not 'validity'`.

**Timed soft injects during GPS replay** (before user abort), from
`datex_replay_events.json` / oneshot host log — **soft DATEX count = 4**:

| Id | mins before arrival | along_m | vehicle_along_m |
|---|---|---|---|
| syn-se-inari | 29.83 | 112670 | 74400 |
| syn-se-pajala | 29.4 | 360515 | 322800 |
| syn-se-skelleftea | 29.32 | 738407 | 700800 |
| syn-se-ornskoldsvik | 29.64 | 1090429 | 1052400 |

No completed mid-sim `auto_reroute_count` / polyline distance delta was
recorded after those injects (GPS aborted at step **871/1698**, ~**1045 km**
along route). Soft inject + Penalize intent is documented; a finished
geometry reroute proof is not.

---

## GPS town crawl (waived)

User stopped GPS replay because every mock position change on SM-P613
emitted a device beep. GPS injection was killed and **not** restarted.

Progress at abort: step **871 / 1698**, ~**1045.2 / 2036.2 km**, towns
seen in progress JSON: Bugøynes, Inari, Skellefteå (Örnsköldsvik screenshot
also appeared in host log). Those captures are **not** treated as required
town-screenshot deliverables after the waiver.

---

## Comparison vs `geojson-routes/elsa-sjuvass.kml`

Reference KML and the kept plan polyline share the Bugøynes → Sjuvasslia
land corridor (Finland / Sweden / Østlandet spine).

| | KML reference | This campaign plan |
|---|---|---|
| Vertices | **33 911** | **35 426** |
| Length | **1944.235 km** | polyline **2036.289 km** (report **2038.523 km**) |
| Delta length | | **+92.1 km (~4.73%)** vs KML |
| Start | 69.9742, 29.63342 | 69.97414, 29.63376 (**0.014 km**) |
| End | 59.80326, 9.39866 | 59.80318, 9.39787 (**0.045 km**) |
| Coarse sample offset (plan→KML) | | median **~1.2 km**, p90 **~66.5 km**, max **~124 km**; **~77%** of samples within 5 km |
| Ferry | none in KML property sense | plan **0** ferry legs |
| Via | none | none |

Interpretation: endpoints match; planned route is slightly longer than the
reference KML and mostly follows the same corridor, with larger coarse
offsets on some inland samples (expected when the live MWM path diverges
from the canned KML chord).

---

## Honest gaps

- GPS town crawl / required town screenshots: **waived** (device noise).
- Soft DATEX timed injects: **4** before abort; sits 5–6 (Sveg, Elverum) not reached.
- Finished-plan `datex_impacts=0` with XML parse warning; no captured
  mid-drive geometry reroute / `rerouted=true` proof.
- Wild camping **64** (bands) vs enum **80** — both > 0; reported both.
- Soft break settings field **4.0 h** vs chunked plan interval **1.50 h**.
- Fuel stops: formula only.
- A later instrument restart began rewriting device `report.json` after the
  preserved pull; numbers above are from the successful plan ART / pre-overwrite pull.
- Attractions: complete **total + per-type** recovered; per-hit `hits_snip`
  JSON lost with ART `/tmp` eviction (four names kept from the written report).
- Tunnels: app enum **5** with non-tunnel road/bridge labels; OSM re-derivation
  used geojson corridor (ART polyline gone) — **25** on Østlandet; FI/SE/nord-norge
  **0** on that coarse chord (not proof the live MWM plan had none there).
