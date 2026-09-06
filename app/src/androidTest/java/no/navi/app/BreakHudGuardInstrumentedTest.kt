package no.navi.app

import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertTextContains
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithTag
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.BeforeClass
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.CorridorRouteResult

/**
 * Hard rule: no break countdown (time or distance) without an active planned route.
 * Uses the host-planned Grimåsfeltet→Nysethvegen polyline — never a synthetic stub.
 */
@RunWith(AndroidJUnit4::class)
class BreakHudGuardInstrumentedTest {
    companion object {
        @JvmStatic
        @BeforeClass
        fun beforeClass() {
            val pkg = InstrumentationRegistry.getInstrumentation().targetContext.packageName
            runCatching {
                InstrumentationRegistry
                    .getInstrumentation()
                    .uiAutomation
                    .grantRuntimePermission(pkg, android.Manifest.permission.ACCESS_FINE_LOCATION)
            }
            NaviMapTestHooks.hideUiChrome = false
            NaviMapTestHooks.hideSearchChrome = true
            NaviMapTestHooks.pendingRoute = null
            NaviMapTestHooks.requestClearRoute = false
            NaviMapTestHooks.styleReady = false
        }
    }

    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

    @Before
    fun setUp() {
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = true
        NaviMapTestHooks.gpsAltitudeM = 412.0
        NaviMapTestHooks.pendingRoute = null
        NaviMapTestHooks.requestClearRoute = false
        NaviMapTestHooks.pendingCamera = Triple(60.722, 10.613, 13.0)
        MapHudPrefs.saveBreakAsDistance(ctx, false)
        MapHudPrefs.savePreferMetric(ctx, true)
    }

