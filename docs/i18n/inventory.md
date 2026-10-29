# i18n inventory (feat/i18n-locale)

Branch base: `origin/dev` @ `5dd5d0756`. Inventory before production code changes.
Related prior spec (WASM translation packs, not implemented): `docs/plugins/i18n-translation-spec.md`.
This work targets **device locale** via Android resources + shared name selection, not the optional i18n WASM plugin.

Legend: **Loc?** = already in `res/values*/strings.xml` (or equivalent). Proposed keys use `snake_case` namespaces.

---

## 1. Existing locales and RTL / language config

| Item | Location | Loc? | Notes / proposal |
|---|---|---|---|
| Default English strings | `app/src/main/res/values/strings.xml` | yes | 6 keys only |
| Norwegian Bokmål | `app/src/main/res/values-nb/strings.xml` | yes | Maintained; **do not rewrite** approved strings |
| Norwegian Nynorsk | — | removed | Not maintained; removed from `values-nn/` and `locales_config.xml` |
| Other app locales | — | no | Do not add (e.g. Arabic UI) without asking |
| `android:supportsRtl` | `app/src/main/AndroidManifest.xml:18` | yes | Already `true` |
| `locales_config.xml` | `app/src/main/res/xml/locales_config.xml` | yes | `en` + `nb` only |
| App label | Manifest `android:label="Navi"` | n/a | Brand; keep `translatable="false"` if moved |
| Lint MissingTranslation | `app/build.gradle.kts` | n/a | `MissingTranslation` / `ExtraTranslation` disabled so incomplete nb keys do not fail the build |
| Plugin CSV catalogs | `docs/plugins/translations.csv` | docs only | Not loaded by app |

### Existing string keys (all three locales)

| Key | EN (values) | nb / nn | Loc? |
|---|---|---|---|
| `via_point_reached` | Via point reached | Via-punkt nådd | yes |
| `via_point_n_of_m_reached` | Via point %1$d of %2$d reached | Via-punkt %1$d av %2$d nådd | yes |
| `cat_licence_listen_only` | You may listen, but do not transmit… | translated | yes — reuse |
| `cat_plugin_toggle_label` | CAT / CATS radio | CAT / CATS-radio | yes — reuse |
| `cat_update_repeaters` | Update repeaters | Oppdater… | yes |
| `cat_update_repeaters_hint` | Downloads from navi-server… | translated | yes |

Call sites already using `stringResource` / `getString`: `CatStatusSheet.kt`, `DriveHud.kt` (CAT toggle/licence), `RouteGuidanceModels.kt` (via-point).

---

## 2. Hardcoded Kotlin / Compose UI strings

Rough scan: **~260** `Text`/`NaviText`/`label`/`contentDescription` literals; **~98** `status` / progress assignments; **~336** combined unique user-facing hits across `app/src/main/java`. Vast majority **Loc? = no**.

### 2.1 By file (priority for Step 2)

