//! Offline name/address search via SQLite FTS5.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, TryLockError};
use std::time::Duration;

use osmpbf::Element;
use rusqlite::{params, Connection, Result as SqlResult};

use crate::download::phase_timing;
use crate::storage::Storage;

mod place_context;

pub use place_context::{format_place_display, PLACE_INDEX_SCHEMA_VERSION};

/// Place-index `kind` for OSM `building=*` + `name=*` (no amenity/shop/place).
pub const NAMED_BUILDING_KIND: &str = "building";

/// Commit SQLite/FTS inserts this often so a force-close cannot roll back the
/// entire write, and so WAL readers are not blocked for minutes.
const INSERT_COMMIT_BATCH: usize = 50_000;
/// Refresh place-index UI labels during long PBF walks so Android does not
/// appear frozen on a single "scanning ways…" / "scanning nodes…" string.
const PLACE_INDEX_PROGRESS_HEARTBEAT: usize = 25_000;

/// One writer at a time for discard + open + `load_from_pbf`. Android can
/// launch PlaceIndexBackground and RegionDownloadBackground against the same
/// DB; without this, both parse the PBF (~500 MB each) and OOM a 3.5 GB tablet.
///
/// Also serializes Kotlin `PlaceIndexReady.clearRegionRows` (via FFI
/// acquire/release) so Android framework SQLite never races rusqlite WAL
/// `-shm` (SIGBUS on deleted shm).
static PLACE_INDEX_BUILD_LOCK: Mutex<()> = Mutex::new(());

thread_local! {
    /// FFI acquire/release holds the same mutex on this thread until release.
    static PLACE_INDEX_BUILD_LOCK_TLS: std::cell::RefCell<Option<MutexGuard<'static, ()>>> =
        const { std::cell::RefCell::new(None) };
}

/// Hold while building or cache-checking a place index. Poison is recovered so
/// a panicked builder cannot deadlock later callers.
pub fn lock_place_index_build() -> MutexGuard<'static, ()> {
    PLACE_INDEX_BUILD_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Same as [`lock_place_index_build`], but if another builder already holds the
/// mutex, set a 0/6 Place-index label so the UI is not stuck on "starting…".
pub fn lock_place_index_build_with_progress() -> MutexGuard<'static, ()> {
    match PLACE_INDEX_BUILD_LOCK.try_lock() {
        Ok(guard) => guard,
        Err(TryLockError::WouldBlock) => {
            crate::download::progress::set(
                0,
                Some(6),
                "Place index: waiting for another index build…",
            );
            PLACE_INDEX_BUILD_LOCK
                .lock()
                .unwrap_or_else(|e| e.into_inner())
        }
        Err(TryLockError::Poisoned(p)) => p.into_inner(),
    }
}

/// Acquire [`PLACE_INDEX_BUILD_LOCK`] on this thread for FFI / Kotlin callers.
///
/// Blocks until the lock is free (another `ensure_place_index` may be mid-build).
/// Must be paired with [`place_index_build_lock_release`] on the **same** thread.
/// Nested acquire on the same thread returns an error string rather than
/// deadlocking.
pub fn place_index_build_lock_acquire() -> Result<(), String> {
    PLACE_INDEX_BUILD_LOCK_TLS.with(|slot| {
        if slot.borrow().is_some() {
            return Err("place_index_build_lock already held on this thread".into());
        }
        let guard = lock_place_index_build_with_progress();
        *slot.borrow_mut() = Some(guard);
        Ok(())
    })
}

/// Release a prior [`place_index_build_lock_acquire`] on this thread.
pub fn place_index_build_lock_release() -> Result<(), String> {
    PLACE_INDEX_BUILD_LOCK_TLS.with(|slot| {
        if slot.borrow_mut().take().is_none() {
            return Err("place_index_build_lock not held on this thread".into());
        }
        Ok(())
    })
}

/// True when this thread currently holds the build lock via FFI acquire.
pub fn place_index_build_lock_held_on_thread() -> bool {
    PLACE_INDEX_BUILD_LOCK_TLS.with(|slot| slot.borrow().is_some())
}

