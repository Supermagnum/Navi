# Repeater source audit

The tables in sections 1a–1f are the before state, measured on `dad2b3a9b`. Section "After the source-handling fix" is the same data imported again on `833a61b29`, plus the DCS string read found by that re-run.

Read-only audit of what CAT stores after the real importers run. No rule was changed in the before measurement.

Catalog fetched from `https://navigate-me.duckdns.org/repeaters/current.json` (catalog generation `20261021T033207Z`). Country bundles were decompressed and passed through `import_mirror_payload`. The KML files and the one CSV in the local repeater folder were passed through `parse_kml_bytes`, `parse_csv_bytes`, and `merge_records`. Installed map packs are not a repeater source: nothing in the app reads `amateur_radio` tags out of a pack.

`import_osm_relation_xml` and `import_osm_json` are used by tests and the LA5MR fixture. Update does not call them.

## Where each source comes from

| Source | What Update actually loads | Countries in this catalog |
|---|---|---|
| OSM | navi-server bundle `source=osm`, Norway only, generation `20261013T091219Z` | NO. SE, DK, and DE are not published |
| RadioID | navi-server bundles `source=radioid`, generation `20261021T033207Z` | NO, SE, DK, DE |
| Files | KML/KMZ/CSV in the repeater drop folder, imported before the server fetch | From each file's `state_id` |

`fetch_and_update` downloads every catalog source except `repeaterbook`. The Norway OSM bundle is downloaded. `parse_source_name` maps `radioid` to `RadioId` and `repeaterbook` to `RepeaterBook`. Every other name, including `osm`, is stored as `User`.

## 1a. Counts

### OSM Norway, as published

666 records. 1 has no callsign and is skipped. 665 are handed to the importer. Every record has a position (`position_null` is false on all 666). There is no OSM bundle for SE, DK, or DE, so those counts are 0.

| Mode | Records | `exclude_recommended` |
|---|---|---|
| FM | 291 | false |
| APRS | 282 | true |
| DMR | 58 | false |
| DSTAR | 31 | false |
| DATA | 4 | false |

JSON fields the mirror importer reads: `callsign`, `lat`/`lon`, `position_null`, `output_hz`, `shift_hz` (missing becomes 0 MHz), `mode` (missing would become `DMR`), `dmr_color_code`, `exclude_recommended`. It does not read `ctcss_encode_hz`, `dcs_encode`, `network`, `toneburst_hz`, or `osm_tags`.

Tags present on the published `osm_tags` (this is the bundle, not a live pack scan):

| Tag | Missing |
|---|---|
| `communication:amateur_radio:callsign` (or `callsign`) | 1 of 666 |
| `communication:amateur_radio:repeater:frequency_out` | 0 of 666 |
| `communication:amateur_radio:repeater:shift` | 312 of 666 |
| `communication:amateur_radio:repeater:ctcss` | 452 of 666 |
| JSON `network` | 634 of 666 |

Among the 291 FM rows: 2 have no `shift_hz`, 84 have no `ctcss_encode_hz`, 259 have no `network`.

The row id is `osm-{callsign}` because the payload has no `radioid_locator` and no `id`. 665 records collapse to 389 callsigns. `list_sites` then hides APRS (and 144.800 MHz). What the UI can list is 232 rows: FM 177, DMR 40, DSTAR 13, DATA 2. Of those 232, CTCSS is empty on all of them, `network_id` is empty on all of them, and 8 have shift 0.

### RadioID, as published and as stored

Every imported RadioID row is DMR. None is FM. The importer stores `callsign`, position, `output_hz`, `shift_hz`, `mode`, `dmr_color_code`, `exclude_recommended`, and the attribution string. `network` is present in the JSON as an object and is not copied. `operational_status` is present and is not copied. CTCSS and DCS are not in the payload, and the importer always stores them as empty. `exclude_recommended` is false on every row. `operational_status` is `on-air` on every row.

| Country | Catalog rows | Imported | FM | DMR | With position | Without position |
|---|---|---|---|---|---|---|
| NO | 90 | 90 | 0 | 90 | 71 | 19 |
| SE | 164 | 164 | 0 | 164 | 141 | 23 |
| DK | 105 | 104 | 0 | 104 | 78 | 26 |
| DE | 756 | 756 | 0 | 756 | 712 | 44 |

