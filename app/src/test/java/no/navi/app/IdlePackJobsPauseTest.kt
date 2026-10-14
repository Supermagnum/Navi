package no.navi.app

import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger

class IdlePackJobsPauseTest {
    @get:Rule
    val tmp = TemporaryFolder()

    @After
    fun tearDown() {
        IdlePackJobs.executeJobs = false
        IdlePackJobs.testRunJob = { false }
        val deadline = System.currentTimeMillis() + 5_000
        while (IdlePackJobs.isRunning() && System.currentTimeMillis() < deadline) {
            Thread.sleep(20)
        }
        IdlePackJobs.resetForTests()
        if (RoutePlanGate.isRunning()) {
            RoutePlanGate.end()
        }
    }

    @Test
    fun plan_during_skeleton_waits_then_runs() {
        IdlePackJobs.resetForTests()
        IdlePackJobs.executeJobs = true
        val started = CountDownLatch(1)
        val finished = AtomicBoolean(false)
        IdlePackJobs.testRunJob = { job ->
            if (job.kind == IdlePackJobs.Kind.SKELETON) {
                started.countDown()
                Thread.sleep(250)
                finished.set(true)
            }
            false
        }
        val packDir = tmp.newFolder("packs")
        IdlePackJobs.offerSkeleton(packDir, "test-latest")
        assertTrue("skeleton job must start", started.await(2, TimeUnit.SECONDS))
        val t0 = System.currentTimeMillis()
        assertTrue(RoutePlanGate.tryBegin())
        val waited = System.currentTimeMillis() - t0
        assertTrue("skeleton must finish before the plan proceeds", finished.get())
        assertTrue("plan waited ${waited}ms", waited >= 150)
        val pause = IdlePackJobs.lastPauseOutcome()
        assertEquals("waited", pause.action)
        assertEquals(IdlePackJobs.Kind.SKELETON, pause.kind)
        RoutePlanGate.end()
    }

    @Test
    fun plan_during_place_index_pauses_without_waiting_for_finish() {
        IdlePackJobs.resetForTests()
        IdlePackJobs.executeJobs = true
        val started = CountDownLatch(1)
        val finished = AtomicBoolean(false)
        IdlePackJobs.testRunJob = { job ->
            if (job.kind == IdlePackJobs.Kind.PLACE_INDEX) {
                started.countDown()
                Thread.sleep(250)
                finished.set(true)
                true
            } else {
                false
            }
        }
        val packDir = tmp.newFolder("packs")
        val pbf = File(packDir, "fu38-pause.osm.pbf").apply { writeText("x") }
        // Scratch database created and removed with this test. Never the product index.
        val db = File(packDir, "place_index.db")
        IdlePackJobs.offerPlaceIndex(pbf, db, "test/fu38-pause")
        assertTrue("place-index job must start", started.await(2, TimeUnit.SECONDS))
        val t0 = System.currentTimeMillis()
        assertTrue(RoutePlanGate.tryBegin())
        val waited = System.currentTimeMillis() - t0
        assertTrue("index must unwind before the plan proceeds", finished.get())
        assertTrue("plan waited ${waited}ms for pause", waited >= 150)
        val pause = IdlePackJobs.lastPauseOutcome()
        assertEquals("paused", pause.action)
        assertEquals(IdlePackJobs.Kind.PLACE_INDEX, pause.kind)
        RoutePlanGate.end()
    }

    @Test
    fun replace_cancels_running_plan_then_starts() {
        assertTrue(RoutePlanGate.tryBegin())
        val cancelled = AtomicBoolean(false)
        val ender =
            Thread {
                Thread.sleep(40)
                RoutePlanGate.end()
            }
        ender.start()
        assertTrue(RoutePlanGate.tryBeginOrReplace({ cancelled.set(true) }, timeoutMs = 2_000))
        assertTrue(cancelled.get())
        ender.join(1_000)
        RoutePlanGate.end()
    }

    @Test
    fun plan_starts_within_two_seconds_while_slow_index_runs() {
        IdlePackJobs.resetForTests()
        IdlePackJobs.executeJobs = true
        val started = CountDownLatch(1)
        val runs = AtomicInteger(0)
        IdlePackJobs.testRunJob = { job ->
            if (job.kind != IdlePackJobs.Kind.PLACE_INDEX) {
                false
            } else if (runs.incrementAndGet() > 1) {
                false
            } else {
                started.countDown()
                val until = System.currentTimeMillis() + 5_000
                while (System.currentTimeMillis() < until) {
                    if (IdlePackJobs.pauseRequested) {
                        break
                    }
                    Thread.sleep(20)
                }
                true
            }
        }
        val packDir = tmp.newFolder("packs-slow")
        val pbf = File(packDir, "fu56-slow.osm.pbf").apply { writeText("x") }
        val db = File(packDir, "place_index.db")
        IdlePackJobs.offerPlaceIndex(pbf, db, "test/fu56-slow")
        assertTrue("index job must start", started.await(2, TimeUnit.SECONDS))
        val t0 = System.currentTimeMillis()
        assertTrue(RoutePlanGate.tryBegin())
        val waited = System.currentTimeMillis() - t0
        assertTrue("plan waited ${waited}ms, cap is 2000", waited <= 2_200)
        RoutePlanGate.end()
    }

    @Test
    fun paused_index_resumes_after_plan_without_losing_the_job() {
        IdlePackJobs.resetForTests()
        IdlePackJobs.executeJobs = true
        val started = CountDownLatch(1)
        val resumed = CountDownLatch(1)
        val runs = AtomicInteger(0)
        IdlePackJobs.testRunJob = { job ->
            if (job.kind != IdlePackJobs.Kind.PLACE_INDEX) {
                false
            } else {
                val n = runs.incrementAndGet()
                if (n == 1) {
                    started.countDown()
                    val until = System.currentTimeMillis() + 3_000
                    while (System.currentTimeMillis() < until && !IdlePackJobs.pauseRequested) {
                        Thread.sleep(10)
                    }
                    true
                } else {
                    resumed.countDown()
                    false
                }
            }
        }
        val packDir = tmp.newFolder("packs-resume")
        val pbf = File(packDir, "fu56-resume.osm.pbf").apply { writeText("x") }
        val db = File(packDir, "place_index.db")
        IdlePackJobs.offerPlaceIndex(pbf, db, "test/fu56-resume")
        assertTrue("index job must start", started.await(2, TimeUnit.SECONDS))
        assertTrue(RoutePlanGate.tryBegin())
        RoutePlanGate.end()
        assertTrue("paused index must resume after the plan", resumed.await(3, TimeUnit.SECONDS))
        assertEquals(2, runs.get())
    }
}
