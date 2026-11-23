package no.navi.app

import android.os.SystemClock
import android.util.Log
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.AndroidComposeTestRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextClearance
import androidx.compose.ui.test.performTextInput
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.filters.LargeTest
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.rule.GrantPermissionRule
import androidx.test.uiautomator.By
import androidx.test.uiautomator.UiDevice
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TestWatcher
import org.junit.runner.Description
import org.junit.runner.RunWith
import java.io.File
import java.util.Locale

/**
 * UI Plan timing: St1 Espa → McDonald's Elverum and St1 Espa → Litjfossen.
 * Measures wall time from [btn_plan_route] click until [NaviMapTestHooks.lastPlanReport]
 * shows PASS with a corridor polyline (full MainActivity plan path).
 */
@RunWith(AndroidJUnit4::class)
@LargeTest
class EnrichPlanSpeedUiInstrumentedTest {
    @get:Rule(order = 0)
    val prefRule =
        object : TestWatcher() {
            override fun starting(description: Description) {
                val ctx = InstrumentationRegistry.getInstrumentation().targetContext
                // Avoid long-trip download waits; maps for these corridors are already on device.
                MapHudPrefs.saveLongTripEnabled(ctx, false)
                MapHudPrefs.saveSpeedCameraPromptShown(ctx, true)
            }
        }

    @get:Rule(order = 1)
    val launchRule = LaunchMainActivityKeepAliveRule()

    @get:Rule(order = 2)
    val composeRule = AndroidComposeTestRule(launchRule) { it.requireActivity() }

    @get:Rule(order = 3)
    val permissionRule: GrantPermissionRule =
        GrantPermissionRule.grant(
            android.Manifest.permission.ACCESS_FINE_LOCATION,
            android.Manifest.permission.ACCESS_COARSE_LOCATION,
        )

    private lateinit var device: UiDevice
    private lateinit var dataDir: File

    private val st1Espa = Triple("St1 Espa", 60.5621914, 11.2561239)
    private val mcdonaldsElverum = Triple("McDonald's Elverum", 60.8871362, 11.5154077)
    private val litjfossen = Triple("Litjfossen", 62.6692299, 10.1522021)

    @Before
    fun setUp() {
        device = UiDevice.getInstance(InstrumentationRegistry.getInstrumentation())
        dataDir = NaviAppData.resolve(composeRule.activity).also { it.mkdirs() }
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = false
        assumeMwmReady()
        settle(1_200)
    }

    @Test
    fun ui_plan_st1_espa_to_mcdonalds_elverum() {
        uiPlanAndMeasure(
            to = mcdonaldsElverum,
            reportName = "enrich_plan_speed_ui_espa_elverum.txt",
            logLabel = "ui_espa_elverum",
            minDistanceKm = 30.0,
        )
    }

    @Test
    fun ui_plan_st1_espa_to_litjfossen() {
        uiPlanAndMeasure(
            to = litjfossen,
            reportName = "enrich_plan_speed_ui_espa_litjfossen.txt",
            logLabel = "ui_espa_litjfossen",
            minDistanceKm = 200.0,
        )
    }

    private fun uiPlanAndMeasure(
        to: Triple<String, Double, Double>,
        reportName: String,
        logLabel: String,
        minDistanceKm: Double,
    ) {
        openRoutePanel()
        runCatching { clickTag("chip_profile_car") }

        typeCoordAndPickHit("chip_from", st1Espa.second, st1Espa.third, "from")
        typeCoordAndPickHit("chip_to", to.second, to.third, "to")

        // Warm-up plan so the timed run is not cold-session dominated.
        NaviMapTestHooks.lastPlanReport = ""
        NaviMapTestHooks.lastPlanDistanceKm = 0.0
        NaviMapTestHooks.lastRoutePolylineChars = 0
        composeRule.onNodeWithTag("btn_plan_route", useUnmergedTree = true).performScrollTo()
        clickTag("btn_plan_route")
        waitForPlanPass(timeoutMs = 120_000, minDistanceKm = minDistanceKm * 0.5)

        // Timed UI plan.
        NaviMapTestHooks.lastPlanReport = ""
        NaviMapTestHooks.lastPlanDistanceKm = 0.0
        NaviMapTestHooks.lastRoutePolylineChars = 0
        openRoutePanel()
        composeRule.onNodeWithTag("btn_plan_route", useUnmergedTree = true).performScrollTo()
        val t0 = SystemClock.elapsedRealtime()
        clickTag("btn_plan_route")
        val ok = waitForPlanPass(timeoutMs = 120_000, minDistanceKm = minDistanceKm)
        val planMs = SystemClock.elapsedRealtime() - t0

        val report = NaviMapTestHooks.lastPlanReport
        val distanceKm = NaviMapTestHooks.lastPlanDistanceKm
        File(dataDir, reportName).writeText(
            buildString {
                appendLine("path=compose_ui_btn_plan_route")
                appendLine("from=${st1Espa.first} ${st1Espa.second},${st1Espa.third}")
                appendLine("to=${to.first} ${to.second},${to.third}")
                appendLine("plan_ms=$planMs")
                appendLine("distance_km=${"%.3f".format(Locale.US, distanceKm)}")
                appendLine("polyline_chars=${NaviMapTestHooks.lastRoutePolylineChars}")
                appendLine("--- report ---")
                appendLine(report)
            },
        )
        Log.i(
            TAG,
            "$logLabel plan_ms=$planMs distance_km=$distanceKm " +
                "pass=${report.contains("PASS")} ok=$ok",
        )

        assertTrue("$logLabel planned", ok)
        assertTrue("$logLabel PASS\n${report.take(400)}", report.contains("PASS"))
        assertTrue(
            "$logLabel async enrich\n${report.take(600)}",
            report.contains("mwm_enrich_lookahead=async"),
        )
        assertTrue(
            "$logLabel plan_ms=$planMs must be <= 6000",
            planMs <= 6_000L,
        )
    }