DK drops `OZ4DMR`: `output_hz` is 0 and the position is null. Locator ids are unique in these four files, so RadioID rows do not collapse.

### Files

The drop folder used here is the local `klm/` directory (gitignored). Seven KML files and one CSV. The CSV is `repeaters_merged.csv` (402 data rows). `parse_csv_bytes` rejects the whole file: the frequency column is `tx`, which is not one of `frequency`, `freq`, `output`, `output frequency`, `downlink`. Coordinate columns are `osm_lat`, `radioid_lat`, `repeaterbook_lat`, `best_lat`, and similar. None of those names is `lat` or `latitude`, so no CSV row is imported.

KML, from `parse_kml_bytes`. Mode is FM unless the description contains DMR, D-STAR, or C4FM. None of these files did, so every placemark is classed as FM. Shift is never present. A placemark with no frequency or no coordinates is counted below as rejected and is not stored.

| Country | Placemarks | Stored with position | Rejected | Off-air | No CTCSS | DMR or other |
|---|---|---|---|---|---|---|
| NO | 205 | 200 | 5 | 18 | 87 | 0 |
| SE | 359 | 355 | 4 | 26 | 173 | 0 |
| DK | 139 | 139 | 0 | 15 | 100 | 0 |
| DE | 1603 | 1596 | 7 | 107 | 1175 | 0 |

`merge_records` on the 2306 placemarks: 16 rejected, 2290 added, 0 merged into OSM, 0 overrides. The stored id is `rb-kml-{callsign}-{frequency}`. That collapses 2290 adds to 2178 `RepeaterBook` rows, all with positions, 2020 of them FM-programmable (the rest are off-air). `network_id` is empty on every one. KML has no network field.

## 1b. Overlap

### Keys the code uses

File to OSM, in `best_osm_match`, and only if the stored row's source is `RepeaterSource::Osm`:

- callsign, case-insensitive
- both positions present: distance at most `DEDUP_DISTANCE_KM` (1.0 km). Frequency is not required for this match
- either side missing a position: output frequency within `FREQ_MATCH_MHZ` (0.0125 MHz, 12.5 kHz)

A frequency difference above 12.5 kHz, a CTCSS difference above 0.05 Hz, or a known shift difference above 0.001 MHz then overwrites that field on the OSM row. The file position is not copied.

RadioID is not matched to OSM and not matched to files. There is no callsign, frequency, or distance comparison. A RadioID row is inserted under its own id (`radioid-{locator}`).

Mirror OSM is stored as `User`, so the file matcher does not see it. Running the real merge against the imported Norway bundle produced `merged_osm=0` and `overrides=0`.

### What the app stores

Intersection cells are 0 in every country. Rows sit beside each other.

| Category | NO | SE | DK | DE |
|---|---|---|---|---|
| OSM + RadioID + files | 0 | 0 | 0 | 0 |
| OSM + RadioID only | 0 | 0 | 0 | 0 |
| OSM + files only | 0 | 0 | 0 | 0 |
| OSM only (UI-visible after collapse and APRS filter) | 232 | 0 | 0 | 0 |
| RadioID + files only | 0 | 0 | 0 | 0 |
| RadioID only | 90 | 164 | 104 | 756 |
| Files only (placed KML, before the cross-file id collapse) | 200 | 355 | 139 | 1596 |

The file column sums to 2290. After the `rb-kml-{callsign}-{frequency}` collapse the table holds 2178 file rows. That collapse is not split by country here.

SE, DK, and DE have no OSM bundle, so their OSM cells are 0 because the data is absent, not because a matcher ran.

### Same keys applied to the published rows

This is not what the importer does. It is the overlap those keys would see.

Norway RadioID versus published OSM nodes: 43 of 90 RadioID rows have the same callsign within 1 km of an OSM node. 14 have the same callsign and the same output frequency (within 12.5 kHz) but every OSM node of that callsign is more than 1 km away. Example of a close pair the app does not merge: RadioID `LD1OA` at 434.8875 MHz is 1.3 m from OSM node `14266844182`. Example of a far pair: RadioID `LD6FF` at 434.875 MHz is about 145 km from OSM nodes `14270152248` and `3655921327`.

