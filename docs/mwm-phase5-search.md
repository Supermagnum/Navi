# Phase 5 - offline search on MWMs

Status: device-accepted on Samsung `R52TB0JQEDE` (search suite + cancel +
off-road). See acceptance notes and expectation audit below.

## Inventory

### What CoMaps gives us (pin `v2026.11.07-16`)

| Piece | Used for |
|-------|----------|
| `search::Engine` | Async free-text and category search over registered MWMs; cancellable via `ProcessorHandle` |
| `search::Result` / `Results` | Name, address, localized type, centre point; results arrive as a growing array (append only) |
| `search::ReverseGeocoder` | Nearest exact address (`GetNearbyAddress`), nearest street fallback (`GetNearbyStreets`) |
| `search::DisplayedCategories` | 23 category keys (`category_fuel`, `category_water`, ...); `ForEachSynonym` gives per-locale query strings |
| `IndexRouter::FindClosestProjectionToRoad` | Nearest-road snap (routing side, see `docs/mwm-phase4-parity.md` A2) |

Required data: `categories.txt` and `categories_brands.txt` (staged by
`scripts/stage-comaps-data-assets.sh`; the brands file is new in this phase
because the ranker loads it), `countries.txt` + `packed_polygons.bin` (already
staged), and per-MWM search index sections plus `World.mwm` for cities and
countries.

### Shim and Rust surface

C ABI (`native/comaps_shim/navi_comaps.h`):

- `navi_comaps_snap_to_road` / `navi_comaps_free_snap`
- `navi_comaps_set_search_locale`, `_set_search_viewport`, `_set_search_position`
- `navi_comaps_search` (async, batches via callback), `_search_cancel`,
  `navi_comaps_free_search_result`
- `navi_comaps_category_keys`, `navi_comaps_category_query`, `navi_comaps_free_string`
- `navi_comaps_reverse_geocode` / `navi_comaps_free_address`

Rust: `core/src/mwm/comaps_search.rs` (types, category mapping, merge, native
wrapper), `core/src/mwm/comaps_snap.rs` (snap suggestion). UniFFI
(`navi-ffi/src/mwm_comaps_ffi.rs`): `comaps_search_enabled`,
`comaps_search_start` (listener `FfiSearchListener`), `comaps_search_cancel`,
`comaps_search_set_*`, `comaps_category_keys`, `comaps_category_query`,
`comaps_quick_category_key`, `comaps_poi_category_mappings`,
`comaps_search_merge`, `comaps_reverse_geocode`, `comaps_session_snap_suggestion`.

### Data source

`RoutesBuilder` keeps a pool of `FrozenDataSource` objects that is borrowed per
route (and exposes no accessor), so a long-lived `search::Engine` cannot share
it. The shim therefore opens **its own `FrozenDataSource`** over the same MWM
files (`FindAllLocalMapsAndCleanup`, all leaves plus `World`, not
`WorldCoasts`). MWMs are memory-mapped, so the extra registry costs address
space, not a second copy of the data. The engine is created lazily on the first
search or reverse geocode and dropped by `navi_comaps_register_mwms()` (after a
download) and `navi_comaps_shutdown()`; the next call re-registers. Search uses
one engine thread; the `RoutesBuilder` thread count is unchanged.

### Async and cancellation

`navi_comaps_search` returns at once with a request id. Batches contain only the
hits appended since the previous batch (non-point and suggest results are
skipped); the final batch has `is_final = 1` and `cancelled = 1` when the
request was cancelled or timed out (engine default 8 s). `navi_comaps_search_cancel(id)`
(0 = all) cancels the `ProcessorHandle`. Callbacks run on the search worker
thread; the Rust trampoline copies the batch, frees it and drops the listener
on the final batch. A request that is still queued when the engine is torn down
does not receive a final batch and leaks its small listener box.

### Navi catalogs and merge

`NaviPoiSource` is a read-only trait; `PoiIndexSource` wraps the OSM POI index,
`StaticPoiSource` wraps any in-memory list (CAT repeaters, saved places).
`merge_hits` rules:

- Source labels: `comaps` or `navi:<catalog>`.
- A Navi hit with the same normalized name within 100 m of an existing hit is
  folded into it (`merged_sources`), never silently dropped.
- Text mode: CoMaps order first; Navi-only exact-name matches go to the front
  unless CoMaps already has an exact match; other Navi-only hits follow by
  distance.
- Category mode: everything by distance when known.
- Storage of the catalogs is untouched.

### Flag behaviour

`comaps_search_enabled()` is true only with the native core linked **and**
`NAVI_MWM_CLIENT_ENABLED` on. Every search export returns an empty or
"unavailable" result otherwise, and the app keeps using the existing
`place_index` FTS search (`core/src/search`). The `.rkyv` path is not modified.
The app's own search UI is **not** switched over in this change; the Kotlin
side still has to call `comapsSearchStart` and `comapsSearchMerge` (see open
items).

## Category mapping (Navi `PoiCategory` to CoMaps)

Checked against `data/categories.txt` at the pin.

| Navi category | CoMaps key | Coverage | Note |
|---------------|-----------|----------|------|
| Water | `category_water` | full | springs and drinking water |
| Cabin | `category_hotel` | partial | alpine/wilderness huts share the hotel group |
| OvernightFacility | `category_hotel` | partial | huts, camp sites, hotels |
| TentSite | `category_hotel` | partial | camp_site in hotel group; caravan sites are `category_rv` |
| Lodging | `category_hotel` | full | hotel / motel / guest house |
| Restroom | `category_toilet` | full | `amenity=toilets` |
| General | none | **gap** | no group |
| NetworkHut | none | **gap** | DNT/network membership is not in MWM data |
| CraftBrewery | none | **gap** | `craft=brewery` has no group; free text only |
| Fishing | none | **gap** | not a group |
| RestArea | none | **gap** | `highway=rest_area` / `services` have no group (`category_parking` is a weak stand-in) |

Quick chips outside `PoiCategory`: fuel -> `category_fuel`, EV charging ->
`charging_station` (a group in `categories.txt` that is not in the displayed
list; `ForEachSynonym` reads it fine), water, toilet, parking, hotel, RV, food,
pharmacy, hospital.

Gaps and consequences:

1. Navi-only categories (NetworkHut, General, Fishing, RestArea, CraftBrewery)
   are answered by the Navi POI catalog only; the merge labels them `navi:poi`.
2. Partial groups return extra types (a "cabin" query via `category_hotel`
   also returns hotels); filter on `feature_type` if a strict cabin list is
   needed.
3. CoMaps has no shelter group. `gapahuk` / `ly` are free-text queries in the
   suite and may rank poorly.
4. Category strings come from `categories.txt` per locale with an English
   fallback; Norwegian coverage varies per group.

## Search suite

`core/tests/fixtures/mwm/search-suite-ostlandet.json` (copied to
`app/src/androidTest/assets/`): 43 queries - towns (6), streets (4), addresses
(3), postcodes (3), cabins (4), shelters (2), water (3), fuel (3), EV (3),
misspellings (4), æøå (3) and 5 reverse points. Each row has an
`expect_top_any` / `expect_address_any` / `min_results` expectation and a
`confidence`. The expected names were authored from OSM knowledge and **have not
been confirmed against the 261105 MWMs**; `medium` rows are reported but do not
gate.

Host checks (no engine): fixture shape, category mapping, merge with Navi POI
sources, parity-suite `expect` values (`mwm_phase5_fixtures`,
`mwm::comaps_search`, `mwm::comaps_snap`).

Device: `ComapsPhase5SearchInstrumentedTest` skips unless the native core is
linked, the flag is on and `World.mwm` plus at least one leaf are present under
`navi-mwm-test/261105`. It runs the suite (gate: >= 80% of `high` rows),
cancellation, and the off-road snap flow.

