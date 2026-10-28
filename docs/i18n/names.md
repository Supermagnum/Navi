# Localized place and road names

Shared selection rule for UI labels, search presentation, guidance street lines,
map styles, and plugin POI names. Refs (`ref`, `int_ref`) are **never**
translated; they may still be shown beside a name.

## Selection order

Given device locale BCP 47 (e.g. `nb-NO`, `en-US`, `ar`):

1. `name:<full locale>` (normalized, e.g. `name:nb-no` / `name:nb-NO` if present)
2. `name:<language>` — for Norwegian also try `name:no` after `name:nb` / `name:nn`
3. If the device **language** uses a Latin script and the local `name` is mostly
   non-Latin (or missing): `int_name`, then `name:en`, then `name:latin`
4. `name`

Implementation:

- Rust: `driver_break_core::i18n::localized_name` / `localized_name_with_secondary`
- MapLibre: `BasemapLocalizedLabels` coalesce chain rebuilt on style load and
  locale change (approximation of step 3 without per-feature script tests)
- Place index: `names_json` column + `set_place_search_locale` (see below)

## Secondary line

When the selected primary differs from local `name`, UIs that have room should
show primary first and local `name` as secondary (search hits, place sheets).
Place-index hits format this as `primary · local` in the `name` field when the
locale is set.

## Search

Findable by local, alt/loc, and localized names when those strings are present in
`search_doc` (filled at index build from `name:*` / `int_name`).

## place_index: `names_json` (lazy, non-destructive)

- Column: `names_json TEXT NOT NULL DEFAULT ''` — JSON object of tag keys
  (`name:en`, `int_name`, …) to values.
- Added with `ALTER TABLE … ADD COLUMN` on writer open. Does **not** bump
  `PLACE_INDEX_SCHEMA_VERSION` (v6 intact checks stay green; no rebuild; no
  reset of `name_index_build.complete`).
- New downloads / region updates populate `names_json` and expand `search_doc`.
- Existing regions keep empty `names_json` until the next region update.

## Road-name sidecar (option C) — STOPPED

Checked live place-source for `europe/norway/ostlandet`
(`europe_norway_ostlandet-latest.navi-place-source.osm.pbf`, ~22 MB):

| Metric | Count |
| --- | --- |
| `highway=*` ways | 27 |
| Named highways | 27 |
| Highways with real `name:*` (excl. etymology) | 1 (`name:ko` on a plaza) |

Place-source is a place/POI extract: ~208k named nodes with rich `name:*`, but
**not** the routable highway network. Guidance edge labels key on pack
`edge_name` / `ref` (OSM `name` only in v9). Building a name+ref sidecar from
place-source would not cover roads.

**Missing for option C:** highway ways with localization tags in the
place-source (or other client-already-downloaded) PBF. Do not fetch extra data
or change navi-server. Pack format bump (option A) remains rejected.

Fallback: local pack `edge_name` / `ref` (option B) until a future approved
data path exists.

## Data limits (do not bypass without approval)

- Graph packs (v9): single `edge_name` from OSM `name` only — no pack format bump
  in this work.
- Map tiles (Protomaps / Liberty): often expose `name:*`; style coalesce can use
  them immediately.
