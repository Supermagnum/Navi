package no.navi.app

import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference

class PlaceIndexCompactTest {
    @get:Rule
    val tmp = TemporaryFolder()

    private val workers = mutableListOf<CountDownLatch>()

    @After
    fun tearDown() {
        workers.forEach { it.countDown() }
        val deadline = System.currentTimeMillis() + 3_000
        while (PlaceIndexCompact.workerActiveForTests() && System.currentTimeMillis() < deadline) {
            Thread.sleep(10)
        }
    }

    /** Files stand in for databases; the "content" is a region to count map. */
    private class FakeEngine(
        val counts: MutableMap<String, Map<String, Long>?> = mutableMapOf(),
        var freelist: Double? = 0.5,
        var quick: String = "ok",
        var vacuum: PlaceIndexCompact.VacuumResult = PlaceIndexCompact.VacuumResult.Ok,
        var vacuumGate: CountDownLatch? = null,
        var vacuumStarted: CountDownLatch = CountDownLatch(1),
        var checkpointOk: Boolean = true,
        var onCheckpoint: () -> Unit = {},
    ) : PlaceIndexCompact.Engine {
        var vacuumCalls = 0

        override fun quickCheck(file: File) = quick

        override fun regionCounts(file: File) = counts[file.name]

        override fun freelistRatio(file: File) = freelist

        override fun vacuumInto(
            live: File,
            tmp: File,
        ): PlaceIndexCompact.VacuumResult {
            vacuumCalls++
            vacuumStarted.countDown()
            vacuumGate?.await(10, TimeUnit.SECONDS)
            if (vacuum == PlaceIndexCompact.VacuumResult.Ok) {
                tmp.writeBytes(ByteArray(40) { 7 })
                counts[tmp.name] = counts[live.name]
            }
            return vacuum
        }

        override fun checkpoint(live: File): Boolean {
            onCheckpoint()
            return checkpointOk
        }
    }

    private class FakeEnv(
        override val engine: FakeEngine,
        var free: Long = Long.MAX_VALUE,
        var abort: () -> PlaceIndexCompact.AbortReason? = { null },
        var lockAvailable: Boolean = true,
    ) : PlaceIndexCompact.Env {
        var lockTaken = 0

        override fun freeBytes(dir: File) = free

        override fun abortReason() = abort()

        override fun withBuildLock(block: () -> Unit): Boolean {
            if (!lockAvailable) return false
            lockTaken++
            block()
            return true
        }
    }

    private val regionA = "europe/norway/ostlandet"
    private val regionB = "europe/norway/vestlandet"

    private fun setup(
        liveBytes: Int = 1000,
        withBak: Boolean = true,
        liveCounts: Map<String, Long> = mapOf(regionA to 10L, regionB to 5L),
        bakCounts: Map<String, Long>? = mapOf(regionA to 10L, regionB to 5L),
    ): Pair<File, FakeEngine> {
        val dir = tmp.newFolder()
        File(dir, PlaceIndexStorage.DB_NAME).writeBytes(ByteArray(liveBytes) { 1 })
        if (withBak) File(dir, PlaceIndexCompact.BAK_NAME).writeBytes(ByteArray(100) { 2 })
        val eng = FakeEngine()
        eng.counts[PlaceIndexStorage.DB_NAME] = liveCounts
        eng.counts[PlaceIndexCompact.BAK_NAME] = bakCounts
        return dir to eng
    }

    private fun live(dir: File) = File(dir, PlaceIndexStorage.DB_NAME)

    // ---- free-space gate ---------------------------------------------

