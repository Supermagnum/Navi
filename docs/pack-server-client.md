# Pack server client (connectivity + acquisition routing)

**Status (2026-09-05, branch `package-test`):** discovery and soft routing are
wired into the Android region-download path. **Pack download / manifest
verify are not implemented yet** — when the host lists a region as ready, Navi
still falls through to Geofabrik extract download + on-device convert.

Server contract (ops / URL layout): 
[Supermagnum/navi-server `docs/client-fetch.md`](https://github.com/Supermagnum/navi-server/blob/main/docs/client-fetch.md).

Related product direction: [`precomputed-index-and-route-cache.md`](precomputed-index-and-route-cache.md).

---

## What the client does today

Before a Tools **Download region** run:

1. `GET {base_url}/current.json` with a short timeout (`pack_server::check_connectivity`).
2. Pure routing: `resolve_region_source(region_id, connectivity)` →
   `RegionSource::Server` or `RegionSource::Local`.
3. If Server → call stub `try_fetch_region_packs` (always soft-fails today) →
   fall through to Local.
4. Local path (unchanged): Geofabrik `-latest.osm.pbf` via
   `provisionRegionData` → bind → place index → `ensureIndexedMaps` /
   `convert_region_packs`.

Failures (timeout, DNS, non-2xx, empty catalog, region missing, stub) are
**silent automatic fallback** — no user-facing hard error, no retry loop
against the pack host. Logs: tag `NaviPack` / `RegionDownloadBg`.

---

## Changing the pack server address

The client never assumes a fixed public hostname in call sites. Resolution
order for the base URL (no trailing slash required; `/current.json` is
appended):

1. Explicit argument (CLI / UniFFI `packServerBaseUrl`)
2. Environment variable `NAVI_PACK_SERVER_BASE_URL`
3. Compile-time default `DEFAULT_PACK_SERVER_BASE_URL` in
   `core/src/pack_server/mod.rs` (currently `http://192.168.1.195`)

| How | Example |
|---|---|
| **Host CLI (one-shot)** | `cargo run -p driver-break-core --bin pack-server-check -- http://192.168.1.50` |
| **Host env (CLI / tests)** | `NAVI_PACK_SERVER_BASE_URL=https://navigate-me.duckdns.org cargo run -p driver-break-core --bin pack-server-check` |
| **Ignored live unit test** | `NAVI_PACK_SERVER_BASE_URL=http://… cargo test -p driver-break-core --lib pack_server::tests::live_pack_host_discovery -- --ignored --nocapture` |
| **Android / UniFFI** | App calls `defaultPackServerBaseUrl()` then `decideRegionAcquisition(regionId, packServerBaseUrl)`. There is **no Tools UI field yet** — to point a device build at another host, either change `DEFAULT_PACK_SERVER_BASE_URL` and rebuild native + APK, or pass a non-null `packServerBaseUrl` from Kotlin (tests already do this for the unreachable case). |
| **Future public host** | Intended later: `https://navigate-me.duckdns.org` (ports not forwarded yet). Prefer env / argument over baking that string into call sites. |

When the host is unreachable or wrong, acquisition **soft-falls back** to
Geofabrik — changing the address does not risk a hard download failure.

---

## Generation fields (do not confuse)

`current.json` has two different strings named “generation”:

| Field | Meaning | Use for freshness? |
|---|---|---|
| Top-level `generation` → `PackCatalog::catalog_generation` | Catalog last-touched marker (publish timestamp **or** a script label such as `migrate-geofabrik-paths`) | **No** |
| Per-region `generation` → `ReadyRegion::generation` | Bake id under `/packs/<region_id>/<generation>/` (typically `20260904T…Z…`) | **Yes** (when cache invalidation exists) |

---

## Code map

| Piece | Location |
|---|---|
| Connectivity + parse | `core/src/pack_server/mod.rs` |
| `resolve_region_source` / `plan_region_acquisition` | `core/src/pack_server/acquisition.rs` |
| CLI smoke | `cargo run -p driver-break-core --bin pack-server-check -- [base_url]` |
| UniFFI | `decide_region_acquisition`, `default_pack_server_base_url` (`navi-ffi`) |
| App hooks | `MainActivity.startRegionDownload`, `RegionDownloadBackground.ensureStarted` |
| Local download | UniFFI `provision_region_data` → `routing::region::provision_region_with_elev_tar` |
| Local convert | UniFFI `ensure_indexed_maps` → `routing::indexed::convert::convert_region_packs` |
| Device tests | `PackServerRoutingInstrumentedTest` |

`execute_local_convert` is **always `true` until pack-fetch is implemented**,
including when `source == Server` (means “stub deferred to local”, not
“server fetch plus local convert”). See TODO on
`RegionAcquisitionPlan::execute_local_convert`.

---

## Manual checks

```bash
# Host catalog
curl -sI http://192.168.1.195/current.json
cargo run -p driver-break-core --bin pack-server-check -- http://192.168.1.195

# Unit tests (no network except ignored live test)
cargo test -p driver-break-core --lib pack_server

# Device (LAN Wi-Fi to pack host)
./scripts/build-android-native.sh aarch64-linux-android release
./gradlew :app:connectedDebugAndroidTest \
  -Pandroid.testInstrumentationRunnerArguments.class=no.navi.app.PackServerRoutingInstrumentedTest
```
