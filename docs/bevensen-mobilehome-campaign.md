# Bad Bevensen → Dalsøren MobileHome campaign

## Latest — 2026-10-28 05:00 (found; in band; wild camp 96; DATEX penalties)

Worktree **`CAT`**. Emulator **`Navi_8c_4G_128G`** (`emulator-5554` only; tablet
R52TB0JQEDE never used). Host AVD is configured for 8 cores; the running
emulator exposed **6 cores** (max observed this run). Release `libnavi` x86_64
after Stage-B Ready-tile cover gate (Puttgarden densify demotion). App pid
**12871**. Long trip **ON**, pack volume **`uuid:BF1B-9F0E`**. No map pans.
Dummy Hamlib disconnected for this plan. No screenshots.

GPS `adb emu geo fix 10.587198 53.079686`. Profile `mobile_home`, **eco on**,
wild camping **on**, ferries allowed (land excl won). From 53.079686, 10.587198
(Bevensen) · via 61.8691419, 9.1055130 (Vågåvegen 80) · to 61.4433766, 7.4614016
(Dalsøren / Lustravegen). Soft daily budget 6 h / break 1.5 h / rest 15 min.
Vehicle: VW Transporter T6 2.0 BiTDi 4Motion camper (H 2.477 m, W 2.297 m,
L 5.304 m, total 3020.4 kg, axle 1661.2 kg, tank 70 L).

`planning_start` ~04:58:36. Stage-B demoted ferry densify (`free`,
Puttgarden ~11.228E east of SH Ready tile max lon ~11.166) and fell back to
`excl_Rødby (DK)_Puttgarden (D)` (`ferries=0`). `long_trip densify span=8.789
hops=18`
`hops_latlon=53.0797,10.5872;53.9732,10.2268;54.6737,9.4246;54.8066,9.3280;55.4105,10.0744;55.4579,11.9916;55.5655,12.8925;55.9241,12.8373;56.7786,12.8052;57.8845,11.9455;58.8816,11.2737;59.1026,11.2517;59.3743,10.7846;59.7591,10.8108;60.0836,11.1491;60.9099,10.8157;61.8691,9.1055;61.4434,7.4614`.
Corridor Indexed on SD: Niedersachsen, Schleswig-Holstein, Denmark, Skåne,
Halland, Västra Götaland, Ostlandet, Sorlandet, Vestlandet. `terminate=found`
05:00:27 (`planning_done` 110.6 s, expansions 1206555). Peak hop `vm_hwm_mb=1491`.

Densify order: Kiel / Flensburg land approach → Sønderjylland → Fyn →
Sjælland / Øresund → Skåne/Halland/VG E6 → Oslo → Mjøsa approach → Otta →
Vågå → Lom → Sognefjell → Dalsøren. Maneuvers include B 207, B 77/B 202,
Fynske Motorvej (E 20), Vestmotorvejen / Amagermotorvejen / Øresundsmotorvejen
(E 20), E 6, **Ottadalsvegen (15)**, **Sognefjellsvegen (55)**. Zero Kalvehave /
Sakskøbing / Rågeleje street hits. `route_ferry_legs=0`.

Wild camping: `suggestAlongRoute kind=OK` `accepted=96` `rejected=64`
`on_foot=96` (SE 46 / NO 50). DATEX: six synthetic MaintenanceWorks penalties
injected under `files/datex_cache` via host ADB; plan-time `datex_nearby`
penalties observed on corridor (DE A7 chord, NO Ottadal via). Attractions: 4.
Multi-day soft budget: 3 days (overnights First Camp Malmö Sibbarp; Overnight
7762407986).

**Duration band note:** driving time **987.8 min (~16.46 h)** is slightly under
the historical ~17–22 h window. Distance is in band. Operator accepted **16.46 h
as in-band** for this CAT re-plan (2026-10-28).

| Metric | This plan | EXPECTED | OK |
|---|---|---|---|
| Distance | **1585.313 km** | 1461.3–1648.6 km | **yes** |
| Driving time | **987.8 min (~16.46 h)** | ~17–22 h (16.46 h accepted) | **yes** |
| Ferries | **0** (router) | allowed; ferry shortcut is not a fail | yes |
| Eco | **on** | on | yes |
| Wild camping | **accepted=96** | > 0 | **yes** |
| DATEX penalties | **injected 6; nearby penalties on route** | > 0 | **yes** |
| E6 Oslo–Otta corridor | yes (E 6 / Gausdal–Gudbrandsdal) | must | yes |
| Ottadalsvegen Otta–Vågå–Lom | yes | must | yes |
| Lolland/Kalvehave densify detours | no | must not | yes |
| Plan | **`terminate=found`** | complete | yes |