    @Test
    fun free_space_gate_needs_live_size_plus_margin() {
        val live = 1_000L * 1024 * 1024
        val need = PlaceIndexCompact.requiredFreeBytes(live)
        assertTrue(need >= live + PlaceIndexCompact.MARGIN_BYTES)
        assertTrue(need >= live + live / 10)
        assertTrue(PlaceIndexCompact.hasFreeSpace(need, live))
        assertFalse(PlaceIndexCompact.hasFreeSpace(need - 1, live))
        assertFalse(PlaceIndexCompact.hasFreeSpace(live, live))
        assertTrue("wal counts toward the need", PlaceIndexCompact.requiredFreeBytes(live, 5L) == need + 5L)
    }

    @Test
    fun margin_has_floor_for_small_index() {
        assertEquals(
            1_000L + PlaceIndexCompact.MARGIN_BYTES,
            PlaceIndexCompact.requiredFreeBytes(1_000L),
        )
    }

    @Test
    fun low_space_defers_without_touching_live_or_bak() {
        val (dir, eng) = setup()
        val before = live(dir).readBytes()
        val env = FakeEnv(eng, free = 10L)
        val out = PlaceIndexCompact.run(dir, env)
        assertTrue(out is PlaceIndexCompact.Outcome.Deferred)
        assertTrue((out as PlaceIndexCompact.Outcome.Deferred).reason.startsWith("insufficient_space"))
        assertEquals(0, eng.vacuumCalls)
        assertTrue(before.contentEquals(live(dir).readBytes()))
        assertTrue(PlaceIndexCompact.bakFile(dir).isFile)
        assertFalse(PlaceIndexCompact.tmpFile(dir).exists())
        assertEquals(PlaceIndexCompact.State.DEFERRED, PlaceIndexCompact.readStamp(dir)?.state)
    }

    @Test
    fun deferred_stamp_is_not_retried_until_backoff_elapsed() {
        val (dir, eng) = setup()
        PlaceIndexCompact.run(dir, FakeEnv(eng, free = 1L))
        val at = PlaceIndexCompact.readStamp(dir)!!.atMs
        assertFalse(PlaceIndexCompact.shouldOffer(dir, at + 1_000L))
        assertTrue(PlaceIndexCompact.shouldOffer(dir, at + PlaceIndexCompact.DEFER_RETRY_MS))
    }

    // ---- bak row-count policy ----------------------------------------

    @Test
    fun bak_deleted_when_all_overlapping_regions_match() {
        val d = PlaceIndexCompact.bakDecision(mapOf("a" to 3L, "b" to 4L, "c" to 9L), mapOf("a" to 3L, "b" to 4L))
        assertTrue(d.delete)
        assertTrue(d.mismatches.isEmpty())
    }

    @Test
    fun bak_only_region_does_not_block_delete() {
        val d = PlaceIndexCompact.bakDecision(mapOf("a" to 3L), mapOf("a" to 3L, "gone" to 8L))
        assertTrue(d.delete)
    }

    @Test
    fun bak_kept_on_any_mismatch_and_lists_them() {
        val d = PlaceIndexCompact.bakDecision(mapOf("a" to 3L, "b" to 4L), mapOf("a" to 3L, "b" to 5L))
        assertFalse(d.delete)
        assertEquals(1, d.mismatches.size)
        assertTrue(d.mismatches[0].startsWith("b "))
    }

    @Test
    fun bak_kept_when_unreadable_or_no_overlap() {
        assertFalse(PlaceIndexCompact.bakDecision(mapOf("a" to 3L), null).delete)
        assertFalse(PlaceIndexCompact.bakDecision(mapOf("a" to 3L), mapOf("z" to 3L)).delete)
        assertTrue(PlaceIndexCompact.bakDecision(mapOf("a" to 3L), emptyMap()).delete)
    }

    @Test
    fun run_keeps_bak_and_reports_mismatch() {
        val (dir, eng) =
            setup(
                liveCounts = mapOf(regionA to 10L, regionB to 6L),
                bakCounts = mapOf(regionA to 10L, regionB to 5L),
            )
        val out = PlaceIndexCompact.run(dir, FakeEnv(eng))
        out as PlaceIndexCompact.Outcome.Done
        assertTrue(out.compacted)
        assertFalse(out.bakDeleted)
        assertEquals(1, out.bakMismatches.size)
        assertTrue(PlaceIndexCompact.bakFile(dir).isFile)
    }

