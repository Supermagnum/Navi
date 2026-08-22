# UI language / translation plugin (specification)

**Status:** specification only — not implemented.  
**Path:** `docs/plugins/i18n-translation-spec.md`  
**Architecture:** planned WASM guest via `plugin-host` / `plugin-sdk` and
capability-gated `HostApi` ([`plugins.md`](../plugins.md)). String catalogs and
locale choice stay out of the trusted routing core; the host owns Compose UI
text lookup and optional pack download.
**System requirements** (all plugins): user **enable/disable** toggle; any
device link uses host-mediated **USB** / **Bluetooth**
([`plugins.md` — enable/disable](../plugins.md#enable--disable-required),
[USB/Bluetooth](../plugins.md#external-device-io--usb-and-bluetooth-required)).

Working title / id suggestion: `i18n` / `ui_translation`.

---

## Translator working tables (source English)

Human-editable catalogs for filling translations live next to this spec:

| File | Format |
|---|---|
| [`translations.csv`](translations.csv) | UTF-8 CSV (standard spreadsheet import) |

Columns: **Language** is only a header placeholder (leave cells blank — it
marks that language columns follow). **English** holds source UI wording from
the current app: both **individual words** (e.g. Apply, Norway, Ostlandet, ETA,
Basemap) and **full phrases** / status lines. Words are listed first, then
phrases. **Norwegian (Norsk)** and **Swedish (Svenska)** stay blank until a
translator fills them. This CSV is the working source for a future
`messages.json` pack; it is **not** loaded by the app today.

---

## Current product state (do not invent a toggle)

As of this writing:

- The Android Automotive UI strings are **English only** (hard-coded / English
  resources in the Compose host).
- There is **no language-switching control** in map settings, drive settings, or
  tools.
- Do **not** infer UI language from GPS or SIM/network country. That would
  override the language the user already chose in Android. Display **units**
  may infer once from SIM country; locale must not piggyback on that.
- Repository docs may exist in English and Norwegian (`README.md` /
  `docs/Norwegian.md`, `docs/pictures.md` / `docs/bilder.md`, …). That is
  **documentation**, not an in-app locale system.

This plugin is how a future contributor would add selectable UI languages
**without** baking every language into the core APK or UniFFI surface.

---

## Goals

1. Let the host resolve UI strings by **stable message id** + **locale tag**
   (BCP 47, e.g. `en`, `nb-NO`, `de`).
2. Ship translation catalogs as **offline packs** the user installs (or that
   ship beside the APK), not as silent network fetches from the WASM guest.
3. Keep the default UI English when no pack is installed or a key is missing
   (**fallback to English**, then to the message id for diagnostics).
4. Stay capability-gated: the guest may rank packs or validate catalogs; the
   host draws every Compose label.

## Non-goals

- Implementing the plugin or a language toggle in this pass.
- Translating OSM place names / road names (those stay as mapped; optional
  future `name:xx` preference is separate).
- Localizing the Rust core log strings or UniFFI error enums (host maps known
  error codes to UI strings).
- Forcing RTL layout engines inside WASM — the host applies Android/Compose
  layout direction from the chosen locale.
- Replacing voice-guidance language packs ([`voice-guidance.md`](../voice-guidance.md));
  UI locale and spoken pack language may share a preference later, but they are
  different catalogs.

---

## Host capabilities (proposed)

| Capability | Purpose |
|---|---|
| `log` | Diagnostics |
| `i18n_catalog_query` (new) | List installed packs: locale, version, key count, path/hash |
| `i18n_string_resolve` (new) | Resolve `message_id` + optional ICU-style args → UTF-8 string into guest buffer (or host resolves without guest — see below) |
| `i18n_locale_get` / `i18n_locale_set` (new) | Read/write active UI locale preference (host persists) |
| `plugin_kv` / `storage` (new) | Optional guest state (last validated pack hash) |

**Preferred split:** the host performs the actual `stringResource`-style lookup
on the UI thread from an in-memory map loaded from the pack. The WASM guest is
optional for:

- validating pack integrity,
- merging overlay catalogs,
- suggesting a locale from `admin_region_read` / system locale.

A minimal first ship can be **host-only** catalog loading with no WASM, then
wrap the same files behind this plugin API later. The capability names above
still define the contract.

The guest must **not** open network sockets to pull translations. Pack install
is a host/Tools action (same pattern as PMTiles / voice packs).

---

## Catalog format (proposed)

Offline directory (example):

```text
{dataDir}/i18n/
  en/messages.json
  nb-NO/messages.json
  de/messages.json
  manifest.json
```

`manifest.json` (sketch):

```json
{
  "schema": 1,
  "default_locale": "en",
  "packs": [
    { "locale": "en", "name": "English", "file": "en/messages.json", "version": "1.0.0" },
    { "locale": "nb-NO", "name": "Norsk (bokmål)", "file": "nb-NO/messages.json", "version": "1.0.0" }
  ]
}
```

`messages.json` (flat message ids → strings; ICU `{name}` placeholders allowed):

```json
{
  "map.settings.title": "Map / display settings",
  "drive.settings.title": "Drive / vehicle settings",
  "drive.profile.hiking": "Hiking",
  "drive.profile.hiking.hint": "Select Hiking before planning foot routes; other modes use the road graph.",
  "hud.break_in_min": "Break in {minutes} min"
}
```

Rules:

- **Stable ids** in `domain.screen.control` form; never use English source text
  as the lookup key.
- English pack is mandatory and is the fallback.
- No HTML in strings; Compose applies styling.
- Plurals: either ICU MessageFormat subset hosted in Kotlin, or explicit keys
  (`hud.break_in_min_one` / `_other`) for v1 simplicity.

---

## Host UI integration

1. Replace hard-coded Compose literals with `NaviStrings.t("map.settings.title")`
   (name illustrative) that reads the active catalog.
2. Add a **Language** row to map or drive settings **only when** at least one
   non-English pack is installed (or always show English + installed packs).
   Until packs exist, **do not show a dead toggle**.
3. Persist `ui_locale` next to other `MapHudPrefs` / app config.
4. On missing key: log once at debug, show English, then id if English missing.

Approach-instruction street names, POI names, basemap labels, and **road-sign
catalogue / children-zone warning labels** (`name_en`, `label` from
[`road-signs.md`](../road-signs.md) / UniFFI warning JSON) remain data-driven,
not UI catalog strings. Soft chrome around those boxes (“Children ahead”
wrappers already in FFI, camera copy, settings titles) *can* move into packs
when this plugin ships.

---

## Plugin guest responsibilities (when WASM is used)

1. On load: call `i18n_catalog_query`; verify hashes / schema version.
2. Optionally propose locale from system or `admin_region_read` (never change
   locale without user confirmation).
3. Never block routing; catalog load is T0/T1 budget only at settings open /
   app start.

---

## Testing

| Check | Expectation |
|---|---|
| No packs installed | English UI; no language control, or control listing English only |
| `nb-NO` pack installed | User can select it; settings titles switch; restart keeps locale |
| Missing key in `nb-NO` | English fallback for that key only |
| Remove pack while selected | Fall back to English; clear stale preference |
| Hiking hint string | Documents that Hiking mode must be selected for foot routing |

---

## Relation to documentation translations

Human-maintained docs (`docs/Norwegian.md`, `docs/bilder.md`, …) are **not** this
plugin. Keep shipping parallel markdown for docs as today. The plugin covers
**in-app** UI chrome only.

---

## Design rules (same family as other plugins)

1. Offline-first; network pack download is host/Tools, user-initiated.
2. User **enable/disable** per [`plugins.md`](../plugins.md#enable--disable-required);
   core routing works with the plugin disabled (English strings in host).
3. No silent mutation of OSM / graph caches.
4. Privacy: locale preference stays on device; no phone-home of UI language.
5. Hardware I/O (if any) is host-mediated **USB** / **Bluetooth** only
   ([`plugins.md`](../plugins.md#external-device-io--usb-and-bluetooth-required)).
