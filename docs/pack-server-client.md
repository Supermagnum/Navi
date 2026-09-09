# Pack server client (connectivity + acquisition + install)

**Status (branch `dev`):** discovery against the public pack host, region-pill
greens, **pack download / sha256 verify / leaf-stem install** (with download
progress), then **on-device Geofabrik PBF download + place-index build** (same
`NameIndex` path as local convert, with phase progress). navi-server does
**not** publish a place index or a real PBF.
[`try_fetch_region_packs`](../core/src/pack_server/fetch.rs) GETs
`manifest.json` + files under `/packs/<region_id>/<generation>/`, remaps bake
stems (`europe_monaco-latest`) to Geofabrik leaf stems (`monaco-latest`), and
writes a `{leaf}.navi-server-install.json` sidecar so planners treat packs as
Ready. [`ensure_place_index_after_pack_install`](../core/src/pack_server/place_index_after.rs)
then fetches `https://download.geofabrik.de/<region>-latest.osm.pbf` (replacing
the pack-install stub) and builds `place_index.db`.

On any pack fetch failure (network, 404, checksum, missing `data_dir`), Navi
falls through to Geofabrik extract download + on-device convert + place index
(`local-bake`).

### Region pills + Download buttons (Tools)

| State | Appearance / label |
|---|---|
| Listed in `current.json` (path or child) **or** local `{leaf}-latest.navi-manifest.json` | Green chip |
| Pack server lists selected path | **Download region** (green) — install packs, then Geofabrik PBF + place index |
| Not on pack server | **Download region + build place index** — Geofabrik + convert + place index |
| Selected path pill-ready | **Check for OSM updates** also green |
| Basemap / DEM buttons | Unchanged |

`discover_pack_catalog` / UniFFI `discoverPackCatalog` feeds the ready-id list.
Use **Refresh pack availability** to re-probe without leaving Tools.

Server contract (ops / URL layout):
[Supermagnum/navi-server `docs/client-fetch.md`](https://github.com/Supermagnum/navi-server/blob/main/docs/client-fetch.md).

---

## Host fallback chain

1. `https://navigate-me.duckdns.org` — tag `server-duckdns`
2. Geofabrik + on-device convert — tag `local-bake`

Per-host connect / discovery timeout: **3s** (`CONNECTIVITY_TIMEOUT`).
`NAVI_PACK_SERVER_BASE_URL` (or UniFFI override) forces a single host for
tests/ops.

---

## What the client does today

Before a Tools **Download region** run:

1. Probe the pack host for `current.json` (inside the background job only).
2. `resolve_region_source` → `RegionSource::Server` or `Local`.
3. If Server + `data_dir` → `try_fetch_region_packs` (install packs; reports
   byte progress via `download_progress`). On success,
   `execute_local_convert = false`, then
   `ensure_place_index_after_pack_install` (Geofabrik PBF + `place_index.db`,
   `force_rebuild` on every pack-server install/update; place-index phases
   report progress).
4. Else Local path: Geofabrik `-latest.osm.pbf` via `provisionRegionData` →
   bind → place index → `ensureIndexedMaps`.

**Tradeoff:** pack-server installs still pull a full Geofabrik extract for
search (on top of published packs). That can be hundreds of MB–GB per region
but is required for a complete place index without server-side FTS publish.

Logs: tag `NaviPack` / `RegionDownloadBg`, including `data_source=…`,
`connectivity_ms` / `fetch_ms`, and place-index `pbf_ms` / `index_ms`.

**UI:** while packs download, Tools shows `downloadProgressSnapshot` labels
such as “Fetching packs (3/74): …”. After packs land, status becomes
“Downloading extract + building place index…” with Geofabrik byte progress,
then place-index phase labels until `ensurePackRegionPlaceIndex` returns.

---

## Generation fields

| Field | Freshness? |
|---|---|
| Top-level `generation` → `catalog_generation` | **No** |
| Per-region `generation` → `ReadyRegion::generation` | **Yes** (when cache logic exists) |

---

## Manual checks

```bash
cargo run -p driver-break-core --bin pack-server-check
cargo test -p driver-break-core --lib pack_server
# Live pack + Geofabrik place-index (network, large downloads):
cargo test -p driver-break-core --test pack_server_place_index_live -- --ignored --nocapture
```
