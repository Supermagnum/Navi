# Future-proofing audit (2026-07-29)

This document is the canonical tracked output of the 2026-07-29 future-proofing audit.
It exists to keep findings and priority ordering in one discoverable place, since the
audit itself identified documentation sprawl as a project risk.

Scope: investigation findings plus the 2026-07-29 fix pass against the prioritized
action table. Update the action table status / last-verified dates when items close.

## Status legend

- **Action required**: finding needs follow-up work.
- **Informational (healthy)**: finding was checked and is currently acceptable.
- **Resolved (fix pass)**: addressed in the 2026-07-29 prioritized fix pass (see action table).

## 1) Hardcoded external references

### Findings

- **Action required**: Protomaps fallback URL is a dated fixed value in `core/src/routing/basemap/extract.rs` (`PROTOMAPS_PLANET_FALLBACK_URL = https://build.protomaps.com/20260722.pmtiles`), and is reused as `DEFAULT_PMTILES_BASE_URL` in `core/src/routing/basemap/regions.rs`. Dynamic resolution exists (`resolve_planet_url_blocking`), but the fallback remains date-pinned.
- **Action required**: Mapterhorn runtime endpoints are fixed constants in `app/src/main/java/no/navi/app/MapterhornTerrain.kt` (`TILEJSON_URL`, `MAPTERHORN_PLANET_URL`, and tile-template URLs). No local mirror/failover source is defined in code.
- **Resolved (fix pass)**: Geofabrik latest-PBF URL construction is centralized in `core/src/routing/osm_update.rs` (`geofabrik_latest_pbf_url`, `geofabrik_updates_base`) and exposed via UniFFI (`geofabrik_latest_pbf_url` / `geofabrik_updates_base_url`). `MainActivity.kt` no longer interpolates the Geofabrik host string inline.
- **Informational (healthy)**: Core DEM download sources are centralized by provider module and clearly scoped in code: `core/src/routing/elevation/sources/copernicus.rs`, `core/src/routing/elevation/sources/srtm.rs`, and `core/src/routing/elevation/sources/viewfinder.rs`.
- **Informational (healthy)**: OpenFreeMap Liberty endpoint is centralized in `app/src/main/java/no/navi/app/BasemapStyleResolver.kt` (`LIBERTY_URL`), not scattered through unrelated files.
- **Action required**: Some pinned tool/dependency versions do not carry local rationale comments near pin sites (`build.gradle.kts`, `app/build.gradle.kts`, `gradle/wrapper/gradle-wrapper.properties`), unlike the MapLibre Vulkan pin which is explicitly justified in `README.md` and `docs/map-styles.md`.

## 2) Plugin/core boundary discipline

### Findings

- **Informational (healthy)**: Core-vs-plugin placement is mostly consistent with `docs/architecture.md`:
  - Hard constraints in core (for example truck HOS logic in `core/src/routing/rest/truck_multi_day.rs`, pack selection in `core/src/routing/rest/hos_jurisdiction.rs`).
  - Advisory/vendor-specific extensions kept in plugin specs (`docs/plugins.md` and `docs/plugins/*.md`).
- **Informational (healthy)**: Network-following preference belongs in core route-costing and is correctly implemented there (`core/src/routing/graph/network_pref.rs`, persisted via `core/src/storage/config_store.rs` and exposed through `navi-ffi/src/lib.rs`).
- **Informational (healthy)**: `HostApi` has not accumulated bespoke plugin-specific methods. Current interface remains narrow and reusable in `plugin-host/src/abi.rs` (`position_read`, `poi_query`, `poi_write`, `log`).
- **Resolved (fix pass)**: HOS jurisdiction pack selection uses shared offline ISO rings (`country_iso_at` / `country_polys.rs`), not elevation bbox heuristics. `gb` is excluded from `EC561_FAMILY` (decline → Unknown). Stale “interim” framing removed from `hos_jurisdiction.rs`.

## 3) Standards vs vendor lock-in audit

### Findings

- **Informational (healthy)**: Open standards and open formats are primary in core paths:
  - OSM PBF, SQLite/FTS5, PMTiles, and open geodata pipelines across `core/`, `navi-ffi/`, and related docs (`docs/API.md`, `docs/PROTOCOLS.md`, `docs/map-styles.md`).