| File | Approx hits | Loc? | Proposed key namespace |
|---|---|---|---|
| `MainActivity.kt` | ~122 Text + many status | mostly no | `dialog_*`, `search_*`, `tools_*`, `route_*`, `status_*`, `profile_*`, `vehicle_*` |
| `DriveHud.kt` | ~76 | mostly no; CAT keys yes | `hud_*`, `settings_*`, `units_*`, `travel_*` |
| `CatStatusSheet.kt` | ~37 + 3 stringResource | mixed | `cat_*` (reuse existing; add network/backend labels) |
| `MapLongPress.kt` | ~10 | no | `map_mark_*`, `save_place_*` |
| `CampingSuggestionSheet.kt` | ~7 | no | `camping_*` |
| `ImuPluginPanel.kt` | ~3 + status | no | `imu_*` |
| `ApproachInstructionBox.kt` | Off route / exits / a11y | no | `approach_*` |
| `PoiLookaheadHudChip.kt` | Dismiss | no | `poi_lookahead_dismiss` |
| `WeatherHudChip.kt` | Refresh | no | `action_refresh` |
| `GeofabrikDownloadCatalog.kt` | 95 unique English labels | no | `geo_continent_*`, `geo_country_*` (or keep English geographic proper names with `translatable="false"` where endonyms are standard — prefer string resources for continents/UI chrome; country labels as resources keyed by path) |
| `RegionDownloadBackground.kt` | Downloading… statuses | no | `download_*` (or map Rust codes in Step 4) |
| `RegionProgressMessages.kt` | concatenation helpers | no | format with `%1$s` / plurals |
| `StayInCountry.kt` | route error sentence | no | `error_stay_in_country` |
| `PlaceSearchHint.kt` | online search hint | no | `hint_online_place_search` |
| `DownloadedRegionDelete.kt` | Long trip gate | no | `error_turn_off_long_trip` |
| `RouteGuidanceModels.kt` | highway class English fallbacks | partial | map codes → `highway_class_*` |
| `NaviStorageVolumes.kt` | storage labels | check | `storage_*` |
| `DiagnosticLog.kt` | export UI | no | `diagnostics_*` |

### 2.2 Representative hardcoded rows (file, line, text, Loc?, proposed key)

| File | Line | Current text / field | Loc? | Proposed key / source |
|---|---|---|---|---|
| `ApproachInstructionBox.kt` | 140–154 | Off route; Recalculating may take several seconds | no | `approach_off_route`, `approach_recalculating_hint` |
| `ApproachInstructionBox.kt` | 165–168 | first/second/third exit | no | `approach_exit_first`… / plurals |
| `ApproachInstructionBox.kt` | 208 | Next maneuver. (contentDescription) | no | `approach_next_maneuver_a11y` |
| `DriveHud.kt` | 1188 | Currently on $street | no | `hud_currently_on` (`%1$s`) |
| `DriveHud.kt` | 350+ | Map, Weather overlay, DATEX, Long trip, Units, Metric… | no | `settings_*` / `hud_*` |
| `DriveHud.kt` | 681–689 | CAT toggle / licence | **yes** | reuse `cat_*` |
| `MainActivity.kt` | 6751+ | Speed camera warnings; Enable; Not now | no | `dialog_speed_camera_*` |
| `MainActivity.kt` | 6792+ | Off trail; Recalculate; Keep route | no | `dialog_off_trail_*` |
| `MainActivity.kt` | 6829+ | Map data needed; Download … | no | `dialog_map_data_needed`, `action_download_region` |
| `MainActivity.kt` | 6868+ | Longer route available / Accept | no | `dialog_longer_route_*` |
| `MainActivity.kt` | 6915+ | Delete route? / Cancel / Delete | no | `dialog_delete_route_*`, `action_cancel`, `action_delete` |
| `MainActivity.kt` | 7082+ | From / To / Via / Search / Searching… | no | `waypoint_from`, `waypoint_to`, `action_search`, `search_in_progress` |
| `MainActivity.kt` | 7568+ | Profile, Avoid tolls/ferries/tunnels, Vehicle limits… | no | `profile_*`, `avoid_*`, `vehicle_*` |
| `MainActivity.kt` | 6515 | Recalculating route… / Planning route… | no | `status_recalculating_route`, `status_planning_route` |
| `MainActivity.kt` | 4929 | Arrived at destination | no | `status_arrived` |
| `MainActivity.kt` | 2460 / 8232 | Destination (fallback end name) | no | `label_destination` |
| `MapLongPress.kt` | 119–212 | Marked location; Set as From/To; Save place | no | `map_long_press_*` |
| `CampingSuggestionSheet.kt` | 84–317 | Overnight camping…; Camp here tonight | no | `camping_*` |
| `ImuPluginPanel.kt` | 45–86 | IMU pitch…; Calibrate; Reset; Waiting… | no | `imu_*` |
| `CatStatusSheet.kt` | 175 | CAT / CATS radio (title Text) | duplicate of resource | use `cat_plugin_toggle_label` |
| `CatStatusSheet.kt` | 191+ | Move non-OSM repeaters…; Backend; Hamlib; Benshi | no | `cat_*` (Hamlib/Benshi/rigctld: `translatable="false"`) |
| `GeofabrikDownloadCatalog.kt` | 26–32 | Asia, Africa, Europe… continents | no | `geo_continent_*` |
| `GeofabrikDownloadCatalog.kt` | ~95 labels | Egypt, Tunisia, Norway, … | no | `geo_region_<path_slug>` or leave proper names EN with note in missing-translations |

