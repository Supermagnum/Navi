package no.navi.app

import android.database.sqlite.SQLiteDatabase
import android.util.Log
import java.io.File

/**
 * Read-only place-index integrity for one `region_id`.
 *
 * Never opens the DB read-write, never runs DDL, never deletes the shared file.
 * Intact requires real rows for that region (complete=1 with 0 or a handful of
 * rows is missing). Call off the main thread.
 */
object PlaceIndexIntact {
    const val TAG = "PlaceIndexIntact"
    const val SCHEMA_VERSION = RegionDownloadBackground.PLACE_INDEX_SCHEMA_VERSION
    const val MIN_DB_BYTES = 10_000L

    /** Written count may exceed surviving rows slightly; 10% floor rejects 14-vs-1.5M. */
    const val MIN_COUNT_RATIO = 0.10
    const val QUICK_CHECK_STAMP = "place_index_quick_check.txt"
    const val QUICK_CHECK_MAX_AGE_MS = 12L * 60L * 60L * 1000L

    data class Probe(
        val intact: Boolean,
        val dbBytes: Long,
        val userVersion: Int,
        val complete: Int?,
        val expected: Long,
        val written: Long,
        val rowCount: Long,
        val legacy: Boolean,
        val reason: String,
        val sourceSha256: String = "",
        val indexSource: String = "",
    )

    /** Stand-in for `place_index.db` in host JVM tests, which have no Android SQLite. */
    internal interface RowSource {
        /** region_id to row count, or null when [dataDir] has no database. */
        fun rows(dataDir: File): Map<String, Long>?

        fun clearRegion(
            dataDir: File,
            regionId: String,
        )
    }

    @Volatile
    internal var rowSourceForTests: RowSource? = null

    fun isIntact(
        dataDir: File,
        regionId: String,
    ): Boolean = probe(dataDir, regionId).intact

    /** Not intact, nothing read; [reason] says why. */
    fun unavailable(reason: String): Probe =
        Probe(
            intact = false,
            dbBytes = 0L,
            userVersion = 0,
            complete = null,
            expected = 0L,
            written = 0L,
            rowCount = 0L,
            legacy = false,
            reason = reason,
        )

    fun probe(
        dataDir: File,
        regionId: String,
    ): Probe {
        val rid = PackRegionAvailability.normalize(regionId)
        val empty = unavailable("empty_region_id")
        if (rid.isEmpty()) return empty
        rowSourceForTests?.let { src ->
            val rows = src.rows(dataDir) ?: return empty.copy(reason = "db_missing")
            val n = rows[rid] ?: 0L
            return empty.copy(
                intact = n > 0L,
                userVersion = SCHEMA_VERSION,
                rowCount = n,
                reason = if (n > 0L) "ok" else "no_rows_no_build",
            )
        }
        val dbFile = File(dataDir, "place_index.db")
        if (!dbFile.isFile) {
            return empty.copy(reason = "db_missing")
        }
        val bytes = dbFile.length()
        if (bytes < MIN_DB_BYTES) {
            return empty.copy(dbBytes = bytes, reason = "db_tiny")
        }
        // Torn -wal/-shm after a killed index build SIGBUS's Android SQLite in
        // walIndexReadHdr (uncatchable). Recover before any framework open.
        recoverWalSidecars(dbFile)
        return runCatching {
            SQLiteDatabase
                .openDatabase(
                    dbFile.absolutePath,
                    null,
                    SQLiteDatabase.OPEN_READONLY,
                ).use { db ->
                    probeOpen(db, bytes, rid)
                }
        }.getOrElse { t ->
            Log.w(TAG, "readonly probe failed region=$rid: ${t.message}")
            empty.copy(dbBytes = bytes, reason = "open_failed")
        }
    }

    /**
     * Region ids that have a build record or rows in the database under
     * [dataDir]. Empty when the file is missing, tiny or unreadable.
     */
    fun indexedRegionIds(dataDir: File): Set<String> {
        rowSourceForTests?.let { src ->
            return src
                .rows(dataDir)
                ?.filterValues { it > 0L }
                ?.keys
                ?.map { PackRegionAvailability.normalize(it) }
                ?.toSet()
                .orEmpty()
        }
        val dbFile = File(dataDir, "place_index.db")
        if (!dbFile.isFile || dbFile.length() < MIN_DB_BYTES) return emptySet()
        recoverWalSidecars(dbFile)
        return runCatching {
            SQLiteDatabase
                .openDatabase(dbFile.absolutePath, null, SQLiteDatabase.OPEN_READONLY)
                .use { db ->
                    val cursor =
                        runCatching { db.rawQuery("SELECT region_id FROM name_index_build", null) }
                            .getOrElse {
                                db.rawQuery("SELECT DISTINCT region_id FROM name_entries", null)
                            }
                    cursor.use { c ->
                        buildSet {
                            while (c.moveToNext()) {
                                val id = PackRegionAvailability.normalize(c.getString(0) ?: "")
                                if (id.isNotEmpty()) add(id)
                            }
                        }
                    }
                }
        }.getOrDefault(emptySet())
    }

