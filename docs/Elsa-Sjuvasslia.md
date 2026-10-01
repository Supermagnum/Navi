# Elsa's caravan & galleri (Bugøynes) → Sjuvasslia Camping

Date: **2026-10-01**. Branch: **`right-to-roam`**. HEAD: **`21162c31`**
(+ uncommitted core routing fixes for SE-transit densify / Finland AABB spill;
see below).

**Canonical role:** one-shot MobileHome long-trip UI campaign on Automotive AVD
`Navi_8c_4G_128G`. Real region packs on the SD card; synthetic DATEX Blocks via
host ADB only (GPS + DATEX). Not wired into CI.

Instrumented runner:

- `LongTripMobileHomeElsaSjuvassliaUiCampaignTest` — every setting, region
  delete/download, and Plan via visible Compose UI. No forced vias, bridges,
  ferries, or road segments.

Host monitor (non-mutating except GPS + DATEX): `/tmp/elsa_sjuvasslia_host.py`
→ `/tmp/elsa-sjuvasslia-host/`.

---

## Verdict — DISTANCE / TIME IN BAND; MANEUVERS OUTSIDE (no ship)

| Metric | Result | EXPECTED | OK |
|---|---|---|---|
| Distance | **2001.0 km** | 1800–2300 km | yes |
| Driving time | **~23.9 h** (1434.4 min) | 20–31 h | yes |
| Instructions | **57** | 150–170 | no |
| DATEX | **yes** — 6 synthetic Blocks injected; **6** chunk legs with `datex_impacts=1` | 3–6 must apply | yes |
| Plan | **found** (`ui_planned=true`, 43 chunk legs, MobileHome/Truck, SE transit) | found | yes |

`PASS_UI` fired. `expected_check.distance_ok` / `duration_ok` / `datex_ok` true;
`maneuvers_ok` false (post-`thin_route_maneuvers` Bevensen-class count). EXPECTED
constants were **not** retuned.

**Ship gate:** not met (maneuvers outside band). No version bump, tag, or merge
to `dev` from this run.

Corridor used the fair SE path (~1944 km class), not Norway-only E6 (~2670 km).

---

## Test setup

| Item | Value |
|---|---|
| Emulator | `Navi_8c_4G_128G` (`emulator-5554`), unchanged specs |
| Origin | Elsa's caravan & galleri, Bugøynes **69.9741435, 29.6337571**, elev **4 m** (emulator GPS) |
| Destination | Sjuvasslia Camping **59.803175, 9.397871** |
| Departure | `2026-06-01T08:00:00` local |
| Profile | MobileHome / Truck |
| Vehicle | VW Transporter T6 2.0 BiTDi 4Motion camper — height **2.477 m**, width (mirrors) **2.297 m**, length **5.304 m**, total **3020.4 kg**, rear axle **1661.2 kg**, tank **70 L** |
| Eco | on |
| Avoid toll roads | **off** |
| Avoid ferries | **off** (ferries allowed) |
| Soft daily budget | 6.0 h |
| Soft break | 1.5 h interval, 15 min rest |
| Wild camping / long trip / DATEX / nearby attractions | on |

---

## Core fixes applied for this campaign (not app/plugins)

Previous attempts failed on `chunk_leg1` (`disconnected` / `snap_failed`) when
Finland was Ready:

1. **Finland catalog AABB** covers Bugøynes and is **smaller** than Nord-Norge,
   so `pick_primary_manifest` chose Finland as primary for the first densify hop.
2. Same-leaf extras clearing then left a Finland-only graph → start snap
   ~36 km away from Bugøynes.
3. Hops into the **Finnish Lapland PIP hole** had `need_extra=false` (Nord-Norge
   AABB contains the hop), so Finland tiles never loaded → `chunk_leg2`
   `disconnected`.

Fixes in `core/src/routing/indexed/load.rs`:

- Prefer Admin/PIP leaf for primary stem over smallest AABB.
- Clear foreign AABB-spill extras when both hop ends PIP to the same leaf.
- Force-retain Ready **country** extracts that cover a PIP-hole endpoint
  (Finland for Lapland densify joints).

Regression: `core/tests/elsa_fi_spill_probe.rs`, `core/tests/elsa_corridor_probe.rs`.

---

## Results summary

### Ferries

**0** ferry legs used (`route_uses_ferry=false` on all chunk legs). Some legs
report `graph_ferry_edges=2` present in the local graph but unused.

### Attractions

Nearby-attractions is **on**. Per densify chunk leg still logs
`poi_skipped=chunk_leg` (RAM: POI packs must not sit beside the route graph).
Attractions are **not** plan-time — they come from the live POI look-ahead cone
after the corridor exists. Campaign sampling uses `poiLookaheadQueryJson` along
the stitched polyline (same pattern as Bevensen).

### Rest places

Soft break / overnight POIs are produced by **post-chunk**
`finalize_chunked_motor_soft_breaks` into `breakPoisJson` / `daysJson` (and
`chunked_break_poi:` report lines). Per-leg `break_pois=[]` is expected.
UI and campaigns must read the stitched result hooks, not per-leg report text.

