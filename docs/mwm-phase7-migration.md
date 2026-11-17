# Phase 7 -- migrating existing `.rkyv` downloads to CoMaps leaf sets

Status: **implemented and run end to end on the Samsung SM-P613 (`R52TB0JQEDE`, releasetest, Navi only)**. Phase 8 is
untouched. Plan and contracts: `docs/mwm-client-migration.md` section 2 (Phase 7). Phase 6b decisions that this
phase closes: `docs/mwm-phase6b-drape.md` section 15 (outcomes in section 8 below).

Screenshots and logs of the device runs are in `test-artifacts/phase7/` (gitignored, `adb screencap`); the index is
in section 9.

## 1. What the user sees

1. **Detection, no download.** On the first start with the MWM path, Navi lists the `.rkyv` regions that have a loadable
   graph (`InstalledMaps`), measures what their files occupy, and (only then) asks the catalog for the leaf sets. If
   there is nothing to migrate, nothing is shown. Nothing is downloaded, moved or deleted without a tap.
2. **One screen** (`01_offer.png`):
   - *Maps on this device now*: each old region with its size and how many new map areas it maps to.
   - *Map areas you will get*: the union of the leaf sets from `region-migration.json`, each with its size; leaves shared
     by several regions are listed once ("shared by 2 regions, downloaded once"), leaves that are already installed are
     marked, leaves that lie mostly outside the region (border roads) are marked.
   - *Download size*, how much the sharing saved, *free space needed and available*, and the sentence that the old maps are
     removed **only after the new maps are downloaded and verified, and only if the user says yes**.
   - **Wi-Fi only** switch (default on when the download is above 250 MB; the user can turn it off).
   - Buttons: **Migrate now**, **Choose regions** (every region and every leaf gets a checkbox; sizes, sharing and
     space are recomputed on each change), **Later** (the offer is not shown again for three days; it stays available
     under Tools / Map storage / "Move old maps to the new format").
3. **Regions without a mapping** (`32_offer_with_unmapped_region.png`) are listed as "no matching new map area; the old
   map is kept", cannot be ticked, and are never offered for deletion.
4. **Not enough free space** (`09_not_enough_space.png`) disables "Migrate now" and says how much more is needed; the
   user can choose fewer regions or free space. Nothing is deleted to make room.
5. **Migrate now** starts a foreground service (data-sync) with a progress notification, and the progress screen
   (`17_a_running.png`): bytes done of planned, status line, Wi-Fi only switch, **Pause / Resume**, **Run in background**,
   **Cancel migration** (asks first; removes only the partial download).
6. **Done**: after the switch the app re-reads every installed file from disk and verifies it (`23b_a_done_dialog_verified.png`).
   Only then it asks "Delete the old maps now? This frees X" with **Delete old maps (X)** / **Keep them**.
   Declining keeps them; they are then listed under Map storage with a Delete button (`28_storage_old_maps_listed.png`),
   which verifies the new maps again before it deletes anything.
7. **Map storage** (Tools / Map storage): release version, installed leaf sets per country group with sizes, delete per
   leaf (with confirmation), "an update is available" with its download size, older releases and unfinished downloads,
   the old maps kept after a migration, and the entry to start / reopen the migration.

## 2. How it works

### Rust (`core/src/mwm/`)

| File | Role |
|---|---|
| `migrate.rs` | `build_plan` (pure planning), `MigrationState` + its persistence, `run_migration`, `verify_installed`, update offer, "Later" reminder, kept-region list |
| `install.rs` | `install_leaf_set_with_catalog` (the migration pins its catalog), process-wide install guard (one install at a time) |
| `download.rs`, `core/src/download/http.rs` | download cancel inside a running transfer (checked every chunk and while connecting; `.partial` is kept and resumed with a Range request) |
| `map_storage.rs` | "Map storage" summary, per-leaf delete, delete of older releases |
| `navi-ffi/src/mwm_migrate_ffi.rs` | UniFFI surface (JSON for plan / state / summary) |