Norway KML versus the collapsed stored OSM row (one position per callsign): 114 KML rows would match if that row were `RepeaterSource::Osm`. 39 KML rows share the callsign but are more than 1 km from the surviving OSM position. Published OSM itself contains 83 pairs with the same callsign and the same output frequency more than 1 km apart. `LA5MR` at 145.625 MHz, OSM nodes `1705107664` and `12635462528`, are 155.8 km apart. The importer keeps one of them.

## 1c. Which source wins

Only the three disjoint categories exist after import. "Wins" means the value stored on that row. Nothing is copied across sources.

| Field | OSM bundle (stored as User) | RadioID | KML |
|---|---|---|---|
| Position | The last payload row for that callsign. Other OSM nodes with the same callsign are overwritten | Payload lat/lon. `position_accurate` is false when `position_null` or a coordinate is missing | KML coordinates. Rejected placemarks are not stored |
| Output frequency | `output_hz` of the surviving row | `output_hz`. Row skipped when it is 0 | Description. Missing frequency rejects the placemark |
| Shift | `shift_hz`, or 0 when the field is absent | `shift_hz`, or 0 when absent | Always 0. KML has no offset |
| CTCSS / DCS | Empty. The fields are not read | Empty. Not in the payload | CTCSS when the description has a tone from 60 to 260 Hz. 1750 Hz is not stored. DCS empty |
| Network | Empty. Not read | Empty. The JSON object is not read | Empty. KML has no network |
| Mode | Payload `mode` | Payload `mode` (all DMR here) | FM, unless the description text contains DMR, D-STAR, or C4FM |
| On-air | `exclude_recommended` (true for the 282 APRS rows). No status field in the OSM bundle | Not read. `operational_status` stays out of the row. `exclude_recommended` is false, so the row is not marked excluded | `On-air: No` sets `exclude_recommended` and `conflict_note` `off-air` |

Provenance in the CAT sheet is the `source` name plus `attribution` when that string is set. Mirror rows get the bundle attribution (`© OpenStreetMap contributors` or `Data courtesy of RadioID.net`). KML rows get `RepeaterBook KML: {filename}` and source `repeater_book`.

There is no repeater marker layer on the map. The CAT sheet lists rows from `list_near_ui` inside 150 km. That list includes DMR and excluded rows. Rows with no usable position are listed with no distance, and that list is not limited to the current country.

The network dropdown keeps rows with a non-empty `network_id`, a usable position, `exclude_recommended` false, and distance at most 100 km. All three sources store an empty `network_id` on this data, so none of these rows enter the dropdown.

Switching uses the same host query and then drops rows whose `network_id` is empty. None of these rows are used for switching.

A later `fetch_and_update` that applies a new catalog generation copies the live database, deletes every `RadioId` and `RepeaterBook` row, then inserts the new bundles. KML rows imported earlier in the same Update are deleted when that fetch applies. OSM-as-User rows are not deleted. They are upserted again, which overwrites the callsign key. If the catalog generation sidecar already matches, the fetch returns before that delete, and the KML rows from the local import stay.

## 1d. Position

An OSM position does not override every other source, in Norway or anywhere else. `apply_one` keeps the OSM latitude and longitude only when the existing row is `RepeaterSource::Osm`. The mirror never writes that source. The file row is added next to the User row, with the file's own position. There is no country branch.

Outside Norway the catalog has no OSM bundle. The file position is the file row's position. The RadioID position is the RadioID row's position. Neither replaces the other. That behaviour is the matcher and the mirror source tag. It is not written down as a rule for countries other than Norway. README says OSM is the source for Norway. It does not state a position rule for SE, DK, or DE.

The same callsign and the same output frequency more than 1 km apart are not one repeater. `best_osm_match` refuses the pair when both positions exist and the distance is over 1 km, even if the frequency matches. They would be two rows if the OSM row were source `Osm`. In the mirror path they are still two rows when the second source is a file or RadioID, because those ids differ. They do not both appear as map markers. Both can appear in the CAT sheet if each is inside 150 km of the query. Published OSM does not keep both nodes: the shared id `osm-{callsign}` leaves one. Count of published OSM pairs over 1 km with the same callsign and frequency: 83. `LA5MR` 145.625 MHz, nodes `1705107664` and `12635462528`, 155.8 km. Count of RadioID rows in that far class against published OSM: 14. `LD6FF` 434.875 MHz is one of them.