/// Delete one region's rows under [`PLACE_INDEX_BUILD_LOCK`] using rusqlite
/// (same stack as `ensure_place_index`). Prefer this over Android framework
/// SQLite for clears that may overlap a background index build.
pub fn clear_place_index_region_rows(
    index_db: impl AsRef<Path>,
    region_id: &str,
) -> Result<(), String> {
    let index_db = index_db.as_ref();
    let region_id = region_id.trim().trim_matches('/');
    if region_id.is_empty() {
        return Err("region_id empty".into());
    }
    if !index_db.is_file() {
        return Ok(());
    }
    let _build = lock_place_index_build_with_progress();
    let mut idx = NameIndex::open(index_db).map_err(|e| format!("open index: {e}"))?;
    idx.clear_region(region_id)
        .map_err(|e| format!("clear region: {e}"))?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct NameHit {
    pub osm_id: i64,
    pub name: String,
    pub kind: String,
    pub lat: f64,
    pub lon: f64,
    pub sub_area: String,
    pub municipality: String,
    /// Geofabrik path this row was indexed under (empty for legacy rows).
    pub region_id: String,
}

/// Local FTS5 name index for settlements, POIs, huts, peaks, and named ways.
pub struct NameIndex {
    conn: Connection,
}

impl NameIndex {
    pub fn open_in_memory() -> SqlResult<Self> {
        let conn = Connection::open_in_memory()?;
        Self::migrate(&conn)?;
        Self::backfill_legacy_complete(&conn)?;
        Ok(Self { conn })
    }

    pub fn open(path: impl AsRef<Path>) -> SqlResult<Self> {
        let conn = Connection::open(path)?;
        // WAL writers rarely wait on readers; keep a short timeout for checkpoints.
        conn.busy_timeout(Duration::from_secs(5))?;
        let pragmas_t0 = phase_timing::start("place_index.open_db.pragmas");
        Self::apply_file_pragmas(&conn)?;
        phase_timing::end("place_index.open_db.pragmas", pragmas_t0);
        // GROUP BY over name_entries on every open dominated reopen (~383 ms of
        // 384 ms for 2.1M rows in the ignored timing test; tablet open_db ~7 s).
        // Production writers (`load_from_pbf_for_region` → `upsert_build_progress`)
        // always upsert name_index_build. `upsert_entry*` is tests-only and does
        // not; those DBs already have the table from `open`, so they skip too.
        let had_build_table = Self::name_index_build_table_exists(&conn)?;
        let migrate_t0 = phase_timing::start("place_index.open_db.migrate");
        Self::migrate(&conn)?;
        phase_timing::end("place_index.open_db.migrate", migrate_t0);
        let backfill_t0 = phase_timing::start("place_index.open_db.backfill_legacy_complete");
        if !had_build_table {
            Self::backfill_legacy_complete(&conn)?;
        }
        phase_timing::end("place_index.open_db.backfill_legacy_complete", backfill_t0);
        Ok(Self { conn })
    }

    /// Query-only open: no migrate/DDL, no 30s busy wait on the UI path.
    pub fn open_readonly(path: impl AsRef<Path>) -> SqlResult<Self> {
        let conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        conn.busy_timeout(Duration::from_millis(250))?;
        Ok(Self { conn })
    }

    fn apply_file_pragmas(conn: &Connection) -> SqlResult<()> {
        // WAL lets overlay/search readers see a consistent snapshot during writes.
        // NORMAL (not FULL) is the SQLite-recommended pairing with WAL: fsync on
        // WAL frame commit is skipped; a checkpoint still durable enough for a
        // rebuildable search index.
        let mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))?;
        if !mode.eq_ignore_ascii_case("wal") {
            log::warn!(
                target: "NaviSearch",
                "place-index: journal_mode={mode}, expected wal"
            );
        }
        conn.execute_batch("PRAGMA synchronous = NORMAL;")?;
        Ok(())
    }

    fn migrate(conn: &Connection) -> SqlResult<()> {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS name_entries (
                osm_id INTEGER PRIMARY KEY NOT NULL,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                lat REAL NOT NULL,
                lon REAL NOT NULL,
                sub_area TEXT NOT NULL DEFAULT '',
                municipality TEXT NOT NULL DEFAULT '',
                region_id TEXT NOT NULL DEFAULT ''
            );
            CREATE VIRTUAL TABLE IF NOT EXISTS name_fts USING fts5(
                name,
                kind,
                content='name_entries',
                content_rowid='osm_id'
            );
            CREATE TABLE IF NOT EXISTS name_index_build (
                region_id TEXT PRIMARY KEY NOT NULL,
                expected INTEGER NOT NULL DEFAULT 0,
                written INTEGER NOT NULL DEFAULT 0,
                complete INTEGER NOT NULL DEFAULT 0
            );
            ",
        )?;
        Self::ensure_context_columns(conn)?;
        Ok(())
    }

    fn name_index_build_table_exists(conn: &Connection) -> SqlResult<bool> {
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'name_index_build'",
            [],
            |row| row.get(0),
        )?;
        Ok(n > 0)
    }

    /// Existing DBs have rows but no build-progress row; treat them as finished
    /// so cache_hit still skips a rebuild. Never overwrite an in-progress row.
    fn backfill_legacy_complete(conn: &Connection) -> SqlResult<()> {
        conn.execute_batch(
            "
            INSERT OR IGNORE INTO name_index_build(region_id, expected, written, complete)
            SELECT region_id, COUNT(*), COUNT(*), 1
            FROM name_entries
            WHERE region_id != ''
            GROUP BY region_id;
            INSERT OR IGNORE INTO name_index_build(region_id, expected, written, complete)
            SELECT '', COUNT(*), COUNT(*), 1
            FROM name_entries
            WHERE region_id = ''
            HAVING COUNT(*) > 0;
            ",
        )?;
        Ok(())
    }

    fn ensure_context_columns(conn: &Connection) -> SqlResult<()> {
        let mut has_sub = false;
        let mut has_muni = false;
        let mut has_region = false;
        let mut stmt = conn.prepare("PRAGMA table_info(name_entries)")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
        for r in rows {
            match r?.as_str() {
                "sub_area" => has_sub = true,
                "municipality" => has_muni = true,
                "region_id" => has_region = true,
                _ => {}
            }
        }
        if !has_sub {
            conn.execute(
                "ALTER TABLE name_entries ADD COLUMN sub_area TEXT NOT NULL DEFAULT ''",
                [],
            )?;
        }
        if !has_muni {
            conn.execute(
                "ALTER TABLE name_entries ADD COLUMN municipality TEXT NOT NULL DEFAULT ''",
                [],
            )?;
        }
        if !has_region {
            conn.execute(
                "ALTER TABLE name_entries ADD COLUMN region_id TEXT NOT NULL DEFAULT ''",
                [],
            )?;
        }
        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_name_entries_region_id ON name_entries(region_id);",
        )?;
        Ok(())
    }

    /// True when `name_entries` has at least one row (built index, not a stub).
    /// Read-only: never creates the DB file.
    pub fn has_entries(path: impl AsRef<Path>) -> bool {
        let path = path.as_ref();
        if !path.is_file() {
            return false;
        }
        let Ok(conn) =
            Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        else {
            return false;
        };
        conn.query_row("SELECT 1 FROM name_entries LIMIT 1", [], |_| Ok(()))
            .is_ok()
    }

    /// True when this DB already has at least one row for `region_id`.
    pub fn has_entries_for_region(path: impl AsRef<Path>, region_id: &str) -> bool {
        let path = path.as_ref();
        let region_id = region_id.trim().trim_matches('/');
        if !path.is_file() || region_id.is_empty() {
            return false;
        }
        let Ok(conn) =
            Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        else {
            return false;
        };
        conn.query_row(
            "SELECT 1 FROM name_entries WHERE region_id = ?1 LIMIT 1",
            params![region_id],
            |_| Ok(()),
        )
        .is_ok()
    }

    /// True when this DB was built by a context-aware `load_from_pbf`.
    pub fn is_current_schema(path: impl AsRef<Path>) -> bool {
        let Ok(conn) = Connection::open(path.as_ref()) else {
            return false;
        };
        let v: i32 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap_or(0);
        v >= PLACE_INDEX_SCHEMA_VERSION
    }

    /// Delete an on-device index whose `user_version` is below
    /// [`PLACE_INDEX_SCHEMA_VERSION`].
    ///
    /// Used before rebuild so a multi-region DB cannot keep pre-bump row kinds
    /// (e.g. buildings as `named`) after only one region is re-indexed and the
    /// pragma advances. Returns true when a file was removed.
    pub fn discard_if_schema_stale(path: impl AsRef<Path>) -> bool {
        let path = path.as_ref();
        if !path.is_file() || Self::is_current_schema(path) {
            return false;
        }
        match std::fs::remove_file(path) {
            Ok(()) => {
                Self::remove_wal_sidecars(path);
                log::info!(
                    target: "NaviSearch",
                    "place-index: discarded stale schema DB {}",
                    path.display()
                );
                true
            }
            Err(e) => {
                log::warn!(
                    target: "NaviSearch",
                    "place-index: failed to discard stale schema DB {}: {e}",
                    path.display()
                );
                false
            }
        }
    }

    fn remove_wal_sidecars(path: &Path) {
        for suffix in ["-wal", "-shm"] {
            let mut sidecar = path.as_os_str().to_os_string();
            sidecar.push(suffix);
            let _ = std::fs::remove_file(PathBuf::from(sidecar));
        }
    }

    /// True when this region's index finished a write (not a mid-build partial).
    ///
    /// Missing `name_index_build` table (never opened with this code) falls back
    /// to [`has_entries_for_region`] / [`has_entries`] so legacy DBs still cache-hit.
    pub fn region_index_complete(path: impl AsRef<Path>, region_id: &str) -> bool {
        let path = path.as_ref();
        let region_id = region_id.trim().trim_matches('/');
        if !path.is_file() {
            return false;
        }
        let Ok(conn) =
            Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        else {
            return false;
        };
        match conn.query_row(
            "SELECT complete FROM name_index_build WHERE region_id = ?1",
            params![region_id],
            |row| row.get::<_, i64>(0),
        ) {
            Ok(v) => v != 0,
            Err(_) => {
                if region_id.is_empty() {
                    Self::has_entries(path)
                } else {
                    Self::has_entries_for_region(path, region_id)
                }
            }
        }
    }

    fn build_is_interrupted(conn: &Connection, region_id: &str) -> bool {
        conn.query_row(
            "SELECT complete FROM name_index_build WHERE region_id = ?1",
            params![region_id],
            |row| row.get::<_, i64>(0),
        )
        .ok()
        .is_some_and(|v| v == 0)
    }

    fn build_written(conn: &Connection, region_id: &str) -> i64 {
        conn.query_row(
            "SELECT written FROM name_index_build WHERE region_id = ?1",
            params![region_id],
            |row| row.get(0),
        )
        .unwrap_or(0)
    }

    /// Full-file rebuild with empty `region_id` (legacy / single-extract callers).
    pub fn load_from_pbf(&mut self, path: impl AsRef<Path>) -> anyhow::Result<usize> {
        self.load_from_pbf_for_region(path, "")
    }

    /// Index one region's PBF into the shared DB without wiping other regions.
    ///
    /// When `region_id` is non-empty, only that region's rows are replaced.
    /// When empty, the whole table is cleared (legacy single-extract path).
    pub fn load_from_pbf_for_region(
        &mut self,
        path: impl AsRef<Path>,
        region_id: &str,
    ) -> anyhow::Result<usize> {
        let total_t0 = phase_timing::start("place_index.total");
        let _bg = crate::download::pbf_priority::BackgroundIndexerGuard::enter();
        let path = path.as_ref();
        let region_id = region_id.trim().trim_matches('/').to_string();
        let mut batch: Vec<(i64, String, String, f64, f64)> = Vec::new();
        const PHASES: u64 = 6;
        let interrupted = Self::build_is_interrupted(&self.conn, &region_id);
        let phase_prefix = if interrupted {
            "Place index: previous build interrupted, restarting — "
        } else {
            "Place index: "
        };
        // Admin polygons use their own PBF passes (relations → ways → nodes).
        // Sub-labels are set inside load_admin_from_pbf so the 0/6 phase is not
        // a single frozen "admin boundaries…" string for a minute-plus scan.
        let admin_t0 = phase_timing::start("place_index.admin");
        let admin_rings =
            place_context::load_admin_from_pbf(path, phase_prefix).unwrap_or_else(|e| {
                log::warn!("admin boundary load for place context skipped: {e:#}");
                Vec::new()
            });
        phase_timing::end_detail(
            "place_index.admin",
            admin_t0,
            &format!("admin_rings={}", admin_rings.len()),
        );

        // Pass 1: collect named closed/open ways that need node centroids
        // (tourism=zoo, amenity areas, etc. are often ways, not nodes).
        crate::download::progress::set(1, Some(PHASES), &format!("{phase_prefix}scanning ways…"));
        let ways_t0 = phase_timing::start("place_index.ways");
        let mut way_jobs: Vec<(i64, String, String, Vec<i64>)> = Vec::new();
        let mut needed_nodes: std::collections::HashSet<i64> = std::collections::HashSet::new();
        let mut ways_visited = 0usize;
        let mut ways_last_hb = 0usize;
        {
            crate::download::pbf_priority::for_each_pbf_elements(path, |element| {
                ways_visited += 1;
                if ways_visited - ways_last_hb >= PLACE_INDEX_PROGRESS_HEARTBEAT {
                    ways_last_hb = ways_visited;
                    crate::download::progress::set(
                        1,
                        Some(PHASES),
                        &format!("{phase_prefix}scanning ways… ({} found)", way_jobs.len()),
                    );
                }
                let Element::Way(way) = element else {
                    return;
                };
                let tags: Vec<(String, String)> = way
                    .tags()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect();
                let Some((_, name, kind, _, _)) = classify_named(
                    way.id(),
                    0.0,
                    0.0,
                    tags.iter().map(|(k, v)| (k.as_str(), v.as_str())),
                ) else {
                    return;
                };
                // Skip unnamed highway geometries — those flood the index; keep
                // amenity/tourism/place/leisure/shop and other classified POIs.
                if kind.starts_with("highway:") {
                    return;
                }
                let refs: Vec<i64> = way.refs().collect();
                if refs.is_empty() {
                    return;
                }
                for id in &refs {
                    needed_nodes.insert(*id);
                }
                way_jobs.push((way.id(), name, kind, refs));
            })?;
        }
        phase_timing::end_detail(
            "place_index.ways",
            ways_t0,
            &format!(
                "way_jobs={} needed_nodes={}",
                way_jobs.len(),
                needed_nodes.len()
            ),
        );

        // Pass 2: nodes (search hits) + coords for way centroids.
        crate::download::progress::set(2, Some(PHASES), &format!("{phase_prefix}scanning nodes…"));
        let nodes_t0 = phase_timing::start("place_index.nodes");
        let mut node_coords: std::collections::HashMap<i64, (f64, f64)> =
            std::collections::HashMap::with_capacity(needed_nodes.len());
        let mut nodes_visited = 0usize;
        let mut nodes_last_hb = 0usize;
        {
            crate::download::pbf_priority::for_each_pbf_elements(path, |element| {
                nodes_visited += 1;
                if nodes_visited - nodes_last_hb >= PLACE_INDEX_PROGRESS_HEARTBEAT {
                    nodes_last_hb = nodes_visited;
                    crate::download::progress::set(
                        2,
                        Some(PHASES),
                        &format!("{phase_prefix}scanning nodes… ({} found)", batch.len()),
                    );
                }
                match element {
                    Element::Node(node) => {
                        let id = node.id();
                        let lat = node.lat();
                        let lon = node.lon();
                        if needed_nodes.contains(&id) {
                            node_coords.insert(id, (lat, lon));
                        }
                        if let Some(hit) = classify_named(id, lat, lon, node.tags()) {
                            batch.push(hit);
                        }
                    }
                    Element::DenseNode(node) => {
                        let id = node.id;
                        let lat = node.lat();
                        let lon = node.lon();
                        if needed_nodes.contains(&id) {
                            node_coords.insert(id, (lat, lon));
                        }
                        if let Some(hit) = classify_named(id, lat, lon, node.tags()) {
                            batch.push(hit);
                        }
                    }
                    _ => {}
                }
            })?;
        }
        let node_hits = batch.len();
        phase_timing::end_detail(
            "place_index.nodes",
            nodes_t0,
            &format!(
                "node_hits={} centroid_coords={}",
                node_hits,
                node_coords.len()
            ),
        );

        // Surface centroids on the progress UI — this pass used to leave the
        // last "scanning nodes…" label frozen for a long stretch.
        crate::download::progress::set(2, Some(PHASES), &format!("{phase_prefix}way centroids…"));
        let centroids_t0 = phase_timing::start("place_index.way_centroids");
        let mut way_hits = 0usize;
        let centroid_total = way_jobs.len();
        for (i, (way_id, name, kind, refs)) in way_jobs.into_iter().enumerate() {
            let mut sum_lat = 0.0;
            let mut sum_lon = 0.0;
            let mut n = 0usize;
            for id in refs {
                if let Some((lat, lon)) = node_coords.get(&id) {
                    sum_lat += lat;
                    sum_lon += lon;
                    n += 1;
                }
            }
            if n == 0 {
                continue;
            }
            batch.push((way_id, name, kind, sum_lat / n as f64, sum_lon / n as f64));
            way_hits += 1;
            if (i + 1) % PLACE_INDEX_PROGRESS_HEARTBEAT == 0 {
                crate::download::progress::set(
                    2,
                    Some(PHASES),
                    &format!("{phase_prefix}way centroids… ({way_hits} / {centroid_total})"),
                );
            }
        }
        drop(node_coords);
        phase_timing::end_detail(
            "place_index.way_centroids",
            centroids_t0,
            &format!("way_hits={way_hits} batch={}", batch.len()),
        );

        // Official hiking/cycling route relations (name/ref/operator) for To/Via search.
        // Relation ids are distinct from node ids in OSM; store relation id as-is
        // (FTS rowid = osm_id).
        crate::download::progress::set(3, Some(PHASES), &format!("{phase_prefix}named routes…"));
        let routes_t0 = phase_timing::start("place_index.named_routes");
        crate::download::pbf_priority::yield_if_foreground_plan();
        let mut route_hits = 0usize;
        match crate::routing::graph::load_named_route_entries(path) {
            Ok(routes) => {
                route_hits = routes.len();
                for r in routes {
                    batch.push((r.osm_id, r.name, r.kind, r.lat, r.lon));
                }
            }
            Err(e) => {
                log::warn!("named route relation index skipped: {e:#}");
            }
        }
        phase_timing::end_detail(
            "place_index.named_routes",
            routes_t0,
            &format!("route_hits={route_hits} batch={}", batch.len()),
        );

        crate::download::progress::set(
            4,
            Some(PHASES),
            &format!("{phase_prefix}resolving context…"),
        );
        let ctx_t0 = phase_timing::start("place_index.resolve_context");
        let sub_areas = batch
            .iter()
            .filter_map(|(osm_id, name, kind, lat, lon)| {
                place_context::sub_area_pt(*osm_id, name.clone(), kind, *lat, *lon)
            })
            .collect();
        let resolver =
            place_context::ContextResolver::from_admin_and_sub_areas(admin_rings, sub_areas);
        phase_timing::end_detail(
            "place_index.resolve_context",
            ctx_t0,
            &format!("batch={}", batch.len()),
        );

        crate::download::progress::set(
            5,
            Some(PHASES),
            &format!("{phase_prefix}writing database…"),
        );
        let write_t0 = phase_timing::start("place_index.sqlite_write");
        let resume_written = if interrupted {
            Self::build_written(&self.conn, &region_id)
        } else {
            0
        };
        let resume = interrupted && resume_written > 0;
        let skip: HashSet<i64> = if resume {
            Self::osm_ids_for_region(&self.conn, &region_id)?
        } else {
            HashSet::new()
        };

        let clear_t0 = phase_timing::start("place_index.sqlite_clear_region");
        {
            let tx = self.conn.unchecked_transaction()?;
            if !resume {
                Self::clear_region_rows(&tx, &region_id)?;
            }
            Self::upsert_build_progress(
                &tx,
                &region_id,
                batch.len() as i64,
                if resume { resume_written } else { 0 },
                false,
            )?;
            tx.commit()?;
        }
        phase_timing::end("place_index.sqlite_clear_region", clear_t0);

        let insert_t0 = phase_timing::start("place_index.sqlite_insert_rows");
        let total = batch.len();
        let mut inserted = skip.len();
        let mut since_commit = 0usize;
        let mut tx = self.conn.unchecked_transaction()?;
        for (osm_id, name, kind, lat, lon) in &batch {
            if skip.contains(osm_id) {
                continue;
            }
            let ctx = resolver.resolve(*osm_id, name, kind, *lat, *lon);
            // osm_id may already exist from another region at a landsdel border —
            // replace and refresh FTS for that id.
            let _ = tx.execute(
                "INSERT INTO name_fts(name_fts, rowid, name, kind) VALUES('delete', ?1, NULL, NULL)",
                params![osm_id],
            );
            tx.execute(
                "INSERT OR REPLACE INTO name_entries(osm_id, name, kind, lat, lon, sub_area, municipality, region_id)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                params![
                    osm_id,
                    name,
                    kind,
                    lat,
                    lon,
                    ctx.sub_area,
                    ctx.municipality,
                    region_id
                ],
            )?;
            tx.execute(
                "INSERT INTO name_fts(rowid, name, kind) VALUES (?1,?2,?3)",
                params![osm_id, name, kind],
            )?;
            inserted += 1;
            since_commit += 1;
            if since_commit >= INSERT_COMMIT_BATCH {
                Self::upsert_build_progress(&tx, &region_id, total as i64, inserted as i64, false)?;
                tx.commit()?;
                crate::download::progress::set(
                    inserted as u64,
                    Some(total as u64),
                    &format!("{phase_prefix}writing database…"),
                );
                crate::download::pbf_priority::yield_if_foreground_plan();
                tx = self.conn.unchecked_transaction()?;
                since_commit = 0;
            }
        }
        phase_timing::end_detail(
            "place_index.sqlite_insert_rows",
            insert_t0,
            &format!("rows={total} resumed={resume}"),
        );
        let commit_t0 = phase_timing::start("place_index.sqlite_commit");
        tx.execute_batch(&format!(
            "PRAGMA user_version = {PLACE_INDEX_SCHEMA_VERSION};"
        ))?;
        Self::upsert_build_progress(&tx, &region_id, total as i64, inserted as i64, true)?;
        tx.commit()?;
        phase_timing::end("place_index.sqlite_commit", commit_t0);
        phase_timing::end_detail(
            "place_index.sqlite_write",
            write_t0,
            &format!("rows={total}"),
        );
        crate::download::progress::set(PHASES, Some(PHASES), "Place index ready");
        phase_timing::end_detail(
            "place_index.total",
            total_t0,
            &format!("indexed={}", batch.len()),
        );
        Ok(batch.len())
    }

    /// Remove all place rows for one region (or the entire index when `region_id` is empty).
    pub fn clear_region(&mut self, region_id: &str) -> SqlResult<()> {
        let region_id = region_id.trim().trim_matches('/');
        let tx = self.conn.unchecked_transaction()?;
        Self::clear_region_rows(&tx, region_id)?;
        tx.commit()?;
        Ok(())
    }

    fn clear_region_rows(tx: &rusqlite::Transaction<'_>, region_id: &str) -> SqlResult<()> {
        if region_id.is_empty() {
            tx.execute_batch(
                "
                DELETE FROM name_entries;
                INSERT INTO name_fts(name_fts) VALUES('delete-all');
                ",
            )?;
            return Ok(());
        }
        {
            let mut stmt = tx.prepare("SELECT osm_id FROM name_entries WHERE region_id = ?1")?;
            let ids: Vec<i64> = stmt
                .query_map(params![region_id], |row| row.get(0))?
                .collect::<SqlResult<Vec<_>>>()?;
            drop(stmt);
            for osm_id in ids {
                let _ = tx.execute(
                    "INSERT INTO name_fts(name_fts, rowid, name, kind) VALUES('delete', ?1, NULL, NULL)",
                    params![osm_id],
                );
            }
        }
        tx.execute(
            "DELETE FROM name_entries WHERE region_id = ?1",
            params![region_id],
        )?;
        // External-content FTS5 can retain orphan index rows when content was
        // deleted without matching FTS 'delete' commands (e.g. Android framework
        // SQLite lacking FTS5 cleared name_entries first). Rebuild syncs the
        // index to the remaining content table so orphans cannot MATCH.
        let _ = tx.execute_batch("INSERT INTO name_fts(name_fts) VALUES('rebuild');");
        Ok(())
    }

    fn osm_ids_for_region(conn: &Connection, region_id: &str) -> SqlResult<HashSet<i64>> {
        let mut stmt = conn.prepare("SELECT osm_id FROM name_entries WHERE region_id = ?1")?;
        let rows = stmt.query_map(params![region_id], |row| row.get(0))?;
        rows.collect()
    }

    fn upsert_build_progress(
        tx: &rusqlite::Transaction<'_>,
        region_id: &str,
        expected: i64,
        written: i64,
        complete: bool,
    ) -> SqlResult<()> {
        tx.execute(
            "INSERT INTO name_index_build(region_id, expected, written, complete)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(region_id) DO UPDATE SET
                expected = excluded.expected,
                written = excluded.written,
                complete = excluded.complete",
            params![region_id, expected, written, i64::from(complete)],
        )?;
        Ok(())
    }

    /// Insert or replace one name row (tests / incremental updates).
    pub fn upsert_entry(
        &mut self,
        osm_id: i64,
        name: String,
        kind: String,
        lat: f64,
        lon: f64,
    ) -> SqlResult<()> {
        self.upsert_entry_with_context(osm_id, name, kind, lat, lon, String::new(), String::new())
    }

    pub fn upsert_entry_with_context(
        &mut self,
        osm_id: i64,
        name: String,
        kind: String,
        lat: f64,
        lon: f64,
        sub_area: String,
        municipality: String,
    ) -> SqlResult<()> {
        self.upsert_entry_with_region(
            osm_id,
            name,
            kind,
            lat,
            lon,
            sub_area,
            municipality,
            String::new(),
        )
    }

    pub fn upsert_entry_with_region(
        &mut self,
        osm_id: i64,
        name: String,
        kind: String,
        lat: f64,
        lon: f64,
        sub_area: String,
        municipality: String,
        region_id: String,
    ) -> SqlResult<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO name_entries(osm_id, name, kind, lat, lon, sub_area, municipality, region_id)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![osm_id, name, kind, lat, lon, sub_area, municipality, region_id],
        )?;
        // Rebuild FTS row for this id (delete + insert keeps content sync).
        let _ = self.conn.execute(
            "INSERT INTO name_fts(name_fts, rowid, name, kind) VALUES('delete', ?1, NULL, NULL)",
            params![osm_id],
        );
        self.conn.execute(
            "INSERT INTO name_fts(rowid, name, kind) VALUES (?1,?2,?3)",
            params![osm_id, name, kind],
        )?;
        Ok(())
    }

    pub fn search(&self, query: &str, limit: usize) -> SqlResult<Vec<NameHit>> {
        let q = query.trim();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let prefix = format!("{}*", q.replace('"', ""));
        // Over-fetch then rank: FTS order is not ideal when many addr:* rows share
        // a street-name prefix with a settlement (e.g. "Nordre Ott*" → Ottvegen).
        let fetch = (limit.saturating_mul(8)).clamp(40, 200);
        let mut stmt = self.conn.prepare(
            "
            SELECT e.osm_id, e.name, e.kind, e.lat, e.lon, e.sub_area, e.municipality, e.region_id
            FROM name_fts f
            JOIN name_entries e ON e.osm_id = f.rowid
            WHERE name_fts MATCH ?1
            LIMIT ?2
            ",
        )?;
        let rows = stmt.query_map(params![prefix, fetch as i64], |row| {
            Ok(NameHit {
                osm_id: row.get(0)?,
                name: row.get(1)?,
                kind: row.get(2)?,
                lat: row.get(3)?,
                lon: row.get(4)?,
                sub_area: row.get(5)?,
                municipality: row.get(6)?,
                region_id: row.get(7)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        let q_lower = q.to_lowercase();
        out.sort_by(|a, b| {
            let score = |h: &NameHit| -> (i32, i32, usize) {
                let name_l = h.name.to_lowercase();
                let starts = if name_l.starts_with(&q_lower) { 0 } else { 1 };
                let kind_rank = if h.kind.starts_with("place:") {
                    0
                } else if h.kind.starts_with("tourism:")
                    || h.kind.starts_with("amenity:")
                    || h.kind.starts_with("leisure:")
                    || h.kind.starts_with("natural:")
                {
                    1
                } else if h.kind.starts_with("addr:") {
                    3
                } else {
                    2
                };
                (starts, kind_rank, h.name.len())
            };
            score(a).cmp(&score(b))
        });
        out.truncate(limit);
        Ok(out)
    }

    /// Entries within [radius_m] of `(lat, lon)`, nearest first (Haversine).
    ///
    /// Uses a degree bbox prefilter then exact distance — fine for place-index
    /// sizes; not a substitute for a dedicated road-edge spatial index.
    pub fn nearby(
        &self,
        lat: f64,
        lon: f64,
        radius_m: f64,
        limit: usize,
    ) -> SqlResult<Vec<NameHit>> {
        let radius_m = radius_m.max(1.0);
        let limit = limit.max(1);
        let lat_pad = radius_m / 111_320.0;
        let lon_pad = radius_m / (111_320.0 * lat.to_radians().cos().max(0.2));
        let mut stmt = self.conn.prepare(
            "
            SELECT osm_id, name, kind, lat, lon, sub_area, municipality, region_id
            FROM name_entries
            WHERE lat BETWEEN ?1 AND ?2 AND lon BETWEEN ?3 AND ?4
            ",
        )?;
        let rows = stmt.query_map(
            params![lat - lat_pad, lat + lat_pad, lon - lon_pad, lon + lon_pad],
            |row| {
                Ok(NameHit {
                    osm_id: row.get(0)?,
                    name: row.get(1)?,
                    kind: row.get(2)?,
                    lat: row.get(3)?,
                    lon: row.get(4)?,
                    sub_area: row.get(5)?,
                    municipality: row.get(6)?,
                    region_id: row.get(7)?,
                })
            },
        )?;
        let mut scored: Vec<(f64, NameHit)> = Vec::new();
        for r in rows {
            let hit = r?;
            let d_m = crate::tracks::haversine_km(lat, lon, hit.lat, hit.lon) * 1000.0;
            if d_m <= radius_m {
                scored.push((d_m, hit));
            }
        }
        scored.sort_by(|a, b| a.0.total_cmp(&b.0));
        Ok(scored.into_iter().take(limit).map(|(_, h)| h).collect())
    }

    /// Named places of an exact `kind` inside a lat/lon bbox (inclusive).
    pub fn places_of_kind_in_bbox(
        &self,
        kind: &str,
        min_lat: f64,
        min_lon: f64,
        max_lat: f64,
        max_lon: f64,
        limit: usize,
    ) -> SqlResult<Vec<NameHit>> {
        let limit = limit.max(1);
        let mut stmt = self.conn.prepare(
            "
            SELECT osm_id, name, kind, lat, lon, sub_area, municipality, region_id
            FROM name_entries
            WHERE kind = ?1
              AND lat BETWEEN ?2 AND ?3
              AND lon BETWEEN ?4 AND ?5
            ORDER BY name
            LIMIT ?6
            ",
        )?;
        let rows = stmt.query_map(
            params![kind, min_lat, max_lat, min_lon, max_lon, limit as i64],
            |row| {
                Ok(NameHit {
                    osm_id: row.get(0)?,
                    name: row.get(1)?,
                    kind: row.get(2)?,
                    lat: row.get(3)?,
                    lon: row.get(4)?,
                    sub_area: row.get(5)?,
                    municipality: row.get(6)?,
                    region_id: row.get(7)?,
                })
            },
        )?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }
}

fn classify_named<'a>(
    osm_id: i64,
    lat: f64,
    lon: f64,
    tags: impl Iterator<Item = (&'a str, &'a str)>,
) -> Option<(i64, String, String, f64, f64)> {
    let mut name = None;
    let mut addr_street = None;
    let mut addr_housenumber = None;
    let mut kind = "named".to_string();
    let mut is_building = false;
    for (k, v) in tags {
        match k {
            "name" => name = Some(v.to_string()),
            "addr:street" => addr_street = Some(v.to_string()),
            "addr:housenumber" => addr_housenumber = Some(v.to_string()),
            "place" => kind = format!("place:{v}"),
            "tourism" => kind = format!("tourism:{v}"),
            "leisure" => kind = format!("leisure:{v}"),
            "natural" if v == "peak" => kind = "natural:peak".into(),
            "highway" => kind = format!("highway:{v}"),
            "amenity" => kind = format!("amenity:{v}"),
            "shop" => kind = format!("shop:{v}"),
            // Plain named footprints (building=* + name=*) — not amenity/shop.
            // Amenity/shop/place above win when present so POIs stay classified.
            "building" if !v.eq_ignore_ascii_case("no") => is_building = true,
            _ => {}
        }
    }
    if name.is_none() {
        if let (Some(street), Some(num)) = (addr_street.as_ref(), addr_housenumber.as_ref()) {
            name = Some(format!("{street} {num}"));
            if kind == "named" {
                kind = "addr:housenumber".into();
            }
        } else if let Some(street) = addr_street {
            name = Some(street);
            if kind == "named" {
                kind = "addr:street".into();
            }
        }
    }
    if kind == "named" && is_building {
        kind = NAMED_BUILDING_KIND.to_string();
    }
    name.map(|n| (osm_id, n, kind, lat, lon))
}

impl NameIndex {
    /// Named building footprints (`kind = building`) inside a lat/lon bbox.
    ///
    /// Requires a place index built at schema ≥ v4 ([`PLACE_INDEX_SCHEMA_VERSION`])
    /// so `classify_named` stored buildings as [`NAMED_BUILDING_KIND`]. Older
    /// on-device indexes are discarded and rebuilt on next `ensure_place_index`.
    pub fn named_buildings_in_bbox(
        &self,
        min_lat: f64,
        min_lon: f64,
        max_lat: f64,
        max_lon: f64,
        limit: usize,
    ) -> SqlResult<Vec<NameHit>> {
        self.places_of_kind_in_bbox(
            NAMED_BUILDING_KIND,
            min_lat,
            min_lon,
            max_lat,
            max_lon,
            limit,
        )
    }
}

/// Saved route persistence (host UI route list).
#[derive(Debug, Clone)]
pub struct SavedRoute {
    pub id: String,
    pub start_lat: f64,
    pub start_lon: f64,
    pub start_name: Option<String>,
    pub end_lat: f64,
    pub end_lon: f64,
    pub end_name: Option<String>,
    pub via_json: String,
    pub profile: String,
    pub vehicle_json: String,
    pub summary_json: String,
    pub created_at: String,
    pub last_break_lat: Option<f64>,
    pub last_break_lon: Option<f64>,
    pub last_overnight_lat: Option<f64>,
    pub last_overnight_lon: Option<f64>,
}

pub struct RouteStore<'a> {
    storage: &'a Storage,
}

impl<'a> RouteStore<'a> {
    pub fn new(storage: &'a Storage) -> Self {
        Self { storage }
    }

    pub fn insert(&self, route: &SavedRoute) -> SqlResult<()> {
        self.storage.with_conn(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO routes(
                    id, start_lat, start_lon, start_name, end_lat, end_lon, end_name,
                    via_json, profile, vehicle_json, summary_json, created_at,
                    last_break_lat, last_break_lon, last_overnight_lat, last_overnight_lon
                ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
                params![
                    route.id,
                    route.start_lat,
                    route.start_lon,
                    route.start_name,
                    route.end_lat,
                    route.end_lon,
                    route.end_name,
                    route.via_json,
                    route.profile,
                    route.vehicle_json,
                    route.summary_json,
                    route.created_at,
                    route.last_break_lat,
                    route.last_break_lon,
                    route.last_overnight_lat,
                    route.last_overnight_lon,
                ],
            )?;
            Ok(())
        })
    }

    pub fn delete(&self, id: &str) -> SqlResult<()> {
        self.storage.with_conn(|conn| {
            conn.execute("DELETE FROM routes WHERE id = ?1", params![id])?;
            Ok(())
        })
    }

    pub fn get(&self, id: &str) -> SqlResult<Option<SavedRoute>> {
        self.storage.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, start_lat, start_lon, start_name, end_lat, end_lon, end_name,
                        via_json, profile, vehicle_json, summary_json, created_at,
                        last_break_lat, last_break_lon, last_overnight_lat, last_overnight_lon
                 FROM routes WHERE id = ?1",
            )?;
            let mut rows = stmt.query(params![id])?;
            let Some(row) = rows.next()? else {
                return Ok(None);
            };
            Ok(Some(SavedRoute {
                id: row.get(0)?,
                start_lat: row.get(1)?,
                start_lon: row.get(2)?,
                start_name: row.get(3)?,
                end_lat: row.get(4)?,
                end_lon: row.get(5)?,
                end_name: row.get(6)?,
                via_json: row.get(7)?,
                profile: row.get(8)?,
                vehicle_json: row.get(9)?,
                summary_json: row.get(10)?,
                created_at: row.get(11)?,
                last_break_lat: row.get(12)?,
                last_break_lon: row.get(13)?,
                last_overnight_lat: row.get(14)?,
                last_overnight_lon: row.get(15)?,
            }))
        })
    }

    pub fn list(&self) -> SqlResult<Vec<SavedRoute>> {
        self.storage.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, start_lat, start_lon, start_name, end_lat, end_lon, end_name,
                        via_json, profile, vehicle_json, summary_json, created_at,
                        last_break_lat, last_break_lon, last_overnight_lat, last_overnight_lon
                 FROM routes ORDER BY created_at DESC",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(SavedRoute {
                    id: row.get(0)?,
                    start_lat: row.get(1)?,
                    start_lon: row.get(2)?,
                    start_name: row.get(3)?,
                    end_lat: row.get(4)?,
                    end_lon: row.get(5)?,
                    end_name: row.get(6)?,
                    via_json: row.get(7)?,
                    profile: row.get(8)?,
                    vehicle_json: row.get(9)?,
                    summary_json: row.get(10)?,
                    created_at: row.get(11)?,
                    last_break_lat: row.get(12)?,
                    last_break_lon: row.get(13)?,
                    last_overnight_lat: row.get(14)?,
                    last_overnight_lon: row.get(15)?,
                })
            })?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            Ok(out)
        })
    }
}

