package no.navi.app

import android.util.Log
import java.io.File
import java.nio.file.AtomicMoveNotSupportedException
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference

/**
 * Idle compaction of `place_index.db` after the v5 to v6 primary-key migrate.
 *
 * The migrate copies every row into a new table and drops the old one, which
 * leaves a large SQLite freelist, and it keeps a one-shot pre-migrate copy
 * (`place_index.db.bak-pk-v6`) that nothing ever removed. On a tablet that
 * doubled the index volume use (issue 155).
 *
 * Contract:
 * - Work happens only on a temp file beside the live DB; the live DB is never
 *   written until one atomic rename, taken under the place-index build lock.
 * - The temp file is verified (`quick_check`, per-region row counts equal to
 *   live) before the swap, and live must be unchanged since the snapshot.
 * - The backup is deleted only when every region present in both the backup
 *   and the compacted index has the same row count; otherwise it is kept and
 *   the mismatches are logged.
 * - A plan or an index build aborts the job; the stamp stays so the next idle
 *   pass resumes. A temp file left by a killed process is discarded on the
 *   next run (VACUUM INTO has no partial state worth resuming).
 *
 * Needs `VACUUM INTO` (SQLite 3.27). When an [Engine] reports it unsupported,
 * the live DB is left as is and only the backup policy runs.
 */
object PlaceIndexCompact {
    const val TAG = "PlaceIndexCompact"
    const val BAK_NAME = "place_index.db.bak-pk-v6"
    const val TMP_NAME = "place_index.db.compact-tmp"
    const val STAMP_NAME = "place_index_compact.txt"

    /** Live is "much larger" than the backup when above this multiple. */
    const val LIVE_BAK_RATIO = 1.5

    /** Below this freelist share there is nothing worth rewriting. */
    const val MIN_FREELIST_RATIO = 0.10

    /** Free space kept on top of the live size. */
    const val MARGIN_BYTES = 64L * 1024L * 1024L
    const val MARGIN_FRACTION = 0.10

    /** A deferred (no space) job is not retried more often than this. */
    const val DEFER_RETRY_MS = 60L * 60L * 1000L
    private const val ABORT_POLL_MS = 150L

    enum class State { NEEDS_COMPACT, DEFERRED, DONE }

    data class Stamp(
        val state: State,
        val atMs: Long,
        val liveLen: Long = 0L,
        val liveMtime: Long = 0L,
        val note: String = "",
    ) {
        fun serialize(): String =
            "state=${state.name}\nat_ms=$atMs\nlive_len=$liveLen\nlive_mtime=$liveMtime\n" +
                "note=${note.replace('\n', ' ')}\n"

        companion object {
            fun parse(text: String): Stamp? {
                val m =
                    text
                        .lineSequence()
                        .mapNotNull { l ->
                            val i = l.indexOf('=')
                            if (i <= 0) null else l.substring(0, i) to l.substring(i + 1)
                        }.toMap()
                val state = runCatching { State.valueOf(m["state"].orEmpty()) }.getOrNull() ?: return null
                return Stamp(
                    state = state,
                    atMs = m["at_ms"]?.toLongOrNull() ?: 0L,
                    liveLen = m["live_len"]?.toLongOrNull() ?: 0L,
                    liveMtime = m["live_mtime"]?.toLongOrNull() ?: 0L,
                    note = m["note"].orEmpty(),
                )
            }
        }
    }

    enum class AbortReason { PLAN, INDEX_BUILD }

    sealed class Outcome {
        /** Nothing to do (no DB, or stamp cleared). */
        data class Skipped(
            val reason: String,
        ) : Outcome()

        data class Deferred(
            val reason: String,
        ) : Outcome()

        data class Aborted(
            val reason: AbortReason,
        ) : Outcome()

        data class Failed(
            val reason: String,
        ) : Outcome()

        data class Done(
            val compacted: Boolean,
            val bakDeleted: Boolean,
            val bakMismatches: List<String>,
        ) : Outcome()
    }

    sealed class VacuumResult {
        object Ok : VacuumResult()

        object Unsupported : VacuumResult()