An OSM repeater that is missing from RadioID and from the files is kept. The mirror does not blank a field because another source lacks it. It does blank CTCSS, DCS, and network because it never reads them, and it turns a missing shift into 0. Extra OSM nodes that share a callsign are dropped by the id, which is separate from the other sources.

An OSM repeater with no frequency is not in this bundle (0 rows lack `frequency_out`). The relation XML importer, which Update does not run, skips a member that lacks `frequency_out` or shift, and skips modulation that is not FM. It does not fill the gap from RadioID or from a file. A missing tone on a mirror row stays empty. The file override that would copy a tone onto an OSM row did not run, because the source tag is `User`.

A RadioID or file row that is missing from OSM is added. RadioID uses its published coordinates, or `position_accurate=false` when those are null. A file uses the KML coordinates. Source is `radio_id` or `repeater_book`, with the attribution above. The file row is removed again if this Update's server apply deletes `RepeaterBook`.

Consensus relocation is the README line "Consensus relocation (default off)." There is no setting and no code under that name in `navi-cat` or the app. It does not move any position while it is absent.

## 1e. Dropdown radius and look-ahead

`InstalledMaps.Region` has no polygon and no bbox. The dropdown does not use a pack outline.

The sheet asks for repeaters inside 150 km (`catPluginRepeaterListUiJson` with radius 150). `networkNamesFromNearby` then drops anything farther than 100 km. `DEFAULT_REGION_RADIUS_KM` is 100 and is what `list_networks` would use. The UI does not call `list_networks`. Follow scoring uses `MAX_RANGE_KM` 150, and the guest query uses `AUTO_TUNE_RADIUS_KM` 150. The dropdown radius is 100 km. The follow and query cap is 150 km.

`plugins/CATS-plugin/src/follow.rs` scores by haversine from the current position. Planned and unplanned replays call the same `tick_at`. History of that plugin (`git log -- plugins/CATS-plugin/src/follow.rs`, and `-S look-ahead` / `-S lookahead` on `plugins/CATS-plugin`) has no route look-ahead implementation and no commit that removes one. The word appears in `f811bf2f2` only as a comment that there is no separate look-ahead. `plugins/poi-lookahead` is the attractions plugin, not CAT repeater selection.

## 1f. Where this differs from the expected rules

Expected: in Norway, OSM is the source of truth, including position. RepeaterBook and RadioID may override an OSM repeater only for frequency, CTCSS/tone, or shift when they differ. They may add repeaters that OSM does not have. No position rule has been set outside Norway.

What the code does instead:

1. Norway OSM from navi-server is stored as `User`, not as `Osm`. The OSM-wins position rule never runs on it. File frequency, CTCSS, and shift are not applied to it (`merged_osm=0` on this data).
2. The row id `osm-{callsign}` keeps one Norway OSM row per callsign. 665 records become 389 stored rows before the APRS filter. That drops sites instead of keeping OSM as published.
3. CTCSS, DCS, and network on the OSM bundle are discarded. A missing shift is stored as 0. Those fields are not left as OSM tagged them.
4. RadioID is never matched to OSM. It does not override frequency, CTCSS, or shift. CTCSS is always empty. All 1,114 imported rows here are DMR, with an empty `network_id`, so they are not in the dropdown and are not used for switching.
5. KML can add rows OSM does not have, but a server apply then deletes every `RepeaterBook` row. A new catalog generation removes the file import that just ran. KML also has an empty `network_id`, so the added rows are not in the dropdown and are not used for switching.
6. The position rule inside `apply_one` is not limited to Norway. It keeps the OSM coordinates in every country, whenever the existing row is actually source `Osm`. Outside Norway there is no OSM bundle, so the question does not arise for mirror data. This audit does not choose a rule for that case.
7. The same callsign and frequency more than 1 km apart stay two rows across sources. Inside the OSM bundle they do not: one callsign keeps one row.
8. The CSV in the repeater folder is not imported.
9. Consensus relocation is not implemented. The README line does not describe a code path.

SE, DK, and DE have no OSM payload, so "OSM is the source of truth" cannot be applied there with this catalog.

## After the source-handling fix

