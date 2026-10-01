# CAT fixture sources

Fetch date: see [`FETCH_DATE.txt`](FETCH_DATE.txt) (UTC).  
Reproducible fetch: [`scripts/fetch-cat-fixtures.sh`](../../scripts/fetch-cat-fixtures.sh).

This tree holds **licence-clean** samples for CAT importer / auto-tune tests.
Callsigns and frequencies are taken from OpenStreetMap (ODbL) or from
synthetic AnyTone CPS shapes that reuse those OSM frequencies. No invented
RF parameters.

---

## OpenStreetMap

| | |
|---|---|
| **Access** | OSM API `0.6` (changeset metadata + download; element GET; relation full) |
| **Licence** | [ODbL 1.0](https://opendatacommons.org/licenses/odbl/) (OpenStreetMap contributors) |
| **May bundle** | Yes (attribution / share-alike for the database) |
| **App import** | Yes |
| **Cross-check only** | No |

### Contents

| Path | What |
|---|---|
| `osm/changeset_189693189.xml` + `_download.xml` | Changeset metadata + osmChange |
| `osm/changeset_189704408.xml` + `_download.xml` | Changeset metadata + osmChange |
| `osm/elements/` | Current versions of every touched repeater node/way (`nodes_current.xml`, per-id XML, `touched_ids.txt`) |
| `osm/la5mr/relation_18780801_full.xml` | LA5MR / Innlandsnettet `type=network` relation with members |
| `ham_shack_way_395284738.json` | Tags only for future ham-shacks doc (way 395284738) |

### Counts (changeset-touched current elements)

Derived from `osm/elements/nodes_current.xml` + way fixture (modulation tags):

| Class | Approx. count | Notes |
|---|---|---|
| FM / NFM (`11K2F3E`) | 16 nodes + 1 way | Includes networked and non-networked |
| DMR-capable (`7K60FXE` / `F7W`) | 2 nodes | LA2DRR, LA7KR |
| APRS / digipeater (`20K0F2D`, often 144.8) | ~27 nodes | Excluded from auto-tune / `non_networked.json` |

Coordinate quality: **node GPS from OSM** (survey/imported; treat as best available offline position).

### Callsigns in changesets 189693189 / 189704408

**189693189** (create/modify):  
LA2DRR, LA2HRR, LA2JRR, LA2XRR, LA5JRR, LA5KR, LA6ZR, LA7GRR, LA7KR, LA7XR,
LD2ER, LD2FN, LD2GB, LD2GD, LD2GF, LD2GG, LD2GI, LD2GO, LD2GP, LA2L-10, LD2GR,
LD2GS, LD2GU, LD2JB, LD2KF, LD2KR, LD2KS, LD2LH, LD2LK, LA7RRA-B, LD2TR, LD2WA,
LD2WB, LD2WC, LD2WD, LA5ARR, LA5TRR, LA7GR, LA5HRR, LD2HG, LD2GN, LA9AR (way),
LA5MR, LA6GR, LA2TRR, LA2RRR.

**189704408**: LA2DRR (modify; frequency tag normalisation).

---

## OpenRepeater (openrepeater.org)

| | |
|---|---|
| **Access** | Public download API `GET /api/downloads?country=Norway&format=json\|csv` (no API key). Authenticated `/api/v1` not used. |
| **Licence / terms** | Site advertises repeater data as **CC0 1.0**. Terms also state: *“You may not resell or commercially redistribute the database without written permission.”* |
| **May bundle** | **Not now** — Norway country export returned **count=0** (no Innlandet rows to bundle). If coverage appears later, prefer CC0 export; still respect the commercial-redistribution clause for product packaging. |
| **App import** | Allowed in principle for CC0 downloads when coverage exists; user/runtime import preferred over shipping empty extracts. |
| **Cross-check only** | Empty Norway attempt kept under `openrepeater/` as proof of fetch. |

See `openrepeater/FETCH_NOTE.json`, `norway_attempt.json`, `norway_attempt.csv`.

---

## RadioID (radioid.net)

| | |
|---|---|
| **Access** | API / dumps documented at https://radioid.net/api/ |
| **Licence / terms** | **Do not** mirror or re-publish as a bulk export, reusable DB, competing directory, or commercial data service without written permission. Normal individual lookup allowed; commercial/bulk/public redistribution requires approval. |
| **May bundle** | **No** (no written permission) |
| **App import** | **No** as a redistributed onboard extract until approval |
| **Cross-check only** | Policy excerpt only: `radioid_POLICY_EXCERPT.txt`. **No DMR data committed.** |

Innlandet bbox extract: **not fetched**.

---

## RepeaterBook

| | |
|---|---|
| **Access** | Disabled in Navi until written API permission (see `docs/CAT.md`) |
| **May bundle / app import** | **No** |
| **Data committed** | **None.** Sync stays off; fixture fetch does not contact repeaterbook.com. |

---

## repeatermap.de

| | |
|---|---|
| **Access** | Public map/search site (DK3ML) |
| **Licence** | FAQ states city-search uses Simplemaps **CC BY 4.0**. **Amateur repeater data is from the repeatermap database and is not part of that CC BY set.** No clear redistribution licence for the repeater DB was found. |
| **May bundle / app import** | **No** |
| **Cross-check only** | **Yes** — manual UI cross-check only. **No repeatermap.de data committed.** |

---

## AnyTone CPS CSV (`anytone/`)

| | |
|---|---|
| **Access** | Hand-built sample shaped like CPS `channel` / `zone` / `gps-roaming` / `offset` exports (`docs/CAT.md`) |
| **Licence** | Sample structure: project test data. **Frequencies/callsigns from OSM fixtures only** (ODbL). |
| **May bundle** | Yes (as test fixtures) |
| **App import** | Yes (represents user-owned CPS export) |

Files:

- `channel.csv` — UTF-8; includes APRS rows, simplex `Channel VFO A`, transliterated `Mjoesa LA5MR`, DMR LA2DRR split across TG/slot rows
- `channel_windows1252.csv` — same rows, Windows-1252
- `zone.csv`, `gps-roaming.csv`
- `offset.csv` — header only (empty body)

---

## `non_networked.json`

FM + DMR sites within ~150 km of the Espa→Dombås corridor that are **not**
members of any `type=network` relation in these fixtures (LA5MR / 18780801),
APRS excluded. Built from OSM fixtures only (OpenRepeater Norway empty;
RadioID not redistributable).

---

## dump_caps (`dump_caps/` and `navi-cat/tests/fixtures/dump_caps/`)

Synthetic Hamlib `\dump_caps` texts for parser tests: Stable, Beta,
missing required lines, Alpha, Untested. Not from a live radio.

---

## navi-server / published packs

Read-only check on **2026-10-01** (this machine). **navi-server was not modified.**

| Location | Result for `communication:amateur_radio:repeater` |
|---|---|
| `core/target/integration-fixtures/ostlandet-latest.osm.pbf` | **Partial** — tag present; osmium/pyosmium scan found **21** hits including relation **18780801** (LA5MR) and several member sites. Pack dated ~2025-07; missing many nodes created in changesets 189693189/189704408; some member frequencies differ from current OSM. |
| `core/target/integration-fixtures/espa-atnbrufossen-corridor.osm.pbf` | **Partial** — **6** hits + relation 18780801 |
| `core/target/integration-fixtures/europe_norway_ostlandet.pmtiles` | **Partial** — plain-string tag key present in the basemap archive (POI/vector layers may carry amateur-radio tags; not a complete repeater DB) |
| Device / published navi-server pack trees | No separate live device pack tree with a complete repeater bake found beyond the integration-fixtures above |

**Verdict: partial.** Client tests should prefer local OSM fixtures / PBF extract when server packs lack complete current repeater tags. Future server-side bake is out of scope for the CAT branch (do not change navi-server here).

---

## Summary licence decisions

| Source | Decision |
|---|---|
| OSM | Bundle + app import (ODbL) |
| OpenRepeater | CC0 claimed; Norway export empty → nothing to bundle; watch commercial-redistrib terms |
| RadioID | Document only; no data |
| RepeaterBook | Disabled; no data |
| repeatermap.de | Cross-check only; no data |
| AnyTone samples | Bundle as fixtures; RF from OSM |
| dump_caps | Synthetic; bundle |