    private fun waitStyle() {
        val deadline = System.currentTimeMillis() + 25_000
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.styleReady) break
            Thread.sleep(200)
        }
        assertTrue("MapLibre style not ready", NaviMapTestHooks.styleReady)
        Thread.sleep(800)
    }

    private fun loadPlannedPolyline(): String {
        val ctx = InstrumentationRegistry.getInstrumentation().context
        return ctx.assets
            .open("raufoss_grimafeltet_nysethvegen.polyline.txt")
            .bufferedReader()
            .use { it.readText().trim() }
            .also {
                assertTrue(it.contains(';'))
                assertTrue(it.count { c -> c == ';' } >= 10)
            }
    }

    private fun injectRealRoute() {
        val polyline = loadPlannedPolyline()
        NaviMapTestHooks.pendingRoute =
            CorridorRouteResult(
                report = "PLANNED Grimåsfeltet → Nysethvegen (Raufoss / Tollerud)",
                distanceKm = 1.953,
                etaMinutes = 1.953 / 50.0 * 60.0,
                cacheHit = true,
                coldBuildS = 0.0,
                warmLoadS = 0.0,
                routePolyline = polyline,
                poiLat = 60.7278207,
                poiLon = 10.6049538,
                poiName = "Nysethvegen",
                poiIconKey = "fuel",
                breakPoisJson = "[]",
                daysJson = "[]",
                simSamplesJson = "[]",
                maneuversJson = "[]",
                priorityPathSharePct = 0.0,
                routeSegmentsJson = "[]",
                offTrailAdvisory = "",
                tollPolicy = "allow",
                padAttemptsJson = "[]",
                searchExpansions = 0u,
                searchTerminateReason = "fail",
                tollAvoidanceIncomplete = false,
                routeUsesTolls = false,
            )
        NaviMapTestHooks.pendingCamera = Triple(60.722, 10.613, 13.0)
        Thread.sleep(1_800)
    }

    private fun assertNoBreakHud() {
        composeRule
            .onAllNodesWithTag("hud_break_countdown", useUnmergedTree = true)
            .assertCountEquals(0)
        assertTrue(
            composeRule
                .onAllNodesWithText("Break in", substring = true, useUnmergedTree = true)
                .fetchSemanticsNodes()
                .isEmpty(),
        )
        assertTrue(
            composeRule
                .onAllNodesWithText("Break reminders off", useUnmergedTree = true)
                .fetchSemanticsNodes()
                .isEmpty(),
        )
        assertFalse(NaviMapTestHooks.lastBreakHudVisible)
        assertNull(NaviMapTestHooks.lastMinutesToBreak)
    }

    @Test
    fun formatBreakHudLine_nullWithoutRoute() {
        assertNull(
            formatBreakHudLine(
                routePlanned = false,
                breakRemindersEnabled = true,
                minutesToBreak = 240.0,
                breakAsDistance = false,
                preferMetric = true,
            ),
        )
        assertNull(
            formatBreakHudLine(
                routePlanned = false,
                breakRemindersEnabled = true,
                minutesToBreak = 240.0,
                breakAsDistance = true,
                preferMetric = true,
            ),
        )
        assertEquals(
            "Break in 240 min",
            formatBreakHudLine(
                routePlanned = true,
                breakRemindersEnabled = true,
                minutesToBreak = 240.0,
                breakAsDistance = false,
                preferMetric = true,
            ),
        )
        assertEquals(
            "Break in 320 km",
            formatBreakHudLine(
                routePlanned = true,
                breakRemindersEnabled = true,
                minutesToBreak = 240.0,
                breakAsDistance = true,
                preferMetric = true,
            ),
        )
    }

    @Test
    fun formatHudSpeedLine_showsSpeedAndLimit() {
        assertNull(formatHudSpeedLine(DriveHudState()))
        assertEquals(
            "72 km/h",
            formatHudSpeedLine(DriveHudState(currentSpeedKmh = 72.0)),
        )
        assertEquals(
            "Limit 80 km/h",
            formatHudSpeedLine(DriveHudState(currentSpeedLimitKmh = 80.0)),
        )
        assertEquals(
            "72 / 80 km/h",
            formatHudSpeedLine(
                DriveHudState(currentSpeedKmh = 72.4, currentSpeedLimitKmh = 80.0),
            ),
        )
    }

    @Test
    fun overspeedHud_marginRejectsSubHalfKmNoise() {
        // Old +0.5 margin would flag 80.6 on an 80 limit; hybrid must not.
        // At 80 km/h: max(4.0, acc, 3.0) = 4.0 without accuracy.
        assertFalse(OverspeedHud.isOverspeed(80.4, 80.0))
        assertFalse(OverspeedHud.isOverspeed(83.5, 80.0))
        assertTrue(OverspeedHud.isOverspeed(84.1, 80.0))
        // At 30 km/h: 5% is 1.5 → floor 3.0 wins.
        assertFalse(OverspeedHud.isOverspeed(32.5, 30.0))
        assertTrue(OverspeedHud.isOverspeed(33.1, 30.0))
        // Reported accuracy widens the gate.
        assertFalse(OverspeedHud.isOverspeed(84.0, 80.0, speedAccuracyKmh = 5.0))
        assertTrue(OverspeedHud.isOverspeed(86.0, 80.0, speedAccuracyKmh = 5.0))
        assertEquals(4.0, OverspeedHud.effectiveMarginKmh(80.0), 1e-9)
        assertEquals(3.0, OverspeedHud.effectiveMarginKmh(30.0), 1e-9)
        assertEquals(5.5, OverspeedHud.effectiveMarginKmh(110.0), 1e-9)
    }

    @Test
    fun noRoute_neverShowsBreakInfo_includingAfterStaleClear() {
        waitStyle()
        // Cold start / idle: no corridor.
        assertNoBreakHud()

        // Plan a real route — break line must appear (time mode).
        injectRealRoute()
        composeRule
            .onNodeWithTag("hud_break_countdown", useUnmergedTree = true)
            .assertIsDisplayed()
        assertTrue(NaviMapTestHooks.lastBreakHudVisible)
        assertTrue(NaviMapTestHooks.lastMinutesToBreak != null)

        // End route — break info must vanish (no stale linger).
        NaviMapTestHooks.requestClearRoute = true
        Thread.sleep(1_200)
        assertNoBreakHud()

        // Distance mode still shows nothing without a route.
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        MapHudPrefs.saveBreakAsDistance(ctx, true)
        // Force prefs reload via drive settings apply path is heavy; inject route then clear again
        // after toggling through hooks by re-injecting with distance pref already saved.
        injectRealRoute()
        // Activity already running — prefs load on next settings apply; set state via clear+reinject
        // after saving prefs, reopen activity state by clearing and checking idle again.
        NaviMapTestHooks.requestClearRoute = true
        Thread.sleep(1_000)
        assertNoBreakHud()
    }

    @Test
    fun withRoute_distanceModeShowsKm() {
        waitStyle()
        injectRealRoute()
        NaviMapTestHooks.requestBreakAsDistance = true
        Thread.sleep(800)
        composeRule
            .onNodeWithTag("hud_break_countdown", useUnmergedTree = true)
            .assertIsDisplayed()
        composeRule
            .onNodeWithTag("hud_break_countdown", useUnmergedTree = true)
            .assertTextContains("Break in", substring = true)
        composeRule
            .onNodeWithTag("hud_break_countdown", useUnmergedTree = true)
            .assertTextContains("km", substring = true)
        NaviMapTestHooks.requestClearRoute = true
        Thread.sleep(1_000)
        assertNoBreakHud()
    }
}
