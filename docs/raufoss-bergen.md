# Raufoss → Bergen busstasjon (Samsung MWM UI campaign)

Date: **2026-11-19**. Device: Samsung **SM-P613** serial **`R52TB0JQEDE`**
(arm64-v8a). App `no.navi.app` **0.5.0-beta** (`versionCode` **35**), debug APK
sha256
`6a9ab027cee3cc42781fb12a2576d79c37abb8a088f0e48f6ef2c51272000915`
(replaces prior `f6890bf97e…` that lacked arm64 `is_tunnel`). **Not** added to
CI. Instrumentation driver `CarRaufossBergenUiCampaignTest` drove the real UI;
ADB was used only for GPS mock + synthetic DATEX (snapped onto the live CoMaps
polyline) and read-only monitoring. Host:
`/tmp/raufoss_bergen_samsung_host.py`. Artifacts:
`/tmp/raufoss-bergen-samsung/attempt_tunnel_fix/`.

Download path: **Tools → Offline maps → Choose region**
(`btn_mwm_regions` / `btn_mwm_install_europe_norway_ostlandet`). Geofabrik
coverage dialogs were dismissed so planning stayed on the MWM path.

Native fix for this PASS: arm64 `liborganicmaps.so` rebuilt with quilt patch
`0010-navi-tunnel-turns-json.patch` so CoMaps turns JSON emits `"is_tunnel":`
(verified via `strings` on staged jniLibs, packaged APK, and device APK).

---

## Verdict

| Metric | Result | EXPECTED | OK |
|---|---|---|---|
| Distance | **428.530 km** | 0–500 km | yes |
| Driving time | **~6.5 h** (389.4 min) | 3–8 h | yes |
| Instructions | **28** maneuvers | 1–170 | yes |
| Plan | **found** (`ui_planned=true`, bands all ok) | found | yes |
| Tunnels | **3** (`route_tunnel_count=3`) | >= 1 on corridor | yes |
| DATEX inject | **yes** — 5 synth sits + mid Block | 3–6 | yes |
| DATEX apply | **yes** — `datex_impacts=5` (`datex_block=1`, `datex_penalize=4`) | Block+penalize | yes |
| DATEX reroute | **mixed** — plan `datex_mwm=reroute_failed` (Block unavoided; poly unchanged); mid-sim `rerouted=true` / auto_reroute_count=1 | geometry change | mixed |
| DATEX render | **yes** — `datex_render.ok=true`, `drape_marks=5`, `compose_marks=1`, `camera_near_sit=true`; screenshot `13b_datex_markers` | markers/HUD | yes |
| Mid-sim | **yes** — host mid Block `syn-mid-block-sim` snapped to polyline; screenshots `14a`/`14b` | mid inject | yes |
| Rest places | **4** soft-break pins (1.5 h) | soft-break spacing | yes |
| Attractions | **7** (`tourism-attraction`) | several | yes |
| JUnit | **OK (1 test)** in 629.7 s (`NaviAndroidTestRunner`) | pass without closing app | yes |

Corridor: Raufoss (Innlandet via Østlandet leaves Hedmark/Oppland) west toward
Bergen. Soft break 1.5 h / 15 min; eco on; wild camping off; long trip off.

---

## Test setup

| Item | Value |
|---|---|
| Device | Samsung SM-P613 / `R52TB0JQEDE` (arm64-v8a only) |
| Origin | Raufoss **60.7277483, 10.6109403**, elev **311 m** (`cmd location` + mock allow) |
| Destination | Bergen busstasjon **60.388114, 5.333857** (coord search) |
| Profile | Car |
| Eco | on |
| Avoid toll / ferries / tunnels | off / off / off (`avoid_tunnels=false`) |
| Soft daily budget | **6.0 h** |
| Soft break | **1.5 h** / **15 min** |
| Wild camping | off |
| Long trip | off |
| DATEX overlay | on |
| Nearby attractions | on |
| Download UI | MWM Choose region (`download_ui=mwm_choose_region`) |
| APK sha256 | `6a9ab027cee3cc42781fb12a2576d79c37abb8a088f0e48f6ef2c51272000915` |

---

## Regions (MWM Choose region)

