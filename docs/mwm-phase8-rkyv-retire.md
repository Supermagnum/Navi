# Phase 8 — client `.rkyv` retirement (preparation)

**Status:** preparation draft only. Do **not** merge until gates **G3** and **G4**
(see PR body / release plan). No version bump, tag, or F-Droid metadata change
in this work.

Base branch: `dev` after PR #216 merge
(`f900721262fd3651df4d003713f0194a6d880009`).

## Gates (client merge)

| Gate | Meaning | This draft |
|------|---------|------------|
| G1 | PR #216 on `dev` | Met |
| G2 | Pre-release follow-ups (headless search viewport/locale/position parity + per-query 21/21; migration pause/resume on Samsung) | **Not met** — see evidence below |
| G3 | A release with MWM as default has shipped | Expected unmet |
| G4 | One full release cycle with `.rkyv` fallback still in client, no bugs that needed it | Expected unmet |

### G2 gaps (honest)

1. **Headless search vs app:** `ComapsPhase5SearchInstrumentedTest` still hardcodes
   locale `nb` and a Norway-scale viewport for unbound queries; production
   `ComapsSearchBridge` uses `Locale.getDefault()` and position bias **without**
   calling `comapsSearchSetViewport` (Everywhere). Not the same
   viewport/position/locale path.
2. **Per-query gate:** instrumented suite still asserts
   `highPass * 5 >= highTotal * 4` (80% of high rows), not per-query 21/21 hard
   fail. Docs note a 21/21 device run, but the **code gate** was not switched.
3. **Migration pause/resume:** Phase 7 device evidence in
   `docs/mwm-phase7-migration.md` shows pause/resume fixed and re-tested on the
   Samsung — that half of G2 looks done; the search half does not.

Device re-runs of Phase 6b / parity / CAT / migration for this retirement PR
are **blocked on G2** (and G3 for production cutover). CI + inventory proceed.

## What stays until a later release

- Phase 7 migration UI / `mwm/migrate.rs` / `MwmMigration.kt` /
  `region-migration.json` handling / delete-old-`.rkyv` confirm flow
- Thin detection of on-disk `.rkyv` regions (`InstalledMaps` /
  `PackRegionAvailability.localBakeReady` file-name checks) so migration can
  still see old downloads
- DATEX, CAT, CoMaps drape, MWM catalog/download/sidecars

## Inventory — remove when G3–G4 allow (sizes = source bytes on tree)

### Confirmed NOT removed yet (not fully replaced)

| Module | Why kept |
|--------|----------|
| `core/src/search/*` (~250 KiB) + FTS5 `place_index` | Phase 5 **merges** CoMaps hits with FTS; not a full replacement |
| Phase 7 migration + delete helpers | Explicit keep |

### Scheduled removal (flag-OFF / classic path)

| Area | Paths (indicative) | Approx. source |
|------|--------------------|----------------|
| Feature flag + flag-OFF CI | `NAVI_MWM_CLIENT_ENABLED`, `-Pnavi.mwmClient`, `BuildConfig.MWM_CLIENT_ENABLED`, `.github/workflows/ci.yml` job `rust-lib-tests-mwm-off` | CI job + Gradle/Rust branches |
| Pack server client (download/catalog for `.rkyv`) | `core/src/pack_server/*` (~183 KiB), UniFFI discover/decide, `RegionDownloadBackground` pack-first path, `IdlePackJobs` | ~183 KiB + Kotlin callers |
| Graph / tile / wetland / POI `.rkyv` loaders | `core/src/routing/indexed/{graph_pack,graph_pack_v8,load,tile_cache,wetland_pack,poi_barrier_*}.rs`, elevation pack binding used only by `.rkyv` | ~448 KiB indexed + elevation share |
| MapLibre + PMTiles (flag-OFF only) | `org.maplibre.gl` native ~**4.5 MiB** compressed; Kotlin list in `mwm-phase6b-drape.md` §5 (`Basemap*`, `OfflinePmtilesBootstrap`, `Pmtiles*`, `Mapterhorn*`, `LocalDemTileServer`, `DemTileFetcher`, `CorridorMapView` MapLibre path, …) | ~4.5 MiB APK + ~150 KiB+ Kotlin |
| Related tests/docs | flag-OFF tests, `pack_server::` CI gate, classic release notes | — |

**Expected APK reduction (order of magnitude):** drop MapLibre native (~4.5 MiB) plus unused dex/assets; Rust `.rkyv` loaders are in `libnavi.so` (measure with `bloaty` / before-after `libnavi.so` once deletion lands). Releasetest APK on this tree was ~144 MiB (includes CoMaps); classic release APK ~53 MiB historically without CoMaps.

## This preparation branch does

1. Document the retirement list and merge gates (this file).
2. Remove the **flag-OFF CI job** and stop advertising `-Pnavi.mwmClient=false` as a
   supported product path (Gradle still defaults ON; Rust default remains ON).
3. Point `docs/mwm-release-plan.md` / migration docs at Phase 8 prep.

Full deletion of `pack_server` / MapLibre / graph loaders is intentionally
**not** completed in the first prep commit: it must stay green with Phase 7
detection, and device matrices are G2-blocked. Follow-up commits on this branch
(or stacked drafts) should delete modules per the table once G2 is closed and
before G3 merge.

## Server side (separate repo)

Destructive DocumentRoot / ZFS steps: `/tmp/navi-rkyv-retire.sh` (operator).
Adoption measure: navi-server `scripts/phase8/measure-rkyv-adoption.py`.
Recommended old-client HTTP behaviour: **stub** `current.json` (empty `regions` +
`message`), not bare 404 — classic clients soft-fail 404 to “Geofabrik download +
place index” which is worse UX than an explicit update message + offline use of
already downloaded packs.