### 2.3 Notifications

No `NotificationChannel` / `setContentTitle` user channels found in app Kotlin for this tree. Progress is in-app status text. Re-check if a foreground service channel is added later.

### 2.4 Concatenations / plurals needed

| Pattern | Loc? | Proposal |
|---|---|---|
| `Clear all vias (${viaPoints.size})` | no | plurals `clear_all_vias` |
| `Currently on $street` | no | `hud_currently_on` |
| `Download ${displayName}` | no | `action_download_named` |
| `RegionProgressMessages` index/total | no | `progress_region_n_of_m` |
| Via point n of m | **yes** | already `via_point_n_of_m_reached` |

### 2.5 Keep `translatable="false"` (examples)

OSM tags, callsigns, protocol names, paths, brand/style names: `Hamlib`, `Benshi`, `rigctld`, `DATEX`, `Liberty`, `Protomaps`, `Mapterhorn`, Geofabrik path ids (`europe/norway/ostlandet`), ISO country codes, radio model numbers, URLs.

---

## 3. Rust → UI via FFI (classify)

### 3a. Stable key / code (already or easy)

| Surface | Source | Shown as | Class | Proposal |
|---|---|---|---|---|
| Maneuver icon | `nav` / UniFFI `iconKey` (`nav_straight`, …) | PNG, not English | **(a) code** | Keep; client formats distance + street |
| Approach phase | `approachPhaseForDistance` | enum | **(a)** | Keep |
| Via-point events | Kotlin + `R.string.via_*` | localized | **(a)** | Keep |
| PlaceHit / route names | UniFFI structs | OSM `name` string | data | Step 3 name rule |
| Graph edge `name` / `road_ref` | pack `edge_name` | English/local OSM | data | Step 3; **no name:xx in pack today** |

### 3b. English shown as-is (must become codes + params)

| Surface | File (approx) | Example English | Loc? | Proposed message code |
|---|---|---|---|---|
| Download progress | `core/src/routing/region.rs`, `download/http.rs` | Downloading region… | no | `progress.download_region` |
| DEM / basemap | `basemap/extract.rs` | Downloading terrain DEM… / map tiles… | no | `progress.download_dem`, `progress.download_tiles` |
| Elevation | `elevation/sources/*.rs` | Downloading elevation… | no | `progress.download_elevation` |
| Indexed convert | `indexed/convert.rs` | Building indexed maps: … | no | `progress.build_indexed_*` |
| Graph build | `indexed/load.rs` | Building graph {file}… | no | `progress.build_graph` + param |
| Pack server | `pack_server/acquisition.rs` | Downloading updated pack… / Rebuilding locally… | no | `progress.pack_*` |
| Place-source | `pack_server/place_source.rs` | Downloading place-source for {id}… | no | `progress.place_source` |
| Highway class HUD | `routing/eta.rs` `highway_class_display_label` | Motorway, Living street, … | no | return **tag** (`motorway`) or code; Kotlin `highway_class_*` |
| Destination fallback | `navi-ffi` / MainActivity | `"Destination"` | no | client `label_destination` |
| IMU status | `plugins/imu/engine` | Calibration rejected…; Not calibrated… | no | plugin message keys (Step 5) |
| Camping legal_basis / notes | `right-to-roam-camping/engine` | long English sentences | no | message keys + args |
| CATS demo labels | `plugins/CATS-plugin` | Demo: leave network… | no | `cat.demo_*` |

