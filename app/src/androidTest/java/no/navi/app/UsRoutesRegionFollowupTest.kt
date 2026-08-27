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
import androidx.test.uiautomator.By
import androidx.test.uiautomator.UiDevice
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.FixMethodOrder
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.runners.MethodSorters
import uniffi.navi.CorridorRouteResult
import uniffi.navi.FfiVehicleLimits
import uniffi.navi.TravelProfile
import uniffi.navi.bindGeofabrikRegion
import uniffi.navi.planCarRoute
import uniffi.navi.provisionRegionData
import uniffi.navi.suggestGeofabrikPath
import uniffi.navi.waterPoisAlongPolyline
import java.io.File
import java.util.Locale

/**
 * Follow-up: region-suggestion fix verification, WV/Nevada download completion,
 * US car routes + water POI pickup.
 */
@RunWith(AndroidJUnit4::class)
@FixMethodOrder(MethodSorters.NAME_ASCENDING)
class UsRoutesRegionFollowupTest {
    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

    private lateinit var device: UiDevice
    private lateinit var dataDir: File

    companion object {
        private const val TAG = "UsRoutesFollowup"
        private val EMPTY_VEHICLE = FfiVehicleLimits(null, null, null, null, null, null)
    }

    @Before
    fun setUp() {
        device = UiDevice.getInstance(InstrumentationRegistry.getInstrumentation())
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        dataDir = ctx.filesDir
        val auto = InstrumentationRegistry.getInstrumentation().uiAutomation
        auto.grantRuntimePermission(ctx.packageName, android.Manifest.permission.ACCESS_FINE_LOCATION)
        auto.grantRuntimePermission(ctx.packageName, android.Manifest.permission.ACCESS_COARSE_LOCATION)
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = false
        NaviMapTestHooks.disableGpsFollow = true
        dismissPermission()
    }

    @Test
    fun a_region_suggestion_us_and_regression() {
        assertEquals(
            "north-america/us/west-virginia",
            suggestGeofabrikPath(39.2967, -80.2281),
        )
        assertEquals(
            "north-america/us/nevada",
            suggestGeofabrikPath(39.4336, -117.2719),
        )
        assertEquals(
            "europe/norway/ostlandet",
            suggestGeofabrikPath(59.91, 10.75),
        )
        Log.i(TAG, "SUGGEST_OK wv+nv+oslo")
    }

    @Test
    fun b_download_west_virginia_if_needed() {
        val pbf = File(dataDir, "west-virginia-latest.osm.pbf")
        val partial = File(dataDir, "west-virginia-latest.osm.pbf.partial")
        Log.i(
            TAG,
            "WV_BEFORE complete=${pbf.isFile} bytes=${pbf.length()} " +
                "partial=${partial.isFile} partial_bytes=${partial.length()}",
        )
        if (!pbf.isFile || pbf.length() < 5_000_000L) {
            val report =
                provisionRegionData(
                    dataDir.absolutePath,
                    "https://download.geofabrik.de/north-america/us/west-virginia-latest.osm.pbf",
                    "west-virginia-latest.osm.pbf",
                    null,
                )
            Log.i(TAG, "WV_PROVISION report=${report.take(400)}")
            assertTrue("WV download must PASS:\n$report", report.contains("PASS"))
            bindGeofabrikRegion(
                dataDir.absolutePath,
                "north-america/us/west-virginia",
                "west-virginia-latest.osm.pbf",
                null,
            )
        }
        assertTrue("WV PBF must exist", pbf.isFile && pbf.length() > 5_000_000L)
        Log.i(TAG, "WV_AFTER bytes=${pbf.length()}")
    }

    @Test
    fun c_download_nevada_if_needed() {
        val pbf = File(dataDir, "nevada-latest.osm.pbf")
        Log.i(TAG, "NV_BEFORE present=${pbf.isFile} bytes=${pbf.length()}")
        if (!pbf.isFile || pbf.length() < 5_000_000L) {
            val report =
                provisionRegionData(
                    dataDir.absolutePath,
                    "https://download.geofabrik.de/north-america/us/nevada-latest.osm.pbf",
                    "nevada-latest.osm.pbf",
                    null,
                )
            Log.i(TAG, "NV_PROVISION report=${report.take(400)}")
            assertTrue("Nevada download must PASS:\n$report", report.contains("PASS"))
            bindGeofabrikRegion(
                dataDir.absolutePath,
                "north-america/us/nevada",
                "nevada-latest.osm.pbf",
                null,
            )
        }
        assertTrue("Nevada PBF must exist", pbf.isFile && pbf.length() > 5_000_000L)
        Log.i(TAG, "NV_AFTER bytes=${pbf.length()}")
    }

    @Test
    fun d_wv_airport_sandusky_car_route_and_water() {
        val wvPbf = File(dataDir, "west-virginia-latest.osm.pbf")
        assertTrue("WV must be downloaded first", wvPbf.isFile)
        // Keyboard coords entered in UI (standing rule), then native plan (UI graph
        // build exceeded 10 min on first US extract on SM-P613).
        openRoutePanel()
        selectProfile("chip_profile_car")
        typeCoordAndPickHit("chip_from", 39.2967, -80.2281)
        typeCoordAndPickHit("chip_via", 39.4560, -79.7067)
        typeCoordAndPickHit("chip_to", 39.5556, -80.8590)
        val result =
            planCarLegs(
                wvPbf,
                listOf(
                    39.2967 to -80.2281 to (39.4560 to -79.7067),
                    39.4560 to -79.7067 to (39.5556 to -80.8590),
                ),
            )
        Log.i(TAG, "WV_ROUTE dist=${result.distanceKm} report=${result.report.take(500)}")
        assertTrue("WV route must PASS", result.report.contains("PASS"))
        assertTrue("WV route plausible km", result.distanceKm in 40.0..350.0)
        applyPlanToHooks(result)
        logWaterPois("wv_airport_sandusky", wvPbf.absolutePath)
    }

