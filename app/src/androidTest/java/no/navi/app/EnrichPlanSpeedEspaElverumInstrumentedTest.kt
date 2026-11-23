package no.navi.app

import android.os.SystemClock
import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.filters.LargeTest
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.TravelProfile
import java.io.File
import java.util.Locale

/**
 * St1 Espa → McDonald's Elverum: plan wall time must stay under 6 s after
 * look-ahead seeding left the critical path.
 *
 * Measures [MwmProductionPlanner.plan] only (not UI search / region download).
 */
@RunWith(AndroidJUnit4::class)
@LargeTest
class EnrichPlanSpeedEspaElverumInstrumentedTest {
    private val h = ComapsPhase6Harness

    // Nominatim: St1 Espa / McDonald's Elverum.
    private val st1Espa = Triple("St1 Espa", 60.5621914, 11.2561239)
    private val mcdonaldsElverum = Triple("McDonald's Elverum", 60.8871362, 11.5154077)

    @Before
    fun openSession() {
        assumeTrue("MWM client flag", MwmClientBootstrap.isEnabled())
        assertTrue(
            "CoMaps session",
            ComapsSessionBootstrap.ensureOpen(h.context, force = false),
        )
        assertTrue("production planner", MwmProductionPlanner.shouldUse(h.context))
    }

    @Test
    fun st1_espa_to_mcdonalds_elverum_plans_under_6s() {
        // Warm-up: first CoMaps route after session open can pay index cost.
        runCatching {
            MwmProductionPlanner.plan(
                context = h.context,
                profile = TravelProfile.CAR,
                startLat = st1Espa.second,
                startLon = st1Espa.third,
                endLat = mcdonaldsElverum.second,
                endLon = mcdonaldsElverum.third,
                vias = emptyList(),
                avoidMotorways = false,
                avoidTolls = false,
                avoidFerries = false,
                turnsLocale = "nb",
            )
        }

        val t0 = SystemClock.elapsedRealtime()
        val result =
            MwmProductionPlanner.plan(
                context = h.context,
                profile = TravelProfile.CAR,
                startLat = st1Espa.second,
                startLon = st1Espa.third,
                endLat = mcdonaldsElverum.second,
                endLon = mcdonaldsElverum.third,
                vias = emptyList(),
                avoidMotorways = false,
                avoidTolls = false,
                avoidFerries = false,
                turnsLocale = "nb",
            )
        val planMs = SystemClock.elapsedRealtime() - t0

        val dataDir = NaviAppData.resolve(h.context)
        File(dataDir, "enrich_plan_speed_espa_elverum.txt").writeText(
            buildString {
                appendLine("from=${st1Espa.first} ${st1Espa.second},${st1Espa.third}")
                appendLine(
                    "to=${mcdonaldsElverum.first} " +
                        "${mcdonaldsElverum.second},${mcdonaldsElverum.third}",
                )
                appendLine("plan_ms=$planMs")
                appendLine("distance_km=${"%.3f".format(Locale.US, result.distanceKm)}")
                appendLine("eta_min=${"%.1f".format(Locale.US, result.etaMinutes)}")
                appendLine("--- report ---")
                appendLine(result.report)
            },
        )
        Log.i(
            TAG,
            "espa_elverum plan_ms=$planMs distance_km=${result.distanceKm} " +
                "pass=${result.report.contains("PASS")}",
        )

        assertTrue("PLANNER=mwm\n${result.report.take(400)}", result.report.contains("PLANNER=mwm"))
        assertTrue("PASS\n${result.report.take(400)}", result.report.contains("PASS"))
        assertTrue("polyline", result.routePolyline.length > 100)
        assertTrue(
            "look-ahead must be async on critical path\n${result.report.take(600)}",
            result.report.contains("mwm_enrich_lookahead=async"),
        )
        assertTrue(
            "plan_ms=$planMs must be <= 6000 (prefer shorter)",
            planMs <= 6_000L,
        )
    }

    companion object {
        private const val TAG = "EnrichPlanSpeed"
    }
}