    // ---- compaction + swap -------------------------------------------

    @Test
    fun happy_path_swaps_and_deletes_bak() {
        val (dir, eng) = setup()
        File(live(dir).path + "-wal").writeBytes(ByteArray(3))
        File(live(dir).path + "-shm").writeBytes(ByteArray(3))
        val env = FakeEnv(eng)
        val out = PlaceIndexCompact.run(dir, env)
        out as PlaceIndexCompact.Outcome.Done
        assertTrue(out.compacted)
        assertTrue(out.bakDeleted)
        assertEquals(40, live(dir).length())
        assertFalse(File(live(dir).path + "-wal").exists())
        assertFalse(File(live(dir).path + "-shm").exists())
        assertFalse(PlaceIndexCompact.tmpFile(dir).exists())
        assertFalse(PlaceIndexCompact.bakFile(dir).exists())
        assertEquals(1, env.lockTaken)
        val stamp = PlaceIndexCompact.readStamp(dir)!!
        assertEquals(PlaceIndexCompact.State.DONE, stamp.state)
        assertFalse(PlaceIndexCompact.shouldOffer(dir))
    }

    @Test
    fun low_freelist_skips_rewrite_but_still_applies_bak_policy() {
        val (dir, eng) = setup()
        eng.freelist = 0.01
        val out = PlaceIndexCompact.run(dir, FakeEnv(eng)) as PlaceIndexCompact.Outcome.Done
        assertFalse(out.compacted)
        assertTrue(out.bakDeleted)
        assertEquals(0, eng.vacuumCalls)
        assertEquals(1000, live(dir).length())
    }

    @Test
    fun vacuum_unsupported_leaves_live_and_still_applies_bak_policy() {
        val (dir, eng) = setup()
        eng.vacuum = PlaceIndexCompact.VacuumResult.Unsupported
        val out = PlaceIndexCompact.run(dir, FakeEnv(eng)) as PlaceIndexCompact.Outcome.Done
        assertFalse(out.compacted)
        assertTrue(out.bakDeleted)
        assertEquals(1000, live(dir).length())
        assertFalse(PlaceIndexCompact.tmpFile(dir).exists())
    }

    @Test
    fun failed_quick_check_discards_temp_and_keeps_live_and_bak() {
        val (dir, eng) = setup()
        eng.quick = "database disk image is malformed"
        val out = PlaceIndexCompact.run(dir, FakeEnv(eng))
        assertTrue(out is PlaceIndexCompact.Outcome.Failed)
        assertEquals(1000, live(dir).length())
        assertTrue(PlaceIndexCompact.bakFile(dir).isFile)
        assertFalse(PlaceIndexCompact.tmpFile(dir).exists())
        assertEquals(PlaceIndexCompact.State.DEFERRED, PlaceIndexCompact.readStamp(dir)?.state)
    }

    @Test
    fun live_written_since_snapshot_never_swaps() {
        val (dir, eng) = setup()
        // A builder adds rows between the vacuum snapshot and the swap.
        eng.onCheckpoint = { eng.counts[PlaceIndexStorage.DB_NAME] = mapOf(regionA to 11L, regionB to 5L) }
        val out = PlaceIndexCompact.run(dir, FakeEnv(eng))
        out as PlaceIndexCompact.Outcome.Failed
        assertEquals("live_changed_since_snapshot", out.reason)
        assertEquals(1000, live(dir).length())
        assertFalse(PlaceIndexCompact.tmpFile(dir).exists())
        assertTrue(PlaceIndexCompact.bakFile(dir).isFile)
    }

