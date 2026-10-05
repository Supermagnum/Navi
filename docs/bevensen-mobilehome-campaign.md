# Bad Bevensen → Dalsøren MobileHome campaign

## Latest — 2026-10-05 20:34 (found; in band; wild camp 116; DATEX 6)

Worktree **`dev`**. Emulator **`Navi_8c_4G_128G`** (`emulator-5554`). Release
`libnavi` x86_64 after densify E47/Farø restore + camping pack-stem fix
(`pick_primary_manifest` PIP before PBF-stem pack; `find_planning_pbf` prefers
manifest-backed leaf PBFs). App pid **24244**. No extra vias, no hop stubs, no
screenshots.

GPS `adb emu geo fix 10.587198 53.079686`. Profile `mobile_home`, **eco on**
(overlapping debug-intent forced eco=true; densify hops identical to eco-off
verify), wild camping **on**, ferries allowed. From 53.079686, 10.587198
(Bevensen) · via 61.8691419, 9.1055130 (Vågåvegen 80) · to 61.4433766, 7.4614016
(Dalsøren / Lustravegen).

`planning_start` 20:23:00. `long_trip densify span=8.789 hops=18`
`hops_latlon=53.0797,10.5872;54.2100,11.0250;54.3999,11.3643;55.1125,12.0400;55.9100,13.0520;56.9350,12.4960;57.5950,12.2428;58.2550,11.9895;58.4437,11.2087;59.2762,10.8637;59.9100,10.7500;60.7950,11.0680;61.1150,10.4660;61.7720,9.4200;61.8691,9.1055;61.8380,8.5690;61.6170,8.0440;61.4434,7.4614`.
Chunked plan-time DATEX `impacts=6 block=0 penalize=6`. `terminate=found` 20:27:06
(246.2 s, expansions 2377036).

Densify order: Fehmarn → E47/Farø (~55.11N, 12.04E) → Skåne/Halland/VG E6 →
Oslo → Hamar (east Mjøsa) → Lillehammer → Otta → Vågå → Lom → Sognefjell →
Dalsøren. Maneuvers: B 207 / E 47, Sydmotorvejen (E 47) Rødby, Næstvedvej /
Farø, Amagermotorvejen / Øresund E 20, E 6 Solberg–Oslo–Hamar–Lillehammer–Otta,
**Ottadalsvegen (15)**, **Sognefjellsvegen (55)**. Zero Kalvehave / Sakskøbing /
Rågeleje street hits. `route_ferry_legs=0`.

Wild camping: `suggestAlongRoute kind=OK` `accepted=116` `rejected=44`
`on_foot=116` (graph ~357 s then wasm). DATEX: `penalize=6`.

| Metric | This plan | EXPECTED | OK |
|---|---|---|---|
| Distance | **1589.478 km** | 1461.3–1648.6 km | **yes** |
| Driving time | **1187.3 min (19.79 h)** | 17–22 h | **yes** |
| Ferries | **0** (router) | allowed; ferry shortcut is not a fail | yes |
| Eco | **on** (intent contention) | densify corridor matches eco-off | yes |
| Wild camping | **accepted=116** | > 0 | **yes** |
| DATEX penalties | **penalize=6 block=0** | > 0 | **yes** |
| E6 Oslo–Otta / east Mjøsa | yes (Hamar ~11.13E) | must | yes |
| Ottadalsvegen Otta–Vågå–Lom | yes | must | yes |
| Lolland/Kalvehave densify detours | no | must not | yes |
| Plan | **`terminate=found`** | complete | yes |

Release gate: **open** (in-band + no named DK coast detours + wild camp + DATEX).
Tag target **v0.3.18-beta** (`versionCode` 23).

### Wild camp `segment_errors=35` (fixed)

Earlier camping-on Plans returned
`corridor graph segments produced no seeds (pack_dirs=2; segment_errors=35)`.
Logcat (Info, not Warn) showed every segment fail with
`indexed pack missing or incomplete` on **correct** DE→NO bboxes — not
`asia/pakistan` / lon-lat swap. Root cause: `find_planning_pbf` preferred the
first `*-latest.osm.pbf` under pack roots / `files/`, which was often
`sweden-latest.osm.pbf` (country extract, **no** Ready `sweden-latest`
manifest). Graph load then returned `PackLoadError::Missing` before PIP could
re-home to Niedersachsen / leaf packs. Fix: prefer manifest-backed leaf PBFs;
`pick_primary_manifest` PIP/cover before requiring the planning PBF stem pack.
After install: `planning_pbf=…/schleswig-holstein-latest.osm.pbf`,
`seeds_raw=280`, `accepted=116`.