Release gate: **open** (in-band + land excl cover-gate + wild camp + DATEX).
Ship follows CI + version bump on `CAT`.

### Puttgarden cover-gate (fixed this run)

Earlier same-day Plans (`04:26` / `04:46`) ended
`terminate=hop_end_tile_missing` when Stage-B densify pinned hop end
`54.50282,11.22822` (Puttgarden) east of Ready SH tile max lon ~11.166.
Fix in `core/src/routing/corridor_skeleton.rs`: demote densify alts whose hop
ends sit outside Ready tiles; fallback to fewest-km covered alt; snap uncovered
`split_hops` ends to last covered coarse node. Regression:
`puttgarden_outside_sh_tile_fails_ready_cover_gate`.

### Comparison vs sibling `bad-luster.json` / `bad-luster.kml`

Same three pins: Heidestraße Bad Bevensen → Vågåvegen 80 → Lustravegen.
**The ferry shortcut is not a campaign fail**; this plan took the land excl.

| | Navi this plan | bad-luster |
|---|---|---|
| Distance | **1585.313 km** | **1440.985 km** |
| Time | 987.8 min / **~16.46 h** | 64572 s / **17.94 h** |
| Ferries | 0 tagged legs | **1**: Rødby–Puttgarden (~18.9 km) |
| Denmark | land Flensburg/Fyn/E 20 → Øresund | A 1 → ferry → E 47 → E 20/E 47 |
| Norway | E 6 → Ottadalsvegen → Sognefjellsvegen 55 | same corridor |
| Via | Vågåvegen 80 | same |
| Finish | Sognefjellsvegen 55 / Lustravegen | same |

Delta vs luster: **+144.3 km**, **−1.48 h**. Extra length is the land excl of
the ferry plus DATEX/highway costs, not Lolland/Kalvehave loops.

### Additional ground-truth KML (regression refs; not this Plan)

Same sibling folder — use for densify / highway preference / ferry overlay
(real OSM ferries only, no stubs):

- `elsa-sjuvass.kml` / `elsa-sjuvass.geojson`
- `breneriroa-aga.kml` / `breneriroa-aga.geojson` — correct ferry is
  **Kinsarvik–Utne** (not a lesser ferry or land detour)

## Prior — 2026-10-05 20:34 (found; in band; wild camp 116; DATEX 6)

Worktree **`dev`**. Emulator **`Navi_8c_4G_128G`** (`emulator-5554`). Release
`libnavi` x86_64 after densify E47/Farø restore + camping pack-stem fix.
App pid **24244**. Distance **1589.478 km**, **1187.3 min (19.79 h)**, ferries 0,
wild camp accepted=116, DATEX penalize=6. Fehmarn → E47/Farø densify (not used
on 2026-10-28 land excl).

## Fixes in this tree

- `core/src/routing/corridor_skeleton.rs` — Ready-tile densify cover gate;
  demote uncovered ferry alts; snap uncovered hop ends; regression
  `puttgarden_outside_sh_tile_fails_ready_cover_gate`.
- Prior (still in tree): `plan_bbox.rs` E47/Farø / east-Mjøsa densify;
  camping `pick_primary_manifest` / `find_planning_pbf` leaf preference.
- DATEX inject remains host-local under app `datex_cache` (not committed).

Do not add this campaign to CI workflows.

## How to replay (local only)

```
adb emu geo fix 10.587198 53.079686
adb shell am start -n no.navi.app/.MainActivity --ez navi_long_trip true \
  --ez navi_auto_plan true --ez navi_eco true --ez navi_avoid_ferries false \
  --es navi_profile mobile_home --es navi_from_name Bevensen \
  --ed navi_from_lat 53.079686 --ed navi_from_lon 10.587198 \
  --es navi_via1_name Vagavegen80 --ed navi_via1_lat 61.8691419 \
  --ed navi_via1_lon 9.1055130 --es navi_to_name Dalsoren \
  --ed navi_to_lat 61.4433766 --ed navi_to_lon 7.4614016
```
