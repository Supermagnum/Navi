# Pack server client (connectivity + acquisition + install)

**Status (branch `map-data`):** discovery, LAN → duckdns host chain, region-pill
greens, and **pack download / sha256 verify / leaf-stem install** are in.
[`try_fetch_region_packs`](../core/src/pack_server/fetch.rs) GETs
`manifest.json` + files under `/packs/<region_id>/<generation>/`, remaps bake
stems (`europe_monaco-latest`) to Geofabrik leaf stems (`monaco-latest`), and
writes a `{leaf}.navi-server-install.json` sidecar so planners treat packs as
Ready without a real extract PBF.

On any fetch failure (network, 404, checksum, missing `data_dir`), Navi falls
through to Geofabrik extract download + on-device convert + place index
(`local-bake`).

### Region pills + Download buttons (Tools)

| State | Appearance / label |
|---|---|
| Listed in `current.json` (path or child) **or** local `{leaf}-latest.navi-manifest.json` | Green chip |
| Pack server lists selected path | **Download region** (green button) — install packs only |
| Not on pack server | **Download region + build place index** — Geofabrik + convert + place index |
| Selected path pill-ready | **Check for OSM updates** also green |
| Basemap / DEM buttons | Unchanged |

`discover_pack_catalog` / UniFFI `discoverPackCatalog` feeds the ready-id list.
Use **Refresh pack availability** to re-probe without leaving Tools.

Server contract (ops / URL layout):
[Supermagnum/navi-server `docs/client-fetch.md`](https://github.com/Supermagnum/navi-server/blob/main/docs/client-fetch.md).

---

## Host fallback chain

1. `http://192.168.1.195` — tag `server-lan`
2. `https://navigate-me.duckdns.org` — tag `server-duckdns`
3. Geofabrik + on-device convert — tag `local-bake`

Per-host connect / discovery timeout: **3s** (`CONNECTIVITY_TIMEOUT`).
`NAVI_PACK_SERVER_BASE_URL` (or UniFFI override) collapses the chain to a
single host for tests/ops.

---

## API merge notes (DATEX + package-test)

| Symbol | Role |
|---|---|
| `check_connectivity` / `_blocking` | Catalog-aware discovery → `Connectivity` |
| `check_connectivity_chain` | LAN → duckdns ordered probe |
| `probe_current_json` | DATEX-era bool liveness probe (renamed; does not shadow catalog API) |
| `http_get_text` / `http_get_bytes` / `base_url` / `USER_AGENT` / `PackServerError` | Shared HTTP helpers (DATEX + pack fetch) |
| `try_fetch_region_packs` | Manifest + file GET, sha256, leaf remap, install |
| `plan_region_acquisition` | Routing + fetch + `data_source` tag |

---

## What the client does today

Before a Tools **Download region** run:

1. Probe the host chain for `current.json`.
2. `resolve_region_source` → `RegionSource::Server` or `Local`.
3. If Server + `data_dir` → `try_fetch_region_packs` (install). On success,
   `execute_local_convert = false` (skip place index / local bake).
4. Else Local path: Geofabrik `-latest.osm.pbf` via `provisionRegionData` →
   bind → place index → `ensureIndexedMaps`.

Logs: tag `NaviPack` / `RegionDownloadBg`, including `data_source=…`.

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
```