        data class Failed(
            val message: String,
        ) : VacuumResult()
    }

    /** SQLite access. Android implementation below; fakes in JVM tests. */
    interface Engine {
        fun quickCheck(file: File): String

        /** region_id to row count in `name_entries`, or null when unreadable. */
        fun regionCounts(file: File): Map<String, Long>?

        /** freelist_count / page_count, or null when unreadable. */
        fun freelistRatio(file: File): Double?

        /** Blocking `VACUUM INTO tmp` from a read-only view of [live]. */
        fun vacuumInto(
            live: File,
            tmp: File,
        ): VacuumResult

        /** `wal_checkpoint(TRUNCATE)` so the main file carries all content. */
        fun checkpoint(live: File): Boolean
    }

    interface Env {
        val engine: Engine

        /** Bytes available on the volume that holds [dir]. */
        fun freeBytes(dir: File): Long

        /** Non-null when the job must stop (plan active, index build started). */
        fun abortReason(): AbortReason?

        /** Run [block] holding the place-index build lock; false (block not run) if unavailable. */
        fun withBuildLock(block: () -> Unit): Boolean

        fun nowMs(): Long = System.currentTimeMillis()
    }

    private val workerActive = AtomicBoolean(false)

    fun bakFile(dir: File) = File(dir, BAK_NAME)

    fun tmpFile(dir: File) = File(dir, TMP_NAME)

    fun stampFile(dir: File) = File(dir, STAMP_NAME)

    fun readStamp(dir: File): Stamp? =
        runCatching { stampFile(dir).takeIf { it.isFile }?.readText() }
            .getOrNull()
            ?.let { Stamp.parse(it) }

    private fun writeStamp(
        dir: File,
        stamp: Stamp,
    ) {
        runCatching { stampFile(dir).writeText(stamp.serialize()) }
            .onFailure { Log.w(TAG, "stamp write failed: ${it.message}") }
    }

    // ---- pure policy -------------------------------------------------

    /** Free space needed to write the compacted copy: live (+WAL) + margin. */
    fun requiredFreeBytes(
        liveBytes: Long,
        walBytes: Long = 0L,
    ): Long {
        val margin = maxOf(MARGIN_BYTES, (liveBytes * MARGIN_FRACTION).toLong())
        return liveBytes + walBytes + margin
    }

    fun hasFreeSpace(
        freeBytes: Long,
        liveBytes: Long,
        walBytes: Long = 0L,
    ): Boolean = freeBytes >= requiredFreeBytes(liveBytes, walBytes)

    data class BakDecision(
        val delete: Boolean,
        val mismatches: List<String>,
        val reason: String,
    )

    /**
     * Delete the backup only if every region present in both has the same row
     * count. Regions only in the backup (removed by the user) do not block, but
     * a backup with rows and no overlap at all is kept: nothing proves it.
     */
    fun bakDecision(
        live: Map<String, Long>,
        bak: Map<String, Long>?,
    ): BakDecision {
        if (bak == null) return BakDecision(false, emptyList(), "bak_unreadable")
        val overlap = bak.keys.filter { it in live }
        val mismatches =
            overlap
                .filter { bak[it] != live[it] }
                .map { "$it live=${live[it]} bak=${bak[it]}" }
        if (mismatches.isNotEmpty()) return BakDecision(false, mismatches, "row_count_mismatch")
        if (overlap.isEmpty() && bak.values.any { it > 0L }) {
            return BakDecision(false, emptyList(), "no_overlapping_region")
        }
        return BakDecision(true, emptyList(), "counts_match")
    }