**Constraint:** Presentation only — do not change routing/guidance logic. Minimal FFI: prefer enum/code + string params over new fat APIs. **Do not change navi-server.**

---

## 4. WASM plugins (user-visible)

Host stays **wasmtime**. Spec already prefers host-resolved message ids (`docs/plugins/i18n-translation-spec.md`).

| Plugin | User-visible strings | Loc? | Proposal |
|---|---|---|---|
| CATS-plugin | Sheet mostly Kotlin; demo step labels in Rust | mixed | Host keys; CAT licence toggle **already** `cat_licence_listen_only` |
| right-to-roam-camping | `legal_basis`, `notes`, sheet titles in Kotlin | no | Message keys + args; POI names via Step 3 |
| imu | `status_text` English paragraphs | no | Status enum → host strings |
| weather | Icon pack `name` in manifests (Mostly Clear…) | no | Map known condition codes; leave asset names or key table |
| poi-lookahead | HUD chip Kotlin | no | `poi_lookahead_*` |
| datex | Settings labels in DriveHud | no | `datex_*`; host/port `translatable="false"` |
| log-hello / trap-guest / busy-loop / memory-bomb | test guests | n/a | Skip or minimal |

Document host resolution in `docs/i18n/plugins.md` (Step 5).

---

## 5. Name data sources

Shared rule (to implement in Step 3 / `docs/i18n/names.md`):

1. `name:<full device locale>`
2. `name:<device language>` (`nb` then `no` for Norwegian)
3. If device language is Latin script and local `name` is not: `int_name`, then `name:en`, then `name:latin`
4. `name`
Refs never translated.

### 5.1 Protomaps offline style

| Item | Finding | Loc? | Proposal |
|---|---|---|---|
| Style | `app/src/main/assets/map-styles/protomaps-light/style.template.json` | n/a | `text-field` is `coalesce(get name, get ref)` for roads; places use `get name` only |
| Runtime tweaks | `BasemapPeakElevationStyle.kt`, `BasemapStyleResolver.kt` | n/a | Rebuild label expressions on style load + locale change; bump style epoch if needed |
| Tile attrs | Protomaps basemap typically exposes `name`, `name:*`, sometimes `name:en` / script tags depending on build | sample in style: only `name`/`ref` referenced | Prefer MapLibre `coalesce` over `name:<lang>` chain matching Step 3 |

### 5.2 OpenFreeMap Liberty (online)

| Item | Finding | Proposal |
|---|---|---|
| URL | `BasemapStyleResolver.LIBERTY_URL` | On load, patch symbol `text-field` with same coalesce chain |
| Peak labels | `BasemapPeakElevationStyle.libertyTextField` | Include localized name fields before elevation suffix |

### 5.3 Graph packs

| Item | Finding | WAIT? |
|---|---|---|
| Format | `GRAPH_FORMAT_VERSION = 9`; `edge_name: Vec<String>` | |
| Tags read at build | `builder.rs` `.read_tag("name")` `.read_tag("ref")` `.read_tag("int_ref")` only | **Missing `name:xx` / `int_name`** |
| Guidance presentation | `nav::prefer_street_label` / `maneuver_street_label` use single `name` | Apply selection only if multi-name available |

**WAIT (pack format):** Do **not** bump pack format or touch navi-server. Report: packs store one OSM `name` (+ ref). Options for later user decision:

- A) Extend pack to store name map / preferred localized string (format bump + server rebuild)
- B) Client-side only: accept local `name` for TBT until packs gain aliases
- C) Optional sidecar name table per region (no graph format bump)

Until decided: implement selection API for maps/place index/plugins; guidance uses best available single name.

### 5.4 place_index.db