    /**
     * Why the whole place index is unusable under [dataDir], or null when the
     * file is present and opens. Every region counts as missing when non-null.
     */
    fun fileProblem(dataDir: File): String? {
        rowSourceForTests?.let { src ->
            return if (src.rows(dataDir) == null) "file missing" else null
        }
        val dbFile = File(dataDir, "place_index.db")
        if (!dbFile.isFile) return "file missing"
        val bytes = dbFile.length()
        if (bytes < MIN_DB_BYTES) return "file empty ($bytes bytes)"
        recoverWalSidecars(dbFile)
        val opens =
            runCatching {
                SQLiteDatabase
                    .openDatabase(dbFile.absolutePath, null, SQLiteDatabase.OPEN_READONLY)
                    .use { db -> db.rawQuery("PRAGMA user_version", null).use { it.moveToFirst() } }
            }.getOrDefault(false)
        if (!opens) return "file unreadable"
        return corruptionStampProblem(dataDir)
    }

    /**
     * Automatic recovery before any Android framework open. Call on cold start
     * and from every probe path — end users must not need adb.
     *
     * Order: UniFFI checkpoint → framework RW checkpoint → drop `-shm` →
     * quarantine unreadable `-wal` so the main file can still open.
     */
    fun recoverBeforeOpen(dataDir: File) {
        val dbFile = File(dataDir, "place_index.db")
        if (dbFile.isFile) recoverWalSidecars(dbFile)
    }

    /**
     * Checkpoint torn WAL / drop stale shm before Android framework opens.
     * Prefers UniFFI rust path; falls back to framework SQLite then quarantine.
     */
    private fun recoverWalSidecars(dbFile: File) {
        val path = dbFile.absolutePath
        val wal = File("$path-wal")
        val shm = File("$path-shm")
        if (!wal.isFile && !shm.isFile) return
        val native =
            runCatching { uniffi.navi.placeIndexRecoverWal(path) }
                .onFailure { t -> Log.w(TAG, "placeIndexRecoverWal threw: ${t.message}") }
                .getOrNull()
        if (native != null && !native.startsWith("FAIL")) {
            Log.i(TAG, "wal recover native ok")
            return
        }
        if (native != null) {
            Log.w(TAG, "placeIndexRecoverWal: $native")
        }
        // Framework fallback (UniFFI not loaded yet, or rust checkpoint failed).
        if (shm.isFile()) {
            runCatching { shm.delete() }
                .onFailure { t -> Log.w(TAG, "delete shm failed: ${t.message}") }
        }
        val checkpointed =
            runCatching {
                SQLiteDatabase.openDatabase(path, null, SQLiteDatabase.OPEN_READWRITE).use { db ->
                    db.rawQuery("PRAGMA wal_checkpoint(TRUNCATE)", null).use { c ->
                        c.moveToFirst()
                    }
                }
                true
            }.getOrElse { t ->
                Log.w(TAG, "framework wal_checkpoint failed: ${t.message}")
                false
            }
        if (checkpointed) {
            Log.i(TAG, "wal recover framework checkpoint ok")
            return
        }
        // Last resort: quarantine WAL so OPEN_READONLY can use the main file.
        // Uncheckpointed frames may be lost; incomplete regions rebuild later.
        if (wal.isFile()) {
            val bad = File("$path-wal.bad")
            runCatching {
                if (bad.exists()) bad.delete()
                if (!wal.renameTo(bad)) wal.delete()
            }.onFailure { t -> Log.w(TAG, "quarantine wal failed: ${t.message}") }
            Log.w(TAG, "quarantined place_index.db-wal; may need region re-index")
        }
    }

    /** Idle-only stamp from [runIdleQuickCheck]; never opens SQLite here. */
    fun corruptionStampProblem(dataDir: File): String? {
        val stamp = File(dataDir, QUICK_CHECK_STAMP)
        if (!stamp.isFile) return null
        val first =
            stamp
                .readLines()
                .firstOrNull()
                ?.trim()
                .orEmpty()
        if (first.isEmpty() || first == "ok") return null
        return "damaged ($first)"
    }

    fun quickCheckStampIsFresh(dataDir: File): Boolean {
        val stamp = File(dataDir, QUICK_CHECK_STAMP)
        if (!stamp.isFile) return false
        return System.currentTimeMillis() - stamp.lastModified() < QUICK_CHECK_MAX_AGE_MS
    }

