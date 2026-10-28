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
- Place index: `names_json` column + `set_place_search_locale`
- Guidance / route summary / "Currently on": offline PMTiles lookup (option D)

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
- WAL crash / upgrade notes: [`place-index-wal.md`](place-index-wal.md).

## Road names in guidance — option D (offline PMTiles)

**Option C (place-source sidecar) dropped.** Place-source for Østlandet had only
27 `highway=*` ways and essentially no useful `name:*` (1 plaza `name:ko`). It
is a place/POI extract, not the routable network.

**Option D (shipped):** after each plan / recalculation, read the region's
downloaded Protomaps `.pmtiles` directly (Rust `pmtiles` + MVT decode — not
MapLibre's rendered tiles). For each maneuver / street label, load the tile at
the archive max zoom near that point, match a `roads` feature by local `name`
(+ `ref` when present), then apply the selection order above. Results are cached
in memory for the active route only. No new files, no extra downloads, no pack
format bump, no navi-server change.

No tile match or no `name:*` for the device language → keep the pack local name
(option B fallback).

### Verification counts (z14 samples, Protomaps `20261028`)

| Region | Road features | Named | With `name:*` / `int_name` | Notes |
| --- | ---: | ---: | ---: | --- |
| Østlandet / Oslo | 3672 | 3039 | 2 | Almost no localized road tags (Latin local `name`) |
| Tunisia / Tunis | 4157 | 3574 | 3019 | Rich `name:fr` / `name:ar` (and some `name:en`) |

Tablet Ostlandet archive (`europe_norway_ostlandet.pmtiles`, maxzoom 15) matches
the sparse `name:*` pattern on sampled Oslo tiles.

## Data limits (do not bypass without approval)

- Graph packs (v9): single `edge_name` from OSM `name` only — no pack format bump
  in this work.
- Map tiles (Protomaps / Liberty): expose `name` / `name:*` on the `roads` layer
  where OSM has them; used for map labels and option D guidance lookup.
