package no.navi.app

import android.graphics.Bitmap
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
import org.json.JSONObject
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.BeforeClass
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import kotlin.math.asin
import kotlin.math.cos
import kotlin.math.min
import kotlin.math.pow
import kotlin.math.sin
import kotlin.math.sqrt

/**
 * Real corridor: Fv1890 through Vallset (Stange, not Hamar) past Vallset skole.
 *
 * OSM has no `traffic_sign=NO:142` / `hazard=children` in the Østlandet extract.
 * The adjacent-road warning that *is* tagged and catalogue-matched is `NO:109`
 * (speed hump) on Skalbergvegen next to the school.
 */
@RunWith(AndroidJUnit4::class)
class RoadSignSchoolCorridorInstrumentedTest {
    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

    private lateinit var device: UiDevice

    companion object {
        private const val TAG = "RoadSignSchool"
        private const val OUT = "/data/local/tmp/navi_road_sign_school"

        // Skalbergvegen (Fv1890) north of the 30-zone, through Vallset.
        private val START = 60.68420 to 11.34080

        // South-east of Vallset skole along the local corridor.
        private val END = 60.67850 to 11.34400

        // OSM node 9988748740: traffic_sign=NO:109,802[50 m];362.40
        private val SIGN_109 = 60.6801722 to 11.3458249
        private val SCHOOL_CENTER = 60.6811042 to 11.3422291

        @JvmStatic
        @BeforeClass
        fun beforeClass() {
            val ctx = InstrumentationRegistry.getInstrumentation().targetContext
            val auto = InstrumentationRegistry.getInstrumentation().uiAutomation
            auto.grantRuntimePermission(ctx.packageName, android.Manifest.permission.ACCESS_FINE_LOCATION)
            auto.grantRuntimePermission(ctx.packageName, android.Manifest.permission.ACCESS_COARSE_LOCATION)
            val dataDir = NaviAppData.resolve(ctx).also { it.mkdirs() }
            OstlandetOfflineFixtures.ensureInstalled(dataDir)
            auto.executeShellCommand("mkdir -p $OUT && chmod 777 $OUT").close()
        }
    }

    @Before
    fun setUp() {
        device = UiDevice.getInstance(InstrumentationRegistry.getInstrumentation())
        NaviMapTestHooks.disableGpsFollow = true
        NaviMapTestHooks.followGps = false
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = false
        NaviMapTestHooks.forceOnlineBasemap = false
        NaviMapTestHooks.ignoreLiveGpsFixes = false
        NaviMapTestHooks.requestStopRouteSimulation = true
        Thread.sleep(300)
        NaviMapTestHooks.requestStopRouteSimulation = false
        NaviMapTestHooks.simulatingActive = false
        NaviMapTestHooks.lastRoadSignWarningJson = "{}"
        dismissPermission()
    }

