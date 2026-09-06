package no.navi.app

import android.util.Log
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
import uniffi.navi.planHikingRoute
import java.io.File
import kotlin.math.asin
import kotlin.math.cos
import kotlin.math.pow
import kotlin.math.sin
import kotlin.math.sqrt

/**
 * Regression: Car must not use Hamar Torggata (way 25968694, motor_vehicle=no).
 * Keyboard From/To as lat, lon on SM-P613 with Ostlandet data.
 */
@RunWith(AndroidJUnit4::class)
class MotorAccessBarrierInstrumentedTest {
    companion object {
        private const val TAG = "MotorAccessFix"
        private const val START_LAT = 60.7915000
        private const val START_LON = 11.0769500
        private const val END_LAT = 60.7923500
        private const val END_LON = 11.0761000
        private val TORGGATA_MID = 60.79195 to 11.07652

        // Kirkebyskogen corridor across bollard 879594792
        private const val KIRK_START_LAT = 60.7782000
        private const val KIRK_START_LON = 10.6868000
        private const val KIRK_END_LAT = 60.7779000
        private const val KIRK_END_LON = 10.6890000
        private val BOLLARD = 60.7780734 to 10.6878354

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
            return 2.0 * 6_378_100.0 * asin(sqrt(h))
        }

        private fun minDistToPolylineM(
            polyline: String,
            lat: Double,
            lon: Double,
        ): Double {
            if (polyline.isBlank()) return Double.POSITIVE_INFINITY
            var best = Double.POSITIVE_INFINITY
            for (part in polyline.split(';')) {
                val bits = part.split(',')
                if (bits.size < 2) continue
                val plo = bits[0].toDoubleOrNull() ?: continue
                val pla = bits[1].toDoubleOrNull() ?: continue
                best = minOf(best, haversineM(lat, lon, pla, plo))
            }
            return best
        }

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
                    ?: error("need ostlandet-latest.osm.pbf on device")