Plan rules (all in `build_plan`, unit tested):

- Region to leaves through `region-migration.json` (by `region_id`, then by `bake_id`), leaves taken from the catalog
  with their size (MWM + sidecar). Leaves the catalog does not know are reported and skipped.
- **Shared leaves are counted once** (a map keyed by leaf id). World and WorldCoasts are added once when the installed
  World does not match the release.
- Free space needed = download + the World files a second time (they are copied to the storage root on activation) +
  256 MiB margin. The plan is `enough_space = free >= needed`.
- Large = download above 250 MiB; "Wi-Fi only" defaults to on for large migrations.

State (`mwm/migration-cache/state.json`, atomic writes) and what it pins:

| Status | Meaning |
|---|---|
| `queued` | chosen by the user, not started or restarted after a process restart |
| `running` | the service is downloading |
| `paused` | paused by the user (explicit resume needed) |
| `waiting_wifi` | Wi-Fi only is on and no unmetered network is active |
| `waiting_network` | no network or the connection dropped; retried when a network is back |
| `failed` | no space, checksum, release no longer served; the message is kept, "Try again" resumes |
| `verified` | downloaded, switched in and verified from disk; the old-maps question is open |
| `done` | the user answered (deleted or kept) |

`mwm/migration-cache/catalog-{version}.json` is the catalog the plan was made with. **A release published while the
migration runs is not looked at**: the migration finishes against the pinned release, and Map storage then offers the
update (`14_storage_after_b_update_offer.png`). `kept.json` lists regions that were migrated and kept, so they are never
offered again.

Atomic switch: every leaf is downloaded into `staging/{version}` and verified there (MWM size + SHA-1 against the
signed `countries.txt`, sidecar SHA-256, MWM fingerprint); nothing in staging is visible to the router. When all leaves
and World are in, `.complete` is written and `activate_release` renames the staging tree into `releases/{version}` and
promotes World to the root (or `merge_staging_into_release` moves the new files into an already active release of the
same version). Then `verify_installed` checks every chosen leaf again from the installed paths (size + SHA-1, sidecar
size + SHA-256, World matches the active release). Only a passing check sets `verified`.

### Kotlin (`app/src/main/java/no/navi/app/`)

`MwmMigration.kt` (Compose state, detection, plan calls), `MwmMigrationService.kt` (foreground service),
`MwmMigrationUi.kt` (offer, progress, old-maps question), `MwmMapStorageUi.kt` (Map storage). The service decides when a
run may start: Wi-Fi only with a metered or absent network waits for Wi-Fi; no network waits for a network; a network
callback stops a running download within a quarter second when Wi-Fi is lost and resumes it when Wi-Fi returns; the
state survives a process kill (`resume after restart` on the next app start).

## 3. Device results

Samsung SM-P613, Android 14, releasetest, Wi-Fi (unmetered), USB powered, free space 19 GB at the start. The old state
was created with a flag-OFF build through the old path (`RegionDownloadBackground.ensureStarted`, the call of the Tools
region download), then the flag-ON build was installed over it (same package, data kept).

### Old state (flag OFF build, old path)

| Region | Pack download | On disk (measured by Navi) |
|---|---|---|
| Østlandet | 3 min 22 s (10:44:43 to 10:48:05) | 2.80 GB (server pack 2,549,326,770 B + 275 MB place-index extract partial) |
| Sørlandet | 30 s | 332 MB |

Navi measured the old Østlandet at 3.42 GB when the flag-ON app first started: an old-path resume (below) had written a
620 MB PMTiles partial. See "Defects found" for the cause and the fix.

### Scenario B: Sørlandet only, with a catalog change while it runs (through a local pass-through proxy)

