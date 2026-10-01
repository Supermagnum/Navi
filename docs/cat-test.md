# CAT test report and corridor evidence

Product / safety specification: [`CAT.md`](CAT.md).  
Fixture licences and fetch notes: [`testdata/cat/SOURCES.md`](../testdata/cat/SOURCES.md).  
Plugin overview: [`plugins.md`](plugins.md).

This file holds **CAT branch test results**, fixture-check outcomes, importer
coverage, CSV import path / format notes, and the Espa→Dombås repeater-switch
expectations. It does **not** treat Hamlib dummy-rig (`rigctld -m 1`) runs as
product route evidence (dummy is CI-only; see [Out of scope](#out-of-scope)).

Fetch date for committed fixtures: see
[`testdata/cat/FETCH_DATE.txt`](../testdata/cat/FETCH_DATE.txt).

---

## Corridor under test

| | |
|---|---|
| Route | Espa → Dombås |
| Espa | ~60.563, 11.257 |
| Dombås | ~62.076, 9.128 |
| Great-circle length (fixture path) | ~203 km |
| Non-networked fixture | [`testdata/cat/non_networked.json`](../testdata/cat/non_networked.json) |
| Network fixture | OSM relation [18780801](https://www.openstreetmap.org/relation/18780801) (`LA5MR`) — [`testdata/cat/osm/la5mr/relation_18780801_full.xml`](../testdata/cat/osm/la5mr/relation_18780801_full.xml) |

Follow hysteresis (product defaults in `plugins/CATS-plugin/src/follow.rs`):

| Rule | Value |
|---|---|
| Closer margin | **5 km** or **20 %** of current distance (whichever is larger) |
| Minimum dwell on current site | **30 s** |
| Minimum gap between switches | **60 s** |

Assumptions for the expected switch logs below: great-circle path Espa→Dombås,
constant **80 km/h**, sample every **0.5 km**, sites within **150 km**, NFM /
`11K2F3E` only. This is **fixture-derived expected behaviour**, not a live drive
log and not a dummy-rigctld run.

**Site identity note:** several LA5MR member nodes share callsign `LA5MR`.
Expected network-follow switches below key sites by **OSM node id** (and optional
name). Callsign-only identity would collapse those members.

---

## Run A — non-networked repeaters (Espa → Dombås)

**When / why the plugin would change site:** auto-tune picks the nearest usable
NFM site within 150 km that is **not** in a `type=network` relation. Along the
route the nearest site changes; applying the same hysteresis / dwell / gap rules
as follow mode avoids flapping when two sites are nearly equidistant.

Fixture: [`non_networked.json`](../testdata/cat/non_networked.json) (18 entries;
APRS excluded at build time). FM candidates used for the log below are the
`modes: ["FM"]` rows.

### Candidate sites (along-route order)

| Along GC (km) | Off-route (km) | Callsign | Lat, lon | Frequency |
|---:|---:|---|---|---|
| 0.0 | 55.2 | LA2XRR | 60.32422, 12.13833 | 145.7125 |
| 0.0 | 55.2 | LD2KR | 60.32422, 12.13833 | 434.5625 |
| 0.0 | 59.8 | LA2RRR | 60.16761, 11.99439 | 51.83 |
| 0.0 | 59.8 | LA5KR | 60.16761, 11.99439 | 145.65 |
| 0.0 | 81.4 | LA7XR | 59.96090, 12.09532 | 144.6625 |
| 10.5 | 13.9 | LA6GR | 60.56958, 10.93798 | 434.75 |
| 12.6 | 34.8 | LA5MR | 60.47817, 10.60182 | 145.625 |
| 41.9 | 88.8 | LA5TRR | 61.31938, 12.19015 | 145.725 (OSM node 5576656903; **not** relation member — distinct from LA5MR member 5576656416) |
| 75.7 | 40.7 | LA5HRR | 61.33200, 11.09416 | 145.65 |
| 91.4 | 33.1 | LA6ZR | 61.07699, 9.78839 | 145.75 |
| 94.3 | 2.8 | LA5ARR | 61.28031, 10.31136 | 434.775 |
| 131.2 | 5.5 | LA7GR | 61.51386, 9.79466 | 145.725 |
| 181.8 | 4.1 | LA2JRR | 61.89757, 9.28445 | 145.6625 |
| 181.8 | 4.1 | LA7GRR | 61.89757, 9.28445 | 434.8 |
| 187.9 | 60.2 | LA5JRR | 61.66131, 8.33539 | 145.7875 |
| 203.0 | 26.2 | LA2HRR | 62.21251, 9.53892 | 145.7 |

### Expected switch log (fixture-derived)

| Route km | Elapsed @ 80 km/h | Tuned site | Dist to site (km) | Reason |
|---:|---:|---|---:|---|
| 0.0 | 0 s | LA6GR | 17.4 | Initial nearest at Espa |
| 54.0 | ~2430 s | LA5ARR | 40.5 | From LA6GR; closer by 5.5 km (cur 46.0) — absolute 5 km margin |
| 115.5 | ~5197 s | LA7GR | 16.7 | From LA5ARR; closer by 4.7 km (cur 21.3) — **20 %** margin (4.26 km) |
| 159.0 | ~7155 s | LA2JRR | 23.1 | From LA7GR; closer by 5.1 km (cur 28.2) |

---

## Run B — LA5MR networked repeaters (same route)

**When / why the plugin would switch:** with network follow enabled for
`LA5MR` / relation 18780801, only **member** NFM sites are candidates. A switch
fires when another member is nearer by the hysteresis margin, dwell ≥ 30 s, gap
≥ 60 s, and host interlocks (PTT off; DCD if available) pass. Pinning suspends
follow.

### LA5MR usable NFM members (fixture)

| Along GC (km) | Off-route (km) | Site | Lat, lon | Frequency |
|---:|---:|---|---|---|
| 0.0 | 51.5 | LA5MR (node 1011896359) | 60.22987, 10.60662 | 145.6375 |
| 36.8 | 1.7 | LA5MR (node 1473491257) | 60.84576, 10.89618 | 145.625 |
| 41.9 | 88.7 | LA5TRR (node 5576656416) | 61.31951, 12.18899 | 145.725 |
| 65.5 | 99.8 | LA6NR (node 5576442558) | 60.53668, 9.06172 | 145.775 |
| 72.5 | 57.7 | LA5MR / Bagn hovedsender (2641537344) | 60.80862, 9.61589 | 145.625 |
| 73.0 | 19.6 | LA2TRR (node 5576506182) | 61.00756, 10.19180 | 145.2375 |
| 110.8 | 72.8 | LA5MR (node 5576503609) | 61.01846, 8.97393 | 145.625 |
| 155.1 | 91.8 | LA5MR (node 12635462528) | 61.25322, 8.20304 | 145.625 |

Incomplete member node 5576656337 (no frequency / modulation) is omitted.
Linking ways in the relation are not tune targets.

### Expected switch log (fixture-derived)

| Route km | Elapsed @ 80 km/h | Tuned site | Dist to site (km) | Reason |
|---:|---:|---|---:|---|
| 0.0 | 0 s | LA5MR (node 1473491257) | 37.1 | Initial nearest network member at Espa |
| 63.5 | ~2857 s | LA2TRR (node 5576506182) | 21.7 | From 1473491257; closer by 5.1 km (cur 26.8) |
| 169.0 | ~7604 s | LA5MR (node 12635462528) | 92.8 | From LA2TRR; closer by 5.3 km (cur 98.1) |

---

## Fixture / database fetch checklist

Outcomes from [`SOURCES.md`](../testdata/cat/SOURCES.md) and
[`scripts/fetch-cat-fixtures.sh`](../scripts/fetch-cat-fixtures.sh)
(recorded 2026-10-01 unless noted).

| Source | Result | Notes |
|---|---|---|
| OSM changesets 189693189 / 189704408 | **PASS** (bundled) | Callsigns LA2DRR … LA5TRR, LA5MR, … — see SOURCES |
| OSM LA5MR relation 18780801 full | **PASS** (bundled) | Network members + site tags |
| OSM element current snapshots | **PASS** (bundled) | Under `testdata/cat/osm/elements/` |
| `non_networked.json` | **PASS** (bundled) | 18 FM/DMR corridor sites; APRS excluded |
| OpenRepeater Norway download | **Empty** (count=0) | Proof under `openrepeater/`; nothing to import |
| RadioID | **Blocked** (policy) | Excerpt only; no DMR data committed |
| RepeaterBook | **Disabled** | No contact; no data |
| repeatermap.de | **Cross-check only** | No data committed |
| AnyTone CPS samples | **PASS** (bundled) | `testdata/cat/anytone/` — UTF-8 + Windows-1252 channel files |
| dump_caps fixtures | **PASS** (bundled) | Stable / Beta / missing / Alpha / Untested |
| navi-server / pack PBF / pmtiles | **Partial** | ostlandet fixture PBF: 21 hits incl. relation 18780801; incomplete vs current OSM; client prefers local fixtures |
| Hamlib Android lock | tag **4.7.2**, NDK **30.0.14904198** | `scripts/hamlib-android.lock` |
| Geofabrik (Elsa / long-route packs) | Soft-PASS campaign notes | Trailing-slash dated-URL retry; oppland/hedmark → ostlandet; Sweden PBF filename dedupe; `place_index_skipped=1` in soft-PASS logs |
| Elsa long-route (prior campaign) | Recorded | 8 corridor regions auto-downloaded, Via `(none)`, no forced packs — not CAT radio evidence |

### Automated importer coverage (added on CAT)

| Test | What it asserts |
|---|---|
| `navi-cat` `import_fixtures` (UTF-8 `channel.csv`) | **PASS** — 7 sites; APRS + simplex out; DMR TG rows deduped |
| `navi-cat` `import_fixtures` (Windows-1252 `channel_windows1252.csv`) | **PASS** — same count after decode |
| `navi-cat` `import_fixtures` (`non_networked.json`) | **PASS** — 18 entries; LA6GR queryable near Espa |
| `navi-cat` `import_fixtures` (OpenRepeater Norway empty) | **PASS** — `{count:0, repeaters:[]}` → 0 rows |
| `navi-cat` `import_fixtures` (`offset.csv` header-only) | **PASS** — empty body accepted |
| Unit: `parse_mhz("-0,6 Mhz")` | **PASS** |
| Unit / scenario 12 | **PASS** — `import_repeaterbook` always errors |

Run: `cargo test -p navi-cat --test import_fixtures` (and existing
`--test scenarios`).

---

## Scenario suite summary (`navi-cat/tests/scenarios.rs`)

Desktop / CI coverage for CAT.md scenarios 1–12 (MockRig / fixtures — **not**
dummy radio product-route claims):

| # | Scenario | Coverage |
|---|---|---|
| 1 | LA5MR network query | Network filter returns member |
| 2 | Non-networked + no APRS | Query excludes APRS |
| 3 | Single FM full read-back | `program_vfo1_verified` |
| 4 | Non-networked DMR import/dedupe | Dedupe key; no program unless profile can read back |
| 5 | Cross-source conflict flags | Conflict note preserved |
| 6 | Filter APRS / simplex / CSV-only no position | Auto-tune set excludes them |
| 7 | Gating parser fixtures | Stable / Beta / missing / Alpha |
| 8 | Error paths | Gate refuse, PTT block, mismatch retry |
| 9 | Never transmit | No set-PTT / `T` |
| 10 | Server-file vs OSM | Prefer fixtures when packs partial |
| 11 | Sandbox boundary | Guest cannot skip read-back / ungated program |
| 12 | RepeaterBook off | Importer errors; no network dependency |

Plugin unit tests: `navi-plugin-cats` select / follow hysteresis.  
JVM: `CatSerialLoopbackBridgeTest` (fake USB/BT → loopback TCP; remote endpoint
`10.0.2.2:4532`). Real USB OTG + Bluetooth SPP read-back on a physical
transceiver remains a **manual** field check after verifying Hamlib backend
status for that model.

---

## CSV import path

| Role | Path |
|---|---|
| **On device (Android)** | `{Context.filesDir}/cat/import/` — typically `/data/user/0/no.navi.app/files/cat/import/` |
| **Onboard DB** | `{Context.filesDir}/cat_repeaters.sqlite` |
| **Repo fixtures** | [`testdata/cat/anytone/`](../testdata/cat/anytone/) |

The app creates `cat/import/` at CAT bootstrap (`CatBootstrap`). Copy AnyTone CPS
exports into that directory (`channel.csv`, optional `zone.csv`,
`gps-roaming.csv`, `offset.csv`). The CAT status sheet shows this path hint.

Encoding: **UTF-8** or **Windows-1252**; newlines **CRLF** or **LF**.

---

## AnyTone CPS CSV format

Aligned with importers in `navi-cat/src/importers.rs` and samples under
`testdata/cat/anytone/`. Spec summary also in [`CAT.md`](CAT.md#anytone-cps-csv-exports).

### `channel.csv`

One row per channel. Key columns:

| Column | Role |
|---|---|
| Channel Name | Display / match key (max 16 chars in CPS) |
| Receive Frequency / Transmit Frequency | MHz |
| Channel Type | `A-Analog` or `D-Digital` (DMR) |
| Band Width | `12.5K` / `25K` |
| CTCSS/DCS Encode / Decode | Analog access tones |
| Color Code / Slot (and Contact TG) | DMR |
| APRS RX and related APRS columns | Non-empty / On → **exclude** row |

No coordinates in `channel.csv` → CSV-only sites are **not** distance auto-tuned
until cross-referenced to OSM / other positioned sources.

**Filtering rules**

- Exclude channel names containing `APRS`, or APRS columns set.
- Exclude **simplex** (`RX == TX`), e.g. `Channel VFO A`.
- DMR (`D-Digital`): one physical repeater often has several TG/slot rows →
  dedupe by **RX + TX + color code**. Analog channels are not collapsed when
  they share RX/TX (different callsigns / networks).

**Name patterns:** transliteration / truncation (`Mjoesa`, `Toensberg`,
`Kr.sund N`); analog `Town CALLSIGN`; DMR often `Town 242` / `Town lokal`
without callsign.

### `zone.csv`

Zones with pipe-separated member names and RX/TX lists. Names such as
`ANALOG INNLANDET` are a **region hint**, not a precise lat/lon.

### `gps-roaming.csv`

Roaming points as degrees + minutes with N/S and E/W flags and radius km.
All-zero rows mean unused.

### `offset.csv`

May be **header only** (empty body); importers must still accept the file.

Fixture samples: `channel.csv` (UTF-8), `channel_windows1252.csv` (CP1252),
`zone.csv`, `gps-roaming.csv`, `offset.csv`.

---

## Branch test report (summary)

| Item | Result |
|---|---|
| Hamlib lock | tag **4.7.2**, NDK **30.0.14904198** |
| OSM / LA5MR / non_networked fixtures | Bundled; see SOURCES |
| Server-file repeaters | **Partial** (21 hits in ostlandet fixture PBF) |
| OpenRepeater Norway | Empty export |
| RadioID / RepeaterBook / repeatermap | Policy / disabled; no redistributed data |
| Desktop unit tests | `navi-cat` gating/program/repeater + scenarios 1–12 + **import_fixtures**; `navi-plugin-cats` select/follow |
| JVM bridge | Loopback bridge unit tests green |
| Emulator / hardware | Remote `10.0.2.2:4532` path documented; physical USB/BT still required for field confirmation |
| Corridor switch logs | Fixture-derived Run A / Run B above |

Plugin guest logic: `plugins/CATS-plugin/`. Host radio safety: `navi-cat`.

---

## Out of scope

Hamlib **dummy** rig (`rigctld -m 1`) is used only for CI gating / protocol
plumbing. Those runs are **not** product CAT route results and are not tabulated
here.