            val elev = File(dataDir, "elevation").absolutePath
            val carCache = File(dataDir, "graph-cache-${pbf.nameWithoutExtension}-car")
            carCache.mkdirs()
            val warm =
                planCarRoute(
                    pbf.absolutePath,
                    elev,
                    carCache.absolutePath,
                    START_LAT,
                    START_LON,
                    END_LAT,
                    END_LON,
                    false,
                    TravelProfile.CAR,
                    false,
                    uniffi.navi.FfiTollPolicy.ALLOW,
                    false,
                    FfiVehicleLimits(null, null, null, null, null, null),
                    false,
                    dataDir = "",
                )
            check(warm.report.contains("PASS")) { "car prewarm failed: ${warm.report}" }
            val near = minDistToPolylineM(warm.routePolyline, TORGGATA_MID.first, TORGGATA_MID.second)
            Log.i(
                TAG,
                "prewarm_car_torggata km=${warm.distanceKm} near_mid_m=$near " +
                    "direct_carriageway_km=0.066",
            )
            // Direct Torggata chord is ~66 m. A correct car detour must be longer and
            // must not reuse that carriageway (host test asserts way id omitted).
            check(warm.distanceKm > 0.12) {
                "Car must detour around Torggata motor_vehicle=no; km=${warm.distanceKm} near_mid_m=$near"
            }
            // On-carriageway polylines sit within ~20 m of mid; adjacent-street detours
            // in this dense grid can still be ~20–40 m away, so distance is the gate.
            check(near > 12.0 || warm.distanceKm > 0.15) {
                "Car polyline still hugs Torggata mid (near_mid_m=$near km=${warm.distanceKm})"
            }
        }
    }

    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

    @Before
    fun setUp() {
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

    private fun selectProfile(chipTag: String) {
        composeRule.onNodeWithTag("profile_menu", useUnmergedTree = true).performScrollTo()
        runCatching { clickTag("btn_open_profile") }
        clickTag(chipTag)
        runCatching { clickTag("btn_save_profile") }
        Thread.sleep(400)
    }

    private fun openRoutePanel() {
        val styleDeadline = System.currentTimeMillis() + 60_000
        while (System.currentTimeMillis() < styleDeadline && !NaviMapTestHooks.styleReady) {
            Thread.sleep(400)
        }
        assertTrue("style ready", NaviMapTestHooks.styleReady)
        NaviMapTestHooks.hideSearchChrome = false
        Thread.sleep(1_000)
        runCatching {
            composeRule.onNodeWithTag("field_search", useUnmergedTree = true).assertIsDisplayed()
        }.onFailure { clickTag("btn_open_search") }
        composeRule.onNodeWithTag("field_search", useUnmergedTree = true).assertIsDisplayed()
    }

    @Test
    fun keyboard_car_detours_torggata_motor_vehicle_no() {
        openRoutePanel()
        selectProfile("chip_profile_car")
        typeCoordAndPickHit("chip_from", START_LAT, START_LON)
        typeCoordAndPickHit("chip_to", END_LAT, END_LON)
        composeRule.onNodeWithTag("btn_plan_route", useUnmergedTree = true).performScrollTo()
        clickTag("btn_plan_route")

        val deadline = System.currentTimeMillis() + 180_000
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.lastPlanReport.contains("PASS") ||
                NaviMapTestHooks.lastPlanReport.contains("FAIL")
            ) {
                break
            }
            Thread.sleep(500)
        }
        assertTrue(
            "plan must PASS: ${NaviMapTestHooks.lastPlanReport.take(400)}",
            NaviMapTestHooks.lastPlanReport.contains("PASS"),
        )
        val near =
            minDistToPolylineM(
                NaviMapTestHooks.lastRoutePolyline,
                TORGGATA_MID.first,
                TORGGATA_MID.second,
            )
        Log.i(
            TAG,
            "UI_CAR torggata km=${NaviMapTestHooks.lastPlanDistanceKm} near_mid_m=$near",
        )
        assertTrue(
            "Car must detour around Torggata (km=${NaviMapTestHooks.lastPlanDistanceKm})",
            NaviMapTestHooks.lastPlanDistanceKm > 0.12,
        )
        assertTrue(
            "Car polyline should not sit on the banned carriageway (near_mid_m=$near)",
            near > 12.0 || NaviMapTestHooks.lastPlanDistanceKm > 0.15,
        )
    }

    @Test
    fun native_hiking_still_uses_kirkebyskogen_corridor() {
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        val dataDir = NaviAppData.resolve(ctx)
        val pbf =
            listOf(
                File(dataDir, "ostlandet-latest.osm.pbf"),
                File("/data/local/tmp/navi_fixtures/ostlandet-latest.osm.pbf"),
            ).firstOrNull { it.isFile && it.length() > 1_000_000 }
                ?: error("need ostlandet pbf")
        val elev = File(dataDir, "elevation").absolutePath
        val cache = File(dataDir, "graph-cache-${pbf.nameWithoutExtension}-foot")
        cache.mkdirs()
        val waypoints =
            """[{"name":"A","lat":$KIRK_START_LAT,"lon":$KIRK_START_LON},""" +
                """{"name":"B","lat":$KIRK_END_LAT,"lon":$KIRK_END_LON}]"""
        val hike =
            planHikingRoute(
                pbf.absolutePath,
                elev,
                cache.absolutePath,
                waypoints,
                false,
                false,
                dataDir = "",
            )
        assertTrue("hiking PASS: ${hike.report.take(300)}", hike.report.contains("PASS"))
        val near = minDistToPolylineM(hike.routePolyline, BOLLARD.first, BOLLARD.second)
        Log.i(TAG, "native_hike_kirkeby near_bollard_m=$near km=${hike.distanceKm}")
        assertTrue(
            "Hiking should still pass near motor-only bollard (near_m=$near)",
            near < 40.0,
        )
    }

    @Test
    fun native_car_avoids_kirkebyskogen_motor_ban() {
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        val dataDir = NaviAppData.resolve(ctx)
        val pbf =
            listOf(
                File(dataDir, "ostlandet-latest.osm.pbf"),
                File("/data/local/tmp/navi_fixtures/ostlandet-latest.osm.pbf"),
            ).firstOrNull { it.isFile && it.length() > 1_000_000 }
                ?: error("need ostlandet pbf")
        val elev = File(dataDir, "elevation").absolutePath
        val cache = File(dataDir, "graph-cache-${pbf.nameWithoutExtension}-car")
        cache.mkdirs()
        val car =
            planCarRoute(
                pbf.absolutePath,
                elev,
                cache.absolutePath,
                KIRK_START_LAT,
                KIRK_START_LON,
                KIRK_END_LAT,
                KIRK_END_LON,
                false,
                TravelProfile.CAR,
                false,
                uniffi.navi.FfiTollPolicy.ALLOW,
                false,
                FfiVehicleLimits(null, null, null, null, null, null),
                false,
                dataDir = "",
            )
        assertTrue("car PASS: ${car.report.take(300)}", car.report.contains("PASS"))
        val near = minDistToPolylineM(car.routePolyline, BOLLARD.first, BOLLARD.second)
        Log.i(TAG, "native_car_kirkeby near_bollard_m=$near km=${car.distanceKm}")
        assertFalse(
            "Car must not pass through Kirkebyskogen bollard corridor (near_m=$near)",
            near < 25.0,
        )
    }
}
