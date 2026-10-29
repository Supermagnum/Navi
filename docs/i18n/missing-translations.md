# Missing translations

## Status (feat/i18n-locale continuation)

- Road-name sidecar (option C): **STOPPED** — place-source lacks highway `name:*`
  (see `docs/i18n/names.md`).
- Road names in guidance: **option D shipped** (offline PMTiles `name:*` lookup +
  option B local-name fallback). FFI:
  `localizeRouteStreetsFromPmtiles` / `clearLocalizedRouteStreetsCache`.
- place_index: additive `names_json` column (no schema-version bump / no rebuild).
- Remaining English: OsmUpdateUserCopy / PackRegionAvailability / IndexedMapsBackground
  idle lines that are not yet wired through `strings.xml` (if any) — UI resource
  keys for Bokmål are complete; see `docs/i18n/nb-review.md`.

English (`values/strings.xml`) is the source of truth. Incomplete locale files
do not fail the build (`MissingTranslation` / `ExtraTranslation` disabled in
`app/build.gradle.kts`).

## UI languages (English + Bokmål only)

Navi ships **English** (`values/`) as the default and **Norwegian Bokmål**
(`values-nb/`). Nynorsk (`values-nn/`) is **not** maintained and was removed;
`locales_config.xml` lists only `en` and `nb`, so the per-app language picker
shows English and Norsk bokmål only.

Android does **not** fall back from `nn` (Nynorsk) to `nb` (Bokmål). A device
whose language is Nynorsk-only will show **English** UI strings unless Bokmål
(`nb`) is also in the device language list (or the user picks Bokmål in the
per-app language settings).

## values-nb (Bokmål)

Filled for common chrome, dialogs, approach, continents, highway classes,
download progress, Tools/status, CAT/camping/IMU, profiles, settings, and
Geofabrik country labels in the download catalog. Existing strings remain
**approved as-is**. Previously missing keys were translated in `i18n/nb-complete`
(see `docs/i18n/nb-review.md`).

No keys currently missing for `values-nb`.


## Geofabrik country labels

Most country proper names remain English in `GeofabrikDownloadCatalog` data
(endonyms often match). Continent chips use resources.