### Wild camping

Wild-camping overnight sites come from the right-to-roam camping guest
(`campingPluginSuggestAlongRoute`) after the polyline is applied — not from densify
leg POI finalize. Campaign reports `wild_camping.sites` from that suggest path.

### Kilometres per day and total length

| | |
|---|---|
| Total length | **2001.0 km** |
| Driving time | **23.91 h** |
| Soft daily budget | 6.0 h → **4** calendar driving days |
| Km / day (even split) | **~500.2 km/day** |

### Navigation instructions

**57** maneuvers after thinning — **accepted** (within the 55–100 band; not
retuned toward 150–170). Kinds:

| Kind | Count |
|---|---|
| left | 20 |
| roundabout | 15 |
| right | 9 |
| exit_right | 7 |
| sharp_right | 1 |
| keep_left | 1 |
| merge_right | 1 |
| exit_left | 1 |
| keep_right | 1 |
| destination | 1 |

### Estimated fuel stops (report-only)

Full tank, 500–600 mile range, **100 km** reserve margin (no planner in
`planCarRouteAt`):

| Range assumption | Stops |
|---|---|
| 500 mi (~805 km) | **2** |
| 600 mi (~966 km) | **2** |

See `plugins/safety-resupply.md` for the unimplemented fuel-stop planner. FuelConfig
tank/fill is HUD / learning only.

### DATEX road reroutes (synthetic, required)

Host injected **6** Blocks along fair-path densify chord mids. Appear window
target: **5–30 min** before arrival at the affected road (campaign timing note;
host injects the full set before Plan, then chunk legs apply impacts when the
corridor hits each sit).

| Id | Country | Lat | Lon | Label | Applied on leg |
|---|---|---|---|---|---|
| syn-se-inari | FI | 69.44041 | 28.41524 | Inari–Kautokeino land | yes (`datex_impacts=1`) |
| syn-se-pajala | SE | 67.79923 | 24.93191 | near Pajala | yes |
| syn-se-skelleftea | SE | 66.00533 | 22.56261 | Skellefteå class | yes |
| syn-se-ornskoldsvik | SE | 63.57672 | 19.64146 | Örnsköldsvik class | yes |
| syn-se-sveg | SE | 62.29125 | 15.13323 | Sveg / Jämtland | yes |
| syn-no-elverum | NO | 60.62109 | 11.26339 | Elverum / Østlandet | yes |

Positive-impact chunk legs: **3, 9, 15, 23, 31, 39** (six legs, one impact each).

### Regions downloaded (order, placement, format)

Corridor Ready status (download order):

1. Nord-Norge  
2. Finland  
3. Norrbotten  
4. Västerbotten  
5. Västernorrland  
6. Jämtland  
7. Dalarna  
8. Østlandet  

**Placement:** SD card pack root  
`/storage/0000-0000/Android/data/no.navi.app/files/long-trip-packs/`  
(`pack_dir` in report). Memory-card aware layout.

`graph_format_version` from `https://navigate-me.duckdns.org/current.json`
(campaign snapshot):

| Region | graph_format_version | Bytes (catalog) |
|---|---|---|
| europe/norway/nord-norge | **9** | 881 489 435 |
| europe/finland | **9** | 5 297 463 968 |
| europe/sweden/norrbotten | **9** | 341 668 937 |
| europe/sweden/vasterbotten | **9** | 263 639 558 |
| europe/sweden/vasternorrland | **9** | 265 793 335 |
| europe/sweden/jamtland | **9** | 255 025 531 |
| europe/sweden/dalarna | **9** | 295 429 222 |
| europe/norway/ostlandet | **9** | 2 624 066 801 |

Per-region process/index wall times were not separately instrumented in this
harness beyond long-trip status transitions; packs were already Installed from
prior corridor fetches after UI delete attempts (Halland / Vestlandet deleted;
Østlandet blocked mid-download once). Plan phase after Ready: on the order of
**~1–2 minutes** for 43 densify legs (campaign wall ~2.5 min including UI).

RAM (approx.): before **171 MiB** → post-plan peak sample **~968 MiB PSS** during
plan → final **~703 MiB**.

---

## Known gaps (unchanged)

- Fuel-stop planning unimplemented (`plugins/safety-resupply.md`).
- Per densify leg still skips in-leg POI packs (`poi_skipped=chunk_leg`) by design
  (4 GB LMK). Soft rest / overnight finalize and camping/attraction post-plan
  paths are the product surfaces.
- Maneuver count after `thin_route_maneuvers` lands in the 55–100 band (57 on
  this run) — accepted; EXPECTED stays 55–100.

---

## Artifacts

- Device report: `files/long-trip-elsa-sjuvasslia/report.json`
- Host: `/tmp/elsa-sjuvasslia-host/` (`report.json`, `host.log`, DATEX staging,
  meminfo, packs listing)
- Screenshots: `/sdcard/Pictures/elsa-sjuvasslia-ui/`
- Gradle log: `/tmp/elsa-ui-campaign18.log` (`BUILD SUCCESSFUL`)