## Acceptance notes (Samsung `R52TB0JQEDE`, 2026-11-15)

- Phase 5 instrumented class green: `offroadEndpointIsNotAutoSnapped`,
  `searchIsCancellable`, `runSearchSuite` (high-confidence **21/21**).
- Multi-query SEGV root cause: `CountryInfoGetter::m_affiliations` was null
  (`SetAffiliations` never called). Fixed in `EnsureSearch` by loading
  `storage::Storage` affiliations (same as `TestSearchEngine::InitAffiliations`).
- Unbound To-field queries use Everywhere (`navi_comaps_clear_search_viewport`);
  map-biased search keeps Viewport. Suggest hits with a point are kept (exact
  town queries often surface the city as a Suggest).
- Medium misses left documented: a few reverse-geocode pins land on a parallel
  street; one address row ranks the city above the house number.
- Off-road choice flow: first route returns `END_NOT_ON_ROAD` with an empty
  polyline (never auto-snaps). Snap suggestion still requires confirm/cancel.
- Staging ships `subtypes.csv`, `countries_meta.txt`, and nb/en
  `countries-strings/*/localize.json` (see `stage-comaps-data-assets.sh`).
- Inventory: interactive Navi place search is SQLite FTS5 (`place_index.db`);
  `.rkyv` POI packs are spatial-only. Phase 5 merges CoMaps hits with FTS hits;
  `.rkyv` is untouched.

## Expectation audit (suite closeout)

| Query | Old | New | Classification |
|-------|-----|-----|----------------|
| `cabin_gjendebu` | briefly accepted `Gjeddebu` / top_n=3 | **reverted** to high, top2=`Gjendebu` | Temporary alt hid ranking; after Affiliations/Everywhere/Suggest fixes the cabin ranks correctly — original kept |
| `street_*` top_n 3→5 | top_n=5 briefly | **reverted to top_n=3** | Inflating top_n would hide POI-over-street ranking; streets pass at top_n=3 after engine fixes |
| `expect_top_any` also matches `address` | name only | name **or** address | Result-shape harness (empty/partial names), not ranking inflate |
| `reverse_hamar_strandgata` | 60.7945,11.0678 | 60.794125,11.07143 (Strandgata 53) | Original pin was at Stortorget; nearest building Storhamargata 2 (16 m) — **fixture pin error**, not engine |
| `reverse_gjovik_storgata` | 60.7957,10.6915 | 60.79568,10.69102 (Storgata 3) | Pin sat between Hunnsvegen and Storgata; Hunnsvegen 3E was 24 m vs Storgata 3 at 26 m — **fixture pin error** |
| `address_strandgata_hamar_10` | Strandgata 10 | **Strandgata 13** (id renamed) | No Strandgata 10 in Hamar OSM/`place_index`; 13 exists — **bad suite data** |

## Three medium / data misses (FTS5 compare)

Device `place_index.db` (Ostlandet, pulled from Samsung app files):

1. **reverse_hamar_strandgata (old pin):** MWM `Storhamargata` @ 16.4 m. FTS
   nearby same: Storhamargata 2 @ 16.4 m. Not a regression; pin fixed.
2. **reverse_gjovik_storgata (old pin):** MWM `Hunnsvegen` @ 23.9 m. FTS
   nearby same: Hunnsvegen 3E @ 23.9 m, Storgata 3 @ 26.3 m. Not a regression;
   pin fixed.
3. **address_strandgata_hamar_10:** MWM top results were city/street labels,
   no house. FTS/`name_entries`: **zero** `Strandgata 10` in municipality
   Hamar (numbers jump 2,4,13,…). Not a regression; query retargeted to 13.

## Open items

- App search is wired (`ComapsSearchBridge` + `ComapsSessionBootstrap`).
- Production planning (flag ON) goes through `MwmProductionPlanner` (climb
  published there). Flag OFF keeps `.rkyv` `planCarRoute`.
- Shelter and brewery coverage if the owner wants parity with the Navi catalog.

