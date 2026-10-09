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

class IdlePackJobsPauseTest {
    @get:Rule
    val tmp = TemporaryFolder()

    @After
    fun tearDown() {
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
}
