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

## Secondary line

When the selected primary differs from local `name`, UIs that have room should
show primary first and local `name` as secondary (search hits, place sheets).

## Search

Findable by local and localized names when those strings are present in the
search document / tag map. See WAIT notes in `inventory.md` for place_index and
graph pack gaps (`name:xx` / `int_name` not stored yet).

## Data limits (do not bypass without approval)

- Graph packs (v9): single `edge_name` from OSM `name` only — no pack format bump
  in this work.
- `place_index.db`: single display `name` plus `alt_name`/`loc_name` in
  `search_doc` — schema migration requires user OK.
- Map tiles (Protomaps / Liberty): often expose `name:*`; style coalesce can use
  them immediately.