    /**
     * Whether the idle queue should offer a compact job. Stamps
     * [State.NEEDS_COMPACT] when the backup exists and live is much larger.
     */
    fun shouldOffer(
        dir: File,
        nowMs: Long = System.currentTimeMillis(),
    ): Boolean {
        val live = File(dir, PlaceIndexStorage.DB_NAME)
        if (!live.isFile) return false
        val stamp = readStamp(dir)
        if (stamp != null) {
            when (stamp.state) {
                State.NEEDS_COMPACT -> return true
                State.DEFERRED -> return nowMs - stamp.atMs >= DEFER_RETRY_MS
                State.DONE ->
                    if (stamp.liveLen == live.length() && stamp.liveMtime == live.lastModified()) return false
            }
        }
        val bak = bakFile(dir)
        if (!bak.isFile) return false
        if (live.length() <= (bak.length() * LIVE_BAK_RATIO).toLong()) return false
        writeStamp(
            dir,
            Stamp(State.NEEDS_COMPACT, nowMs, live.length(), live.lastModified(), "bak_present_live_larger"),
        )
        Log.i(TAG, "needs_compact live=${live.length()} bak=${bak.length()}")
        return true
    }

    // ---- job ---------------------------------------------------------

    /**
     * One compaction pass. A deferral or failure is stamped [State.DEFERRED]
     * so the idle queue retries after [DEFER_RETRY_MS], not on every tick; a
     * plan or index-build abort keeps the stamp and resumes on the next pass.
     */
    fun run(
        dir: File,
        env: Env,
    ): Outcome {
        val outcome = runPass(dir, env)
        val note =
            when (outcome) {
                is Outcome.Failed -> outcome.reason
                is Outcome.Deferred -> outcome.reason.takeUnless { it.startsWith("previous_vacuum") }
                else -> null
            }
        if (note != null) {
            val live = File(dir, PlaceIndexStorage.DB_NAME)
            writeStamp(dir, Stamp(State.DEFERRED, env.nowMs(), live.length(), live.lastModified(), note))
        }
        return outcome
    }

    private fun runPass(
        dir: File,
        env: Env,
    ): Outcome {
        val live = File(dir, PlaceIndexStorage.DB_NAME)
        val tmp = tmpFile(dir)
        if (!live.isFile) {
            discardTemp(tmp)
            return Outcome.Skipped("db_missing")
        }
        if (workerActive.get()) return Outcome.Deferred("previous_vacuum_still_running")
        // A temp left by a killed process is incomplete by definition.
        if (tmp.exists()) {
            Log.i(TAG, "discarding leftover temp ${tmp.name} (${tmp.length()} bytes)")
            discardTemp(tmp)
        }
        env.abortReason()?.let { return Outcome.Aborted(it) }

        val engine = env.engine
        val ratio = engine.freelistRatio(live) ?: return fail(dir, "freelist_unreadable")
        var compacted = false
        if (ratio >= MIN_FREELIST_RATIO) {
            val wal = File(live.path + "-wal")
            val liveBytes = live.length()
            val walBytes = if (wal.isFile) wal.length() else 0L
            val free = env.freeBytes(dir)
            if (!hasFreeSpace(free, liveBytes, walBytes)) {
                val reason =
                    "insufficient_space free=$free need=${requiredFreeBytes(liveBytes, walBytes)} live=$liveBytes"
                Log.w(TAG, "deferred: $reason")
                return Outcome.Deferred(reason)
            }
            when (val v = vacuumWithAbort(env, live, tmp)) {
                is Vacuumed.Aborted -> return Outcome.Aborted(v.reason)
                is Vacuumed.Busy -> return Outcome.Deferred("previous_vacuum_still_running")
                is Vacuumed.Result ->
                    when (val r = v.result) {
                        VacuumResult.Ok -> {
                            val swapped = verifyAndSwap(dir, env, live, tmp)
                            if (swapped != null) return swapped
                            compacted = true
                        }
                        VacuumResult.Unsupported -> {
                            Log.w(TAG, "VACUUM INTO unsupported on this SQLite; live left as is")
                            discardTemp(tmp)
                        }
                        is VacuumResult.Failed -> {
                            discardTemp(tmp)
                            return fail(dir, "vacuum_failed: ${r.message}")
                        }
                    }
            }
        } else {
            Log.i(TAG, "freelist ratio $ratio below $MIN_FREELIST_RATIO; skip rewrite")
        }

        // Backup policy runs against the (possibly just swapped) live DB.
        var bakDeleted = false
        var mismatches = emptyList<String>()
        val bak = bakFile(dir)
        if (bak.isFile) {
            val liveCounts = engine.regionCounts(live) ?: return fail(dir, "live_counts_unreadable")
            val decision = bakDecision(liveCounts, engine.regionCounts(bak))
            mismatches = decision.mismatches
            if (decision.delete) {
                bakDeleted = bak.delete()
                Log.i(TAG, "bak deleted=$bakDeleted (${decision.reason})")
            } else {
                Log.w(TAG, "bak kept (${decision.reason}) mismatches=${decision.mismatches}")
            }
        }
        writeStamp(
            dir,
            Stamp(State.DONE, env.nowMs(), live.length(), live.lastModified(), "compacted=$compacted bak_deleted=$bakDeleted"),
        )
        return Outcome.Done(compacted, bakDeleted, mismatches)
    }

