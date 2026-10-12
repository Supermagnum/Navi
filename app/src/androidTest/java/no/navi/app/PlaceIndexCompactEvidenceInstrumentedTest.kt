package no.navi.app

import android.database.sqlite.SQLiteDatabase
import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/**
 * Emulator evidence for PR #161 / #155: before/after freelist reclaim, bak
 * row-count policy, and the simulated low-space defer log line.
 *
 * Writes a durable report under `/data/local/tmp/place_index_compact_evidence/`
 * and mirrors key lines to logcat tag [TAG].
 */
@RunWith(AndroidJUnit4::class)
class PlaceIndexCompactEvidenceInstrumentedTest {
    private companion object {
        const val TAG = "PlaceIndexCompactEvid"
        const val REGION_A = "europe/norway/ostlandet"
        const val REGION_B = "europe/norway/vestlandet"
        const val KEEP_ROWS = 400
        const val BLOAT_EXTRA = 2_400
    }

    private data class Stats(
        val bytes: Long,
        val pageCount: Long,
        val freelistCount: Long,
    ) {
        val freelistRatio: Double
            get() = if (pageCount <= 0L) 0.0 else freelistCount.toDouble() / pageCount.toDouble()

        fun line(prefix: String): String =
            "$prefix bytes=$bytes page_count=$pageCount freelist_count=$freelistCount " +
                "freelist_ratio=${"%.4f".format(freelistRatio)}"
    }

    @Test
    fun emulator_compact_before_after_bak_and_defer() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val work =
            File(context.cacheDir, "place_index_compact_evidence").also {
                it.deleteRecursively()
                it.mkdirs()
            }
        val outDir =
            File(context.getExternalFilesDir(null), "pr161-place-index-compact").also {
                it.mkdirs()
            }
        val lines = mutableListOf<String>()
        fun emit(msg: String) {
            lines += msg
            Log.i(TAG, msg)
        }

        emit("serial=${android.os.Build.SERIAL} model=${android.os.Build.MODEL}")
        emit("work_dir=${work.absolutePath}")
        emit("out_dir=${outDir.absolutePath}")

        // ---- 1) Plant bloated migrated-style live + matching bak --------------
        val live = File(work, PlaceIndexStorage.DB_NAME)
        plantBloatedLive(live, regionARows = KEEP_ROWS, regionBRows = KEEP_ROWS / 2, extraThenDelete = BLOAT_EXTRA)
        val bak = File(work, PlaceIndexCompact.BAK_NAME)
        // Compact bak (pre-migrate copy): same row counts, no freelist bloat.
        plantBak(bak, regionARows = KEEP_ROWS, regionBRows = KEEP_ROWS / 2)

        val before = stats(live)
        emit(before.line("BEFORE"))
        emit("bak_bytes=${bak.length()} live_bak_ratio=${"%.3f".format(live.length().toDouble() / bak.length().toDouble())}")
        assertTrue(
            "fixture freelist must be above MIN_FREELIST_RATIO",
            before.freelistRatio >= PlaceIndexCompact.MIN_FREELIST_RATIO,
        )

        val liveCountsBefore = regionCounts(live)!!
        val bakCounts = regionCounts(bak)!!
        emit("live_counts=$liveCountsBefore")
        emit("bak_counts=$bakCounts")
        val decisionPreview = PlaceIndexCompact.bakDecision(liveCountsBefore, bakCounts)
        emit("bak_decision_preview delete=${decisionPreview.delete} reason=${decisionPreview.reason}")

        // ---- 2) Compact with real SQLite VACUUM INTO + local lock stub --------
        val env =
            object : PlaceIndexCompact.Env {
                override val engine = PlaceIndexCompact.AndroidEngine

                override fun freeBytes(dir: File): Long = Long.MAX_VALUE / 4

                override fun abortReason(): PlaceIndexCompact.AbortReason? = null