Choose regions, untick Østlandet: 3 leaves (Southern, North Denmark, Rogaland) + World = **447 MB**, needed 778 MB,
available 1.74 GB. Started 11:01:30; the file `/tmp/phase7-flip` was created 6 s later so the proxy answered
`/mwm/current.json` with version 261106. **Verified 11:02:11 (41 s for 447 MB), deep verify 2 s.** The migration finished
on 261105 (pinned). Delete dialog: "Delete old maps (340 MB)" tapped: **340,273,786 B freed** (`RegionDelete` log), the
size of the old Sørlandet files. Map storage then showed "Release 261106 is available (installed: 261105). Update
download: 447 MB" (`14_storage_after_b_update_offer.png`). The update itself was not run (the 261106 files do not exist
on the mirrors; the proxy only relabels the version).

### Scenario A: Østlandet (shares Norway_Southern and North Denmark with Sørlandet, already installed by B)

Plan: 9 leaves, 2 already installed, **download 922 MB**, needed 1.19 GB, available 1.50 GB
(`16_a_offer_after_b.png`). Started 11:05:15 and verified 11:10:25:

| Time | Step | Result |
|---|---|---|
| 11:05:37 | Wi-Fi off (`svc wifi disable`) at 228 MB | "No network. Continues when the connection is back", then "Waiting for Wi-Fi" (Wi-Fi only is on); `waiting_network` / `waiting_wifi` in the state (`18_a_wifi_off.png`) |
| 11:06:14 | Wi-Fi on | resumed by itself after the network callback, 5 s (`19_a_wifi_back_resumed.png`) |
| 11:06:49 | `am force-stop` at about 560 MB | next start: `resume after restart status=running`, continued at 649 MB (`20_a_after_force_stop_resumed.png`) |
| 11:07:21 | Pause at 777 MB | "Paused" (`21_a_paused.png`) |
| 11:07:38 | Resume | a defect: "paused by user" and nothing ran (cause: the state still said `paused`); fixed (state is set to `queued` on an explicit resume) |
| 11:09:29 | free space reduced to 53 MB, Resume | `insufficient space for download: need 144558642 bytes, available 53067776 bytes` -> status `failed` with the message (`22_a_no_space_failed.png`); the downloaded files stay in staging |
| 11:10:01 | space freed, "Try again" | continued from 777 MB, verified 11:10:25 |

"Delete old maps (3.42 GB)" was **not** tapped: "Keep them". Then Map storage listed "Old maps (previous format):
Ostlandet (3.42 GB)", the old files were still on disk, and later **Delete** + confirm freed **3,422,226,664 B**
(`30_storage_after_old_deleted.png`, free space 1.23 GB -> 4.60 GB). A second "Migrate now" with everything installed
downloaded nothing (plan: 0 B) and went straight to verified.

### Final storage state (`state-after_migration.json`)

Active release 261105, World at the root (`world-release.json` 261105), 9 leaves + sidecars (1.17 GB in
`releases/261105`), staging empty, no old regions, migration state `done`, decision `kept` for Østlandet (then deleted
from Map storage). Per-leaf delete ("Western", with confirmation) worked without a crash and the drape kept rendering;
the leaf was installed again with the app's downloader afterwards (13 s).

### Edge cases

| Case | Where | Result |
|---|---|---|
| Not enough free space (plan) | device `09_not_enough_space.png`, host test `free_space_gate`, Kotlin disabled button | message "537 MB more is needed", "Migrate now" does nothing (checked: no service, no state) |
| Not enough free space (during the download) | device, scenario A | `failed` with the exact numbers, files kept, resumes after space is freed |
| Partial / interrupted | device (Wi-Fi off, force-stop, pause), host `pause_resumes_with_range_and_does_not_switch_early`, `network_loss_waits_then_resumes` | resumed with Range requests; nothing switched before all leaves verified |
| Region without a mapping | device `32_offer_with_unmapped_region.png`, unit test | listed, not selectable, old map kept, not in the old-maps deletion |
| Many regions | device `33_offer_30_regions.png` (30 stand-in regions: 172 leaves, 15.7 GB, 4 unmapped; plan computed in 0.5 s), unit test with 40 regions | scrolls, space check works |
| Only shared-leaf regions | device scenario B (Sørlandet alone) and A (Østlandet after it), host test | shared leaves downloaded once (`Norway_Southern.mwm` requested once in the host test) |
| New CoMaps release mid-migration | device scenario B (proxy), host `new_release_mid_migration_is_finished_first_then_offered` | finishes on the pinned release; then the update is offered |
| Process death | device (force-stop) | resumed on the next start |
| Network loss with Wi-Fi only | device | waits for Wi-Fi |

