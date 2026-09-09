# DATEX plugin (`datex`)

**Status:** host-owned client in `driver-break-core::datex` with UniFFI +
Android Tools/Map Plugins UI (default **OFF**). Host discovery reuses
[`pack_server::check_connectivity_chain`](../../core/src/pack_server/mod.rs)
(public pack host). WASM guest scaffold under `plugins/datex/` is **not linked**
into the product APK.

**Coverage today: Norway only.** The live feed is NPRA DATEX II (Statens
vegvesen). Other countries’ DATEX / road-authority feeds are not wired; enabling
the plugin outside Norway still only surfaces Norwegian situations when the
navi-server redistributor is polling NPRA.

**Path:** `docs/plugins/datex-plugin.md`  
**Server ops:** [navi-server `docs/datex-npra.md`](https://github.com/Supermagnum/navi-server/blob/main/docs/datex-npra.md)

---

## Host fallback (same tags as map packs)

| Order | Base | `data_source` tag |
|---|---|---|
| 1 | `https://navigate-me.duckdns.org` | `server-duckdns` |
| fail | (no traffic this session) | `none` |

There is **no** `local-bake` path for live traffic. Per-host connect timeout is
3s ([`CONNECTIVITY_TIMEOUT`](../../core/src/pack_server/mod.rs)). After a hop
succeeds, DATEX **sticks** to that base for the process until it fails, then
re-runs discovery.

DATEX availability on a resolved host uses
[`probe_path`](../../core/src/pack_server/mod.rs) on `/datex/source.json`
(same probe shape as [`probe_current_json`](../../core/src/pack_server/mod.rs)).

---

## Adding DATEX services and cadence (server)

Credentials and upstream polling live on **navi-server**, never on the device.
Full operator detail:
[navi-server `docs/datex-npra.md`](https://github.com/Supermagnum/navi-server/blob/main/docs/datex-npra.md).

### Enable (interactive)

```bash
sudo /media/navi/navi-server/scripts/setup-server.sh --apply-datex
```

Prompts for NPRA username/password, writes secrets mode `0600`, and installs
`systemd/navi-datex-npra.timer` / `.service`. Preview: `setup-server.sh --dry-run`.

### Enable (file edit)

1. Create `data/secrets/datex_npra.env` with `NAV_DATEX_USERNAME` /
   `NAV_DATEX_PASSWORD` (mode `0600`).
2. In `data/config.env`: `NAVI_DATEX_NPRA_ENABLED=1`, point
   `NAVI_DATEX_NPRA_SECRETS_FILE` at that file, set a real contact in
   `NAVI_DATEX_NPRA_USER_AGENT`.
3. Install units from `systemd/navi-datex-npra.*`, then
   `systemctl enable --now navi-datex-npra.timer`.

### Cadence (poll intervals)

| Config key | Default | Role |
|---|---|---|
| `NAVI_DATEX_NPRA_POLL_INTERVAL_SECS` | `300` | Fallback / jitter base for endpoints without an explicit map entry |
| `NAVI_DATEX_NPRA_ENDPOINT_INTERVALS` | Situation + TravelTime `300`; Weather `600`; CCTVSiteTable `43200` (12 h) | Per-endpoint poll cadence (`Endpoint=secs`) |
| `NAVI_DATEX_NPRA_USE_IF_MODIFIED_SINCE` | `1` | Conditional GET to skip unchanged bodies |

Default upstream endpoints (comma-separated in `NAVI_DATEX_NPRA_ENDPOINTS`):
`GetSituation`, `GetTravelTimeData`, `GetMeasuredWeatherData`,
`GetCCTVSiteTable`. Path pattern:
`{BASE}/datexapi/{Endpoint}/pullsnapshotdata`.

**Add or retune a service:** append the DATEX endpoint name to
`NAVI_DATEX_NPRA_ENDPOINTS`, set its seconds in
`NAVI_DATEX_NPRA_ENDPOINT_INTERVALS`, restart/reload the timer unit, and confirm
files appear under DocumentRoot `/datex/` (`source.json` + `<Endpoint>.xml`).
SOAP filtered pulls are out of scope. Uninstall:
`scripts/uninstall-datex-npra.sh` (`--purge` also drops secrets).

### Client cadence (Navi app)

Independent of the server timer: the host client refuses polls more often than
[`DatexConfig.min_poll_interval_secs`](../../core/src/datex/config.rs) (default
**300 s**, clamped to the Situation TTL). Route-active + Wi-Fi-only defaults
still apply (see [Network economy](#network-economy)).

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
| `use_discovery_chain` | `true` | Public pack host (override host/port when false) |
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
| `Penalize` | Nearby edges keep searchable; cost × `penalize_mult` (delay-scaled when `delayTimeValue` is present, else `DATEX_PENALIZE_MULT` = 50) |
| `Block` | Nearby edges hard-excluded from A* |

Classification is **type-aware**: `xsi:type` selects the rule, then structured
fields and a narrow free-text check refine it. Implemented in
[`classify_impact`](../../core/src/datex/impact.rs).

These defaults come from one live GetSituation snapshot (2797 records). Counts
below are **relative frequency**, not a guarantee that every type appears in every
poll. Treat them as a starting mapping, not verified ground truth.

### NPRA live types (17)

| `xsi:type` | Default classification | Reasoning / override |
|---|---|---|
| `RoadOrCarriagewayOrLaneManagement` | **Penalize** (scale by `delayTimeValue` when present, else lanes/severity) | Only type with real numeric delay data in the snapshot. If `delays` is present but `delayTimeValue` is absent, fall back to the lane/severity heuristic (may Ignore). Closure text still **Block**. |
| `MaintenanceWorks` | **Penalize** when `lanes>0` or `severity` ∈ {medium, high, highest}, else **Ignore** | Severity always populated in the snapshot. Closure text (**Block**) is an adjustment vs the starting table: the Espa fixture Oslo E6 record is this type with “vegen er stengt”. |
| `GeneralNetworkManagement` | **Penalize** when `lanes>0`, else **Ignore** | Mostly temporary traffic lights / manual directing — usually passable but slower |
| `SpeedManagement` | **Ignore** (structurally exempt) | Speed-limit change alone must not reroute, even if future records add severity/lanes/closure text |
| `ReroutingManagement` | **Penalize** | `followDiversionSigns` implies the direct route is already compromised |
| `ConstructionWorks` | **Block** if free-text indicates closure, else **Penalize** | Lanes always 0 in the sample — cannot rely on lane count |
| `EnvironmentalObstruction` | **Block** | Rockfall / fallen trees / landslip — treat as unpredictable full blockage |
| `PublicEvent` | **Block** if `lanes>0` or free-text says closed, else **Penalize** | Sample includes a lanes=2 / closed case |
| `TransitInformation` | **Ignore** (structurally exempt) | Ferry timetables; not road-related. Must not surface to routing even if future records add severity/lanes |
| `InfrastructureDamageObstruction` | **Block** if strong closure text or `lanes>=2`, else **Penalize** | Softened after national-snapshot review: traffic lights + reduced speed, or “kan passere” partial reopen, must not hard-exclude |
| `NonWeatherRelatedRoadConditions` | **Penalize** | `slipperyRoad` — real hazard, not a closure |
| `AnimalPresenceObstruction` | **Penalize** | `animalsOnTheRoad` — caution/slowdown, not blockage |
| `GeneralObstruction` | **Penalize**, **Block** if free-text suggests full blockage | `objectOnTheRoad` — ambiguous by nature |
| `Accident` | **Block** | Sparse structured data (`severity=unknown` typical). Presence of the record is enough |
| `VehicleObstruction` | **Penalize** | `brokenDownVehicle` — usually one lane, not full closure |
| `PoorEnvironmentConditions` | **Penalize**, scale with `windSpeed` (m/s) when present | `strongWinds` example — relevant mainly for high-profile / vulnerable routing |
| `RoadsideAssistance` | **Ignore** (structurally exempt) | `vehicleRecovery` — informational, not a hazard to the general route |

`planner_impacts` drops **Ignore**. `SpeedManagement`, `TransitInformation`, and
`RoadsideAssistance` never escalate.

### Delay scaling (`RoadOrCarriagewayOrLaneManagement`)

DATEX `delayTimeValue` is seconds. When present it sets `penalize_mult` via
[`delay_penalize_mult`](../../core/src/datex/impact.rs):

- 30 seconds → about ×5.75 (cheap)
- 30 minutes (1800 s) → ×50 (`DATEX_PENALIZE_MULT`)
- longer delays continue up to ×150

When `delays` exists without `delayTimeValue`, use the lane/severity heuristic
for that record instead of the type-default Penalize.

### Free-text closure phrases

Narrow keyword check (case-insensitive substring) on `comment` and
`locationDescription`. **Not** a general NLP pass. List:
[`CLOSURE_PHRASES`](../../core/src/datex/impact.rs) — extend when live comments
surface variants:

| Phrase | Notes |
|---|---|
| `vegen er stengt` | Canonical NPRA Norwegian phrasing |
| `veien er stengt` | Bokmal spelling variant |
| `vegen stengt` / `veien stengt` | Shorter full-road forms |
| `stengt i periode` | Intermittent full closure windows |
| `helt stengt` / `helstengt` | Explicit total closure |
| `sperret` | Blocked / barricaded |
| `road closed` / `carriageway closed` / `fully closed` | English |
| `closed` together with `road` / `vegen` / `vei` | Combined English/Norwegian |

**Not matched:** bare `stengt` (over-matches `ett stengt kjørefelt` / `Et felt stengt`).

**Risk exclusions** ([`CLOSURE_RISK_EXCLUSIONS`](../../core/src/datex/impact.rs)) — never Block from text alone; fall through to the type’s non-closure rule:

| Exclusion | Notes |
|---|---|
| `fare for stengt` | Risk of closure (e.g. strong wind), not closed now |
| `kan bli stengt` / `kunne bli stengt` | Conditional / possible closure |
| `kan bli helt stengt` / `kunne bli helt stengt` | Same, total-closure wording |

Applied as a **Block** override on types that are allowed to escalate. Types that
need free-text because structured fields are insufficient:
`ConstructionWorks`, `PublicEvent`, `GeneralObstruction`, and (with lanes≥2
also enough) `InfrastructureDamageObstruction`. It also applies to
`MaintenanceWorks` (see Oslo E6 fixture). Structurally exempt types skip this
check.

### Schema-valid types that NPRA does not publish

`AbnormalTraffic` and `WeatherRelatedRoadConditions` are schema-valid but have
not appeared in NPRA's live feed. They are classified with the generic
lane/severity/closure heuristic so an unexpected record does not panic. Do not
design routing behaviour around them showing up.

**Unrecognized / future `xsi:type`:** parse succeeds, record is kept for overlay,
impact is **Ignore**, `unrecognized_xsi_type=true`, and a `NaviDatex` warning is
logged. The record is not dropped without trace.

### Convoy / escort (not supported)

NPRA's GetSituation feed has **no** convoy or escort concept: no
`AuthorityOperation` convoy payload, no `WinterDrivingManagement`, and no
convoy-type enum values. This plugin does **not** implement convoy or
chain-requirement handling against DATEX. If that information is wanted later,
it needs a different data source.

### Corridor fixture examples (`espa-atnbru-getsituation.xml`)

All five records in this fixture are `MaintenanceWorks`:

| Situation | Key fields | Impact |
|---|---|---|
| Oslo “vegen er stengt” | lanes=2, text `vegen er stengt` | Block (free-text) |
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
cargo test -p driver-break-core --test datex_classify_types
cargo test -p driver-break-core datex::impact
```