### Detour causes (fixed)

1. **Sjælland leaf proxy dropped** (MV Baltic AABB fringe) → SH→Skåne even-split
   onto Kalvehave (~12.40E). Fixed: skip MV in foreign-fringe check; keep
   E47/Farø leaf centroid. Regression:
   `densify_bevensen_vagaa_does_not_even_split_onto_zealand_coasts`.
2. **Camping corridor pack stem** — `find_planning_pbf` / `pick_primary_manifest`
   required a Ready pack for a country extract (`sweden-latest`) before PIP
   rematch → every overnight segment `indexed pack missing`. Fixed: prefer
   manifest-backed leaf PBFs; PIP/scan before PBF-stem hard-fail.

### Comparison vs sibling `bad-luster.json` / `bad-luster.kml`

Files under `/mnt/2e9a1e9f-2097-408c-ab9a-a01b32f11d28/github-projects/Navi/geojson-routes/`:
`bad-luster.json`, `bad-luster.kml`. Same three pins: Heidestraße Bad Bevensen →
Vågåvegen 80 → Lustravegen. **The ferry shortcut is not a campaign fail.**

| | Navi this plan | bad-luster |
|---|---|---|
| Distance | **1589.478 km** | **1440.985 km** |
| Time | 1187.3 min / **19.79 h** | 64572 s / **17.94 h** |
| Ferries | 0 tagged legs | **1**: Rødby–Puttgarden (~18.9 km) |
| Denmark | B 207 → E 47 Sydmotorvejen → Farø / Næstvedvej → E 20 | A 1 → ferry → E 47 → E 20/E 47 |
| Norway | E 6 Oslo → Hamar → Lillehammer → Otta → Ottadalsvegen → Lom → Rv55 | same corridor |
| Via | Vågåvegen 80 | same |
| Finish | Sognefjellsvegen 55 / Lustravegen | same |

Delta vs luster: **+148.5 km**, **+1.85 h**. Geometry agrees on Fehmarn/E47/Farø,
east-Mjøsa E6, and Ottadalsvegen Otta–Vågå–Lom. Remaining extra length vs ORS is
mostly the missing tagged ferry plus DATEX penalize=6 and highway costs, not
Lolland/Kalvehave loops.

### Additional ground-truth KML (regression refs; not this Plan)

Same sibling folder — use for densify / highway preference / ferry overlay
(real OSM ferries only, no stubs):

- `elsa-sjuvass.kml` / `elsa-sjuvass.geojson`
- `breneriroa-aga.kml` / `breneriroa-aga.geojson` — correct ferry is
  **Kinsarvik–Utne** (not a lesser ferry or land detour)

## Fixes in this tree

- `core/src/routing/plan_bbox.rs` — E47/Farø / west-coast E6 / east-Mjøsa;
  Ottadal westbound densify; regression tests including
  `densify_bevensen_vagaa_does_not_even_split_onto_zealand_coasts`.
- `core/src/routing/indexed/load.rs` — `pick_primary_manifest` PIP/scan before
  requiring planning-PBF stem pack (camping corridor).
- `navi-ffi/src/camping_plugin.rs` — manifest-backed `find_planning_pbf`;
  segment load error logging.
- `navi-ffi/src/lib.rs` — long-trip region densify; `hops_latlon` logging.
- DATEX inject remains `.campaign-datex/` (local only).

Do not add this campaign to CI workflows.

## How to replay (local only)

```
adb emu geo fix 10.587198 53.079686
adb shell am start -n no.navi.app/.MainActivity --ez navi_long_trip true \
  --ez navi_auto_plan true --ez navi_eco false --ez navi_avoid_ferries false \
  --es navi_profile mobile_home --es navi_from_name Bevensen \
  --ed navi_from_lat 53.079686 --ed navi_from_lon 10.587198 \
  --es navi_via1_name Vagavegen80 --ed navi_via1_lat 61.8691419 \
  --ed navi_via1_lon 9.1055130 --es navi_to_name Dalsoren \
  --ed navi_to_lat 61.4433766 --ed navi_to_lon 7.4614016
```