    private fun shell(cmd: String) {
        val pfd = InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand(cmd)
        java.io.FileInputStream(pfd.fileDescriptor).use { input ->
            val buf = ByteArray(4096)
            while (input.read(buf) >= 0) {
            }
        }
        pfd.close()
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

    private fun openRoutePanel() {
        runCatching {
            composeRule.onNodeWithTag("field_search", useUnmergedTree = true).assertIsDisplayed()
        }.onFailure {
            clickTag("btn_open_search")
        }
        composeRule.onNodeWithTag("field_search", useUnmergedTree = true).assertIsDisplayed()
    }

    private fun typeCoordAndPickHit(
        chipTag: String,
        lat: Double,
        lon: Double,
    ) {
        clickTag(chipTag)
        val q = String.format(java.util.Locale.US, "%.7f, %.7f", lat, lon)
        setField("field_search", q)
        val deadline = System.currentTimeMillis() + 10_000
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.lastSearchHitCount >= 1) break
            Thread.sleep(200)
        }
        assertTrue(
            "coordinate hit for $chipTag q=$q hits=${NaviMapTestHooks.lastSearchHitCount}",
            NaviMapTestHooks.lastSearchHitCount >= 1,
        )
        clickTag("search_hit_0")
        Thread.sleep(600)
        NaviMapTestHooks.disableGpsFollow = true
        NaviMapTestHooks.followGps = false
    }

    private fun selectProfile(chip: String) {
        runCatching { clickTag("btn_open_profile") }
        clickTag(chip)
        runCatching { clickTag("btn_save_profile") }
        Thread.sleep(400)
    }

    private fun planAndWait(timeoutMs: Long = 900_000) {
        NaviMapTestHooks.lastRoutePolylineChars = 0
        NaviMapTestHooks.lastPlanReport = ""
        composeRule.onNodeWithTag("btn_plan_route", useUnmergedTree = true).performScrollTo()
        clickTag("btn_plan_route")
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.lastRoutePolylineChars > 100 &&
                NaviMapTestHooks.lastPlanReport.contains("PASS")
            ) {
                return
            }
            Thread.sleep(2_000)
            Log.i(
                TAG,
                "waiting_plan chars=${NaviMapTestHooks.lastRoutePolylineChars} " +
                    "report=${NaviMapTestHooks.lastPlanReport.take(120)}",
            )
        }
        error("plan timeout report=${NaviMapTestHooks.lastPlanReport.take(500)}")
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

    private fun seekNearSchoolFallback() {
        val samples = parseRouteSimSamples(NaviMapTestHooks.lastSimSamplesJson)
        assertTrue("need sim samples", samples.size >= 5)
        val closest = samples.minBy { haversineM(it.lat, it.lon, SCHOOL_CENTER.first, SCHOOL_CENTER.second) }
        val distToSchool = haversineM(closest.lat, closest.lon, SCHOOL_CENTER.first, SCHOOL_CENTER.second)
        assertTrue("planned corridor must pass school area (got ${distToSchool}m)", distToSchool < 220.0)
        val seekM = (closest.cumM - 50.0).coerceAtLeast(0.0)
        Log.i(TAG, "seek_school cumM=$seekM sampleDistToSchool=$distToSchool alongTarget=${closest.cumM}")
        NaviMapTestHooks.disableGpsFollow = true
        NaviMapTestHooks.followGps = false
        NaviMapTestHooks.simulationTimeScale = 0.05
        runCatching { clickTag("btn_simulate_route") }
            .onFailure {
                NaviMapTestHooks.requestPrepareRouteSimulation = true
                Thread.sleep(800)
                NaviMapTestHooks.requestStartRouteSimulation = true
            }
        NaviMapTestHooks.requestSimSeekCumM = seekM
        val deadline = System.currentTimeMillis() + 45_000
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.simulatingActive &&
                NaviMapTestHooks.lastRoadSignWarningJson.contains("\"code\":\"142\"") &&
                NaviMapTestHooks.lastSchoolProximityWarningJson.contains("\"source\":\"children_proximity\"")
            ) {
                return
            }
            Thread.sleep(300)
        }
        error(
            "no school fallback warning json=${NaviMapTestHooks.lastRoadSignWarningJson} " +
                "fallback=${NaviMapTestHooks.lastSchoolProximityWarningJson} " +
                "sim=${NaviMapTestHooks.simulatingActive} along=${NaviMapTestHooks.lastSimAlongM} " +
                "gps=${NaviMapTestHooks.lastGpsLat},${NaviMapTestHooks.lastGpsLon} " +
                "indexed=${NaviMapTestHooks.lastRoadSignsIndexed}",
        )
    }

    private fun closeChromeForMapShot() {
        composeRule.activity.runOnUiThread {
            val imm =
                composeRule.activity.getSystemService(android.content.Context.INPUT_METHOD_SERVICE)
                    as android.view.inputmethod.InputMethodManager
            composeRule.activity.currentFocus?.let { imm.hideSoftInputFromWindow(it.windowToken, 0) }
                ?: imm.hideSoftInputFromWindow(composeRule.activity.window.decorView.windowToken, 0)
        }
        Thread.sleep(400)
        device.executeShellCommand("input keyevent 111")
        runCatching { clickTag("btn_close_search") }
        NaviMapTestHooks.hideSearchChrome = true
        Thread.sleep(800)
    }

    private fun shot(relative: String) {
        val path = "$OUT/$relative"
        shell("mkdir -p $(dirname $path)")
        shell("screencap -p $path")
        shell("chmod 644 $path")
        val f = File(path)
        if (!f.isFile || f.length() < 8_000) {
            val bmp = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
            f.parentFile?.mkdirs()
            f.outputStream().use { bmp.compress(Bitmap.CompressFormat.PNG, 100, it) }
        }
        assertTrue("$relative too small (${f.length()})", f.isFile && f.length() > 40_000)
        Log.i(TAG, "SHOT $relative bytes=${f.length()} warn=${NaviMapTestHooks.lastRoadSignWarningJson}")
    }

    @Test
    fun vallset_skole_proximity_school_warning_on_real_route() {
        waitStyle()
        openRoutePanel()
        selectProfile("chip_profile_car")
        typeCoordAndPickHit("chip_from", START.first, START.second)
        typeCoordAndPickHit("chip_to", END.first, END.second)
        val loadDeadline = System.currentTimeMillis() + 120_000
        while (
            System.currentTimeMillis() < loadDeadline &&
            (NaviMapTestHooks.lastRoadSignsIndexed < 1 || NaviMapTestHooks.lastSchoolPoisIndexed < 1)
        ) {
            Log.i(
                TAG,
                "waiting indexes signs=${NaviMapTestHooks.lastRoadSignsIndexed} schools=${NaviMapTestHooks.lastSchoolPoisIndexed}",
            )
            Thread.sleep(2_000)
        }
        assertTrue(
            "road signs must be indexed before simulate n=${NaviMapTestHooks.lastRoadSignsIndexed}",
            NaviMapTestHooks.lastRoadSignsIndexed >= 1,
        )
        assertTrue(
            "school POIs must be indexed before simulate n=${NaviMapTestHooks.lastSchoolPoisIndexed}",
            NaviMapTestHooks.lastSchoolPoisIndexed >= 1,
        )
        planAndWait(900_000)
        Log.i(TAG, "plan ${NaviMapTestHooks.lastPlanReport.take(240)} km=${NaviMapTestHooks.lastPlanDistanceKm}")
        seekNearSchoolFallback()
        closeChromeForMapShot()
        composeRule.onNodeWithTag("road_sign_warning_box", useUnmergedTree = true).assertIsDisplayed()
        val warn = JSONObject(NaviMapTestHooks.lastRoadSignWarningJson)
        assertTrue("expected 142, got $warn", warn.optString("code") == "142")
        assertTrue(
            "icon_key ${warn.optString("icon_key")}",
            warn.optString("icon_key") == "no_sign_142",
        )
        val phase = warn.optString("phase")
        assertTrue("phase=$phase", phase == "appear" || phase == "urgency")
        val dist = warn.optDouble("distance_m")
        assertTrue("distance $dist not in approach band", dist > 25.0 && dist <= 750.0)
        assertTrue("expected children-proximity source $warn", warn.optString("source") == "children_proximity")
        Log.i(TAG, "school fallback warning ok phase=$phase dist=$dist")
        shot("vallset_skole_school_proximity_142.png")
    }
}
