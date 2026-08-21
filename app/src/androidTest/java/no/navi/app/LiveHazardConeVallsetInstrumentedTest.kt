package no.navi.app

import android.util.Log
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.By
import androidx.test.uiautomator.UiDevice
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.BeforeClass
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.liveHazardConeM
import kotlin.math.asin
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.min
import kotlin.math.pow
import kotlin.math.sin
import kotlin.math.sqrt

/**
 * Route-independent 300 m live hazard cone along the Vallset skole corridor.
 *
 * Does **not** plan a route: drives the built-in simulator along corridor geometry
 * with live position + heading only.
 */
@RunWith(AndroidJUnit4::class)
class LiveHazardConeVallsetInstrumentedTest {
    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

    private lateinit var device: UiDevice

    companion object {
        private const val TAG = "LiveHazardConeVallset"

        // Same corridor as RoadSignSchoolCorridorInstrumentedTest, but with an
        // eastward dog-leg so the simulated path actually approaches NO:109
        // (a straight START→END chord stays ~145 m / ~90° off the hump).
        private val START = 60.68420 to 11.34080
        private val NEAR_SCHOOL = 60.68150 to 11.34220
        private val APPROACH_109 = 60.68030 to 11.34450
        private val AT_109 = 60.6801722 to 11.3458249
        private val END = 60.67850 to 11.34400
        private val SIGN_109 = AT_109
        private val SCHOOL_CENTER = 60.6811042 to 11.3422291
        private val CORRIDOR =
            listOf(START, NEAR_SCHOOL, APPROACH_109, AT_109, END)

        @JvmStatic
        @BeforeClass
        fun beforeClass() {
            val ctx = InstrumentationRegistry.getInstrumentation().targetContext
            val auto = InstrumentationRegistry.getInstrumentation().uiAutomation
            auto.grantRuntimePermission(ctx.packageName, android.Manifest.permission.ACCESS_FINE_LOCATION)
            auto.grantRuntimePermission(ctx.packageName, android.Manifest.permission.ACCESS_COARSE_LOCATION)
            val dataDir = NaviAppData.resolve(ctx).also { it.mkdirs() }
            OstlandetOfflineFixtures.ensureInstalled(dataDir)
        }
    }

    @Before
    fun setUp() {
        device = UiDevice.getInstance(InstrumentationRegistry.getInstrumentation())
        NaviMapTestHooks.disableGpsFollow = true
        NaviMapTestHooks.followGps = false
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = false
        NaviMapTestHooks.ignoreLiveGpsFixes = true
        NaviMapTestHooks.liveHazardConeEnabled = true
        NaviMapTestHooks.requestStopRouteSimulation = true
        Thread.sleep(300)
        NaviMapTestHooks.requestStopRouteSimulation = false
        NaviMapTestHooks.simulatingActive = false
        NaviMapTestHooks.lastRoadSignWarningJson = "{}"
        NaviMapTestHooks.lastSchoolProximityWarningJson = "{}"
        dismissPermission()
    }

    private fun dismissPermission() {
        val deadline = System.currentTimeMillis() + 6_000
        while (System.currentTimeMillis() < deadline) {
            val allow =
                device.findObject(By.text("While using the app"))
                    ?: device.findObject(By.text("Allow"))
                    ?: device.findObject(
                        By.res("com.android.permissioncontroller", "permission_allow_foreground_only_button"),
                    )
            if (allow != null) {
                allow.click()
                Thread.sleep(400)
                continue
            }
            break
        }
    }