    private fun waitForPlanPass(
        timeoutMs: Long,
        minDistanceKm: Double,
    ): Boolean {
        val deadline = SystemClock.elapsedRealtime() + timeoutMs
        while (SystemClock.elapsedRealtime() < deadline) {
            val rep = NaviMapTestHooks.lastPlanReport
            val dist = NaviMapTestHooks.lastPlanDistanceKm
            val poly = NaviMapTestHooks.lastRoutePolylineChars
            if (rep.contains("PASS") && dist >= minDistanceKm && poly > 100) {
                return true
            }
            if (rep.contains("FAIL:") && rep.length > 40) {
                Log.w(TAG, "plan fail early: ${rep.take(240)}")
                return false
            }
            Thread.sleep(200)
            composeRule.mainClock.advanceTimeBy(200)
        }
        return false
    }

    private fun assumeMwmReady() {
        assertTrue("MWM client flag", MwmClientBootstrap.isEnabled())
        assertTrue(
            "CoMaps session",
            ComapsSessionBootstrap.ensureOpen(composeRule.activity, force = false),
        )
    }

    private fun openRoutePanel() {
        val already =
            runCatching {
                composeRule.onNodeWithTag("field_search", useUnmergedTree = true).assertIsDisplayed()
                true
            }.getOrDefault(false)
        if (already) return
        clickTagSoft("btn_open_search")
        settle(400)
        runCatching { device.findObject(By.text("Route"))?.click() }
        settle(500)
        val deadline = SystemClock.elapsedRealtime() + 8_000
        while (SystemClock.elapsedRealtime() < deadline) {
            val ok =
                runCatching {
                    composeRule.onNodeWithTag("field_search", useUnmergedTree = true).assertIsDisplayed()
                    true
                }.getOrDefault(false)
            if (ok) return
            clickTagSoft("btn_open_search")
            runCatching { device.findObject(By.text("Route"))?.click() }
            settle(400)
        }
        composeRule.onNodeWithTag("field_search", useUnmergedTree = true).assertIsDisplayed()
    }

    private fun typeCoordAndPickHit(
        chipTag: String,
        lat: Double,
        lon: Double,
        step: String,
    ) {
        openRoutePanel()
        clickTag(chipTag)
        val q = String.format(Locale.US, "%.7f, %.7f", lat, lon)
        NaviMapTestHooks.lastSearchHitCount = -1
        NaviMapTestHooks.lastAppliedHitLat = Double.NaN
        setField("field_search", q)
        val deadline = SystemClock.elapsedRealtime() + 20_000
        while (SystemClock.elapsedRealtime() < deadline) {
            if (NaviMapTestHooks.lastSearchHitCount >= 1) break
            composeRule.mainClock.advanceTimeBy(200)
            Thread.sleep(200)
        }
        assertTrue(
            "$step: coordinate hits for $q count=${NaviMapTestHooks.lastSearchHitCount}",
            NaviMapTestHooks.lastSearchHitCount >= 1,
        )
        clickTag("search_hit_0")
        val applyDeadline = SystemClock.elapsedRealtime() + 12_000
        while (SystemClock.elapsedRealtime() < applyDeadline) {
            if (!NaviMapTestHooks.lastAppliedHitLat.isNaN()) break
            Thread.sleep(100)
        }
    }

    private fun clickTag(tag: String) {
        val node = composeRule.onNodeWithTag(tag, useUnmergedTree = true)
        runCatching { node.performScrollTo() }
        node.assertExists().performClick()
        settle()
    }

    private fun clickTagSoft(tag: String) {
        runCatching {
            val node = composeRule.onNodeWithTag(tag, useUnmergedTree = true)
            runCatching { node.performScrollTo() }
            node.assertExists().performClick()
            settle()
        }
    }

    private fun setField(
        tag: String,
        value: String,
    ) {
        val node = composeRule.onNodeWithTag(tag, useUnmergedTree = true)
        runCatching { node.performScrollTo() }
        node.performTextClearance()
        node.performTextInput(value)
        settle()
    }

    private fun settle(ms: Long = 250) {
        Thread.sleep(ms)
        composeRule.waitForIdle()
    }

    companion object {
        private const val TAG = "EnrichPlanSpeedUi"
    }
}