Catalog mode **comaps-release**, release **261105**, placement
`/data/user/0/no.navi.app/files/mwm`. Innlandet is covered by Østlandet leaves
(**Norway_Hedmark**, **Norway_Oppland**); no separate Innlandet region id.

### Policy

**UI delete via Map storage before plan; redownload Østlandet first**
(`delete_policy` from report). Driver deleted 9 leaves via Map storage, then
installed `europe_norway_ostlandet` (~119 s to indexed).

### Østlandet `europe_norway_ostlandet`

| Phase | Result |
|---|---|
| Order | Østlandet first after UI-delete |
| Leaves | 9 leaves + 9 sidecars @ **261105** |
| Index timing | `first_ostlandet` **119202 ms**, ok |
| Placement | `/data/user/0/no.navi.app/files/mwm` |
| CoMaps session | open after install (`native_loaded=true`, `native_available=true`) |

Leaves: Denmark_North Denmark Region, Norway_Central, Norway_Hedmark,
Norway_Hordaland, Norway_Oppland, Norway_Southern, Norway_Western,
Sweden_Bergslagen, Sweden_Vastra Gotaland.

### Vestlandet

Planning offered missing-maps download; driver accepted
(`vestlandet_prompt_handled=true`). Corridor leaves already included
Norway_Western / Norway_Hordaland under the Østlandet pack.

---

## Results summary

### Plan

| Field | Value |
|---|---|
| Planner | `mwm` |
| Distance | **428.530 km** |
| ETA | **389.4 min** (~6 h 29 m) |
| Maneuvers | **28** |
| Polyline chars | **489163** |
| Soft breaks | 4 Rest stop pins |
| DATEX on plan | `datex_impacts=5; datex_block=1; datex_penalize=4` + `datex_mwm=overlay_apply; active=5` |
| Attractions | 7 (`tourism-attraction`) |
| Tunnels | **3** — `Vinjadalen@210.39\|Arnavegen@77.83\|Nygårdstangen viadukt@22.69` |

First maneuver: turn left onto **Nysethvegen**. Last: destination at Bergen.
Waypoints summary: Fra **Fabrikkvegen** → Til **60.38811, 5.33386**.

### Tunnels

Measured on this Samsung MWM PASS (`PLAN_MWM_ROUTE` / `attempt_tunnel_fix`):

| Field | Value |
|---|---|
| `avoid_tunnels` | **false** |
| `route_tunnel_count` | **3** |
| `route_tunnel_fp` | `Vinjadalen@210.39\|Arnavegen@77.83\|Nygårdstangen viadukt@22.69` |
| Source | CoMaps turns JSON `"is_tunnel":` via `enrichMwmPlan` / arm64 `liborganicmaps.so` with quilt patch `0010-navi-tunnel-turns-json.patch` |
| Evidence | `RAUFOSS_DONE … tunnels=3`; device `report.json` `corridor_counts.tunnels=3` / `plan_assert.tunnels=3` |

### Navigation instruction types

| Kind | Count |
|---|---|
| roundabout | 19 |
| left | 2 |
| slight_left | 2 |
| straight | 2 |
| right | 1 |
| exit_left | 1 |
| destination | 1 |

### Ferries / tunnels / rest / attractions / wild camping

| Item | Count | Notes |
|---|---|---|
| Ferry legs | **0** | `route_ferry_legs=0` |
| Tunnels | **3** | `route_tunnel_count=3`; see **Tunnels** section |
| Rest places | **4** | soft-break Rest stop pins |
| Attractions | **7** | `attraction_by_type={"tourism-attraction":7}` |
| Wild camping sites | **0** | plugin off |

### Km / day (soft 6 h budget) and total

Total **428.530 km**. Soft daily cap 6.0 h as a proportional split of the
389.4 min ETA:

| Day | Drive hours | Km |
|---|---|---|
| 1 | 6.0 (cap) | ~395.9 |
| 2 | ~0.49 | ~32.6 |

### Fuel stops (report-only estimate)

Full-tank style estimate for 428.5 km with 100 km margin and 500–600 mi range:
**0** stops (`stops_at_500mi=0`, `stops_at_600mi=0`).

### DATEX (synthetic, ADB) — inject / apply / reroute / render / mid-sim