    @Test
    fun build_lock_unavailable_defers_and_leaves_live() {
        val (dir, eng) = setup()
        val out = PlaceIndexCompact.run(dir, FakeEnv(eng, lockAvailable = false))
        assertEquals(PlaceIndexCompact.Outcome.Deferred("build_lock_unavailable"), out)
        assertEquals(1000, live(dir).length())
        assertFalse(PlaceIndexCompact.tmpFile(dir).exists())
    }

    @Test
    fun index_build_after_vacuum_aborts_before_swap() {
        val (dir, eng) = setup()
        val env = FakeEnv(eng, abort = { if (eng.vacuumCalls > 0) PlaceIndexCompact.AbortReason.INDEX_BUILD else null })
        val out = PlaceIndexCompact.run(dir, env)
        assertEquals(PlaceIndexCompact.Outcome.Aborted(PlaceIndexCompact.AbortReason.INDEX_BUILD), out)
        assertEquals(1000, live(dir).length())
        assertFalse(PlaceIndexCompact.tmpFile(dir).exists())
        assertEquals(0, env.lockTaken)
        assertTrue(PlaceIndexCompact.bakFile(dir).isFile)
    }

    @Test
    fun index_build_starting_under_the_lock_aborts_the_swap() {
        val (dir, eng) = setup()
        var calls = 0
        // Calls: pass start, before lock, under lock.
        val env =
            FakeEnv(
                eng,
                abort = { if (++calls >= 3) PlaceIndexCompact.AbortReason.INDEX_BUILD else null },
            )
        val out = PlaceIndexCompact.run(dir, env)
        assertEquals(PlaceIndexCompact.Outcome.Aborted(PlaceIndexCompact.AbortReason.INDEX_BUILD), out)
        assertEquals(1, env.lockTaken)
        assertEquals(1000, live(dir).length())
        assertFalse(PlaceIndexCompact.tmpFile(dir).exists())
    }

    // ---- abort mid-compact -------------------------------------------

    @Test
    fun plan_during_vacuum_aborts_without_touching_live() {
        val (dir, eng) = setup()
        val gate = CountDownLatch(1).also { workers.add(it) }
        eng.vacuumGate = gate
        val planStarted = AtomicReference<PlaceIndexCompact.AbortReason?>(null)
        val env = FakeEnv(eng, abort = { planStarted.get() })
        // Seed the NEEDS stamp as the idle queue would.
        assertTrue(PlaceIndexCompact.shouldOffer(dir))
        val result = AtomicReference<PlaceIndexCompact.Outcome?>(null)
        val runner = Thread { result.set(PlaceIndexCompact.run(dir, env)) }
        runner.start()
        assertTrue("vacuum must start", eng.vacuumStarted.await(5, TimeUnit.SECONDS))
        val before = live(dir).readBytes()
        planStarted.set(PlaceIndexCompact.AbortReason.PLAN)
        runner.join(5_000)
        assertEquals(PlaceIndexCompact.Outcome.Aborted(PlaceIndexCompact.AbortReason.PLAN), result.get())
        assertTrue("live untouched", before.contentEquals(live(dir).readBytes()))
        assertTrue(PlaceIndexCompact.bakFile(dir).isFile)
        assertEquals("stamp kept for resume", PlaceIndexCompact.State.NEEDS_COMPACT, PlaceIndexCompact.readStamp(dir)?.state)
        assertEquals("no swap", 0, env.lockTaken)
        // The abandoned worker finishes later and cleans its own temp.
        gate.countDown()
        val deadline = System.currentTimeMillis() + 3_000
        while (PlaceIndexCompact.workerActiveForTests() && System.currentTimeMillis() < deadline) Thread.sleep(10)
        assertFalse(PlaceIndexCompact.tmpFile(dir).exists())
        assertTrue(before.contentEquals(live(dir).readBytes()))
    }

