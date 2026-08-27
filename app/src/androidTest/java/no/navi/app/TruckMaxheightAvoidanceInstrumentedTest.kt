package no.navi.app

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextClearance
import androidx.compose.ui.test.performTextInput
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.BeforeClass
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.FfiVehicleLimits
import uniffi.navi.TravelProfile
import uniffi.navi.planCarRoute
import java.io.File
import kotlin.math.asin
import kotlin.math.cos
import kotlin.math.min
import kotlin.math.pow
import kotlin.math.sin
import kotlin.math.sqrt

/**
 * SM-P613 real-device truck plan via the Route UI keyboard:
 * live GPS From → typed Via → typed To, Truck profile, height 2.8 m.
 * Must avoid OSM way 31776674 (Fokholgutua, maxheight=2.7).
 *
 * Ostlandet truck graph cold-build is done in [beforeClass] **before** the
 * activity launches — never while the map UI is visible (that freezes the tablet).
 */
@RunWith(AndroidJUnit4::class)
class TruckMaxheightAvoidanceInstrumentedTest {
    companion object {
        // Fokholgutua secondary, OSM way 31776674, maxheight=2.7 (both nodes).
        private val restrictedSamples =
            listOf(
                60.730013 to 11.186320,
                60.730004 to 11.186197,
            )

        private const val VIA_LAT = 60.7307100
        private const val VIA_LON = 11.1866980
        private const val END_LAT = 60.7228810
        private const val END_LON = 11.1530380

        @JvmStatic
        @BeforeClass
        fun beforeClass() {
            val ctx = InstrumentationRegistry.getInstrumentation().targetContext
            val pkg = ctx.packageName
            val auto = InstrumentationRegistry.getInstrumentation().uiAutomation
            auto.grantRuntimePermission(pkg, android.Manifest.permission.ACCESS_FINE_LOCATION)
            auto.grantRuntimePermission(pkg, android.Manifest.permission.ACCESS_COARSE_LOCATION)

            val dataDir = NaviAppData.resolve(ctx)
            OstlandetOfflineFixtures.ensureInstalled(dataDir)
            val pbf =
                listOf(
                    File(dataDir, "ostlandet-latest.osm.pbf"),
                    File("/data/local/tmp/navi_fixtures/ostlandet-latest.osm.pbf"),
                ).firstOrNull { it.isFile && it.length() > 1_000_000 }
                    ?: error("need ostlandet-latest.osm.pbf on device for truck plan")

            // Warm cache with no MainActivity on screen (Compose rule has not launched yet).
            val cache = File(dataDir, "graph-cache-${pbf.nameWithoutExtension}-truck")
            cache.mkdirs()
            android.util.Log.i("TruckMaxheight", "prewarm_start (no UI) cache=${cache.absolutePath}")
            val warm =
                planCarRoute(
                    pbf.absolutePath,
                    File(dataDir, "elevation").absolutePath,
                    cache.absolutePath,
                    VIA_LAT,
                    VIA_LON,
                    END_LAT,
                    END_LON,
                    false,
                    TravelProfile.TRUCK,
                    false,
                    false,
                    false,
                    FfiVehicleLimits(null, null, 2.8, null, null, null),
                    false,
                    "",
                )
            android.util.Log.i(
                "TruckMaxheight",
                "prewarm_done pass=${warm.report.contains("PASS")} km=${warm.distanceKm} " +
                    "cacheHit=${warm.cacheHit} cold=${warm.coldBuildS}s",
            )
            check(warm.report.contains("PASS")) {
                "prewarm via→end truck plan failed: ${warm.report}"
            }
        }
    }

    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

    private val viaLat = VIA_LAT
    private val viaLon = VIA_LON
    private val endLat = END_LAT
    private val endLon = END_LON

    private lateinit var dataDir: File

    @Before
    fun setUp() {
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        dataDir = NaviAppData.resolve(ctx)
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = false
        NaviMapTestHooks.lastPlanReport = ""
        NaviMapTestHooks.lastPlanDistanceKm = 0.0
        NaviMapTestHooks.lastRoutePolyline = ""
        NaviMapTestHooks.lastRoutePolylineChars = 0
    }