    /**
     * Cheap [PRAGMA quick_check] while idle. Not on start. Writes
     * [QUICK_CHECK_STAMP] so [fileProblem] can report a torn index.
     */
    fun runIdleQuickCheck(dataDir: File): String {
        val dbFile = File(dataDir, "place_index.db")
        if (!dbFile.isFile) {
            writeQuickCheckStamp(dataDir, "file missing")
            return "file missing"
        }
        recoverWalSidecars(dbFile)
        val result =
            runCatching {
                SQLiteDatabase
                    .openDatabase(dbFile.absolutePath, null, SQLiteDatabase.OPEN_READONLY)
                    .use { db ->
                        db.rawQuery("PRAGMA quick_check", null).use { c ->
                            if (c.moveToFirst()) c.getString(0).orEmpty() else "empty"
                        }
                    }
            }.getOrElse { t -> "unreadable: ${t.message}" }
        val line = if (result.equals("ok", ignoreCase = true)) "ok" else result
        writeQuickCheckStamp(dataDir, line)
        Log.i(TAG, "idle quick_check=$line")
        return line
    }

    private fun writeQuickCheckStamp(
        dataDir: File,
        result: String,
    ) {
        File(dataDir, QUICK_CHECK_STAMP).writeText("$result\n${System.currentTimeMillis()}\n")
    }

    private fun probeOpen(
        db: SQLiteDatabase,
        bytes: Long,
        rid: String,
    ): Probe {
        val userVersion =
            db.rawQuery("PRAGMA user_version", null).use { c ->
                if (c.moveToFirst()) c.getInt(0) else 0
            }
        if (userVersion < SCHEMA_VERSION) {
            return Probe(
                intact = false,
                dbBytes = bytes,
                userVersion = userVersion,
                complete = null,
                expected = 0L,
                written = 0L,
                rowCount = 0L,
                legacy = false,
                reason = "schema_old",
            )
        }
        val rowCount =
            db
                .rawQuery(
                    "SELECT COUNT(*) FROM name_entries WHERE region_id = ?",
                    arrayOf(rid),
                ).use { c ->
                    if (c.moveToFirst()) c.getLong(0) else 0L
                }
        val build =
            runCatching {
                db
                    .rawQuery(
                        "SELECT complete, expected, written, source_sha256, index_source FROM name_index_build WHERE region_id = ? LIMIT 1",
                        arrayOf(rid),
                    ).use { c ->
                        if (!c.moveToFirst()) {
                            null
                        } else {
                            Quad(
                                c.getInt(0),
                                c.getLong(1),
                                c.getLong(2),
                                c.getString(3).orEmpty(),
                                c.getString(4).orEmpty(),
                            )
                        }
                    }
            }.getOrNull()
                ?: runCatching {
                    db
                        .rawQuery(
                            "SELECT complete, expected, written, source_sha256 FROM name_index_build WHERE region_id = ? LIMIT 1",
                            arrayOf(rid),
                        ).use { c ->
                            if (!c.moveToFirst()) {
                                null
                            } else {
                                Quad(c.getInt(0), c.getLong(1), c.getLong(2), c.getString(3).orEmpty(), "")
                            }
                        }
                }.getOrNull()
                ?: runCatching {
                    db
                        .rawQuery(
                            "SELECT complete, expected, written FROM name_index_build WHERE region_id = ? LIMIT 1",
                            arrayOf(rid),
                        ).use { c ->
                            if (!c.moveToFirst()) {
                                null
                            } else {
                                Quad(c.getInt(0), c.getLong(1), c.getLong(2), "", "")
                            }
                        }
                }.getOrNull()
        if (build == null) {
            val ok = rowCount > 0L
            return Probe(
                intact = ok,
                dbBytes = bytes,
                userVersion = userVersion,
                complete = null,
                expected = 0L,
                written = 0L,
                rowCount = rowCount,
                legacy = ok,
                reason = if (ok) "legacy_rows" else "no_rows_no_build",
            )
        }
        val complete = build.complete
        val expected = build.expected
        val written = build.written
        val sourceSha = build.sourceSha
        val indexSource = build.indexSource
        if (complete == 0) {
            return Probe(
                intact = false,
                dbBytes = bytes,
                userVersion = userVersion,
                complete = 0,
                expected = expected,
                written = written,
                rowCount = rowCount,
                legacy = false,
                reason = "incomplete",
                sourceSha256 = sourceSha,
                indexSource = indexSource,
            )
        }
        if (rowCount <= 0L) {
            return Probe(
                intact = false,
                dbBytes = bytes,
                userVersion = userVersion,
                complete = complete,
                expected = expected,
                written = written,
                rowCount = rowCount,
                legacy = false,
                reason = "complete_empty",
                sourceSha256 = sourceSha,
                indexSource = indexSource,
            )
        }
        val ref = if (written > 0L) written else expected
        val closeEnough = ref <= 0L || rowCount.toDouble() >= ref.toDouble() * MIN_COUNT_RATIO
        return Probe(
            intact = closeEnough,
            dbBytes = bytes,
            userVersion = userVersion,
            complete = complete,
            expected = expected,
            written = written,
            rowCount = rowCount,
            legacy = false,
            reason = if (closeEnough) "ok" else "row_count_far_from_written",
            sourceSha256 = sourceSha,
            indexSource = indexSource,
        )
    }

    private data class Quad(
        val complete: Int,
        val expected: Long,
        val written: Long,
        val sourceSha: String,
        val indexSource: String,
    )
}
