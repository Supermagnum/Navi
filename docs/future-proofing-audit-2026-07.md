# Future-proofing audit (2026-07-29)

This document is the canonical tracked output of the 2026-07-29 future-proofing audit.
It exists to keep findings and priority ordering in one discoverable place, since the
audit itself identified documentation sprawl as a project risk.

Scope: investigation findings only (no broad fixes in this pass), organized by the same
eight categories requested in the audit prompt.

## Status legend

- **Action required**: finding needs follow-up work.
- **Informational (healthy)**: finding was checked and is currently acceptable.

## 1) Hardcoded external references

### Findings

- **Action required**: Protomaps fallback URL is a dated fixed value in `core/src/routing/basemap/extract.rs` (`PROTOMAPS_PLANET_FALLBACK_URL = https://build.protomaps.com/20260722.pmtiles`), and is reused as `DEFAULT_PMTILES_BASE_URL` in `core/src/routing/basemap/regions.rs`. Dynamic resolution exists (`resolve_planet_url_blocking`), but the fallback remains date-pinned.
- **Action required**: Mapterhorn runtime endpoints are fixed constants in `app/src/main/java/no/navi/app/MapterhornTerrain.kt` (`TILEJSON_URL`, `MAPTERHORN_PLANET_URL`, and tile-template URLs). No local mirror/failover source is defined in code.
- **Action required**: Geofabrik URL construction is duplicated: core has canonical builders in `core/src/routing/osm_update.rs` (`geofabrik_latest_url`, `geofabrik_updates_base_url`), while Android UI also interpolates URL strings in `app/src/main/java/no/navi/app/MainActivity.kt` (`https://download.geofabrik.de/$path-latest.osm.pbf`).
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
- **Action required**: Jurisdiction detection logic for legal packs is still labeled interim in behavior terms and uses coarse bbox classification in core (`core/src/routing/rest/hos_jurisdiction.rs`), which raises correctness risk for compliance-related outputs (detailed in category 8 prioritization).

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
- **Action required**: Rust dependency maintenance risk watchlist includes relatively stale crates (audit-time recency check), notably `wee_alloc` and `gpsd_proto`, with additional low-churn dependencies (`osm4routing`, `geotiff`) worth periodic review.
- **Action required**: Android platform baseline is one API level behind current stable release cadence:
  - Project: `compileSdk`/`targetSdk` 35 (`app/build.gradle.kts`).
  - Current stable SDK platform: Android 16 / API 36 (per Android SDK platform release notes).
- **Action required**: MSRV pin (`1.88`) is documented and reproducible (`rust-toolchain.toml`, root `Cargo.toml`), but verification currently demonstrates pinned reproducibility, not a formally proven minimum across an explicit lower-bound matrix.

## 5) Documentation accuracy sweep

### Findings

- **Action required**: Internal contradiction in `docs/android-test-results.md`:
  - A row marks toast-vs-attribution as `confirmed-broken`,
  - Later in the same document the same issue is recorded as fixed with evidence.
  This is stronger than normal staleness; it is a self-conflict within one source.
- **Informational (healthy)**: Large parts of docs are explicit about deferred/spec-only status instead of overstating completeness:
  - `docs/plugins.md`, `docs/PROTOCOLS.md`, `docs/ECU.md`, `docs/ebike-telemetry-diy.md`, `docs/plugins/*.md`.
- **Action required**: Documentation sprawl risk remains material:
  - Operational status is distributed across `README.md`, `docs/architecture.md`, `docs/codebase-map.md`, `docs/android-test-results.md`, `docs/test-results.md`, and point-in-time reports such as `docs/closing-pass-report.md`.
  - This increases the chance of future contradictions.

## 6) Test coverage gaps relative to project maturity

### Findings

- **Informational (healthy)**: CI and local test scaffolding breadth is good:
  - Rust checks + Kotlin checks + Android build in CI (`.github/workflows/ci.yml`).
  - Extensive instrumented test suite under `app/src/androidTest/java/no/navi/app/`.
- **Action required**: Real-hardware validation remains an explicit open gap:
  - `docs/real-hardware-testing.md` still lists critical unresolved on-device checks (native marker rendering behavior, real GPS/IMU behavior, real GPU behavior, hardware-specific regressions).
- **Action required**: Manual Android instrumented workflow instability history means emulator/CI signal is useful but not sufficient for release confidence (`.github/workflows/android-instrumented.yml`, `docs/real-hardware-testing.md`, `docs/debugging.md`).
- **Informational (healthy)**: The test docs correctly separate what is in required CI vs manual/local fixture-heavy runs (`CONTRIBUTING.md`, `docs/build-linux.md`).

## 7) Licensing hygiene sweep

### Findings

