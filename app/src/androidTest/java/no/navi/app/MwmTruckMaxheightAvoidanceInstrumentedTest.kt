package no.navi.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.filters.LargeTest
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.FfiVehicleLimits
import uniffi.navi.TravelProfile
import uniffi.navi.saveVehicleLimits
import java.io.File
import kotlin.math.asin
import kotlin.math.cos
import kotlin.math.min
import kotlin.math.pow
import kotlin.math.sin
import kotlin.math.sqrt

/**
 * MWM-path port of [TruckMaxheightAvoidanceInstrumentedTest]: truck height 2.8 m
 * near Fokholgutua (OSM way 31776674, maxheight=2.7) must not hug the restricted
 * samples when leaf sidecars carry `vehicle_limits`.
 *
 * Skips (Assume) when installed sidecars lack the section — that needs the
 * Nordic/world vehicle_limits rebuild + device refresh.
 */
@RunWith(AndroidJUnit4::class)
@LargeTest
class MwmTruckMaxheightAvoidanceInstrumentedTest {
    private val h = ComapsPhase6Harness

    // Fokholgutua secondary, OSM way 31776674, maxheight=2.7 (both nodes).
    private val restrictedSamples =
        listOf(
            60.730013 to 11.186320,
            60.730004 to 11.186197,
        )

    private val viaLat = 60.7307100
    private val viaLon = 11.1866980
    private val endLat = 60.7228810
    private val endLon = 11.1530380

    // Short approach west of the restriction so car routing has a choice.
    private val startLat = 60.7318000
    private val startLon = 11.1905000

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
    fun truck_height_2_8_avoids_fokholgutua_when_vehicle_limits_present() {
        val dataDir = NaviAppData.resolve(h.context)
        assertTrue(
            "persist height",
            saveVehicleLimits(
                dataDir.absolutePath,
                FfiVehicleLimits(
                    heightM = 2.8,
                    widthM = null,
                    lengthM = null,
                    totalWeightKg = null,
                    axleWeightKg = null,
                    bogieWeightKg = null,
                ),
            ),
        )

        val result =
            MwmProductionPlanner.plan(
                context = h.context,
                profile = TravelProfile.TRUCK,
                startLat = startLat,
                startLon = startLon,
                endLat = endLat,
                endLon = endLon,
                vias = listOf(viaLat to viaLon),
                avoidMotorways = false,
                avoidTolls = false,
                avoidFerries = false,
                turnsLocale = "nb",
            )

        File(dataDir, "mwm_truck_maxheight_report.txt").writeText(
            buildString {
                appendLine("entry=mwm_production_planner")
                appendLine("start=$startLat,$startLon")
                appendLine("via=$viaLat,$viaLon")
                appendLine("end=$endLat,$endLon")
                appendLine("height_m=2.8")
                appendLine("restricted_way=31776674 Fokholgutua maxheight=2.7")
                appendLine("--- plan report ---")
                appendLine(result.report)
            },
        )

        assertTrue("PLANNER=mwm\n${result.report.take(300)}", result.report.contains("PLANNER=mwm"))
        val coverage = VehicleRestrictionUi.parseCoverage(result.report)
        assumeTrue(
            "device sidecars need vehicle_limits (Nordic rebuild). report=${result.report.take(400)}",
            coverage.known && coverage.dataAvailable,
        )
        assertTrue("PASS\n${result.report.take(400)}", result.report.contains("PASS"))
        assertTrue("polyline", result.routePolyline.length > 100)

        val hitsRestricted = routeNearRestricted(result.routePolyline)
        assertFalse(
            "BUG: MWM height-limited route still uses maxheight=2.7 Fokholgutua",
            hitsRestricted,
        )
        assertTrue("detour distance", result.distanceKm > 0.5)
    }

    private fun routeNearRestricted(
        poly: String,
        thresholdM: Double = 25.0,
    ): Boolean {
        val pts = decodePolyline(poly)
        if (pts.isEmpty()) return false
        for ((rlat, rlon) in restrictedSamples) {
            val d = pts.minOf { (la, lo) -> haversineM(la, lo, rlat, rlon) }
            if (d <= thresholdM) return true
        }
        return false
    }

    private fun decodePolyline(poly: String): List<Pair<Double, Double>> =
        poly.split(';').mapNotNull { part ->
            val bits = part.split(',')
            if (bits.size < 2) return@mapNotNull null
            val lo = bits[0].toDoubleOrNull() ?: return@mapNotNull null
            val la = bits[1].toDoubleOrNull() ?: return@mapNotNull null
            la to lo
        }

    private fun haversineM(
        aLat: Double,
        aLon: Double,
        bLat: Double,
        bLon: Double,
    ): Double {
        val r = 6_371_000.0
        val p1 = Math.toRadians(aLat)
        val p2 = Math.toRadians(bLat)
        val dPhi = Math.toRadians(bLat - aLat)
        val dLam = Math.toRadians(bLon - aLon)
        val h =
            sin(dPhi / 2).pow(2) +
                cos(p1) * cos(p2) * sin(dLam / 2).pow(2)
        return 2 * r * asin(min(1.0, sqrt(h)))
    }
}