    @Test
    fun second_pass_is_deferred_while_abandoned_worker_runs() {
        val (dir, eng) = setup()
        val gate = CountDownLatch(1).also { workers.add(it) }
        eng.vacuumGate = gate
        val abort = AtomicReference<PlaceIndexCompact.AbortReason?>(null)
        val runner = Thread { PlaceIndexCompact.run(dir, FakeEnv(eng, abort = { abort.get() })) }
        runner.start()
        assertTrue(eng.vacuumStarted.await(5, TimeUnit.SECONDS))
        abort.set(PlaceIndexCompact.AbortReason.PLAN)
        runner.join(5_000)
        val again = PlaceIndexCompact.run(dir, FakeEnv(eng))
        assertEquals(PlaceIndexCompact.Outcome.Deferred("previous_vacuum_still_running"), again)
        assertEquals(1, eng.vacuumCalls)
    }

    // ---- force-close recovery ----------------------------------------

    @Test
    fun leftover_temp_from_killed_process_is_discarded_then_job_resumes() {
        val (dir, eng) = setup()
        PlaceIndexCompact.tmpFile(dir).writeBytes(ByteArray(999) { 9 })
        File(PlaceIndexCompact.tmpFile(dir).path + "-journal").writeBytes(ByteArray(1))
        assertTrue(PlaceIndexCompact.shouldOffer(dir))
        val out = PlaceIndexCompact.run(dir, FakeEnv(eng)) as PlaceIndexCompact.Outcome.Done
        assertTrue(out.compacted)
        assertEquals("fresh temp, not the leftover", 40, live(dir).length())
        assertFalse(File(PlaceIndexCompact.tmpFile(dir).path + "-journal").exists())
    }

    // ---- stamping ----------------------------------------------------

    @Test
    fun stamp_written_when_bak_present_and_live_much_larger() {
        val (dir, _) = setup(liveBytes = 1000) // bak 100
        assertEquals(null, PlaceIndexCompact.readStamp(dir))
        assertTrue(PlaceIndexCompact.shouldOffer(dir))
        assertEquals(PlaceIndexCompact.State.NEEDS_COMPACT, PlaceIndexCompact.readStamp(dir)?.state)
    }

    @Test
    fun no_offer_without_bak_or_when_live_not_larger() {
        val (noBak, _) = setup(withBak = false)
        assertFalse(PlaceIndexCompact.shouldOffer(noBak))
        val (close, _) = setup(liveBytes = 120)
        assertFalse(PlaceIndexCompact.shouldOffer(close))
        assertEquals(null, PlaceIndexCompact.readStamp(close))
    }

    @Test
    fun done_stamp_not_reoffered_until_live_changes() {
        val (dir, eng) =
            setup(
                liveCounts = mapOf(regionA to 10L, regionB to 6L),
                bakCounts = mapOf(regionA to 10L, regionB to 5L),
            )
        PlaceIndexCompact.run(dir, FakeEnv(eng)) // bak kept (mismatch), live now 40 B
        assertFalse(PlaceIndexCompact.shouldOffer(dir))
        // Live grows again (re-index): evaluated afresh.
        live(dir).writeBytes(ByteArray(5000) { 3 })
        assertTrue(PlaceIndexCompact.shouldOffer(dir))
    }

    @Test
    fun stamp_round_trips() {
        val s = PlaceIndexCompact.Stamp(PlaceIndexCompact.State.DEFERRED, 42L, 7L, 9L, "insufficient_space free=1")
        assertEquals(s, PlaceIndexCompact.Stamp.parse(s.serialize()))
        assertNotNull(PlaceIndexCompact.Stamp.parse("state=DONE\n"))
        assertEquals(null, PlaceIndexCompact.Stamp.parse("garbage"))
    }

    @Test
    fun missing_db_is_skipped() {
        val dir = tmp.newFolder()
        val out = PlaceIndexCompact.run(dir, FakeEnv(FakeEngine()))
        assertTrue(out is PlaceIndexCompact.Outcome.Skipped)
    }
}