    private fun fail(
        dir: File,
        reason: String,
    ): Outcome {
        Log.w(TAG, "compact failed: $reason")
        return Outcome.Failed(reason)
    }

    /** Null on success (live replaced); an outcome otherwise (live untouched). */
    private fun verifyAndSwap(
        dir: File,
        env: Env,
        live: File,
        tmp: File,
    ): Outcome? {
        val engine = env.engine
        if (!tmp.isFile || tmp.length() == 0L) {
            discardTemp(tmp)
            return fail(dir, "temp_empty")
        }
        val qc = engine.quickCheck(tmp)
        if (!qc.equals("ok", ignoreCase = true)) {
            discardTemp(tmp)
            return fail(dir, "temp_quick_check=$qc")
        }
        val tmpCounts = engine.regionCounts(tmp)
        if (tmpCounts == null) {
            discardTemp(tmp)
            return fail(dir, "temp_counts_unreadable")
        }
        env.abortReason()?.let {
            discardTemp(tmp)
            return Outcome.Aborted(it)
        }
        var failure: Outcome? = null
        val locked =
            env.withBuildLock {
                // Re-check under the lock: a builder may have written since
                // the snapshot, in which case the temp copy is stale.
                val abort = env.abortReason()
                failure =
                    when {
                        abort != null -> Outcome.Aborted(abort)
                        !engine.checkpoint(live) -> Outcome.Failed("checkpoint_failed")
                        engine.regionCounts(live) != tmpCounts -> Outcome.Failed("live_changed_since_snapshot")
                        !atomicReplace(tmp, live) -> Outcome.Failed("rename_failed")
                        else -> null
                    }
                if (failure == null) {
                    // Old WAL/SHM must never be replayed onto the new file.
                    File(live.path + "-wal").delete()
                    File(live.path + "-shm").delete()
                }
            }
        if (!locked) {
            discardTemp(tmp)
            return Outcome.Deferred("build_lock_unavailable")
        }
        failure?.let {
            discardTemp(tmp)
            Log.w(TAG, "swap not done: $it")
            return it
        }
        Log.i(TAG, "swapped compacted index live=${live.length()}")
        return null
    }

    private fun atomicReplace(
        from: File,
        to: File,
    ): Boolean =
        runCatching {
            try {
                Files.move(from.toPath(), to.toPath(), StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING)
            } catch (_: AtomicMoveNotSupportedException) {
                Files.move(from.toPath(), to.toPath(), StandardCopyOption.REPLACE_EXISTING)
            }
            true
        }.getOrDefault(false)

    private fun discardTemp(tmp: File) {
        runCatching { tmp.delete() }
        for (s in listOf("-journal", "-wal", "-shm")) runCatching { File(tmp.path + s).delete() }
    }

    private sealed class Vacuumed {
        data class Result(
            val result: VacuumResult,
        ) : Vacuumed()

        data class Aborted(
            val reason: AbortReason,
        ) : Vacuumed()

        object Busy : Vacuumed()
    }