- **Informational (healthy)**: Project-defined open protocol exists for wired DIY e-bike telemetry (`docs/ebike-telemetry-diy.md`, `$NAVIPWR`), documented as open and vendor-neutral.
- **Action required**: Vendor/service dependencies still exist for several critical acquisition paths:
  - Geofabrik regional extract/update service.
  - Mapterhorn TileJSON/planet/tile endpoints.
  - OpenFreeMap online Liberty style endpoint.
  - Earthdata-token path for SRTM branch.
  If these change or disappear, existing local/offline data remains usable, but provisioning/refresh capabilities degrade.
- **Informational (healthy)**: Degradation model is mostly explicit in docs: offline-first routing remains possible once local extracts exist (`docs/architecture.md`, `docs/osm-updates.md`, `README.md`).

## 4) Dependency and toolchain hygiene

### Findings

- **Informational (healthy)**: `cargo deny` and `cargo audit` are enforced in CI, not only configured:
  - Workflow steps exist in `.github/workflows/ci.yml`.
  - Latest observed successful run (`30402861748`) included passing `cargo deny check` and `cargo audit`.
- **Resolved (fix pass)**: Dependency maintenance watch process documented in `docs/CONTRIBUTING.md` (quarterly checklist; near-term flags `wee_alloc`, `gpsd_proto`; lower-priority `osm4routing` / `geotiff`). No immediate crate swap required — replacements scheduled for when guests / GPS wiring next change.
- **Resolved (fix pass)**: Android API 36 baseline **plan** recorded in [`android-api36-plan.md`](android-api36-plan.md) (bump not executed in this pass). Project remains on `compileSdk`/`targetSdk` 35 until that checklist is run green.
- **Resolved (fix pass)**: Toolchain `1.88` pin documented as **reproducibility**, not a proven MSRV (`rust-toolchain.toml`, root `Cargo.toml`, `docs/build-linux.md`). No lower-bound matrix job added in this pass.

## 5) Documentation accuracy sweep

### Findings

- **Resolved (fix pass)**: Internal contradiction in `docs/android-test-results.md` (toast-vs-attribution `confirmed-broken` vs Item 8 fixed) reconciled — Item 7 row now points at Item 8 as current (**fixed and visually confirmed**).
- **Informational (healthy)**: Large parts of docs are explicit about deferred/spec-only status instead of overstating completeness:
  - `docs/plugins.md`, `docs/PROTOCOLS.md`, `docs/ECU.md`, `docs/ebike-telemetry-diy.md`, `docs/plugins/*.md`.
- **Resolved (fix pass)**: Canonical status-doc map added in [`status.md`](status.md) (README = live status; test-results docs = chronological evidence; audits = tracked history). Historical reports retained.

## 6) Test coverage gaps relative to project maturity

### Findings

(Unchanged from investigation pass — not in the seven prioritized fix items.)

## 7) Release / distribution readiness

### Findings

(See category 8 / priority 1 for APRS licensing — resolved in fix pass.)

## 8) Correctness / legal-adjacent risks

### Findings

- **Resolved (fix pass)**: APRS symbol licensing “Unknown” entries excluded from the bundle. Upstream VEC-OH7LZB crops for `/#`, `/-`, `/[` could not be confidently relicensed; replaced with Navi-original SVG/PNG (`aprs_digi`, `aprs_house`, `aprs_human`). `aprs_car.png` retained (CC BY-SA 2.0). Both `COPYRIGHT.md` files and [`icons.md`](icons.md) APRS index updated — no shipping Unknown remains.
- **Resolved (fix pass)**: `hos_jurisdiction.rs` no longer puts `gb` in `EC561_FAMILY`. Tests: Norway/Dublin → Ec561, Kansas → Fmcsa, mid-Atlantic → Unknown, **`london_gb_declines_ec561_pack`**, border Oslo-side → Ec561. Docs (`fmcsa-truck-rest.md`, `jurisdiction-rules.md`) updated.
- **Informational (healthy)**: Some major design choices are already captured with rationale (for example PMTiles range-fetch path and Vulkan move) in `docs/map-styles.md`, `README.md`, and `docs/build-linux.md`.

## Re-prioritized action list (deliberately reconsidered)

This ordering supersedes the initial audit ranking. Two items were intentionally elevated due
to correctness/legal and release risk.