Same catalog and the same local files, imported by `import_mirror_payload` and `merge_directory_into` after `833a61b29`. The DCS code in the Norway bundle is the string `"125"`, not a number. The first re-run stored that row with access unknown. The importer now accepts a numeric string. The counts below include that read. `list_sites` still hides APRS, so mode rows sum to 383. `count_sites` is the table, including APRS.

| What | Before | After |
|---|---|---|
| OSM source tag | `User` | `Osm` |
| OSM rows stored | 665 handed in, collapsed by callsign (389 callsigns; 232 left after the APRS filter) | 665 stored. One published record has no callsign and is skipped. No further collapse |
| `LA5MR` at 145.625 MHz | 1 stored row | 5 stored rows, including nodes `1705107664` and `12635462528` |
| OSM CTCSS rows visible in `list_sites` | 0 | 212 |
| OSM DCS rows | 0 | 1 (`LD1FR`, node `14266941716`, code 125) |
| OSM 1750 Hz burst | not read | 54, stored as burst, CTCSS empty |
| OSM shift left unknown | stored as 0 | 12 visible rows have no shift. It is not written as 0 |
| OSM rows with a network | 0 | 32 rows, 5 names: Agder net, Bergen-voss, Fylkesnettet, LA5MR, Sandnes net |
| RadioID NO | 90 own rows, never matched | 41 folded into an OSM row, 49 kept as RadioID |
| RadioID SE / DK / DE | 164 / 104 / 756 | 164 / 104 / 756 (no OSM bundle to match) |
| `LD1OA` 434.8875 MHz | separate from OSM node `14266844182` (1.3 m) | 1 row, source `Osm`, latitude 59.9800956 |
| CSV `repeaters_merged.csv` | rejected (no `tx` column) | 402 read, 399 placed, 38 of them 1750 Hz burst, 162 added, 153 merged, 82 override notes, 4 rejected, 1 without a position |
| Norway KML `repeaters_2610270617.kml` | 205 placemarks, 200 stored, 0 merged | 204 rows after the within-file duplicate drop: 78 merged, 60 added, 61 override notes, 5 rejected |
| Sweden KML | 359 placemarks, 355 stored, 0 merged | 347 rows after that drop: 317 added, 26 override notes, 4 rejected |
| Denmark KML | 139 placemarks, 139 stored, 0 merged | 138 rows after that drop: 133 added, 5 override notes |
| Germany KML files | 1603 placemarks in the largest file, 1596 stored across the files, 0 merged | 1596 rows after that drop across the four files: 1258 added, 331 override notes, 7 rejected |
| "position differs from OSM" notes | not produced | 61 across the file import |
| Rows with access 1750 Hz burst after OSM and the files | 0 | 183 |
| File rows after a server apply | deleted | rebuilt from the drop folder in the same staging swap |
| Dropdown radius | 100 km after a 150 km query | 150 km, the same value follow uses |
| Dropdown networks on this bundle | none (`network_id` empty) | the 5 FM network names, plus Nearest network (auto), Standalone repeaters (nearest), and Off / manual |
| Consensus relocation | no code | implemented, default off. An OSM row is not moved |

Norway KML merged into OSM where callsign, output frequency within 12.5 kHz, and position within 1 km all held. A field that differed is named on the override note. A file row more than 1 km from every OSM site of that callsign stayed a separate row. Outside Norway, RadioID was left as its own row except where a file matched it. Sweden, Denmark, and Germany have no OSM bundle, so their file rows were not moved onto an OSM position.

`LD1OA` did not match before because RadioID was never compared with OSM, and the OSM row was source `User`, which the file matcher ignored. The frequency difference is 0 and the positions are 1.3 m apart, so the new key matches. The stored position is the OSM node's.

navi-server does not drop CTCSS, DCS, 1750 Hz, shift, or LA5MR relation membership. Those were dropped in the app, and the app reads them now. The published bundle still omits non-radio tags the consensus target wants: `man_made`, `tower:type`, `ele`, and `height`. The exporter is not in the local navi-server checkout, so this audit cannot name its function. LA5MR follow uses the relation name, which the bundle keeps. Consensus therefore uses a DEM peak unless a caller supplies a structure. A missing DEM tile does not move the row.

The code matches the section 1 rules on this data. The re-audit gate does not stop here.
