# OSM extract updates (Geofabrik)

Navi is offline-first: **every network call is an enhancement**, never required
for core routing on an already-downloaded extract. Map data is never swapped in
the background without an explicit user action.

## Cadence

| Mechanism | Behaviour |
|---|---|
| **Check for updates** (Tools) | User-triggered. Compares local metadata to Geofabrik `state.txt`. Downloads **nothing** until Apply. |
| **Weekly reminder** (opt-in) | If enabled, the UI may surface that a check is due after 7 days. Still does **not** download or apply silently. |
| **Apply update** | User-confirmed. Diff-chain or full re-download per plan. |

## Diff vs full re-download

1. Local extract must be bound to a Geofabrik region path (e.g.
   `europe/norway/ostlandet`) via `region_meta.json` / `bind_geofabrik_region`.
2. Remote `…/{region}-updates/state.txt` supplies `sequenceNumber` + `timestamp`.
3. If local sequence is known and behind remote, and the extract is **newer than
   28 days** (`STALENESS_FULL_REDOWNLOAD_DAYS`):
   - **If `osmium` is available:** plan = fetch consecutive `.osc.gz` files and
     apply via `osmium apply-changes` (`method=osc_osmium`).
   - **If `osmium` is not available:** plan = full `*-latest.osm.pbf` download
     immediately — **no** `.osc.gz` files are fetched (they cannot be applied).
4. If the extract is **≥ 28 days** old, sequence unknown, or the diff chain would
   exceed 400 files: plan = **full** `*-latest.osm.pbf` re-download (unchanged).

Custom corridor cuts without Geofabrik metadata return `Unsupported` — re-run
provision / re-cut instead of pretending Geofabrik diffs apply to a bbox extract.

## After apply

Graph cache and place FTS index are invalidated so the next route rebuilds from
the new PBF.

## Code

- `core/src/routing/osm_update.rs`
- UniFFI: `checkOsmUpdates`, `applyOsmUpdate`, `bindGeofabrikRegion`,
  `setOsmWeeklyReminder`, `osmWeeklyReminderDue`