    private fun waitStyle(timeoutMs: Long = 90_000) {
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.styleReady || NaviMapTestHooks.lastReportedLayerCount >= 1) return
            Thread.sleep(400)
        }
    }

    private fun waitLiveHazardsLoaded(timeoutMs: Long = 180_000) {
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.lastLiveHazardChildren >= 1 &&
                NaviMapTestHooks.lastLiveHazardSigns >= 1
            ) {
                return
            }
            Log.i(
                TAG,
                "waiting live hazards signs=${NaviMapTestHooks.lastLiveHazardSigns} " +
                    "children=${NaviMapTestHooks.lastLiveHazardChildren} " +
                    "bumps=${NaviMapTestHooks.lastLiveHazardBumps}",
            )
            Thread.sleep(2_000)
        }
        error(
            "live hazards not loaded signs=${NaviMapTestHooks.lastLiveHazardSigns} " +
                "children=${NaviMapTestHooks.lastLiveHazardChildren}",
        )
    }

    private fun haversineM(
        lat1: Double,
        lon1: Double,
        lat2: Double,
        lon2: Double,
    ): Double {
        val rlat1 = Math.toRadians(lat1)
        val rlat2 = Math.toRadians(lat2)
        val dlat = Math.toRadians(lat2 - lat1)
        val dlon = Math.toRadians(lon2 - lon1)
        val h =
            sin(dlat / 2).pow(2.0) +
                cos(rlat1) * cos(rlat2) * sin(dlon / 2).pow(2.0)
        return 2.0 * 6_378_100.0 * asin(min(1.0, sqrt(h)))
    }

    private fun bearingDeg(
        lat1: Double,
        lon1: Double,
        lat2: Double,
        lon2: Double,
    ): Double {
        val rlat1 = Math.toRadians(lat1)
        val rlat2 = Math.toRadians(lat2)
        val dlon = Math.toRadians(lon2 - lon1)
        val y = sin(dlon) * cos(rlat2)
        val x = cos(rlat1) * sin(rlat2) - sin(rlat1) * cos(rlat2) * cos(dlon)
        return (Math.toDegrees(atan2(y, x)) + 360.0) % 360.0
    }

    private fun densifyCorridor(
        points: List<Pair<Double, Double>>,
        stepM: Double = 25.0,
    ): String {
        require(points.size >= 2)
        val pts = StringBuilder("[")
        var first = true
        for (i in 0 until points.lastIndex) {
            val a = points[i]
            val b = points[i + 1]
            val dist = haversineM(a.first, a.second, b.first, b.second)
            val n = (dist / stepM).toInt().coerceAtLeast(1)
            val from = if (i == 0) 0 else 1
            for (j in from..n) {
                val t = j.toDouble() / n
                val lat = a.first + (b.first - a.first) * t
                val lon = a.second + (b.second - a.second) * t
                if (!first) pts.append(',')
                first = false
                pts.append("[$lat,$lon]")
            }
        }
        pts.append(']')
        return pts.toString()
    }

    private fun startLiveConeSimAlongCorridor() {
        NaviMapTestHooks.ignoreLiveGpsFixes = true
        NaviMapTestHooks.simulationTimeScale = 0.08
        NaviMapTestHooks.liveConeSimSpeedKmh = 40.0
        NaviMapTestHooks.liveConeSimCoordsJson = densifyCorridor(CORRIDOR)
        NaviMapTestHooks.requestStartLiveConeSimulation = true
        val deadline = System.currentTimeMillis() + 30_000
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.simulatingActive) return
            Thread.sleep(200)
        }
        error("live cone simulation did not start")
    }

    private fun seekApproaching(
        targetLat: Double,
        targetLon: Double,
        maxDistM: Double = 80.0,
        // Stay outside APPROACH_HIDE_M (~25 m) and inside the 300 m cone,
        // with the hazard still ahead of travel (not already passed).
        approachLeadM: Double = 180.0,
    ) {
        val samples = parseRouteSimSamples(NaviMapTestHooks.lastSimSamplesJson)
        assertTrue("need sim samples", samples.size >= 5)
        val closest = samples.minBy { haversineM(it.lat, it.lon, targetLat, targetLon) }
        val d = haversineM(closest.lat, closest.lon, targetLat, targetLon)
        assertTrue("corridor must pass within ${maxDistM}m (got ${d}m)", d < maxDistM)
        val seekM = (closest.cumM - approachLeadM).coerceAtLeast(0.0)
        NaviMapTestHooks.requestSimSeekCumM = seekM
        Thread.sleep(800)
    }

    @Test
    fun cone_radius_is_300m_not_200m() {
        assertEquals(300.0, liveHazardConeM(), 0.01)
    }

    @Test
    fun vallset_route_independent_cone_fires_children_and_109() {
        waitStyle()
        waitLiveHazardsLoaded()
        assertEquals(300.0, NaviMapTestHooks.lastLiveHazardConeM, 0.01)
        assertTrue(
            "compact children should be centroids-scale (<<88k)",
            NaviMapTestHooks.lastLiveHazardChildren in 1..20_000,
        )
        assertTrue(
            "compact utf8 should be region-scale MB not tens of MB",
            NaviMapTestHooks.lastLiveHazardCompactUtf8 in 100_000L..8_000_000L,
        )

        startLiveConeSimAlongCorridor()
        seekApproaching(SCHOOL_CENTER.first, SCHOOL_CENTER.second, maxDistM = 220.0)

        val deadline = System.currentTimeMillis() + 60_000
        var sawChildren = false
        var saw109 = false
        while (System.currentTimeMillis() < deadline) {
            val sign = NaviMapTestHooks.lastRoadSignWarningJson
            val school = NaviMapTestHooks.lastSchoolProximityWarningJson
            if (school.contains("\"source\":\"children_proximity\"") ||
                (sign.contains("\"code\":\"142\"") && sign.contains("children"))
            ) {
                sawChildren = true
            }
            if (sign.contains("\"code\":\"109\"") || school.contains("\"code\":\"109\"")) {
                saw109 = true
            }
            // Cone metadata must stay 300 m when present.
            if (sign.contains("cone_m")) {
                assertTrue("cone_m must be 300: $sign", sign.contains("\"cone_m\":300"))
            }
            if (sawChildren) break
            Thread.sleep(300)
        }
        assertTrue(
            "expected children-zone via live cone without planned route; " +
                "sign=${NaviMapTestHooks.lastRoadSignWarningJson} " +
                "school=${NaviMapTestHooks.lastSchoolProximityWarningJson} " +
                "gps=${NaviMapTestHooks.lastGpsLat},${NaviMapTestHooks.lastGpsLon}",
            sawChildren,
        )
        Log.i(TAG, "children_ok 109_seen=$saw109 sign=${NaviMapTestHooks.lastRoadSignWarningJson}")

        // Continue / seek toward the NO:109 hump (still approaching, not on top of it).
        seekApproaching(SIGN_109.first, SIGN_109.second, maxDistM = 80.0, approachLeadM = 120.0)
        val humpDeadline = System.currentTimeMillis() + 45_000
        while (System.currentTimeMillis() < humpDeadline) {
            val sign = NaviMapTestHooks.lastRoadSignWarningJson
            if (sign.contains("\"code\":\"109\"")) {
                saw109 = true
                break
            }
            Thread.sleep(300)
        }
        assertTrue(
            "expected NO:109 speed hump via live cone; sign=${NaviMapTestHooks.lastRoadSignWarningJson}",
            saw109,
        )
    }

    @Test
    fun near_miss_outside_300m_does_not_fire() {
        waitStyle()
        waitLiveHazardsLoaded()
        // Point ~350 m east of Vallset skole, heading west (away from school).
        val schoolLat = SCHOOL_CENTER.first
        val schoolLon = SCHOOL_CENTER.second
        val farLon = schoolLon + (350.0 / (111_320.0 * cos(Math.toRadians(schoolLat))))
        val headingAway = bearingDeg(schoolLat, farLon, schoolLat, farLon + 0.01)
        NaviMapTestHooks.ignoreLiveGpsFixes = true
        NaviMapTestHooks.liveConeSimCoordsJson =
            "[[$schoolLat,$farLon],[$schoolLat,${farLon + 0.002}]]"
        NaviMapTestHooks.liveConeSimSpeedKmh = 30.0
        NaviMapTestHooks.requestStartLiveConeSimulation = true
        Thread.sleep(2_000)
        NaviMapTestHooks.requestSimSeekCumM = 0.0
        Thread.sleep(1_500)

        val sign = NaviMapTestHooks.lastRoadSignWarningJson
        val school = NaviMapTestHooks.lastSchoolProximityWarningJson
        val schoolWarn = roadSignWarningFromJson(school)
        val signWarn = roadSignWarningFromJson(sign)
        // School must not fire: outside 300 m cone.
        assertFalse(
            "near-miss must not fire children at ~350m: school=$school sign=$sign heading=$headingAway",
            schoolWarn.active && schoolWarn.code == "142",
        )
        assertFalse(
            "near-miss must not fire 142 sign: $sign",
            signWarn.active && signWarn.code == "142" && sign.contains("children_proximity"),
        )
        Log.i(TAG, "near_miss_ok gps=${NaviMapTestHooks.lastGpsLat},${NaviMapTestHooks.lastGpsLon} school=$school")
    }
}
