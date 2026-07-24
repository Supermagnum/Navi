package no.navi.app

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.BeforeClass
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.CorridorRouteResult
import uniffi.navi.FfiTruckRestSettings
import uniffi.navi.TravelProfile
import uniffi.navi.loadTruckRestSettings
import uniffi.navi.saveTruckRestSettings
import java.io.File

/**
 * HUD must use TruckRestParams when Truck is selected — not CarRestParams —
 * and changing truck break-after hours must change the displayed Break line.
 */
@RunWith(AndroidJUnit4::class)
class TruckRestHudInstrumentedTest {

    companion object {
        @JvmStatic
        @BeforeClass
        fun beforeClass() {
            val pkg = InstrumentationRegistry.getInstrumentation().targetContext.packageName
            runCatching {
                InstrumentationRegistry.getInstrumentation().uiAutomation
                    .grantRuntimePermission(pkg, android.Manifest.permission.ACCESS_FINE_LOCATION)
            }
            NaviMapTestHooks.hideUiChrome = false
            NaviMapTestHooks.hideSearchChrome = true
            NaviMapTestHooks.styleReady = false
        }
    }

    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

    private lateinit var dataDir: File

    @Before
    fun setUp() {
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        dataDir = NaviAppData.resolve(ctx)
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = true
        NaviMapTestHooks.requestBreakReminders = true
        NaviMapTestHooks.gpsAltitudeM = 412.0
        NaviMapTestHooks.pendingCamera = Triple(60.722, 10.613, 12.0)
        MapHudPrefs.saveBreakAsDistance(ctx, false)
    }

    private fun waitStyle() {
        val deadline = System.currentTimeMillis() + 25_000
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.styleReady) break
            Thread.sleep(200)
        }
        assertTrue("MapLibre style not ready", NaviMapTestHooks.styleReady)
        Thread.sleep(500)
    }

    private fun truckSettings(breakAfterH: Double) = FfiTruckRestSettings(
        mandatoryBreakAfterHours = breakAfterH,
        breakDurationMinutes = 45u,
        preferSplitBreak = false,
        maxDailyDrivingHours = 9.0,
        maxDailyDrivingExtendedHours = 10.0,
        maxDailyExtensionsPerWeek = 2u,
        maxWeeklyDrivingHours = 56.0,
        maxFortnightlyDrivingHours = 90.0,
        exceptionalExtensionArmed = false,
        ecoModeEnabled = false,
    )

    private fun injectRoute() {
        NaviMapTestHooks.pendingRoute = CorridorRouteResult(
            report = "TEST_KIND=TRUCK_HUD\nPASS\n",
            distanceKm = 50.0,
            etaMinutes = 40.0,
            cacheHit = false,
            coldBuildS = 0.0,
            warmLoadS = 0.0,
            routePolyline = "10.0,60.0;10.1,60.1;10.2,60.2",
            poiLat = 60.2,
            poiLon = 10.2,
            poiName = "Test",
            poiIconKey = "fuel",
            breakPoisJson = "[]",
            daysJson = "[]",
        )
        NaviMapTestHooks.requestBreakReminders = true
    }

    @Test
    fun truck_profile_hud_break_minutes_follow_truck_rest_settings() {
        waitStyle()

        assertTrue(saveTruckRestSettings(dataDir.absolutePath, truckSettings(2.0)))
        assertEquals(2.0, loadTruckRestSettings(dataDir.absolutePath).mandatoryBreakAfterHours, 0.01)

        NaviMapTestHooks.requestTravelProfile = TravelProfile.TRUCK
        injectRoute()

        val deadline = System.currentTimeMillis() + 20_000
        var saw120 = false
        while (System.currentTimeMillis() < deadline) {
            composeRule.waitForIdle()
            if (NaviMapTestHooks.lastMinutesToBreak != null &&
                kotlin.math.abs(NaviMapTestHooks.lastMinutesToBreak!! - 120.0) < 1.0
            ) {
                saw120 = true
                break
            }
            Thread.sleep(200)
            NaviMapTestHooks.requestTravelProfile = TravelProfile.TRUCK
            injectRoute()
        }
        assertTrue(
            "Truck HUD must show ~120 min for 2.0 h break-after (got ${NaviMapTestHooks.lastMinutesToBreak})",
            saw120,
        )
        composeRule.onNodeWithText("Break in 120 min", useUnmergedTree = true).assertIsDisplayed()

        NaviMapTestHooks.requestOpenDriveSettings = true
        val openDeadline = System.currentTimeMillis() + 8_000
        while (System.currentTimeMillis() < openDeadline && !NaviMapTestHooks.driveSettingsOpen) {
            Thread.sleep(100)
        }
        assertTrue(NaviMapTestHooks.driveSettingsOpen)
        composeRule.onNodeWithTag("drive_rest_profile_hint", useUnmergedTree = true)
            .assertIsDisplayed()
        assertTrue(
            composeRule.onAllNodesWithText("Truck EC 561", substring = true, useUnmergedTree = true)
                .fetchSemanticsNodes()
                .isNotEmpty(),
        )
        composeRule.onNodeWithTag("btn_close_drive_settings", useUnmergedTree = true)
            .performClick()
        val closeDeadline = System.currentTimeMillis() + 8_000
        while (System.currentTimeMillis() < closeDeadline && NaviMapTestHooks.driveSettingsOpen) {
            Thread.sleep(100)
        }

        assertTrue(saveTruckRestSettings(dataDir.absolutePath, truckSettings(3.0)))
        assertEquals(3.0, loadTruckRestSettings(dataDir.absolutePath).mandatoryBreakAfterHours, 0.01)
        NaviMapTestHooks.requestTravelProfile = TravelProfile.TRUCK
        injectRoute()

        val saw180Deadline = System.currentTimeMillis() + 15_000
        var saw180 = false
        while (System.currentTimeMillis() < saw180Deadline) {
            composeRule.waitForIdle()
            if (NaviMapTestHooks.lastMinutesToBreak != null &&
                kotlin.math.abs(NaviMapTestHooks.lastMinutesToBreak!! - 180.0) < 1.0
            ) {
                saw180 = true
                break
            }
            Thread.sleep(200)
            NaviMapTestHooks.requestTravelProfile = TravelProfile.TRUCK
            injectRoute()
        }
        assertTrue(
            "After editing TruckRestParams to 3.0 h, HUD must show ~180 min (got ${NaviMapTestHooks.lastMinutesToBreak})",
            saw180,
        )
        assertTrue(
            composeRule.onAllNodesWithText("Break in 180 min", useUnmergedTree = true)
                .fetchSemanticsNodes()
                .isNotEmpty(),
        )
        assertTrue(
            composeRule.onAllNodesWithText("Break in 240 min", useUnmergedTree = true)
                .fetchSemanticsNodes()
                .isEmpty(),
        )
    }
}
