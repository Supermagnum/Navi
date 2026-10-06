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
    )

    fun isIntact(
        dataDir: File,
        regionId: String,
    ): Boolean = probe(dataDir, regionId).intact

    fun probe(
        dataDir: File,
        regionId: String,
    ): Probe {
        val rid = PackRegionAvailability.normalize(regionId)
        val empty =
            Probe(
                intact = false,
                dbBytes = 0L,
                userVersion = 0,
                complete = null,
                expected = 0L,
                written = 0L,
                rowCount = 0L,
                legacy = false,
                reason = "empty_region_id",
            )
        if (rid.isEmpty()) return empty
        val dbFile = File(dataDir, "place_index.db")
        if (!dbFile.isFile) {
            return empty.copy(reason = "db_missing")
        }
        val bytes = dbFile.length()
        if (bytes < MIN_DB_BYTES) {
            return empty.copy(dbBytes = bytes, reason = "db_tiny")
        }
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
                        "SELECT complete, expected, written FROM name_index_build WHERE region_id = ? LIMIT 1",
                        arrayOf(rid),
                    ).use { c ->
                        if (!c.moveToFirst()) {
                            null
                        } else {
                            Triple(c.getInt(0), c.getLong(1), c.getLong(2))
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
        val (complete, expected, written) = build
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
        )
    }
}