Not exercised on the device: the metered-Wi-Fi case of the Wi-Fi-only gate (the tablet has no cellular and the rules
exclude the Android settings app that marks a network as metered). The gate reads `NET_CAPABILITY_NOT_METERED`; the
"no unmetered network" branch ran when Wi-Fi was off.

## 4. Defects found by the device runs (all fixed here unless noted)

1. **The old pack pipeline ran on the MWM path.** With leftover partial `.rkyv` downloads, the flag-ON app resumed
   "basemap for europe/norway/ostlandet" (PMTiles extract) at start and grew the old data by 620 MB without a tap.
   `IdlePackJobs` is now off on the MWM path, and the resume / passive indexing effects of the map screen are skipped
   (`MainActivity`).
2. **Resume after pause did nothing** (see table above).
3. **The GPX card crashed the app** when a file had several tracks (a vertical scroll nested in the card's vertical
   scroll); the track chips scroll horizontally now.
4. **The GPX card covered the Tools menu**; it is hidden while Tools is open.
5. **A kept old region was offered again at every start**; `kept.json` and the offer filter fix it.
6. **The Phase 4 / 5 on-device suites could not start since Phase 6b**: they copied `assets/comaps-data`, which the drape
   build no longer packages. They now stage the headless resources from the APK root assets (plus the ICU data).
7. **The CoMaps shim in `native/comaps_shim` and the one the patch series produced had diverged** (Phase 5 search code
   was only in the repository copy, the 6b drape changes only in patch 0004). A from-scratch build produced a
   `liborganicmaps.so` without `navi_comaps_clear_search_viewport`, so `libnavi.so` failed to load on the device
   (found by the first install of this phase). `native/comaps_shim` is now the merge of both and
   `scripts/build-comaps-android.sh` installs it over the patched tree; the F-Droid check builds that way.
8. Reported, not fixed (unrelated): with old regions and no MWM leaves, the route panel still shows the old-path text
   "Ostlandet: indexing..." and "Route planning does not work before indexing and route building is done".

## 5. Tests

| Layer | Tests |
|---|---|
| Rust unit (`core/src/mwm/migrate.rs`, `map_storage.rs`) | 18 + 3: plan (shared leaves once, unmapped, only-unmapped, selection / exclusion, installed leaves, space gate, 40 regions, missing catalog leaf, bake-id fallback), state round trip, "Later", kept regions, error classification, update offer, JSON key shape the app reads |
| Rust end to end (`core/tests/mwm_migration_e2e.rs`) | 5 against a local fake CoMaps mirror + navi-server with real Ed25519 signatures: full migration with deep verify and a corrupted leaf, pause + Range resume (nothing switched), network loss and resume, release published mid-migration, second start refused |
| Kotlin unit (`MwmMigrationParseTest`) | plan / state / storage JSON parsing, byte formatting |
| Device (`Phase7MigrationInstrumentedTest`) | seed through the old path, state dump, plans (shared, no space, unmapped, 40 regions), stand-in regions, proxy hold, Espa hold, puck and APRS screenshots; the migration itself through the UI with adb taps (`scripts/phase7-device-ui.py`) |

## 6. Decision for the owner

- "Later" lasts three days (`LATER_REMINDER_MS`); change it if the reminder should be shorter or never repeat.
- The large-migration Wi-Fi threshold is 250 MiB (`LARGE_MIGRATION_BYTES`).
- After a successful migration the app keeps the old pack files until the user says yes; there is no automatic expiry.

