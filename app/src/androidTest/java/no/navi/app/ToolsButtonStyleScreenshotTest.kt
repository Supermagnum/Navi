package no.navi.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.rule.ActivityTestRule
import androidx.test.uiautomator.By
import androidx.test.uiautomator.UiDevice
import androidx.test.uiautomator.Until
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * One-shot visual check: collapsed HUD Route + Tools must both be filled Material3
 * Buttons (purple pills), not TextButton for Tools.
 *
 * Host pull:
 * adb pull /data/local/tmp/tools_route_button_style.png docs/images/tmp/
 */
@RunWith(AndroidJUnit4::class)
class ToolsButtonStyleScreenshotTest {
    @get:Rule
    val activityRule = ActivityTestRule(MainActivity::class.java, false, false)

    @Before
    fun setUp() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val auto = InstrumentationRegistry.getInstrumentation().uiAutomation
        auto.grantRuntimePermission(context.packageName, android.Manifest.permission.ACCESS_FINE_LOCATION)
        auto.grantRuntimePermission(context.packageName, android.Manifest.permission.ACCESS_COARSE_LOCATION)
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = true
        NaviMapTestHooks.requestCloseTools = true
        NaviMapTestHooks.styleReady = false
    }

    @Test
    fun collapsedRouteAndTools_filledButtonPills() {
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = true
        NaviMapTestHooks.requestCloseTools = true

        activityRule.launchActivity(null)
        assertTrue(activityRule.activity.isFinishing.not())

        assertTrue(
            "styleReady timeout",
            InstrumentedMapCapture.awaitStyleReady(60_000),
        )
        NaviMapTestHooks.hideSearchChrome = true
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.requestCloseTools = true
        Thread.sleep(800)

        val device = UiDevice.getInstance(InstrumentationRegistry.getInstrumentation())
        assertTrue(
            "Route compact button not visible",
            device.wait(Until.hasObject(By.text("Route")), 15_000),
        )
        assertTrue(
            "Tools compact button not visible",
            device.wait(Until.hasObject(By.text("Tools")), 10_000),
        )

        InstrumentedMapCapture.screencapAfterSettle(
            "/data/local/tmp/tools_route_button_style.png",
            timeoutMs = 15_000,
        )
        InstrumentedMapCapture.shell("ls -la /data/local/tmp/tools_route_button_style.png")
    }
}
