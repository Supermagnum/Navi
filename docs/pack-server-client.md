# Pack server client (connectivity + acquisition routing)

**Status (branch `map-data`):** discovery, LAN → duckdns host chain, and soft
routing are merged from `package-test` with DATEX HTTP helpers preserved.
**Pack download / manifest verify are not implemented yet** —
[`try_fetch_region_packs`](../core/src/pack_server/acquisition.rs) still
soft-fails; when the host lists a region as ready, Navi falls through to
Geofabrik extract download + on-device convert (`local-bake`).

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
| `http_get_text` / `http_get_bytes` / `base_url` / `USER_AGENT` / `PackServerError` | Shared HTTP helpers (DATEX + future pack fetch) |
| `plan_region_acquisition` | Routing + stub fetch + `data_source` tag |

---

## What the client does today

Before a Tools **Download region** run:

1. Probe the host chain for `current.json`.
2. `resolve_region_source` → `RegionSource::Server` or `Local`.
3. If Server → stub `try_fetch_region_packs` → fall through to Local.
4. Local path: Geofabrik `-latest.osm.pbf` via `provisionRegionData` → bind →
   place index → `ensureIndexedMaps`.

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
