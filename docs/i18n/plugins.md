# Plugin UI strings (wasmtime host resolution)

Plugins stay in **wasmtime**. User-visible text should be **message keys + args**
(or a small per-plugin string table). The Android host resolves keys with the
device locale and falls back to English.

## Host contract

1. Guest returns a stable id such as `camping.legal.safety_config_unavailable`
   plus optional string/number args (JSON array or ABI tuple).
2. Host looks up `R.string.plugin_<id_with_underscores>` (or a dedicated plugin
   catalog). Missing key → English fallback shipped beside the plugin or the
   raw id in debug builds.
3. Place / POI names use the shared rule in `names.md` when tag maps exist.

CAT licence toggle remains a normal Android string:
`cat_licence_listen_only` (already translated in nb/nn).

## Current state

| Plugin | Status |
|---|---|
| CATS-plugin | Host sheet mostly resources; demo step labels still English in Rust |
| right-to-roam-camping | `legal_basis` / `notes` still English sentences — migrate to keys |
| imu | `status_text` English — prefer status enum → host strings |
| weather | Icon pack display names English in manifests |
| poi-lookahead / datex | Kotlin HUD/settings → string resources |

Do not move string catalogs into the trusted routing core. Optional future
offline packs: `docs/plugins/i18n-translation-spec.md` (not required for device
locale following).