**Blocking flag legend:** `BLOCKING` means this item should block broad release/distribution or
legal-compliance claims until resolved. BLOCKING is lifted only with concrete evidence.

| Priority | Action | Blocking | Status | Last verified / touched | Re-prioritization rationale / evidence |
|---|---|---|---|---|---|
| 1 | Resolve APRS symbol licensing marked "Unknown". | *(lifted)* | **Closed** | 2026-07-29 | Evidence: both `COPYRIGHT.md` files list definite licenses only; Unknown crops excluded; Navi originals for digi/house/human; [`icons.md`](icons.md) APRS index. |
| 2 | Convert jurisdiction bbox heuristics to robust region detection for legal pack selection. | *(lifted)* | **Closed** | 2026-07-29 | Evidence: `country_polys` + `country_iso_at`; `gb` excluded; `cargo test -p driver-break-core hos_jurisdiction` — 6/6 incl. `london_gb_declines_ec561_pack`. |
| 3 | Close `docs/android-test-results.md` contradiction and reduce structural doc sprawl. | No | **Closed** | 2026-07-29 | Item 7 toast row reconciled to Item 8 fixed; [`status.md`](status.md) canonical map. |
| 4 | Centralize duplicated Geofabrik URL construction in Android UI/core boundary. | No | **Closed** | 2026-07-29 | UniFFI `geofabrikLatestPbfUrl` / `geofabrikUpdatesBaseUrl`; `MainActivity` uses FFI builder. |
| 5 | Raise Android platform baseline plan (API 36 + AGP/Kotlin alignment). | No | **Closed (plan)** | 2026-07-29 | Plan only: [`android-api36-plan.md`](android-api36-plan.md). SDK bump deferred until checklist green. |
| 6 | Add dependency maintenance watch process. | No | **Closed** | 2026-07-29 | Quarterly checklist in `docs/CONTRIBUTING.md`; `wee_alloc`/`gpsd_proto` scheduled later, not swapped now. |
| 7 | Record MSRV verification policy explicitly. | No | **Closed** | 2026-07-29 | Documented as reproducibility pin (no matrix). See `rust-toolchain.toml`, `Cargo.toml`, `docs/build-linux.md`. |

## Informational-only findings to carry forward (no immediate action)

These are intentionally recorded so future audits can compare deltas instead of re-discovering them ad hoc.

- `HostApi` remains narrow and non-bloated (`plugin-host/src/abi.rs`).
- Network-following toggle is correctly core-resident route-cost behavior (`core/src/routing/graph/network_pref.rs`).
- CI security/tooling checks are actually enforced (`cargo deny`, `cargo audit` in `.github/workflows/ci.yml`, confirmed in latest successful CI run).
- Offline-first behavior and no-silent-update policy are consistently documented and implemented (`docs/osm-updates.md`, `docs/architecture.md`, `README.md`).
- Core plugin boundary discipline is generally healthy: hard constraints in core, advisory/vendor features in plugin-spec layer.

## Remaining open (outside the seven priorities)

Still **Action required** from categories 1/3 (not blocking this fix pass):

- Dated Protomaps planet fallback URL.
- Fixed Mapterhorn endpoint constants without mirror/failover.
- Vendor acquisition path concentration (Geofabrik / Mapterhorn / OpenFreeMap / Earthdata).
- Missing local rationale comments on some Gradle pin sites.
- API 36 **execution** (plan exists; bump not done).

### Routing plan-time I/O (opened 2026-08-06)

| Priority | Action | Blocking | Status | Last verified / touched | Notes |
|---|---|---|---|---|---|
| — | Evaluate preprocess-once indexed map format (OsmAnd/Navit-class) via phased plan | No | **Phase 4+4b complete** | 2026-08-07 | Live: [`indexed-map-format-plan.md`](indexed-map-format-plan.md). Packs for graph/POI/barrier/wetland; SM-P613 wetland 18.6 s → 93 ms; `.navigph` deprecated. |
| — | Shared multi-consumer PBF parse (graph + poi_barrier) | No | **Folds into Phase 4 converter** | 2026-08-07 | Not a separate track; provision-time pack generation consolidates PBF passes (see indexed-map-format-plan §3.4). |
| — | Graph cache “silent rebuild” audit | No | **Closed** | 2026-08-06 | Cache works for identical OD/bbox; misses on new bbox by design. |
