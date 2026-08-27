package no.navi.app

import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.FfiVehicleLimits
import uniffi.navi.TravelProfile
import uniffi.navi.foregroundPlanActive
import uniffi.navi.foregroundPlanEnter
import uniffi.navi.foregroundPlanLeave
import uniffi.navi.liveSpeedLimitConeJson
import uniffi.navi.planCarRoute
import java.io.File
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference
import kotlin.concurrent.thread

/**
 * GPS cone / road-near bbox builds must skip while a foreground plan owns the
 * PBF (Issue 2 gap). Plan wall time should stay near the idle pack-miss figure,
 * not stretch to multi-tens of minutes from concurrent cone scans.
 */
@RunWith(AndroidJUnit4::class)
class ForegroundPlanConeSkipInstrumentedTest {
    @Test
    fun coneSkipsDuringForegroundPlan_andRecoversAfter() {
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        val dataDir = NaviAppData.resolve(ctx)
        val pbf =
            listOf(
                File(dataDir, "ostlandet-latest.osm.pbf"),
                File("/data/local/tmp/navi_fixtures/ostlandet-latest.osm.pbf"),
            ).firstOrNull { it.isFile && it.length() > 100_000_000L }
        assumeTrue("Ostlandet PBF required", pbf != null)
        val pbfFile = pbf!!
        val cache = File(dataDir, "graph-cache-${pbfFile.nameWithoutExtension}-car")
        cache.mkdirs()
        val elev = File(dataDir, "elevation").also { it.mkdirs() }
        val indexDb = File(dataDir, "place_index.db")

        PlaceIndexBackground.ensureStarted(pbfFile, indexDb)
        IndexedMapsBackground.ensureStarted(pbfFile, dataDir, elev)
        Thread.sleep(4_000)
        assumeTrue(
            "convert or place-index should be running in pre-index state",
            IndexedMapsBackground.isRunning() || PlaceIndexBackground.isRunning(),
        )

        val graphsBefore = graphPackCount(dataDir)
        val maxGraphsDuring = AtomicInteger(graphsBefore)
        val planRunning = AtomicBoolean(true)
        val monitor =
            thread(name = "graph-pack-monitor") {
                while (planRunning.get()) {
                    maxGraphsDuring.updateAndGet { cur ->
                        graphPackCount(dataDir).coerceAtLeast(cur)
                    }
                    Thread.sleep(2_000)
                }
            }

        foregroundPlanEnter()
        val coneFastMs = AtomicInteger(0)
        val coneSlow = AtomicInteger(0)
        val coneCalls = AtomicInteger(0)
        val coneStop = AtomicBoolean(false)
        val coneThread =
            thread(name = "cone-during-plan") {
                while (!coneStop.get()) {
                    val t0 = System.nanoTime()
                    runCatching {
                        liveSpeedLimitConeJson(
                            pbfFile.absolutePath,
                            cache.absolutePath,
                            elev.absolutePath,
                            START_LAT,
                            START_LON,
                            90.0,
                            TravelProfile.CAR,
                            50.0,
                        )
                    }
                    val ms = (System.nanoTime() - t0) / 1_000_000L
                    coneCalls.incrementAndGet()
                    if (ms < 2_000L) {
                        coneFastMs.incrementAndGet()
                    } else {
                        coneSlow.incrementAndGet()
                    }
                    Log.i(TAG, "cone_during_plan_ms=$ms")
                    Thread.sleep(800)
                }
            }

        val planStart = System.currentTimeMillis()
        val result =
            try {
                planCarRoute(
                    pbfFile.absolutePath,
                    elev.absolutePath,
                    cache.absolutePath,
                    START_LAT,
                    START_LON,
                    END_LAT,
                    END_LON,
                    useEco = false,
                    profile = TravelProfile.CAR,
                    avoidMotorways = false,
                    avoidTolls = false,
                    avoidFerries = false,
                    vehicle = EMPTY_VEHICLE,
                    preferOfficialNetworks = false,
                    dataDir = "",
                )
            } finally {
                planRunning.set(false)
                coneStop.set(true)
                foregroundPlanLeave()
            }
        val planMs = System.currentTimeMillis() - planStart
        coneThread.join(15_000)
        monitor.join(5_000)

        Log.i(
            TAG,
            "plan_ms=$planMs pass=${result.report.contains("PASS")} " +
                "cone_calls=${coneCalls.get()} cone_fast=${coneFastMs.get()} " +
                "cone_slow=${coneSlow.get()} graphs_before=$graphsBefore " +
                "graphs_max_during=${maxGraphsDuring.get()} convert=${IndexedMapsBackground.isRunning()}",
        )
        assertTrue("plan must PASS: ${result.report}", result.report.contains("PASS"))
        assertTrue("GPS cone should have been invoked during the plan", coneCalls.get() >= 3)
        assertTrue(
            "cone builds must skip (fast) during the plan, not scan the PBF",
            coneFastMs.get() >= 3 && coneSlow.get() == 0,
        )
        assertTrue(
            "convert must not publish new graph tiles during the plan",
            maxGraphsDuring.get() <= graphsBefore,
        )
        assertFalse("plan leave must clear the foreground-plan flag", foregroundPlanActive())

        val recoverJson = AtomicReference<String>("")
        val recoverMs = AtomicInteger(-1)
        val recoverThread =
            thread(name = "cone-after-plan") {
                val t0 = System.nanoTime()
                recoverJson.set(
                    runCatching {
                        liveSpeedLimitConeJson(
                            pbfFile.absolutePath,
                            cache.absolutePath,
                            elev.absolutePath,
                            START_LAT,
                            START_LON,
                            90.0,
                            TravelProfile.CAR,
                            50.0,
                        )
                    }.getOrDefault("{}"),
                )
                recoverMs.set(((System.nanoTime() - t0) / 1_000_000L).toInt())
            }
        recoverThread.join(20_000)
        val recoverFinished = !recoverThread.isAlive
        Log.i(
            TAG,
            "cone_after_plan finished=$recoverFinished ms=${recoverMs.get()} " +
                "json_len=${recoverJson.get().length}",
        )
        if (recoverFinished) {
            assertTrue(
                "next cone after the plan must not stay on the skip path",
                recoverJson.get() != "{}" || recoverMs.get() >= 500,
            )
        }

        val resumeDeadline = System.currentTimeMillis() + 90_000L
        var graphsAfter = graphPackCount(dataDir)
        while (System.currentTimeMillis() < resumeDeadline && graphsAfter <= graphsBefore) {
            Thread.sleep(5_000)
            graphsAfter = graphPackCount(dataDir)
        }
        Log.i(
            TAG,
            "graphs_after=$graphsAfter convert_still=${IndexedMapsBackground.isRunning()} " +
                "place_index=${PlaceIndexBackground.isRunning()}",
        )
        assertTrue(
            "convert/place-index should resume after the plan",
            graphsAfter > graphsBefore ||
                IndexedMapsBackground.isRunning() ||
                PlaceIndexBackground.isRunning(),
        )
        // Debug PBF fallback can take tens of minutes; cone contention was 12-13 min
        // extra on top. Skip must keep this from becoming unbounded.
        assertTrue(
            "plan wall ${planMs}ms is still far beyond a contended pack-miss",
            planMs < 25 * 60_000L,
        )
    }

    private fun graphPackCount(dataDir: File): Int =
        dataDir.listFiles()?.count { f ->
            f.isFile && f.name.contains("navi-graph-") && f.name.endsWith(".rkyv")
        } ?: 0

    private companion object {
        const val TAG = "ForegroundPlanConeSkip"
        const val START_LAT = 59.9139
        const val START_LON = 10.7522
        const val END_LAT = 59.9333
        const val END_LON = 10.7500
        val EMPTY_VEHICLE =
            FfiVehicleLimits(
                axleWeightKg = null,
                bogieWeightKg = null,
                heightM = null,
                widthM = null,
                lengthM = null,
                totalWeightKg = null,
            )
    }
}