    private fun clickTag(tag: String) {
        val node = composeRule.onNodeWithTag(tag, useUnmergedTree = true)
        runCatching { node.performScrollTo() }
        node.assertIsDisplayed().performClick()
        composeRule.waitForIdle()
    }

    private fun setField(
        tag: String,
        value: String,
    ) {
        val node = composeRule.onNodeWithTag(tag, useUnmergedTree = true)
        runCatching { node.performScrollTo() }
        node.performTextClearance()
        node.performTextInput(value)
        composeRule.waitForIdle()
    }

    private fun typeCoordAndPickHit(
        chipTag: String,
        lat: Double,
        lon: Double,
    ) {
        clickTag(chipTag)
        val q = String.format(java.util.Locale.US, "%.7f, %.7f", lat, lon)
        setField("field_search", q)
        val deadline = System.currentTimeMillis() + 8_000
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.lastSearchHitCount >= 1 &&
                NaviMapTestHooks.lastSearchQuery.contains(q.take(8))
            ) {
                break
            }
            Thread.sleep(200)
        }
        assertTrue(
            "coordinate hit for $chipTag q=$q hits=${NaviMapTestHooks.lastSearchHitCount}",
            NaviMapTestHooks.lastSearchHitCount >= 1,
        )
        clickTag("search_hit_0")
        Thread.sleep(500)
    }

    private fun readLiveGps(): Pair<Double, Double> {
        val pfd =
            InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand(
                "dumpsys location",
            )
        val text =
            java.io
                .FileInputStream(pfd.fileDescriptor)
                .bufferedReader()
                .use { it.readText() }
        pfd.close()
        val gps =
            Regex("""Location\[gps ([+-]?\d+\.\d+),([+-]?\d+\.\d+)""")
                .findAll(text)
                .lastOrNull()
        val fused =
            Regex("""Location\[fused ([+-]?\d+\.\d+),([+-]?\d+\.\d+)""")
                .findAll(text)
                .lastOrNull()
        val m = gps ?: fused
        require(m != null) { "no live gps/fused fix in dumpsys location" }
        val lat = m.groupValues[1].toDouble()
        val lon = m.groupValues[2].toDouble()
        android.util.Log.i(
            "TruckMaxheight",
            "live_gps lat=$lat lon=$lon source=${if (gps != null) "gps" else "fused"}",
        )
        return lat to lon
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

    private fun decodePolyline(poly: String): List<Pair<Double, Double>> {
        return poly.split(';').mapNotNull { part ->
            val bits = part.split(',')
            if (bits.size < 2) return@mapNotNull null
            val lo = bits[0].toDoubleOrNull() ?: return@mapNotNull null
            val la = bits[1].toDoubleOrNull() ?: return@mapNotNull null
            la to lo
        }
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

    @Test
    fun live_gps_truck_height_2_8_avoids_fokholgutua_maxheight_2_7() {
        val (startLat, startLon) = readLiveGps()
        assertTrue("live start must be finite", startLat.isFinite() && startLon.isFinite())

        // Wait for map chrome.
        val styleDeadline = System.currentTimeMillis() + 60_000
        while (System.currentTimeMillis() < styleDeadline && !NaviMapTestHooks.styleReady) {
            Thread.sleep(400)
        }
        assertTrue("style ready", NaviMapTestHooks.styleReady)
        NaviMapTestHooks.hideSearchChrome = false
        Thread.sleep(1_000)

        // Ensure Route panel is open.
        runCatching { composeRule.onNodeWithTag("field_search", useUnmergedTree = true).assertIsDisplayed() }
            .onFailure {
                clickTag("btn_open_search")
            }
        composeRule.onNodeWithTag("field_search", useUnmergedTree = true).assertIsDisplayed()

        // From / Via / To all entered via keyboard as lat, lon.
        // From uses the live dumpsys GPS fix (real-GPS rule); Via/To as specified.
        typeCoordAndPickHit("chip_from", startLat, startLon)
        typeCoordAndPickHit("chip_via", viaLat, viaLon)
        typeCoordAndPickHit("chip_to", endLat, endLon)

        // Truck profile + height 2.8 m.
        composeRule.onNodeWithTag("profile_menu", useUnmergedTree = true).performScrollTo()
        runCatching { clickTag("btn_open_profile") }
        clickTag("chip_profile_truck")
        runCatching { clickTag("btn_save_profile") }
        Thread.sleep(400)
        runCatching { clickTag("btn_open_vehicle") }
        setField("field_vehicle_height", "2.8")
        clickTag("btn_save_vehicle")
        Thread.sleep(500)

        // Plan through the real Route UI.
        composeRule.onNodeWithTag("btn_plan_route", useUnmergedTree = true).performScrollTo()
        clickTag("btn_plan_route")

        val planDeadline = System.currentTimeMillis() + 900_000
        while (System.currentTimeMillis() < planDeadline) {
            if (NaviMapTestHooks.lastRoutePolylineChars > 100 &&
                NaviMapTestHooks.lastPlanReport.contains("PASS")
            ) {
                break
            }
            Thread.sleep(2_000)
            if ((System.currentTimeMillis() / 15_000) % 2 == 0L) {
                android.util.Log.i(
                    "TruckMaxheight",
                    "waiting_plan chars=${NaviMapTestHooks.lastRoutePolylineChars} " +
                        "report_len=${NaviMapTestHooks.lastPlanReport.length}",
                )
            }
        }
        assertTrue(
            "plan must PASS via UI: report=${NaviMapTestHooks.lastPlanReport.take(400)}",
            NaviMapTestHooks.lastPlanReport.contains("PASS"),
        )
        assertTrue(
            "route polyline from UI plan",
            NaviMapTestHooks.lastRoutePolylineChars > 100,
        )

        val poly = NaviMapTestHooks.lastRoutePolyline
        val hitsRestricted = routeNearRestricted(poly)
        val avoidanceLine =
            NaviMapTestHooks.lastPlanReport.lineSequence().firstOrNull {
                it.contains("weight/height/width/length-restricted", ignoreCase = true)
            }
        android.util.Log.i(
            "TruckMaxheight",
            "RESULT start=$startLat,$startLon via=$viaLat,$viaLon end=$endLat,$endLon " +
                "total_km=${NaviMapTestHooks.lastPlanDistanceKm} " +
                "near_restricted=$hitsRestricted avoidance_line=$avoidanceLine " +
                "restricted_way=31776674 Fokholgutua maxheight=2.7 " +
                "entry=keyboard_ui",
        )
        android.util.Log.i("TruckMaxheight", "plan_report=${NaviMapTestHooks.lastPlanReport}")

        File(dataDir, "truck_maxheight_report.txt").writeText(
            buildString {
                appendLine("entry=keyboard_ui")
                appendLine("start=$startLat,$startLon")
                appendLine("via=$viaLat,$viaLon")
                appendLine("end=$endLat,$endLon")
                appendLine("height_m=2.8")
                appendLine("restricted_way=31776674 Fokholgutua maxheight=2.7")
                appendLine("total_km=${NaviMapTestHooks.lastPlanDistanceKm}")
                appendLine("near_restricted=$hitsRestricted")
                appendLine("avoidance_line=$avoidanceLine")
                appendLine("--- plan report ---")
                appendLine(NaviMapTestHooks.lastPlanReport)
                appendLine("--- polyline sample (first 12 pts) ---")
                appendLine(decodePolyline(poly).take(12).joinToString(";"))
            },
        )

        NaviMapTestHooks.hideSearchChrome = true
        Thread.sleep(1_500)
        NaviMapTestHooks.pendingCamera = Triple(viaLat, viaLon, 13.0)
        Thread.sleep(3_000)
        InstrumentedMapCapture.screencapAfterSettle(
            "/data/local/tmp/truck_maxheight_avoidance.png",
            8_000,
        )

        assertFalse(
            "BUG: height-limited route still uses maxheight=2.7 Fokholgutua (way 31776674)",
            hitsRestricted,
        )
        if (avoidanceLine != null) {
            val n =
                Regex("""avoids (\d+)""")
                    .find(avoidanceLine)
                    ?.groupValues
                    ?.get(1)
                    ?.toIntOrNull()
            android.util.Log.i("TruckMaxheight", "avoidance_count=$n line=$avoidanceLine")
            assertTrue("avoidance summary should be non-zero when present: $avoidanceLine", (n ?: 0) > 0)
        }
        assertTrue(
            "detour should still be a real road distance",
            NaviMapTestHooks.lastPlanDistanceKm > 0.5,
        )
    }
}
