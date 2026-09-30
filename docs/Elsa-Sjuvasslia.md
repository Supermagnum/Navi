# Elsa's caravan & galleri (Bugøynes) → Sjuvasslia Camping

Date: **2026-09-30**. Branch: **`right-to-roam`**. Base SHA: **`f9b335f6`**
(plus uncommitted Geofabrik trailing-slash fix, southbound E6 densify spine,
and leftover-foreign-pack spine gate).

**Canonical role:** one-shot MobileHome long-trip UI campaign on Automotive AVD
`Navi_8c_4G_128G`. Real region packs; synthetic DATEX Blocks via host ADB only.
Not wired into CI.

Instrumented runner:

- `LongTripMobileHomeElsaSjuvassliaUiCampaignTest` — plan via visible Compose UI
  (From / To / settings / Plan). No forced vias, bridges, or ferries.

---

## Verdict — OUTSIDE EXPECTED BAND (plan found)

| Metric | Result | EXPECTED | OK |
|---|---|---|---|
| Distance | **2700.3 km** | 1800–2300 km | no |
| Driving time | **~41.4 h** (2484.3 min) | 20–31 h | no |
| Instructions | **61** | 150–170 | no |
| DATEX applied | **yes** (max impacts 30; blocks up to 19 on a leg) | applied | yes |
| Plan | **found** (`ui_planned=true`, 30 chunk legs, MobileHome/Truck) | found | yes |

`PASS_UI` fired (plan + polyline + DATEX), but `expected_check` distance /
duration / maneuvers are all false. Maneuver count lands in the Bevensen-class
thin band (55–100) after `thin_route_maneuvers`, not the Elsa 150–170 band.

---

## Fixes applied this session (before successful plan)

1. **Geofabrik extract URL** — `geofabrik_latest_pbf_url` must end with `/`
   (slashless path redirects and failed Nord-Norge place-index extract).
   Østlandet PBF then downloaded successfully (~455 MB).
2. **Southbound E6 densify spine** — `norway_e6_spine_anchors` previously
   required northbound `dlat >= 4°`, so Bugøynes→Sjuvasslia chorded Finnmark
   plateaus and failed `chunk_leg9` (`disconnected` / `bbox_exhausted`).
   Spine now runs both directions (reversed for southbound).
3. **Leftover foreign Ready packs** — Västra Götaland leftover from an earlier
   campaign disabled the Norway-only spine gate. Gate now keys off Norway OD
   endpoints only (Hamar→Minden still excluded: Minden is outside the NO box).
4. **DATEX host inject** — `run-as … sh -c` resets cwd to `/`; inject now uses
   `run-as mkdir -p files/datex_cache` then `cp` without `sh -c`.
5. **Emulator clock stall** — device wall clock lagged ~12 min and froze download
   progress; synced from host before resume.
6. **Place-index SIGBUS** — `clearPlaceIndexRegionRows` during UI region delete
   hit SIGBUS on a corrupted `place_index.db`; wiped DB and re-ran.

---

## Test setup

| Item | Value |
|---|---|
| Emulator | `Navi_8c_4G_128G` (`emulator-5554`) |
| Origin | Elsa's caravan & galleri, Bugøynes 69.9741435, 29.6337571 (elev 4 m) |
| Destination | Sjuvasslia Camping 59.803175, 9.397871 |
| Departure | `2026-06-01T08:00:00` |
| Profile | MobileHome / Truck + VW T6 camper limits |
| Avoid tolls | ON |
| Ferries | use (allowed); **route_uses_ferry=true** on at least one chunk |
| Eco | requested on |
| Soft daily budget | 6.0 h |
| Soft break | 1.5 h / 15 min |
| Wild camping / long trip / DATEX / nearby attractions | on |
| Corridor regions | Nord-Norge → Trøndelag → Østlandet (**Norway only**) |
| Densify | `long_trip_chunked=true; hops=30; chunk_deg=1.15` (E6 spine) |

### ECO-off reference vs campaign corridor

External ECO-off quickest reference (`geojson-routes/elsa-sjuvass.geojson`):
**~1944 km**, same OD. That polyline runs **through Pajala (Finland) and Umeå
(Sweden)** — the fastest road corridor between Bugøynes and Sjuvasslia leaves
Norway in the north and re-enters via Sweden. An ECO-on fair path would differ
from that file; the Finland/Sweden transit remains the geographic baseline.

| Path | Distance | Corridor packs |
|---|---|---|
| Campaign (ECO requested; E6 Stay-in-Country densify) | **2700.3 km** | Nord-Norge, Trøndelag, Østlandet only |
| ECO-off reference GeoJSON | **~1944 km** | needs FI + northern SE as well |

**Relevant regions were not downloaded** for the Finland/Sweden transit:

- Campaign corridor / densify stayed on the Norway E6 spine (southbound fix),
  so long-trip only fetched Norwegian landsdel packs.
- Catalog PIP for Pajala / Haparanda currently falls inside the oversized
  `europe/norway/nord-norge` bbox (`[64.5, 10.0, 71.5, 31.5]`), so adjacency
  does not demand Finland or Norrbotten leaves even when a fair path is there.
- Pack leaf catalog for Sweden stops at Skåne / Halland / Västra Götaland;
  `europe/finland` exists as a country extract, but neither Finland nor
  northern Sweden was on the Installed corridor list for this run.

Campaign excess vs ECO-off reference ≈ **2700 − 1944 ≈ 756 km**, dominated by
forced Norway-only densify + avoid-tolls / DATEX / MobileHome costs — not by
missing Puttgarden (that ferry is on the Bevensen corridor only).

---

## Memory / timing

| Sample | TOTAL PSS |
|---|---|
| Peak during Ostlandet re-fetch / index | **~1513 MiB** |
| Post-plan | **~756 MiB** (report `ram_post_plan_mib`) |
| Final | **~627 MiB** |

Download + plan wall time: **~448 s** after UI setup (packs mostly warm;
Østlandet re-downloaded after UI delete).

---

## Overshoot / miss notes

| Rank | Driver | Evidence |
|---|---|---|
| 1 | Norway-only corridor vs FI/SE fair path | ECO-off reference ~1944 km via Pajala + Umeå; campaign downloaded only Nord-Norge/Trøndelag/Østlandet (~756 km longer). |
| 2 | Instruction band mismatch | Thinned to **61** (Bevensen 55–100 class). Elsa EXPECTED **150–170** was not retuned (forbidden). |
| 3 | Duration follows distance | ~41 h vs 20–31 h EXPECTED. |
| 4 | Avoid-tolls ON + DATEX blocks | Expected product cost vs a toll-OK fair path; up to 19 DATEX blocks on a leg. |

---

## Artifacts

- Device report: `files/long-trip-elsa-sjuvasslia/report.json`
- Host pull: `/tmp/elsa-sjuvasslia-host/report.json`
- Screenshots: `/sdcard/Pictures/elsa-sjuvasslia-ui/`
- Gradle: `/tmp/elsa-sjuvasslia-campaign5.log` (BUILD SUCCESSFUL)