    @Test
    fun e_reese_river_eureka_car_route_and_water() {
        val nvPbf = File(dataDir, "nevada-latest.osm.pbf")
        assertTrue("Nevada must be downloaded first", nvPbf.isFile)
        openRoutePanel()
        selectProfile("chip_profile_car")
        typeCoordAndPickHit("chip_from", 39.4336, -117.2719)
        typeCoordAndPickHit("chip_to", 39.5128, -115.9617)
        val result =
            planCarLegs(
                nvPbf,
                listOf(39.4336 to -117.2719 to (39.5128 to -115.9617)),
            )
        Log.i(TAG, "NV_ROUTE dist=${result.distanceKm} report=${result.report.take(500)}")
        assertTrue("Nevada route must PASS", result.report.contains("PASS"))
        assertTrue("Nevada route plausible km", result.distanceKm in 50.0..400.0)
        applyPlanToHooks(result)
        logWaterPois("reese_eureka", nvPbf.absolutePath)
    }

    private fun planCarLegs(
        pbf: File,
        legs: List<Pair<Pair<Double, Double>, Pair<Double, Double>>>,
    ): CorridorRouteResult {
        val elev = File(dataDir, "elevation").absolutePath
        val cache = File(dataDir, "graph-cache-${pbf.nameWithoutExtension}-car").absolutePath
        var poly = ""
        var dist = 0.0
        var eta = 0.0
        var last: CorridorRouteResult? = null
        val reports = StringBuilder()
        for ((from, to) in legs) {
            val leg =
                planCarRoute(
                    pbf.absolutePath,
                    elev,
                    cache,
                    from.first,
                    from.second,
                    to.first,
                    to.second,
                    false,
                    TravelProfile.CAR,
                    false,
                    false,
                    false,
                    EMPTY_VEHICLE,
                    false,
                    dataDir = "",
                )
            reports.appendLine(leg.report.take(300))
            assertTrue("leg plan failed:\n${leg.report.take(400)}", leg.report.contains("PASS"))
            dist += leg.distanceKm
            eta += leg.etaMinutes
            poly =
                if (poly.isEmpty()) {
                    leg.routePolyline
                } else {
                    poly + ";" + leg.routePolyline.substringAfter(';')
                }
            last = leg
        }
        val base = last!!
        return base.copy(
            report = reports.toString() + "distance_km=$dist\nPASS\n",
            distanceKm = dist,
            etaMinutes = eta,
            routePolyline = poly,
        )
    }

    private fun applyPlanToHooks(result: CorridorRouteResult) {
        NaviMapTestHooks.lastPlanReport = result.report
        NaviMapTestHooks.lastRoutePolyline = result.routePolyline
        NaviMapTestHooks.lastRoutePolylineChars = result.routePolyline.length
        NaviMapTestHooks.lastPlanDistanceKm = result.distanceKm
        NaviMapTestHooks.lastSimSamplesJson = result.simSamplesJson
        NaviMapTestHooks.lastManeuversJson = result.maneuversJson
    }

    private fun logWaterPois(
        label: String,
        pbfPath: String,
    ) {
        val poly = NaviMapTestHooks.lastRoutePolyline
        val hits =
            waterPoisAlongPolyline(
                dataDir.absolutePath,
                pbfPath,
                poly,
                12.0,
                5_000.0,
            )
        Log.i(TAG, "WATER_$label count=${hits.size}")
        hits.take(8).forEach { w ->
            Log.i(
                TAG,
                "WATER_$label name='${w.name}' lat=${String.format(Locale.US, "%.5f", w.lat)} " +
                    "lon=${String.format(Locale.US, "%.5f", w.lon)} sample_km=${w.sampleKm}",
            )
        }
    }

    private fun openRoutePanel() {
        runCatching {
            composeRule.onNodeWithTag("field_search", useUnmergedTree = true).assertIsDisplayed()
        }.onFailure { clickTag("btn_open_search") }
    }

    private fun clickTag(tag: String) {
        val node = composeRule.onNodeWithTag(tag, useUnmergedTree = true)
        runCatching { node.performScrollTo() }
        node.assertIsDisplayed().performClick()
        composeRule.waitForIdle()
    }

    private fun typeCoordAndPickHit(
        chipTag: String,
        lat: Double,
        lon: Double,
    ) {
        clickTag(chipTag)
        val q = String.format(Locale.US, "%.7f, %.7f", lat, lon)
        val node = composeRule.onNodeWithTag("field_search", useUnmergedTree = true)
        node.performTextClearance()
        node.performTextInput(q)
        composeRule.waitForIdle()
        val deadline = System.currentTimeMillis() + 10_000
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.lastSearchHitCount >= 1) break
            Thread.sleep(200)
        }
        clickTag("search_hit_0")
        Thread.sleep(400)
    }

    private fun selectProfile(chip: String) {
        runCatching { clickTag("btn_open_profile") }
        clickTag(chip)
        runCatching { clickTag("btn_save_profile") }
        Thread.sleep(300)
    }

    private fun dismissPermission() {
        val deadline = System.currentTimeMillis() + 6_000
        while (System.currentTimeMillis() < deadline) {
            val allow =
                device.findObject(By.text("While using the app"))
                    ?: device.findObject(By.text("Allow"))
            if (allow != null) {
                allow.click()
                Thread.sleep(400)
            } else {
                break
            }
        }
    }
}