    /**
     * `VACUUM INTO` is one uninterruptible statement. Run it on a worker and
     * poll [Env.abortReason]; on abort the worker is abandoned (it only reads
     * live and writes the temp, and removes the temp when it ends).
     */
    private fun vacuumWithAbort(
        env: Env,
        live: File,
        tmp: File,
    ): Vacuumed {
        if (!workerActive.compareAndSet(false, true)) return Vacuumed.Busy
        val abandoned = AtomicBoolean(false)
        val result = AtomicReference<VacuumResult>(VacuumResult.Failed("no_result"))
        val done = CountDownLatch(1)
        val t =
            Thread({
                try {
                    result.set(env.engine.vacuumInto(live, tmp))
                } catch (e: Throwable) {
                    result.set(VacuumResult.Failed(e.message ?: e.javaClass.simpleName))
                } finally {
                    if (abandoned.get() || result.get() !is VacuumResult.Ok) discardTemp(tmp)
                    workerActive.set(false)
                    done.countDown()
                }
            }, "place-index-compact")
        t.isDaemon = true
        t.priority = Thread.MIN_PRIORITY
        t.start()
        while (!done.await(ABORT_POLL_MS, TimeUnit.MILLISECONDS)) {
            val why = env.abortReason() ?: continue
            abandoned.set(true)
            if (done.count == 0L) discardTemp(tmp)
            Log.i(TAG, "aborted during VACUUM INTO: $why")
            return Vacuumed.Aborted(why)
        }
        return Vacuumed.Result(result.get())
    }

    /** True while an abandoned or running VACUUM INTO worker is alive. */
    fun workerActiveForTests(): Boolean = workerActive.get()

    // ---- Android -----------------------------------------------------

    /** Production [Env]: framework SQLite, StatFs, Rust build lock. */
    fun androidEnv(abort: () -> AbortReason?): Env =
        object : Env {
            override val engine: Engine = AndroidEngine

            override fun freeBytes(dir: File): Long = NaviStorageVolumes.freeBytes(dir)

            override fun abortReason(): AbortReason? = abort()

            override fun withBuildLock(block: () -> Unit): Boolean {
                val acquired = runCatching { uniffi.navi.placeIndexBuildLockAcquire() }.getOrNull()
                if (acquired?.startsWith("PASS") != true) {
                    Log.w(TAG, "build lock unavailable: $acquired")
                    return false
                }
                try {
                    block()
                    return true
                } finally {
                    runCatching { uniffi.navi.placeIndexBuildLockRelease() }
                }
            }
        }

    /** Native SQLite only; see [PlaceIndexIntact.readRows] for why not the framework. */
    internal object AndroidEngine : Engine {
        private fun readOnly(
            file: File,
            sql: String,
        ): List<List<String?>>? =
            runCatching { PlaceIndexIntact.readRows(file, sql) }
                .onFailure { Log.w(TAG, "read ${file.name} failed: ${it.message}") }
                .getOrNull()

        override fun quickCheck(file: File): String =
            readOnly(file, "PRAGMA quick_check")?.let { rows ->
                rows
                    .firstOrNull()
                    ?.firstOrNull()
                    .orEmpty()
                    .ifEmpty { "empty" }
            } ?: "unreadable"

        override fun regionCounts(file: File): Map<String, Long>? =
            readOnly(file, "SELECT region_id, COUNT(*) FROM name_entries GROUP BY region_id")?.associate { r ->
                r.getOrNull(0).orEmpty() to (r.getOrNull(1)?.toLongOrNull() ?: 0L)
            }

        override fun freelistRatio(file: File): Double? {
            fun pragma(name: String): Long? = readOnly(file, "PRAGMA $name")?.let { it.firstOrNull()?.firstOrNull()?.toLongOrNull() ?: 0L }
            val pages = pragma("page_count") ?: return null
            val free = pragma("freelist_count") ?: return null
            return if (pages <= 0L) 0.0 else free.toDouble() / pages.toDouble()
        }

        override fun vacuumInto(
            live: File,
            tmp: File,
        ): VacuumResult {
            val report =
                runCatching { uniffi.navi.placeIndexVacuumInto(live.absolutePath, tmp.absolutePath) }
                    .getOrElse { t -> "FAIL: ${t.message}" }
            return if (report.startsWith("PASS")) VacuumResult.Ok else VacuumResult.Failed(report)
        }

        override fun checkpoint(live: File): Boolean = runCatching { uniffi.navi.placeIndexCheckpointTruncate(live.absolutePath) }.getOrNull() == "PASS"
    }
}