/// Named single-coordinate place (distinct from a full saved route corridor).
#[derive(Debug, Clone)]
pub struct SavedPlace {
    pub id: String,
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub kind: String,
    pub created_at: String,
}

pub struct PlaceStore<'a> {
    storage: &'a Storage,
}

impl<'a> PlaceStore<'a> {
    pub fn new(storage: &'a Storage) -> Self {
        Self { storage }
    }

    pub fn insert(&self, place: &SavedPlace) -> SqlResult<()> {
        self.storage.with_conn(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO saved_places(id, name, lat, lon, kind, created_at)
                 VALUES (?1,?2,?3,?4,?5,?6)",
                params![
                    place.id,
                    place.name,
                    place.lat,
                    place.lon,
                    place.kind,
                    place.created_at,
                ],
            )?;
            Ok(())
        })
    }

    pub fn rename(&self, id: &str, name: &str) -> SqlResult<bool> {
        self.storage.with_conn(|conn| {
            let n = conn.execute(
                "UPDATE saved_places SET name = ?1 WHERE id = ?2",
                params![name, id],
            )?;
            Ok(n > 0)
        })
    }

    pub fn delete(&self, id: &str) -> SqlResult<()> {
        self.storage.with_conn(|conn| {
            conn.execute("DELETE FROM saved_places WHERE id = ?1", params![id])?;
            Ok(())
        })
    }

    pub fn list(&self) -> SqlResult<Vec<SavedPlace>> {
        self.storage.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, lat, lon, kind, created_at
                 FROM saved_places ORDER BY created_at DESC",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(SavedPlace {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    lat: row.get(2)?,
                    lon: row.get(3)?,
                    kind: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            Ok(out)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fts_matches_norwegian_special_chars() {
        let mut idx = NameIndex::open_in_memory().expect("mem index");
        idx.upsert_entry(1, "Mjøsvegen".into(), "highway:tertiary".into(), 60.8, 11.0)
            .unwrap();
        idx.upsert_entry(2, "Trollåsveien".into(), "named".into(), 59.8, 10.8)
            .unwrap();
        idx.upsert_entry(
            3,
            "Ævongsli 2".into(),
            "addr:housenumber".into(),
            61.0,
            11.2,
        )
        .unwrap();
        idx.upsert_entry(
            4,
            "Bjørnhollia".into(),
            "tourism:alpine_hut".into(),
            61.7,
            10.0,
        )
        .unwrap();

        let mjos = idx.search("Mjøs", 8).unwrap();
        assert!(
            mjos.iter().any(|h| h.name.contains('ø')),
            "expected ø hit, got {mjos:?}"
        );
        let aas = idx.search("Trollås", 8).unwrap();
        assert!(aas.iter().any(|h| h.name.contains('å')), "got {aas:?}");
        let ae = idx.search("Ævongsli", 8).unwrap();
        assert!(!ae.is_empty(), "æ/Æ query empty");
        let bj = idx.search("Bjørn", 8).unwrap();
        assert!(bj.iter().any(|h| h.name.contains('ø')), "got {bj:?}");
    }

    /// Document SQLite FTS5 unicode61 folding: å/ü fold to ASCII bases; æ/ø do not.
    #[test]
    fn fts_unicode61_folds_aa_but_not_ae_oe() {
        let mut idx = NameIndex::open_in_memory().expect("mem index");
        idx.upsert_entry(1, "Eldåbu".into(), "tourism:alpine_hut".into(), 61.75, 9.97)
            .unwrap();
        idx.upsert_entry(2, "Bærums Verk".into(), "place:suburb".into(), 59.94, 10.50)
            .unwrap();
        idx.upsert_entry(3, "Løten".into(), "place:village".into(), 60.82, 11.34)
            .unwrap();
        idx.upsert_entry(4, "Müllerstraße".into(), "named".into(), 52.5, 13.4)
            .unwrap();

        // Stored glyphs unchanged.
        assert_eq!(idx.search("Eldåbu", 1).unwrap()[0].name, "Eldåbu");
        assert!(idx.search("Bærum", 1).unwrap()[0].name.contains('æ'));
        assert!(idx.search("Løten", 1).unwrap()[0].name.contains('ø'));

        // å / ü fold → ASCII keyboard can find them.
        assert!(
            idx.search("Eldabu", 8)
                .unwrap()
                .iter()
                .any(|h| h.name == "Eldåbu"),
            "å should fold to a"
        );
        assert!(
            idx.search("Muller", 8)
                .unwrap()
                .iter()
                .any(|h| h.name.contains('ü') || h.name.contains("Muller")),
            "ü should fold to u"
        );

        // æ / ø do NOT fold to ae / o — ASCII approximations miss (product quirk).
        assert!(
            idx.search("Baerum", 8).unwrap().is_empty(),
            "æ must not match ae under unicode61"
        );
        assert!(
            idx.search("Loten", 8)
                .unwrap()
                .iter()
                .all(|h| h.name != "Løten"),
            "ø must not match plain o under unicode61"
        );
    }

    #[test]
    fn nearby_finds_peer_gyntvegen_address() {
        let mut idx = NameIndex::open_in_memory().expect("mem");
        idx.upsert_entry(
            1,
            "Peer Gyntvegen 1377".into(),
            "addr:housenumber".into(),
            61.420522,
            9.927719,
        )
        .unwrap();
        idx.upsert_entry(
            2,
            "Steinbrotvegen 4".into(),
            "addr:housenumber".into(),
            61.420086,
            9.927864,
        )
        .unwrap();
        let hits = idx.nearby(61.419774, 9.927647, 120.0, 8).expect("nearby");
        assert!(!hits.is_empty());
        assert!(
            hits.iter().any(|h| h.name.starts_with("Peer Gyntvegen")),
            "expected Peer Gyntvegen near fix, got {hits:?}"
        );
    }

    #[test]
    fn place_store_insert_list_rename_delete() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let db = dir.path().join("navi.db");
        let storage = crate::storage::Storage::open(&db).expect("open");
        let store = PlaceStore::new(&storage);
        store
            .insert(&SavedPlace {
                id: "p1".into(),
                name: "Cabin ridge".into(),
                lat: 61.85,
                lon: 10.23,
                kind: "map-mark".into(),
                created_at: "100".into(),
            })
            .unwrap();
        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "Cabin ridge");
        assert!(store.rename("p1", "Atnbrufossen lookout").unwrap());
        assert_eq!(store.list().unwrap()[0].name, "Atnbrufossen lookout");
        store.delete("p1").unwrap();
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn route_store_get_and_gpx_export_roundtrip() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let db = dir.path().join("navi.db");
        let storage = crate::storage::Storage::open(&db).expect("open");
        let store = RouteStore::new(&storage);
        let via_json = r#"[{"name":"Via","lat":61.2,"lon":10.4}]"#;
        store
            .insert(&SavedRoute {
                id: "r1".into(),
                start_lat: 61.1,
                start_lon: 10.5,
                start_name: Some("Start".into()),
                end_lat: 61.3,
                end_lon: 10.2,
                end_name: Some("End".into()),
                via_json: via_json.into(),
                profile: "car".into(),
                vehicle_json: "{}".into(),
                summary_json: "{}".into(),
                created_at: "2026-09-04T08:00:00Z".into(),
                last_break_lat: None,
                last_break_lon: None,
                last_overnight_lat: None,
                last_overnight_lon: None,
            })
            .unwrap();
        let got = store.get("r1").unwrap().expect("row");
        assert_eq!(got.start_name.as_deref(), Some("Start"));
        let rte = crate::export::route_points_from_saved(
            got.start_lat,
            got.start_lon,
            got.start_name.as_deref(),
            got.end_lat,
            got.end_lon,
            got.end_name.as_deref(),
            &got.via_json,
        );
        assert_eq!(rte.len(), 3);
        let poly = "10.500000,61.100000;10.400000,61.200000;10.200000,61.300000";
        let track = crate::export::parse_route_polyline(poly);
        let xml = crate::export::to_gpx(Some("Start -> End"), Some(&got.created_at), &rte, &track);
        assert!(xml.contains("<rtept"));
        assert_eq!(xml.matches("<rtept").count(), 3);
        assert_eq!(xml.matches("<trkpt").count(), 3);
        for pt in &rte {
            let needle = format!(r#"lat="{:.6}" lon="{:.6}""#, pt.lat, pt.lon);
            assert!(xml.contains(&needle), "missing {needle}");
        }
        assert!(store.get("missing").unwrap().is_none());
    }

    #[test]
    fn search_returns_precomputed_area_context() {
        let mut idx = NameIndex::open_in_memory().expect("mem");
        idx.upsert_entry_with_context(
            1,
            "Båberg".into(),
            "place:farm".into(),
            60.96849,
            10.54821,
            "Brattberg".into(),
            "Gjøvik".into(),
        )
        .unwrap();
        idx.upsert_entry_with_context(
            2,
            "Båberg".into(),
            "place:farm".into(),
            60.92416,
            10.83636,
            "Løken".into(),
            "Ringsaker".into(),
        )
        .unwrap();
        let hits = idx.search("Båberg", 8).unwrap();
        assert_eq!(hits.len(), 2);
        let labels: Vec<String> = hits
            .iter()
            .map(|h| format_place_display(&h.name, &h.sub_area, &h.municipality))
            .collect();
        assert!(
            labels.iter().any(|s| s == "Båberg, Brattberg, Gjøvik"),
            "got {labels:?}"
        );
        assert!(
            labels.iter().any(|s| s == "Båberg, Løken, Ringsaker"),
            "got {labels:?}"
        );
    }

    #[test]
    fn migrate_adds_context_columns_to_legacy_table() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let db = dir.path().join("legacy.db");
        {
            let conn = rusqlite::Connection::open(&db).unwrap();
            conn.execute_batch(
                "
                CREATE TABLE name_entries (
                    osm_id INTEGER PRIMARY KEY NOT NULL,
                    name TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    lat REAL NOT NULL,
                    lon REAL NOT NULL
                );
                CREATE VIRTUAL TABLE name_fts USING fts5(
                    name, kind, content='name_entries', content_rowid='osm_id'
                );
                ",
            )
            .unwrap();
            conn.execute(
                "INSERT INTO name_entries(osm_id, name, kind, lat, lon) VALUES (1,'Tangen','place:village',60.6,11.2)",
                [],
            )
            .unwrap();
        }
        assert!(!NameIndex::is_current_schema(&db));
        let idx = NameIndex::open(&db).expect("open legacy");
        let hits = idx.nearby(60.6, 11.2, 500.0, 4).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "Tangen");
        assert_eq!(hits[0].municipality, "");
        assert_eq!(hits[0].sub_area, "");
    }

    #[test]
    fn discard_if_schema_stale_removes_pre_v4_db() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let db = dir.path().join("stale.db");
        {
            let conn = rusqlite::Connection::open(&db).unwrap();
            conn.execute_batch(
                "
                CREATE TABLE name_entries (
                    osm_id INTEGER PRIMARY KEY NOT NULL,
                    name TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    lat REAL NOT NULL,
                    lon REAL NOT NULL,
                    sub_area TEXT NOT NULL DEFAULT '',
                    municipality TEXT NOT NULL DEFAULT '',
                    region_id TEXT NOT NULL DEFAULT ''
                );
                PRAGMA user_version = 3;
                ",
            )
            .unwrap();
            conn.execute(
                "INSERT INTO name_entries(osm_id, name, kind, lat, lon, region_id)
                 VALUES (1,'Named Hall','named',61.0,10.0,'europe/norway/ostlandet')",
                [],
            )
            .unwrap();
        }
        assert!(!NameIndex::is_current_schema(&db));
        assert!(NameIndex::discard_if_schema_stale(&db));
        assert!(!db.is_file());
        assert!(!NameIndex::discard_if_schema_stale(&db));
    }

    #[test]
    fn multi_region_index_is_additive_and_reindex_preserves_other() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let db = dir.path().join("place_index.db");
        let mut idx = NameIndex::open(&db).expect("open");
        idx.upsert_entry_with_region(
            101,
            "Lillehammer".into(),
            "place:town".into(),
            61.11,
            10.46,
            String::new(),
            String::new(),
            "europe/norway/ostlandet".into(),
        )
        .unwrap();
        idx.upsert_entry_with_region(
            201,
            "Bergen".into(),
            "place:city".into(),
            60.39,
            5.32,
            String::new(),
            String::new(),
            "europe/norway/vestlandet".into(),
        )
        .unwrap();
        drop(idx);

        assert!(NameIndex::has_entries_for_region(
            &db,
            "europe/norway/ostlandet"
        ));
        assert!(NameIndex::has_entries_for_region(
            &db,
            "europe/norway/vestlandet"
        ));

        // Simulate re-index of region A: clear A then re-upsert.
        let mut idx = NameIndex::open(&db).expect("reopen");
        idx.clear_region("europe/norway/ostlandet").unwrap();
        idx.upsert_entry_with_region(
            102,
            "Gjøvik".into(),
            "place:town".into(),
            60.80,
            10.69,
            String::new(),
            String::new(),
            "europe/norway/ostlandet".into(),
        )
        .unwrap();

        let ost = idx.search("Gjøvik", 8).unwrap();
        assert!(ost.iter().any(|h| h.name == "Gjøvik"), "got {ost:?}");
        let vest = idx.search("Bergen", 8).unwrap();
        assert!(
            vest.iter().any(|h| h.name == "Bergen"),
            "vestlandet wiped: {vest:?}"
        );
        let old = idx.search("Lillehammer", 8).unwrap();
        assert!(
            old.iter().all(|h| h.name != "Lillehammer"),
            "old ostlandet row should be gone"
        );
    }

    #[test]
    fn has_entries_false_on_empty_and_true_after_upsert() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let db = dir.path().join("empty.db");
        NameIndex::open(&db).expect("create");
        assert!(!NameIndex::has_entries(&db));
        let mut idx = NameIndex::open(&db).expect("reopen");
        idx.upsert_entry(1, "Oslo".into(), "place:city".into(), 59.91, 10.75)
            .unwrap();
        drop(idx);
        assert!(NameIndex::has_entries(&db));
    }

    #[test]
    fn has_entries_missing_file_does_not_create() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let db = dir.path().join("no-such.db");
        assert!(!NameIndex::has_entries(&db));
        assert!(!db.exists());
    }

    /// Orphan FTS rows (content deleted without FTS delete) must not surface after
    /// clear_region: search JOINs to name_entries, and clear rebuilds FTS.
    #[test]
    fn clear_region_rebuild_drops_orphan_fts_hits() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let db = dir.path().join("orphan.db");
        let mut idx = NameIndex::open(&db).expect("create");
        idx.upsert_entry_with_region(
            42,
            "GamleNavn".into(),
            "place:village".into(),
            60.0,
            10.0,
            String::new(),
            String::new(),
            "europe/norway/ostlandet".into(),
        )
        .unwrap();
        // Simulate Android clear without FTS5: wipe content, leave FTS stale.
        idx.conn
            .execute(
                "DELETE FROM name_entries WHERE region_id = ?1",
                ["europe/norway/ostlandet"],
            )
            .unwrap();
        // Stale FTS may still MATCH; JOIN should yield nothing.
        let pre = idx.search("GamleNavn", 8).unwrap();
        assert!(
            pre.iter().all(|h| h.name != "GamleNavn"),
            "JOIN must hide orphans before rebuild: {pre:?}"
        );
        // clear_region (with rebuild) then re-index under a new name.
        idx.clear_region("europe/norway/ostlandet").unwrap();
        idx.upsert_entry_with_region(
            42,
            "NyttNavn".into(),
            "place:village".into(),
            60.0,
            10.0,
            String::new(),
            String::new(),
            "europe/norway/ostlandet".into(),
        )
        .unwrap();
        let old = idx.search("GamleNavn", 8).unwrap();
        assert!(
            old.iter()
                .all(|h| h.name != "GamleNavn" && h.name != "NyttNavn"),
            "old name must not match after rebuild+rename: {old:?}"
        );
        let neu = idx.search("NyttNavn", 8).unwrap();
        assert!(neu.iter().any(|h| h.name == "NyttNavn"), "got {neu:?}");
    }

    #[test]
    fn classify_named_building_kind_and_amenity_override() {
        let building = classify_named(
            435718754,
            61.885_475,
            10.737_108,
            [("building", "yes"), ("name", "Espedalsvegen 656")].into_iter(),
        )
        .expect("named building");
        assert_eq!(building.2, NAMED_BUILDING_KIND);
        assert_eq!(building.1, "Espedalsvegen 656");

        let amenity = classify_named(
            1,
            60.0,
            10.0,
            [
                ("building", "yes"),
                ("name", "Rådhuset"),
                ("amenity", "townhall"),
            ]
            .into_iter(),
        )
        .expect("townhall");
        assert_eq!(amenity.2, "amenity:townhall");

        assert!(classify_named(
            2,
            60.0,
            10.0,
            [("building", "no"), ("name", "X")].into_iter()
        )
        .is_some());
        let not_building = classify_named(
            2,
            60.0,
            10.0,
            [("building", "no"), ("name", "X")].into_iter(),
        )
        .unwrap();
        assert_eq!(not_building.2, "named");
    }

    /// Espedalsvegen 656 area (OSM way/435718754 coords) round-trips via bbox query.
    #[test]
    fn named_buildings_in_bbox_round_trip_espedal() {
        let mut idx = NameIndex::open_in_memory().expect("mem");
        // Simulate classify_named + upsert for the reported building.
        let (id, name, kind, lat, lon) = classify_named(
            435718754,
            61.885_475,
            10.737_108,
            [("building", "yes"), ("name", "Espedalsvegen 656")].into_iter(),
        )
        .unwrap();
        assert_eq!(kind, NAMED_BUILDING_KIND);
        idx.upsert_entry(id, name.clone(), kind, lat, lon).unwrap();
        // Distractors: city + generic named (not building).
        idx.upsert_entry(9, "Lillehammer".into(), "place:city".into(), 61.115, 10.466)
            .unwrap();
        idx.upsert_entry(10, "Some Peak Label".into(), "named".into(), 61.885, 10.737)
            .unwrap();

        let hits = idx
            .named_buildings_in_bbox(61.88, 10.73, 61.89, 10.74, 32)
            .unwrap();
        assert_eq!(hits.len(), 1, "expected only the building: {hits:?}");
        assert_eq!(hits[0].osm_id, 435718754);
        assert_eq!(hits[0].name, "Espedalsvegen 656");
        assert_eq!(hits[0].kind, NAMED_BUILDING_KIND);
        assert!((hits[0].lat - 61.885_475).abs() < 1e-6);
        assert!((hits[0].lon - 10.737_108).abs() < 1e-6);

        let empty = idx
            .named_buildings_in_bbox(59.0, 10.0, 60.0, 11.0, 8)
            .unwrap();
        assert!(empty.is_empty());
    }

    #[test]
    fn file_index_uses_wal_and_synchronous_normal() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let db = dir.path().join("place_index.db");
        let idx = NameIndex::open(&db).expect("open");
        let mode: String = idx
            .conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(mode.to_lowercase(), "wal");
        let sync: i64 = idx
            .conn
            .query_row("PRAGMA synchronous", [], |row| row.get(0))
            .unwrap();
        assert_eq!(sync, 1, "WAL should use synchronous=NORMAL (1), got {sync}");
    }

    #[test]
    fn wal_readonly_query_does_not_wait_on_writer_transaction() {
        use std::time::Instant;
        let dir = tempfile::tempdir().expect("tmpdir");
        let db = dir.path().join("wal.db");
        let mut writer = NameIndex::open(&db).expect("open");
        writer
            .upsert_entry_with_region(
                1,
                "Oslo".into(),
                "place:city".into(),
                59.91,
                10.75,
                String::new(),
                String::new(),
                "europe/norway/ostlandet".into(),
            )
            .unwrap();
        let tx = writer.conn.unchecked_transaction().unwrap();
        tx.execute(
            "UPDATE name_entries SET name = 'HIDDEN' WHERE osm_id = 1",
            [],
        )
        .unwrap();
        let t0 = Instant::now();
        let reader = NameIndex::open_readonly(&db).expect("readonly");
        let hits = reader.search("Oslo", 8).unwrap();
        let elapsed = t0.elapsed();
        assert!(
            elapsed.as_millis() < 200,
            "readonly open/search blocked for {elapsed:?} (expected WAL snapshot)"
        );
        assert!(
            hits.iter().any(|h| h.name == "Oslo"),
            "WAL reader should see committed snapshot, got {hits:?}"
        );
        drop(hits);
        drop(reader);
        tx.rollback().unwrap();
    }

    #[test]
    fn incomplete_build_is_not_region_index_complete() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let db = dir.path().join("partial.db");
        let mut idx = NameIndex::open(&db).expect("open");
        idx.upsert_entry_with_region(
            1,
            "Oslo".into(),
            "place:city".into(),
            59.91,
            10.75,
            String::new(),
            String::new(),
            "europe/norway/ostlandet".into(),
        )
        .unwrap();
        idx.conn
            .execute(
                "INSERT INTO name_index_build(region_id, expected, written, complete)
                 VALUES ('europe/norway/ostlandet', 100, 1, 0)
                 ON CONFLICT(region_id) DO UPDATE SET written=1, complete=0",
                [],
            )
            .unwrap();
        drop(idx);
        assert!(NameIndex::has_entries_for_region(
            &db,
            "europe/norway/ostlandet"
        ));
        assert!(
            !NameIndex::region_index_complete(&db, "europe/norway/ostlandet"),
            "partial write must not cache-hit"
        );
    }

    #[test]
    fn place_index_build_lock_serializes_concurrent_callers() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::thread;
        use std::time::Duration;

        static INSIDE: AtomicUsize = AtomicUsize::new(0);
        static MAX: AtomicUsize = AtomicUsize::new(0);

        let threads: Vec<_> = (0..4)
            .map(|_| {
                thread::spawn(|| {
                    let _g = lock_place_index_build();
                    let now = INSIDE.fetch_add(1, Ordering::SeqCst) + 1;
                    MAX.fetch_max(now, Ordering::SeqCst);
                    thread::sleep(Duration::from_millis(30));
                    INSIDE.fetch_sub(1, Ordering::SeqCst);
                })
            })
            .collect();
        for t in threads {
            t.join().expect("thread");
        }
        assert_eq!(
            MAX.load(Ordering::SeqCst),
            1,
            "two place-index builders must not overlap"
        );
    }

    #[test]
    fn place_index_build_lock_wait_sets_progress_label() {
        use std::sync::{Arc, Barrier};
        use std::thread;
        use std::time::Duration;

        crate::download::progress::clear();
        let hold = Arc::new(Barrier::new(2));
        let released = Arc::new(Barrier::new(2));
        let holder = thread::spawn({
            let hold = hold.clone();
            let released = released.clone();
            move || {
                let _g = lock_place_index_build();
                hold.wait();
                released.wait();
            }
        });
        hold.wait();
        let waiter = thread::spawn(|| {
            let _g = lock_place_index_build_with_progress();
        });
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            let snap = crate::download::progress::snapshot();
            if snap.label == "Place index: waiting for another index build…" {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "wait label never appeared, last={:?}",
                snap.label
            );
            thread::sleep(Duration::from_millis(5));
        }
        let snap = crate::download::progress::snapshot();
        assert_eq!(snap.units_done, 0);
        assert_eq!(snap.units_total, Some(6));
        released.wait();
        waiter.join().expect("waiter");
        holder.join().expect("holder");
    }

    /// Task 4 race: ensure_place_index (build lock + WAL open) concurrent with
    /// clear on the same DB. Pre-fix Android SQLite clear without this lock
    /// SIGBUS'd ~1/4 of windows; under the shared lock every iteration must
    /// keep max concurrent holders at 1 and finish without error.
    #[test]
    fn clear_region_rows_serializes_against_build_lock_stress() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{Arc, Barrier};
        use std::thread;
        use std::time::Duration;

        let dir = tempfile::tempdir().expect("tmpdir");
        let db = dir.path().join("place_index.db");
        {
            let mut idx = NameIndex::open(&db).expect("open");
            idx.upsert_entry_with_region(
                1,
                "Alpha".into(),
                "place".into(),
                60.0,
                10.0,
                String::new(),
                String::new(),
                "europe/norway/ostlandet".into(),
            )
            .expect("upsert a");
            idx.upsert_entry_with_region(
                2,
                "Beta".into(),
                "place".into(),
                57.0,
                12.0,
                String::new(),
                String::new(),
                "europe/sweden/halland".into(),
            )
            .expect("upsert b");
        }

        let concurrent = Arc::new(AtomicUsize::new(0));
        let max_concurrent = Arc::new(AtomicUsize::new(0));
        let failures = Arc::new(AtomicUsize::new(0));
        const ITERS: usize = 100;

        for i in 0..ITERS {
            let barrier = Arc::new(Barrier::new(2));
            let db_a = db.clone();
            let db_b = db.clone();
            let concurrent_a = concurrent.clone();
            let max_a = max_concurrent.clone();
            let concurrent_b = concurrent.clone();
            let max_b = max_concurrent.clone();
            let failures_b = failures.clone();
            let region = if i % 2 == 0 {
                "europe/sweden/halland"
            } else {
                "europe/norway/ostlandet"
            };

            let builder = thread::spawn({
                let barrier = barrier.clone();
                move || {
                    // Mimic ensure_place_index: hold build lock + open WAL.
                    let _g = lock_place_index_build();
                    let now = concurrent_a.fetch_add(1, Ordering::SeqCst) + 1;
                    max_a.fetch_max(now, Ordering::SeqCst);
                    barrier.wait();
                    let _idx = NameIndex::open(&db_a).expect("builder open");
                    thread::sleep(Duration::from_millis(5));
                    concurrent_a.fetch_sub(1, Ordering::SeqCst);
                }
            });
            let clearer = thread::spawn({
                let barrier = barrier.clone();
                move || {
                    barrier.wait();
                    // Same lock + rusqlite stack as clear_place_index_region_rows
                    // (and as Kotlin's preferred FFI path). Counting under the
                    // lock proves we never overlap the builder's WAL open.
                    let _g = lock_place_index_build();
                    let now = concurrent_b.fetch_add(1, Ordering::SeqCst) + 1;
                    max_b.fetch_max(now, Ordering::SeqCst);
                    let clear_result = (|| {
                        let mut idx = NameIndex::open(&db_b).map_err(|e| e.to_string())?;
                        idx.clear_region(region).map_err(|e| e.to_string())?;
                        Ok::<(), String>(())
                    })();
                    concurrent_b.fetch_sub(1, Ordering::SeqCst);
                    drop(_g);
                    if let Err(e) = clear_result {
                        failures_b.fetch_add(1, Ordering::SeqCst);
                        eprintln!("clear failed: {e}");
                    }
                }
            });
            builder.join().expect("builder");
            clearer.join().expect("clearer");
        }

        assert_eq!(
            failures.load(Ordering::SeqCst),
            0,
            "clear must not fail (was: database is locked / SIGBUS)"
        );
        assert_eq!(
            max_concurrent.load(Ordering::SeqCst),
            1,
            "build lock must serialize builder and clearer ({ITERS} iters)"
        );
        // Re-open after the race window: WAL must still be readable (no
        // truncated -shm / SIGBUS-class corruption).
        NameIndex::open(&db).expect("reopen after stress");
    }

    #[test]
    fn clear_place_index_region_rows_api_waits_on_build_lock() {
        use std::sync::{Arc, Barrier};
        use std::thread;
        use std::time::{Duration, Instant};

        let dir = tempfile::tempdir().expect("tmpdir");
        let db = dir.path().join("place_index.db");
        {
            let mut idx = NameIndex::open(&db).expect("open");
            idx.upsert_entry_with_region(
                1,
                "X".into(),
                "place".into(),
                60.0,
                10.0,
                String::new(),
                String::new(),
                "europe/norway/ostlandet".into(),
            )
            .expect("upsert");
        }

        let hold = Arc::new(Barrier::new(2));
        let released = Arc::new(Barrier::new(2));
        let db_hold = db.clone();
        let holder = thread::spawn({
            let hold = hold.clone();
            let released = released.clone();
            move || {
                let _g = lock_place_index_build();
                let _idx = NameIndex::open(&db_hold).expect("hold open");
                hold.wait();
                thread::sleep(Duration::from_millis(50));
                released.wait();
            }
        });
        hold.wait();
        let started = Instant::now();
        let clearer = thread::spawn({
            let db = db.clone();
            move || clear_place_index_region_rows(&db, "europe/norway/ostlandet")
        });
        thread::sleep(Duration::from_millis(20));
        assert!(
            !clearer.is_finished(),
            "clear_place_index_region_rows must block while build lock is held"
        );
        released.wait();
        let result = clearer.join().expect("clearer");
        holder.join().expect("holder");
        assert!(result.is_ok(), "clear after release: {result:?}");
        assert!(
            started.elapsed() >= Duration::from_millis(50),
            "clear returned before holder finished"
        );
    }

    #[test]
    fn ffi_acquire_release_blocks_concurrent_builder() {
        use std::sync::{Arc, Barrier};
        use std::thread;
        use std::time::{Duration, Instant};

        let hold = Arc::new(Barrier::new(2));
        let released = Arc::new(Barrier::new(2));
        let acquirer = thread::spawn({
            let hold = hold.clone();
            let released = released.clone();
            move || {
                place_index_build_lock_acquire().expect("acquire");
                assert!(place_index_build_lock_held_on_thread());
                hold.wait();
                released.wait();
                place_index_build_lock_release().expect("release");
            }
        });
        hold.wait();
        let started = Instant::now();
        let builder = thread::spawn(|| {
            let _g = lock_place_index_build();
        });
        thread::sleep(Duration::from_millis(40));
        assert!(
            !builder.is_finished(),
            "builder must block while FFI acquire holds the lock"
        );
        released.wait();
        builder.join().expect("builder");
        acquirer.join().expect("acquirer");
        assert!(
            started.elapsed() >= Duration::from_millis(40),
            "builder returned too fast"
        );
    }

    /// Synthetic multi-region DB used to decide whether `backfill_legacy_complete`
    /// dominates `NameIndex::open`. Ignored: seeds >= 2M rows (slow, disk-heavy).
    #[test]
    #[ignore]
    fn open_two_million_rows_prints_timings() {
        use std::time::Instant;

        struct PhaseLog;
        impl log::Log for PhaseLog {
            fn enabled(&self, metadata: &log::Metadata) -> bool {
                metadata.target() == "PHASE_TIMING"
            }
            fn log(&self, record: &log::Record) {
                if self.enabled(record.metadata()) {
                    println!("{}", record.args());
                }
            }
            fn flush(&self) {}
        }
        static LOGGER: PhaseLog = PhaseLog;
        let _ = log::set_logger(&LOGGER);
        log::set_max_level(log::LevelFilter::Info);

        let dir = tempfile::tempdir().expect("tmpdir");
        let db = dir.path().join("big.db");
        {
            let conn = Connection::open(&db).unwrap();
            conn.execute_batch(
                "
                PRAGMA journal_mode = WAL;
                PRAGMA synchronous = OFF;
                CREATE TABLE name_entries (
                    osm_id INTEGER PRIMARY KEY NOT NULL,
                    name TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    lat REAL NOT NULL,
                    lon REAL NOT NULL,
                    sub_area TEXT NOT NULL DEFAULT '',
                    municipality TEXT NOT NULL DEFAULT '',
                    region_id TEXT NOT NULL DEFAULT ''
                );
                CREATE TABLE name_index_build (
                    region_id TEXT PRIMARY KEY NOT NULL,
                    expected INTEGER NOT NULL DEFAULT 0,
                    written INTEGER NOT NULL DEFAULT 0,
                    complete INTEGER NOT NULL DEFAULT 0
                );
                ",
            )
            .unwrap();
            conn.execute(
                "INSERT INTO name_entries(osm_id, name, kind, lat, lon, region_id)
                 VALUES (1, 'n', 'place:village', 60.0, 10.0, 'europe/norway/ostlandet')",
                [],
            )
            .unwrap();
            loop {
                let n: i64 = conn
                    .query_row("SELECT COUNT(*) FROM name_entries", [], |row| row.get(0))
                    .unwrap();
                if n >= 2_000_000 {
                    break;
                }
                conn.execute(
                    "INSERT INTO name_entries(osm_id, name, kind, lat, lon, sub_area, municipality, region_id)
                     SELECT osm_id + ?1, name, kind, lat, lon, sub_area, municipality,
                       CASE ((osm_id + ?1) % 3)
                         WHEN 0 THEN 'europe/norway/ostlandet'
                         WHEN 1 THEN 'europe/norway/vestlandet'
                         ELSE 'europe/norway/trondelag'
                       END
                     FROM name_entries",
                    params![n],
                )
                .unwrap();
            }
            let n: i64 = conn
                .query_row("SELECT COUNT(*) FROM name_entries", [], |row| row.get(0))
                .unwrap();
            let distinct: i64 = conn
                .query_row(
                    "SELECT COUNT(DISTINCT region_id) FROM name_entries",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            println!("seeded {n} name_entries rows across {distinct} region ids");
            conn.execute_batch(
                "
                INSERT INTO name_index_build(region_id, expected, written, complete)
                SELECT region_id, COUNT(*), COUNT(*), 1 FROM name_entries GROUP BY region_id;
                ",
            )
            .unwrap();
        }

        {
            let conn = Connection::open(&db).unwrap();
            conn.busy_timeout(Duration::from_secs(5)).unwrap();
            let t0 = Instant::now();
            NameIndex::apply_file_pragmas(&conn).unwrap();
            println!("apply_file_pragmas {:?}", t0.elapsed());
            let t0 = Instant::now();
            NameIndex::migrate(&conn).unwrap();
            println!("migrate {:?}", t0.elapsed());
            let t0 = Instant::now();
            NameIndex::backfill_legacy_complete(&conn).unwrap();
            println!("backfill_legacy_complete {:?}", t0.elapsed());
        }

        let t0 = Instant::now();
        let _idx = NameIndex::open(&db).expect("open 2M-row index");
        println!(
            "NameIndex::open total {:?} (backfill skipped when name_index_build already exists)",
            t0.elapsed()
        );
    }

    fn name_entries_ddl_with_region() -> &'static str {
        "
            CREATE TABLE name_entries (
                osm_id INTEGER PRIMARY KEY NOT NULL,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                lat REAL NOT NULL,
                lon REAL NOT NULL,
                sub_area TEXT NOT NULL DEFAULT '',
                municipality TEXT NOT NULL DEFAULT '',
                region_id TEXT NOT NULL DEFAULT ''
            );
        "
    }

    #[test]
    fn open_backfills_legacy_db_without_build_table() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let db = dir.path().join("legacy-no-build.db");
        {
            let conn = Connection::open(&db).unwrap();
            conn.execute_batch(name_entries_ddl_with_region()).unwrap();
            conn.execute(
                "INSERT INTO name_entries(osm_id, name, kind, lat, lon, region_id)
                 VALUES (1, 'Oslo', 'place:city', 59.91, 10.75, 'europe/norway/ostlandet')",
                [],
            )
            .unwrap();
        }
        let _idx = NameIndex::open(&db).expect("open legacy");
        assert!(
            NameIndex::region_index_complete(&db, "europe/norway/ostlandet"),
            "legacy rows must get a complete name_index_build row"
        );
    }

    #[test]
    fn open_skips_backfill_when_build_table_already_exists() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let db = dir.path().join("has-build.db");
        {
            let conn = Connection::open(&db).unwrap();
            conn.execute_batch(name_entries_ddl_with_region()).unwrap();
            conn.execute_batch(
                "
                CREATE TABLE name_index_build (
                    region_id TEXT PRIMARY KEY NOT NULL,
                    expected INTEGER NOT NULL DEFAULT 0,
                    written INTEGER NOT NULL DEFAULT 0,
                    complete INTEGER NOT NULL DEFAULT 0
                );
                ",
            )
            .unwrap();
            conn.execute(
                "INSERT INTO name_entries(osm_id, name, kind, lat, lon, region_id)
                 VALUES (1, 'Oslo', 'place:city', 59.91, 10.75, 'europe/norway/ostlandet')",
                [],
            )
            .unwrap();
        }
        let _idx = NameIndex::open(&db).expect("open");
        let conn = Connection::open(&db).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM name_index_build", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(
            n, 0,
            "GROUP BY backfill must not run when name_index_build already existed"
        );
    }
}