Cache: `files/datex_cache/` (`datex-GetSituation.xml`, `datex-cache.json` with
`source_fingerprint=navi-synth-…`, `apply_to_routing`). Host snaps sit
coordinates onto the corridor polyline (`naviMinutesBefore` is HUD band only).
See `docs/plugins/datex-plugin.md` § Campaign / ADB synthetic fixtures.

| Question | Answer | Evidence |
|---|---|---|
| Injected? | **yes** | 5 synth sits + `apply_to_routing`; host snap on corridor |
| Applied? | **yes** | `datex_impacts=5; datex_block=1; datex_penalize=4`; `datex_mwm=overlay_apply; active=5` |
| Reroute (plan)? | **mixed** | `datex_mwm=reroute_failed`; Block `syn-no-rv4-raufoss` status `unavoided`; before=after poly `c429fb22:len=489163`, 428.530 km |
| Rendered? | **yes** | `datex_render`: `drape_marks=5`, `compose_marks=1`, `camera_near_sit=true`, `ok=true`; shot `13b_datex_markers` |
| Mid-sim? | **yes** | `RAUFOSS_AWAIT_DATEX_MID` → host `syn-mid-block-sim` @ 60.80125,10.44703 (~18 min); `datex_mid_sim.rerouted=true`, `auto_reroute_count=1`; shots `14a`/`14b` |

| Id | Min before sit | Impact (host inject) | In 5–30 band |
|---|---|---|---|
| syn-no-rv4-raufoss | 12 | **Block** | yes |
| syn-no-rv33-dokka | 22 | Penalize | yes |
| syn-no-e16-ton | 28 | Penalize | yes |
| syn-no-rv7-hagafoss | 95 | Penalize | no |
| syn-no-rv7-mabo | 180 | Penalize | no |

Classifier note: hard **Block** requires
`RoadOrCarriagewayOrLaneManagement` + closed type; MaintenanceWorks stays
**Penalize**. Applied counters: block=1, penalize=4.

### Fallback handling (vehicle limits / catalog)

| Check | Result |
|---|---|
| Catalog mode | `comaps-release` |
| `catalog-cache.json` | present (~2.3 MB) |
| `vehicle_limits_coverage` in cache | yes |
| `comaps-direct` fallback marker | **not** seen |
| Installed leaves / sidecars | 9 / 9 @ 261105 |

### RAM

| Phase | MiB |
|---|---|
| Before | ~304.5 |
| Post plan | ~863.1 |

### Screenshots

Device `/sdcard/Pictures/raufoss-bergen-ui/`; host pull
`/tmp/raufoss-bergen-samsung/attempt_tunnel_fix/screenshots/device/`: launch,
tools, plugins, after UI-delete, Østlandet installed, car rest, From/To, DATEX,
before/after Plan, `13_plan_result`, `13b_datex_markers`,
`14a_datex_sim_before_inject`, `14b_datex_sim_after_reroute`.

### Product gap

No region download/process indicator on the main HUD; progress is only visible
inside Tools → Offline maps / MWM section.

---

## Root cause fixed (this PASS)

**arm64 `liborganicmaps.so` lacked `"is_tunnel":`** while x86_64 (emulator) had
it. Quilt patch `0010-navi-tunnel-turns-json.patch` was incomplete (helpers only,
wrong context after 0001–0009) so arm64 rebuilds never emitted tunnel tags and
Samsung always reported `route_tunnel_count=0`. Patch was completed (includes +
`IsHighwayTunnelType` / `IsTunnelSegment` + turns JSON wiring), arm64 organicmaps
rebuilt/staged, debug APK reinstalled (`6a9ab027…`), and hard JUnit assert
`tunnels >= 1` restored. This run: **tunnels=3**.

---

## Machine-readable report

Primary JSON: device `files/raufoss-bergen-ui/report.json` (host copy
`/tmp/raufoss-bergen-samsung/attempt_tunnel_fix/report/device_report.json`).
Host log: `/tmp/raufoss-bergen-samsung/attempt_tunnel_fix/logs/host_stdout.txt`.
Instrumentation: `/tmp/raufoss-bergen-samsung/attempt_tunnel_fix/logs/instrument_out.txt`
(`OK (1 test)`). Screenshots:
`/tmp/raufoss-bergen-samsung/attempt_tunnel_fix/screenshots/`. **Not** wired into
CI. Not committed.