                override fun withBuildLock(block: () -> Unit): Boolean {
                    block()
                    return true
                }
            }
        val outcome = PlaceIndexCompact.run(work, env)
        emit("compact_outcome=$outcome")
        assertTrue("expected Done, got $outcome", outcome is PlaceIndexCompact.Outcome.Done)
        val done = outcome as PlaceIndexCompact.Outcome.Done
        assertTrue("should have compacted", done.compacted)
        assertTrue("matching counts should delete bak", done.bakDeleted)
        assertFalse("bak file must be gone", bak.isFile)

        val after = stats(live)
        emit(after.line("AFTER"))
        emit(
            "delta_bytes=${before.bytes - after.bytes} " +
                "delta_pages=${before.pageCount - after.pageCount} " +
                "delta_freelist=${before.freelistCount - after.freelistCount}",
        )
        assertTrue("AFTER bytes should shrink", after.bytes < before.bytes)
        assertTrue("AFTER freelist should drop", after.freelistCount < before.freelistCount)
        emit("bak_deleted=true reason=counts_match live_vs_bak_rows_equal=true")

        // ---- 3) SIMULATED low free space -> deferred log path -----------------
        val deferDir =
            File(context.cacheDir, "place_index_compact_defer").also {
                it.deleteRecursively()
                it.mkdirs()
            }
        val deferLive = File(deferDir, PlaceIndexStorage.DB_NAME)
        plantBloatedLive(deferLive, regionARows = 80, regionBRows = 40, extraThenDelete = 600)
        File(deferDir, PlaceIndexCompact.BAK_NAME).writeBytes(ByteArray(1_024) { 2 })
        val deferBefore = stats(deferLive)
        emit(deferBefore.line("DEFER_FIXTURE"))

        val lowSpaceEnv =
            object : PlaceIndexCompact.Env {
                override val engine = PlaceIndexCompact.AndroidEngine

                override fun freeBytes(dir: File): Long = 10L

                override fun abortReason(): PlaceIndexCompact.AbortReason? = null

                override fun withBuildLock(block: () -> Unit): Boolean {
                    block()
                    return true
                }
            }
        val deferred = PlaceIndexCompact.run(deferDir, lowSpaceEnv)
        emit("defer_outcome=$deferred")
        assertTrue(deferred is PlaceIndexCompact.Outcome.Deferred)
        val reason = (deferred as PlaceIndexCompact.Outcome.Deferred).reason
        assertTrue(reason.startsWith("insufficient_space"))
        // Mirror the production Log.w line shape for logcat grepping.
        val deferLog = "deferred: $reason"
        Log.w(PlaceIndexCompact.TAG, deferLog)
        emit("DEFER_LOG_LINE[${PlaceIndexCompact.TAG}] $deferLog")
        assertEquals(PlaceIndexCompact.State.DEFERRED, PlaceIndexCompact.readStamp(deferDir)?.state)
        assertTrue("low-space path must not touch live", deferLive.length() == deferBefore.bytes)

