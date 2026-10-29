# Place-index WAL SIGBUS (feat/i18n-locale)

## Root cause

After a Tunisia place-index build reported PASS (~51 448 rows), the process left a
large SQLite WAL sidecar (`place_index.db-wal`, hundreds of MB) plus a shared-memory
index (`-shm`). The next launch ran `PlaceIndexIntact.probe` with Android
`SQLiteDatabase.OPEN_READONLY`. Native SQLite then **SIGBUS**’d inside
`walIndexReadHdr` while mapping/reading a torn WAL index. Kotlin `runCatching`
cannot catch a native SIGBUS, so the app crash-looped.

This failure mode is **not** caused by the additive `names_json` column.
`names_json` is `ALTER TABLE … ADD COLUMN` without bumping
`PLACE_INDEX_SCHEMA_VERSION` and does not rewrite WAL frames. The same WAL +
readonly-probe pattern already existed on `origin/dev` (WAL journal mode in
Rust writers; Kotlin probes open readonly with no preflight). What changed in
practice: Tunisia’s larger index under memory pressure made mid-write kills /
uncaught WAL more likely, so the latent crash showed up on this branch.

## Upgrade path (dev/release → this build)

1. Existing intact `place_index.db` opens with current schema version.
2. On writer open (or readonly prep), `ensure_names_json_column` runs
   `ALTER TABLE` if needed — **no** schema-version bump, **no** FTS rebuild,
   **no** wipe of `name_index_build`, **no** reset of `complete=1`.
3. Regions keep their rows; `names_json` stays `''` until that region is
   re-indexed / updated.
4. `PlaceIndexIntact` intact checks still use the same `user_version` /
   `complete` / row-count rules.

## Automatic recovery (end users — no adb)

Manual tablet file surgery is only for developer forensics. The app must heal
itself:

1. **After every successful region build:** `PRAGMA wal_checkpoint(TRUNCATE)` so
   a force-stop cannot leave a multi-hundred-MB sidecar.
2. **Before every open/probe** (Rust `NameIndex::open` / schema helpers;
   Kotlin `PlaceIndexIntact` probes): full `recover_wal_sidecars` /
   `placeIndexRecoverWal` — drop stale `-shm`, checkpoint WAL, `mmap_size=0`.
   Rust **`open_readonly`** (search) does **not** use the 120 s exclusive
   recover (that blocked concurrent writers). It runs
   `prepare_readonly_sidecars`: short busy (50 ms) checkpoint when `-wal` is
   non-empty; on non-busy failure quarantine `-wal`/`-shm`; always
   `mmap_size=0`. Live writers (`SQLITE_BUSY`) leave sidecars alone so WAL
   snapshot reads still work.
3. **Cold start:** `MainActivity` starts a background `PlaceIndexWalRecover`
   thread that calls `PlaceIndexIntact.recoverBeforeOpen`. Search may race
   that thread; `open_readonly` self-preps so a torn WAL cannot SIGBUS.
4. **Fallbacks if UniFFI is unavailable:** framework RW `wal_checkpoint`; then
   quarantine `-wal` to `-wal.bad` so the main file can open (may drop
   uncheckpointed frames; incomplete regions can re-index later).
5. **ANR guard:** probes skip the heavy path when no `-wal` exists, and also
   when `-wal` is **0 bytes** (post-checkpoint leftover). Opening a multi-GB
   main file only to truncate an empty WAL ANR'd cold start on SM-P613. Live
   idle `-shm` must not trigger a multi-second RW open on the UI thread. Heavy
   recover runs at most once per process unless forced at startup (background
   thread priority).

## Option D (guidance road names) — status

| Sample (z14, Protomaps `20261028`) | Road features | With `name:*` / `int_name` |
| --- | ---: | ---: |
| Østlandet / Oslo | 3672 | **2** (sparse) |
| Tunisia / Tunis | 4157 | **3019** (rich `name:fr` / `name:ar`) |

**Shipped:** option D offline PMTiles lookup for maneuver / sim-sample /
“Currently on” street labels (`localize_route_streets_from_pmtiles`), with
**option B** (keep pack local `name`) when the tile has no matching `name:*`.
Option C (place-source highway sidecar) was dropped — see `names.md`.

## Tablet memory (Tunisia index)

Host Geofabrik trial (same PBF ~80 MB): peak process VmHWM ≈ **207 MiB**,
indexed 51 448 rows. On-device PlaceIndexReady earlier logged peak_rss around
**~460 MB** during the tablet build (see session logs); after PASS the toxic
WAL was ~372 MB until checkpointed. After checkpoint the main
`place_index.db` on SM-P613 grew to ~2.2 GB (multi-region FTS); cold start under
low free RAM can still ANR for unrelated main-thread work — empty-WAL skip
removes the recover open from that path.

## Matrix notes (SM-P613, tip `3039a3bbf`)

See conversation PART B/C. Tunisia UI delete (#156) cleared `africa/tunisia`
from `place-index-ready.json` and removed Tunisia packs; Norway rows/search
remained. Live locale change: `PlaceIndexBg action=unchanged` / `cache_hit`
only — no rebuild.
