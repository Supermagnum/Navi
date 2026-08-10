package no.navi.app

import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.BeforeClass
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.currentSpeedKmh
import uniffi.navi.overspeedDeltaKmh
import uniffi.navi.resolveSpeedLimitKmh
import uniffi.navi.updateGpsFix

/**
 * On-device smoke for live GPS speed + applicable speed-limit FFI used by the
 * bottom HUD (`hud_current_speed`).
 */
@RunWith(AndroidJUnit4::class)
class SpeedHudFfiInstrumentedTest {
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
        }
    }

    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

    @Before
    fun resetHooks() {
        NaviMapTestHooks.pendingCurrentStreet = null
        NaviMapTestHooks.lastCurrentStreet = null
        NaviMapTestHooks.lastGpsSpeedKmh = null
        NaviMapTestHooks.lastCurrentSpeedLimitKmh = null
    }

    @Test
    fun updateGpsFix_exposesCurrentSpeedKmh() {
        updateGpsFix(60.8525, 11.0080, available = true, speedKmh = 67.0)
        assertEquals(67.0, currentSpeedKmh()!!, 0.01)
        assertEquals(15.0, overspeedDeltaKmh(95.0, 80.0)!!, 0.01)
    }

    @Test
    fun resolveSpeedLimit_conditionalAndFallback() {
        val fallback = resolveSpeedLimitKmh(null, null, "residential")
        assertEquals(40.0, fallback, 0.01)

        val posted = resolveSpeedLimitKmh(80.0, "50 @ (Mo-Fr 00:00-06:00)", "primary")
        // Either the conditional window matches "now" (50) or base posted (80).
        assertTrue("unexpected limit $posted", posted == 50.0 || posted == 80.0)
    }

    @Test
    fun formatHudSpeedLine_matchesBottomHudHelper() {
        assertEquals(
            "58 / 60 km/h",
            formatHudSpeedLine(
                DriveHudState(
                    currentStreet = "Furnesvegen",
                    currentSpeedKmh = 58.0,
                    currentSpeedLimitKmh = 60.0,
                ),
            ),
        )
        composeRule.waitForIdle()
    }
}