        // ---- Durable report on app external files (host adb pull) ------------
        val report = File(outDir, "results.txt")
        report.writeText(lines.joinToString("\n") + "\n")
        emit("report=${report.absolutePath}")
        File(outDir, "DONE").writeText("ok\n")
    }

    private fun plantBloatedLive(
        file: File,
        regionARows: Int,
        regionBRows: Int,
        extraThenDelete: Int,
    ) {
        file.delete()
        // auto_vacuum=NONE so DELETE leaves freelist pages (migrated-index shape).
        SQLiteDatabase.openOrCreateDatabase(file.absolutePath, null).use { db ->
            db.execSQL("PRAGMA auto_vacuum = NONE")
            db.execSQL("VACUUM")
            db.execSQL(
                """
                CREATE TABLE name_entries (
                    region_id TEXT NOT NULL DEFAULT '',
                    osm_id INTEGER NOT NULL,
                    name TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    lat REAL NOT NULL,
                    lon REAL NOT NULL,
                    sub_area TEXT NOT NULL DEFAULT '',
                    municipality TEXT NOT NULL DEFAULT '',
                    search_doc TEXT NOT NULL DEFAULT '',
                    PRIMARY KEY (region_id, osm_id)
                )
                """.trimIndent(),
            )
            db.execSQL("PRAGMA user_version = 6")
            db.beginTransaction()
            try {
                // Fat search_doc so each deleted row frees whole pages.
                val pad = "x".repeat(2_048)
                fun insert(
                    region: String,
                    id: Long,
                    name: String,
                ) {
                    db.execSQL(
                        "INSERT INTO name_entries(region_id, osm_id, name, kind, lat, lon, search_doc) VALUES (?,?,?,?,?,?,?)",
                        arrayOf<Any>(region, id, name, "place", 60.0, 10.0, name + pad),
                    )
                }
                for (i in 1..regionARows) insert(REGION_A, i.toLong(), "A$i")
                for (i in 1..regionBRows) insert(REGION_B, i.toLong(), "B$i")
                for (i in 1..extraThenDelete) {
                    insert(REGION_A, (100_000L + i), "X$i")
                }
                db.setTransactionSuccessful()
            } finally {
                db.endTransaction()
            }
        }
        // Delete on a fresh connection so pages stay on the freelist.
        SQLiteDatabase.openDatabase(file.absolutePath, null, SQLiteDatabase.OPEN_READWRITE).use { db ->
            db.execSQL("DELETE FROM name_entries WHERE osm_id >= 100000")
        }
    }

    private fun plantBak(
        file: File,
        regionARows: Int,
        regionBRows: Int,
    ) {
        file.delete()
        SQLiteDatabase.openOrCreateDatabase(file.absolutePath, null).use { db ->
            db.execSQL(
                """
                CREATE TABLE name_entries (
                    region_id TEXT NOT NULL DEFAULT '',
                    osm_id INTEGER NOT NULL,
                    name TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    lat REAL NOT NULL,
                    lon REAL NOT NULL,
                    sub_area TEXT NOT NULL DEFAULT '',
                    municipality TEXT NOT NULL DEFAULT '',
                    search_doc TEXT NOT NULL DEFAULT '',
                    PRIMARY KEY (region_id, osm_id)
                )
                """.trimIndent(),
            )
            db.beginTransaction()
            try {
                for (i in 1..regionARows) {
                    db.execSQL(
                        "INSERT INTO name_entries(region_id, osm_id, name, kind, lat, lon, search_doc) VALUES (?,?,?,?,?,?,?)",
                        arrayOf<Any>(REGION_A, i.toLong(), "A$i", "place", 60.0, 10.0, "A$i"),
                    )
                }
                for (i in 1..regionBRows) {
                    db.execSQL(
                        "INSERT INTO name_entries(region_id, osm_id, name, kind, lat, lon, search_doc) VALUES (?,?,?,?,?,?,?)",
                        arrayOf<Any>(REGION_B, i.toLong(), "B$i", "place", 60.0, 10.0, "B$i"),
                    )
                }
                db.setTransactionSuccessful()
            } finally {
                db.endTransaction()
            }
        }
    }

    private fun stats(file: File): Stats =
        SQLiteDatabase.openDatabase(file.absolutePath, null, SQLiteDatabase.OPEN_READONLY).use { db ->
            fun pragma(name: String): Long =
                db.rawQuery("PRAGMA $name", null).use { c ->
                    if (c.moveToFirst()) c.getLong(0) else 0L
                }
            Stats(bytes = file.length(), pageCount = pragma("page_count"), freelistCount = pragma("freelist_count"))
        }

    private fun regionCounts(file: File): Map<String, Long>? =
        SQLiteDatabase.openDatabase(file.absolutePath, null, SQLiteDatabase.OPEN_READONLY).use { db ->
            db.rawQuery("SELECT region_id, COUNT(*) FROM name_entries GROUP BY region_id", null).use { c ->
                buildMap {
                    while (c.moveToNext()) put(c.getString(0).orEmpty(), c.getLong(1))
                }
            }
        }
}