## 7. Local CI (host)

Re-run on this branch before push (`test-artifacts/phase7/logs/ci-local.txt`). Same commands as Phase 6b.

| Check | Result |
|---|---|
| `cargo fmt --check` | pass |
| `cargo clippy` (workspace + `navi-ffi --features diag-bins`, `-D warnings`) | pass |
| `cargo test -p driver-break-core --lib` (flag on and `NAVI_MWM_CLIENT_ENABLED=0`) | pass, **863** each (14 ignored) |
| `cargo test -p navi-ffi --lib` | pass, 25 |
| `cargo test --test mwm_migration_e2e --test gpx_import` | pass, 5 + 13 |
| `cargo test -p navi-cat` / `-p navi-plugin-cats` (incl. `vfo_retry_dry_run`) | pass |
| `plugin-host` isolation tests | pass |
| `check-plugin-host-gate.sh` | fails on untracked `tools/fdroid-check/work*/` trees (pre-existing); `.gitignore` now covers `work-*/` |
| `cargo deny` | advisories only (report; not a merge gate for this phase) — same set as Phase 6b (`libsqlite3-sys`, `zip`, `wasmtime`, `rustls` family, …) |
| `ktlintCheck` / `detekt` / `testDebugUnitTest` (also `-Pnavi.comapsDrape=false`) | pass (`android-ci.txt`) |

## 8. Phase 6b section 15 — outcomes in this phase

| # | Decision | Outcome in Phase 7 |
|---|---|---|
| a | Vehicle limits | **Not built.** Design note only: `docs/mwm-vehicle-limits-design.md` (sidecar `vehicle_limits` section + wetland-style blocking hook; ~8 days; ~60–90 kB zstd for Norway). Matrix keeps 4 PARTIAL rows; UI still says limits are not applied. |
| b | Puck colours + APRS / weather sprites | **Done.** Patch `patches/comaps/0005-navi-fps-cap-puck-states-and-mark-symbols.patch`; Navi SVGs in `comaps-sdk/navi-symbols/` and `core/src/icons/aprs/aprs_car.svg`; atlas from SVG via `scripts/generate-comaps-symbols.py`. Device: `screenshots/puck_{gps,estimated,lost}.png`, `aprs_sprites.png`. |
| c | 30 fps follow / idle; full rate during gestures | **Done.** Cap in patch 0005; idle/follow at ~30 fps; gestures uncapped. Espa→Dombås auto+perf (12×, ~18 min wall): drape **29.9** fps mean, UI **59.8** fps, PSS ~596 MB, thermal 0 (`perf-espa_dombas_auto.json`). Separate 20 min north-up 1× capped (+ optional 5 min uncapped baseline) via `u_perf_northup_follow`. |
| d | GPX import | **Done.** Rust `core/src/import/gpx_import.rs` + UniFFI + Compose card (`GpxImport.kt`); open-with / share / Tools picker. Matrix `gpx_import` **PASS** (round-trip 43.8 km vs 43.9 km). Device screens `40`–`49`. |
| e | F-Droid atlas strategy | **Generate at build time** from SVG (`librsvg2-bin` + `python3-pil`); no committed atlas binaries. Recipe `tools/fdroid-check/metadata/no.navi.app.yml` entry `0.5.0-beta` / vc 35 (renumbered from draft `0.4.0-beta` / 34; CAT `v0.4.x-beta` tags are separate); local image bakes the same packages (incl. Hamlib `autoconf`/`automake`/`libtool`). CoMaps configure may log `Skipping generate symbols...` (no Qt); Gradle `generateComapsSymbols` is the real path. |
| f | CAT VFO: 2 retries then user message; fake-rig dry-run | **Done.** `navi-cat` `follow_program.rs`: 3 attempts total, 1.5 s pause, `ProgramFailureNotice` → CAT sheet string (`cat_vfo_program_failed`); failed site not committed. Host dry-run `plugins/CATS-plugin/tests/vfo_retry_dry_run.rs` pass. Device (fake rig): Espa→Dombås auto **LA5MR → LA6JRR**; standalone **LA5ARR → LA7GR → LA2JRR** (LA6GR held at Espa, programmed on return); Litjfossen **LA5MR → LA5TRR → LA9AR**. Digital-only / invalid-shift skipped. |