- **Action required (release blocker)**: APRS symbol licensing includes unresolved "Unknown" entries in bundled asset metadata:
  - `app/src/main/assets/icons/aprs/COPYRIGHT.md`
  - `core/src/icons/aprs/COPYRIGHT.md`
  This blocks broad distribution planning until each symbol is resolved or excluded.
- **Informational (healthy)**: Navit-derived icon licensing is documented and consistently referenced in `docs/icons.md`.
- **Action required**: APRS licensing status is documented in asset-level COPYRIGHT files but is not summarized in a top-level licensing index (`docs/icons.md` / equivalent), reducing discoverability during release/legal review.
- **Informational (healthy)**: Root repository license exists (`LICENSE`), and docs already distinguish code-license context vs asset provenance (`docs/icons.md`, `README.md`).

## 8) Knowledge-transfer / bus-factor readiness

### Findings

- **Informational (healthy)**: Onboarding coverage is broad and useful for new contributors (`CONTRIBUTING.md`, `docs/architecture.md`, `docs/codebase-map.md`, `docs/API.md`).
- **Action required**: Some high-impact rationale is documented as facts but not always as explicit decision records with alternatives and de-scope boundaries (for example long-term pin policy, legal-pack region mapping criteria, and fallback endpoint governance).
- **Action required (correctness/legal)**: Current legal-jurisdiction classification for truck HOS is coarse and can misclassify:
  - `core/src/routing/rest/hos_jurisdiction.rs` includes `gb` in `EC561_FAMILY`.
  - Classification uses bbox-country lookup rather than robust jurisdiction policy resolution.
  This is a current correctness risk for compliance guidance, not only future maintenance risk.
- **Informational (healthy)**: Some major design choices are already captured with rationale (for example PMTiles range-fetch path and Vulkan move) in `docs/map-styles.md`, `README.md`, and `docs/build-linux.md`.

## Re-prioritized action list (deliberately reconsidered)

This ordering supersedes the initial audit ranking. Two items were intentionally elevated due
to correctness/legal and release risk.

**Blocking flag legend:** `BLOCKING` means this item should block broad release/distribution or
legal-compliance claims until resolved.

| Priority | Action | Blocking | Status | Last verified / touched | Re-prioritization rationale |
|---|---|---|---|---|---|
| 1 | Resolve APRS symbol licensing marked "Unknown". | **BLOCKING** | Open | 2026-07-29 | Elevated from prior #2 to #1. Unresolved asset licensing is a release blocker for wider distribution planning (Play Store/F-Droid), not merely documentation tidiness. |
| 2 | Convert jurisdiction bbox heuristics to robust region detection for legal pack selection. | **BLOCKING** | Open | 2026-07-29 | Elevated from prior #6 to #2. This is current legal-classification correctness risk in core logic; concrete example: `gb` in `EC561_FAMILY` in `core/src/routing/rest/hos_jurisdiction.rs`. |
| 3 | Close `docs/android-test-results.md` contradiction and reduce structural doc sprawl. | No | Open | 2026-07-29 | Kept near this position but broadened from one-line patch. Because contradiction is internal to one doc, follow-up should include status-doc consolidation, not only local text repair. |
| 4 | Centralize duplicated Geofabrik URL construction in Android UI/core boundary. | No | Open | 2026-07-29 | Mechanical cleanup with low risk and high consistency payoff (`MainActivity.kt` vs `core/src/routing/osm_update.rs`). |
| 5 | Raise Android platform baseline plan (API 36 + AGP/Kotlin alignment). | No | Open | 2026-07-29 | Demoted relative to correctness/legal blockers above; still important for platform-churn risk. |
| 6 | Add dependency maintenance watch process. | No | Open | 2026-07-29 | Add periodic recency/security review for both Rust and Gradle ecosystems. |
| 7 | Record MSRV verification policy explicitly. | No | Open | 2026-07-29 | Clarify current MSRV pin as reproducibility unless/until lower-bound matrix verification is added. |

## Informational-only findings to carry forward (no immediate action)

These are intentionally recorded so future audits can compare deltas instead of re-discovering them ad hoc.

- `HostApi` remains narrow and non-bloated (`plugin-host/src/abi.rs`).
- Network-following toggle is correctly core-resident route-cost behavior (`core/src/routing/graph/network_pref.rs`).
- CI security/tooling checks are actually enforced (`cargo deny`, `cargo audit` in `.github/workflows/ci.yml`, confirmed in latest successful CI run).
- Offline-first behavior and no-silent-update policy are consistently documented and implemented (`docs/osm-updates.md`, `docs/architecture.md`, `README.md`).
- Core plugin boundary discipline is generally healthy: hard constraints in core, advisory/vendor features in plugin-spec layer.
