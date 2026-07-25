# Unicode road / place names (æ å ø ä ü …)

Norwegian and German road names routinely include characters outside ASCII.
This doc records the **end-to-end** check: corruption at any stage is a real
bug (search miss or wrong HUD text), not a cosmetic font quirk.

Related: [`current-street.md`](current-street.md), [`approach-instructions.md`](approach-instructions.md).

---

## Pipeline

| Stage | Mechanism | Status |
|---|---|---|
| OSM PBF tags | `osmpbf` → Rust `String` (UTF-8); `name` / `ref` cloned in graph build (`bbox_build` / `builder`) and search index | OK — no Latin-1 reinterpretation |
| Graph cache | Serde JSON/bincode of `Option<String>` for `name` / `road_ref` | OK |
| Sim / maneuver JSON | `serde_json` emits UTF-8 (`Mjøsvegen` stays intact) — unit-tested in `guidance_path` | OK |
| FTS5 place index | Default FTS5 tokenizer is **unicode61** (not ASCII-only). Stored names keep original glyphs. Prefix `MATCH` works when the query uses the real characters (`Mjøs*`, `Bjørn*`, `æ*`) | Content OK; see [diacritic folding](#fts5-diacritic-folding-product-behavior) |
| UniFFI Rust → Kotlin | UniFFI maps Rust UTF-8 ↔ Kotlin `String` (UTF-16); no lossy `as_str` casts on name fields | OK (verified by round-trip tests + live search hits) |
| Compose HUD | Material 3 / platform default typeface (no custom HUD font). MapLibre uses bundled **Noto Sans** glyph PBFs for basemap labels (Latin Extended coverage) | OK for HUD chrome |

### FTS5 diacritic folding (product behavior)

SQLite’s default **unicode61** tokenizer removes many diacritics for matching.
Verified on the Ostlandet place index and in-memory unit tests:

| Character | ASCII fold? | Example |
|---|---|---|
| `å` / `Å` | Yes → `a` | `Eldabu*` finds **Eldåbu** |
| `ä` / `ü` / `ö` | Yes → `a` / `u` / `o` | `Muller*` finds **Müller…** |
| `æ` / `Æ` | **No** | `Baerum*` misses **Bærum**; type `Bærum` |
| `ø` / `Ø` | **No** | `Loten*` misses **Løten**; type `Løten` |

This is **not** mojibake and **not** a UniFFI bug — storage and HUD still show
the correct glyphs. It affects To/Via search when the user types an ASCII
approximation of æ/ø. Hiking UI tests already rely on å-folding
(`Eldabu` → Eldåbu). Do not “fix” æ/ø by corrupting stored names; any
change would be an explicit search-normalization feature.

**German ä/ö/ü:** Ostlandet fixtures are Norway-centric. ä/ö/ü are covered by the
same UTF-8 path and unit strings; live German corridor screenshots are a
**known coverage gap** until a DE extract is used — not a pipeline defect.

---

## Real fixture examples (Østlandet)

From `place_index_search_check.db` / search checks (not invented):

| Character | Example | Notes |
|---|---|---|
| ø | `Mjøsvegen`, `Circle K Mjøsstranda`, `Bjørnhollia` | FTS prefix match confirmed |
| å | `Trollåsveien`, `Grimåsfeltet`, `Kringsjå` | In index / approach tests |
| æ / Æ | `Ævongsli …`, `Østre Æra Camping` | FTS `æ*` returns hits |

Unit tests: `nav::current_road_preserves_norwegian_special_chars`,
`guidance_path::samples_preserve_norwegian_street_utf8`,
`search::fts_matches_norwegian_special_chars`,
`search::fts_unicode61_folds_aa_but_not_ae_oe` (in-memory).

Emulator evidence (`CurrentStreetInstrumentedTest`):

| Char | Shot |
|---|---|
| ø | [`hud/hud_current_street_mjosevegen.png`](images/hud/hud_current_street_mjosevegen.png) |
| å | [`hud/hud_current_street_trollaas.png`](images/hud/hud_current_street_trollaas.png) |
| Æ | [`hud/hud_current_street_aevongsli.png`](images/hud/hud_current_street_aevongsli.png) |

---

## If something breaks

| Symptom | Likely stage |
|---|---|
| Search finds ASCII fold but not `ø` | Expected for æ/ø under unicode61 — type the real letter; see folding table above |
| Search misses `Eldåbu` when typing `Eldabu` | Unexpected (å should fold) — check FTS tokenizer / index rebuild |
| Mojibake in HUD only | Compose / wrong encoding when building the string in Kotlin |
| JSON has `\u00f8` but UI shows `?` | Font / TextField transformation (unlikely with system font) |
| Name correct in Rust tests, wrong in APK | Stale `libnavi.so` / UniFFI bindings — rebuild per [`android-build.md`](android-build.md) |