| Item | Finding | WAIT? |
|---|---|---|
| Schema | `name_entries(name, kind, …, search_doc)` single display `name` | |
| Ingest | `classify_named`: `name`, `alt_name`, `loc_name` into `search_doc`; display prefers `name` then `loc_name` then first `alt_name` | **No `name:xx` / `int_name` columns** |
| Search | FTS on `search_doc` | Can match alt/loc; not language-tagged |

**WAIT (schema):** Propose before implementing:

- Lazy migration on next region update: add optional `names_json` / extra columns; rebuild region rows when that region is refreshed
- Forced full rebuild of `place_index.db`

Until approved: implement selection helper for tag maps; when only one `name` exists, use it; expand search_doc to include `name:xx`/`int_name` **only after** migration approval (or temporary: parse from PBF at build if columns approved).

### 5.5 POI / barrier packs

| Item | Finding | Proposal |
|---|---|---|
| Extract | `poi_barrier_extract.rs` `tags.get("name")` only | Same as graph: missing localized tags → WAIT with packs |
| Plugin POIs | Camping TentSite names from host | Step 3 helper when tags available |

### 5.6 Arabic-script test region (download WAIT)

Smallest suitable Arabic-script Geofabrik region in catalog / `docs/coverage.md`:

| Region | Path | PBF (docs) | Expanded (docs) |
|---|---|---|---|
| **Tunisia** | `africa/tunisia` | ~80 MB | ~1.18 GB |

Alternatives: Egypt ~170 MB / 3.45 GB; Morocco ~232 MB / 2.84 GB.

**WAIT:** Do not download until parent/user OK.

---

## 6. RTL (preview for Step 6)

| Item | Status |
|---|---|
| `supportsRtl` | Already true |
| Compose | Audit `start`/`end` vs `left`/`right`; mirror chrome icons only where sensible |
| Map / compass / turn arrows | Do **not** mirror in meaning-breaking ways |
| Mixed scripts | Bidi isolation for road names in Latin UI |
| Fonts | Offline Noto Sans in Protomaps style; report scripts that tofu on tablet |

---

## 7. Locale-aware formatting (preview for Step 7)

| Item | Current | Proposal |
|---|---|---|
| `DisplayUnits.kt` | `String.format(Locale.US, …)` | Device locale (keep unit system preference separate) |
| Distances in approach box | via `DisplayUnits` | Same |
| Dates/times | scattered | Platform formatters |

---

## 8. Translations (preview for Step 8)

| Locale | Action |
|---|---|
| `values` (en) | Add all new keys |
| `values-nb` | Exists — fill confident keys; else leave missing (approved as-is) |
| `values-nn` | Removed — Nynorsk not maintained |
| New UI languages | **Do not add** without asking |
| Missing list | `docs/i18n/missing-translations.md` |
| Build | Disable fail-on MissingTranslation/ExtraTranslation as needed; document |

---

## Short summary (for parent / first commit)

- **Already localized:** 6 CAT/via-point strings in `en`/`nb`/`nn`; `supportsRtl=true`; no `locales_config.xml`.
- **Hardcoded UI:** hundreds of Compose/status strings (MainActivity, DriveHud, CAT sheet, dialogs, Tools, Geofabrik catalog ~95 labels).
- **Rust→UI:** progress/build labels and highway-class words are English strings; maneuver icons are codes (good); Destination fallback English.
- **Names:** styles use plain `name`/`ref`; graph packs + place_index + POI packs lack `name:xx`/`int_name` storage — **WAIT** before pack format or place_index migration; implement selection rule for surfaces that have tag maps (styles first).
- **Plugins:** camping/IMU/weather English; CAT licence already a resource.
- **Arabic region candidate:** Tunisia (~80 MB PBF) — **WAIT** for download OK.
- **nb locale:** already present — no creation needed.

Next: Steps 2–8 on this branch without waiting for chat approval, stopping only at WAIT points above.
