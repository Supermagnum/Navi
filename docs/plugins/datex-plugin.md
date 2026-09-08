# DATEX plugin (`datex`)

**Status:** host-owned client in `driver-break-core::datex` with UniFFI +
Android Tools/Map Plugins UI (default **OFF**). Host discovery reuses
[`pack_server::check_connectivity_chain`](../../core/src/pack_server/mod.rs)
(LAN → duckdns). WASM guest scaffold under `plugins/datex/` is **not linked**
into the product APK.

**Path:** `docs/plugins/datex-plugin.md`  
**Server ops:** [navi-server `docs/datex-npra.md`](https://github.com/Supermagnum/navi-server/blob/main/docs/datex-npra.md)

---

## Host fallback (same tags as map packs)

| Order | Base | `data_source` tag |
|---|---|---|
| 1 | `http://192.168.1.195` | `server-lan` |
| 2 | `https://navigate-me.duckdns.org` | `server-duckdns` |
| fail | (no traffic this session) | `none` |

There is **no** `local-bake` path for live traffic. Per-host connect timeout is
3s ([`CONNECTIVITY_TIMEOUT`](../../core/src/pack_server/mod.rs)). After a hop
succeeds, DATEX **sticks** to that base for the process until it fails, then
re-runs the chain.

DATEX availability on a resolved host uses
[`probe_path`](../../core/src/pack_server/mod.rs) on `/datex/source.json`
(same probe shape as [`probe_current_json`](../../core/src/pack_server/mod.rs)).

---

## Network economy

| Rule | Behaviour |
|---|---|
| Poll floor | ≥ 300 s (server Situation TTL) |
| Route-active only | No fetch without a planned corridor |
| Wi-Fi only | Default on; cellular skipped when set |
| `source.json` fingerprint | Unchanged body → skip `GetSituation.xml` |
| Persist | `datex_cache/` under app data (XML + freshness marker) |
| Hosts down | Clear actives; do **not** show stale cache as active |

---

## Configuration (`DatexConfig`)

| Field | Default | Meaning |
|---|---|---|
| `enabled` | **`false`** | Master switch |
| `use_discovery_chain` | `true` | LAN → duckdns (override host/port when false) |
| `wifi_only` | `true` | Skip pull off Wi-Fi |
| `min_poll_interval_secs` | `300` | Clamped ≥ server TTL |
| `cache_dir` | optional | Persist last snapshot |

---

## Routing impact (`DatexImpact`)

Each parsed situation gets an `impact` bucket used by the route planner
(mirrors [`TollPolicy`](../../core/src/routing/toll.rs)):

| Bucket | Planner effect |
|---|---|
| `Ignore` | No graph change (overlay only) |
| `Penalize` | Nearby edges keep searchable; cost × `DATEX_PENALIZE_MULT` (50) |
| `Block` | Nearby edges hard-excluded from A* |

**Field → bucket mapping** (first match wins), implemented in
[`classify_impact`](../../core/src/datex/impact.rs):

| Priority | Condition | Bucket |
|---|---|---|
| 1 | Free-text `comment` or `locationDescription` contains a closure cue: `stengt`, `sperret`, `road closed`, `carriageway closed`, `fully closed`, or `closed` together with `road` / `vegen` / `vei` (case-insensitive) | **Block** |
| 2 | `impact/numberOfLanesRestricted` ≥ 2 | **Block** |
| 3 | `impact/numberOfLanesRestricted` ≥ 1 | **Penalize** |
| 4 | Record `severity` ∈ {`medium`, `high`, `highest`} | **Penalize** |
| 5 | Otherwise (e.g. `severity=none` / `low` with 0 lanes) | **Ignore** |

Fixture examples (`espa-atnbru-getsituation.xml`):

| Situation | Key fields | Impact |
|---|---|---|
| Oslo “vegen er stengt” | lanes=2, text `stengt` | Block |
| Ellingrud moving works | lanes=1, severity=low | Penalize |
| Espatunnelen / Akselstua / Langmoen | lanes=0, severity=none | Ignore |

**Hard rule:** only **active** (time-window) situations reach the planner.
Build constraints with [`planner_impacts`](../../core/src/datex/impact.rs) on the
active corridor slice, then set `RouteOptions.datex_impacts`. Edges within
`DATEX_IMPACT_RADIUS_M` (250 m) of a situation display point are matched via
[`edge_distance_m`](../../core/src/routing/graph/road_near.rs).

---

## Errors

- Transport / HTTP → [`PackServerError`](../../core/src/pack_server/mod.rs)
- Malformed `source.json` / DATEX XML → [`DatexFetchError`](../../core/src/datex/fetch.rs)
  (`Source` / parse warnings). **Do not** overload pack-server variants for XML.

---

## Tests

```bash
cargo test -p driver-break-core --test datex_espa_atnbru
cargo test -p driver-break-core --test datex_host_chain
cargo test -p driver-break-core datex::impact
```