### Re-run suites (Samsung `R52TB0JQEDE`, releasetest)

| Suite | Result |
|---|---|
| Phase 6b function matrix | **20 PASS / 4 PARTIAL / 0 FAIL** (`function-matrix-rerun.json`); GPX import no longer NOT_TESTED |
| Phase 4 parity (`ComapsPhase4SuiteInstrumentedTest`) | **OK** — MWM summary 40/41 ok (1 expected failure) |
| Phase 4 turns/wetland/elev (`ComapsPhase4ParityInstrumentedTest`) | **OK** |
| Phase 5 search | **PARTIAL.** Cancel + off-road snap **OK**. Full suite: Everywhere free-text no longer ends without cancel after drape asset staging; Viewport fallback works until CoMaps `Utils.getStringValueByKey` aborts on a null Platform Context in headless (address / type localization). ICU now staged (`icudt75l.dat`). Production UI has a Context; matrix `airplane_mode` still reports search 30 hits. Follow-up: null-safe Utils or a headless Platform Context. |
| Perf north-up 20 min (30 fps cap) | **OK** — drape **29.92** fps mean (max 30.2), UI 59.96, PSS ~566 MB, thermal 0 (`perf-northup-20min-capped.json`) |
| Perf north-up 5 min uncapped | **OK** — drape **59.98** fps mean (cap off), UI 59.84, PSS ~555 MB |
| F-Droid from-source Podman | **Built** `no.navi.app:34` under the earlier mistaken G3 draft label / `generateComapsSymbols` ran; Hamlib bootstrap OK; APK `tools/fdroid-check/work/tmp/no.navi.app_34.apk` (~108 MB). fdroid then failed only on missing GitHub `Binaries:` URL (404) — from-source assemble succeeded. Shipping tag/name is now `v0.5.0-beta` / vc 35. |
| CAT Espa→Dombås auto + perf | **OK** (~18 min); hops LA5MR → LA6JRR |
| CAT Espa→Dombås standalone | **OK**; hops LA5ARR → LA7GR → LA2JRR (+ LA6GR) |
| CAT Espa→Litjfossen auto | **OK**; hops LA5MR → LA5TRR → LA9AR |

## 9. Screenshot index (`test-artifacts/phase7/screens/`, gitignored)

| Files | Content |
|---|---|
| `01_offer`, `02_choose_regions`, `03_choose_leaf_excluded`, `04_choose_region_off` | the offer, choose mode, leaf excluded, region off |
| `05_after_later_map`, `06_restart_no_offer`, `07_tools_menu`, `08_storage_before_migration` | Later, restart without an offer, Tools, Map storage before |
| `09_not_enough_space`, `10_sorlandet_only_plan` | free-space gate, Sørlandet-only plan |
| `11_b_running`, `12_b_done_dialog`, `13_b_old_deleted`, `14_storage_after_b_update_offer`, `15_storage_delete_confirm` | scenario B |
| `16`-`24` | scenario A (offer, running, Wi-Fi off / on, force-stop resume, pause, no space, done dialog, kept) |
| `25`-`31` | Map storage after A, old maps listed and deleted, leaf deleted |
| `26_offer_when_all_installed`, `32_offer_with_unmapped_region`, `33`/`34_offer_30_regions` | idempotent re-run, unmapped, 30 regions |
| `40`-`49` | GPX (card, route to start, navigate along track, long hike, waypoints file, share intent with unsupported content, error, picker) |
| `screenshots/puck_*`, `aprs_sprites*` | position puck states and APRS sprites on the drape |
