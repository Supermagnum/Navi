package no.navi.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.rule.ActivityTestRule
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/**
 * Isolates MapLibre RenderThread SIGSEGV at non-zero camera bearing.
 *
 * Modes:
 * - bearing alone (no screenshot)
 * - bearing then UiAutomation screenshot
 * - small angles (10, 45) vs cardinals (90, 180, 270)
 */
@RunWith(AndroidJUnit4::class)
class BearingCrashIsolationTest {
    @get:Rule
    val activityRule = ActivityTestRule(MainActivity::class.java, false, false)

    private val centerLat = 60.722823
    private val centerLon = 10.613182
    private val zoom = 11.0

    private lateinit var dataDir: File

    @Before
    fun setUp() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        dataDir = NaviAppData.resolve(context)
        NaviMapTestHooks.hideUiChrome = true
        NaviMapTestHooks.hideSearchChrome = true
        NaviMapTestHooks.applyBearingToMap = true
        NaviMapTestHooks.magneticHeadingDeg = null
        NaviMapTestHooks.gpsBearingDeg = null
        NaviMapTestHooks.styleReady = false
        NaviMapTestHooks.pendingCamera = Triple(centerLat, centerLon, zoom)
        // Compass + null magnetic: NorthUp would force bearing 0 every poll tick.
        NaviMapTestHooks.requestRotationMode = MapRotationMode.Compass
    }

    @After
    fun tearDown() {
        // Prefer Compose DisposableEffect onDestroy over relying on GC finalizers
        // for MapLibre's Vulkan MapRenderer (FinalizerDaemon SIGSEGV on AAOS).
        runCatching { activityRule.finishActivity() }
        Thread.sleep(1_500)
        NaviMapTestHooks.applyBearingToMap = false
        NaviMapTestHooks.magneticHeadingDeg = null
        NaviMapTestHooks.pendingBearing = null
    }

    private fun waitStyle() {
        activityRule.launchActivity(null)
        val deadline = System.currentTimeMillis() + 25_000
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.styleReady) break
            Thread.sleep(200)
        }
        assertTrue("styleReady", NaviMapTestHooks.styleReady)
        NaviMapTestHooks.requestRotationMode = MapRotationMode.Compass
        NaviMapTestHooks.pendingCamera = Triple(centerLat, centerLon, zoom)
        Thread.sleep(3_000)
    }

    private fun setBearing(deg: Double) {
        NaviMapTestHooks.applyBearingToMap = true
        // Prefer magnetic path (real Compass HUD path) plus pendingBearing fallback.
        NaviMapTestHooks.magneticHeadingDeg = deg
        NaviMapTestHooks.pendingBearing = deg
        val deadline = System.currentTimeMillis() + 10_000
        while (System.currentTimeMillis() < deadline) {
            if (kotlin.math.abs(NaviMapTestHooks.lastCameraBearing - deg) <= 0.5) return
            Thread.sleep(100)
        }
        assertEquals("bearing", deg, NaviMapTestHooks.lastCameraBearing, 0.5)
    }

    private fun shot(name: String): File {
        Thread.sleep(1_500)
        val bmp = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
        assertTrue("screenshot null $name", bmp != null)
        val out = File(dataDir, name)
        out.outputStream().use { os ->
            bmp!!.compress(android.graphics.Bitmap.CompressFormat.PNG, 90, os)
        }
        bmp.recycle()
        assertTrue(out.length() > 3_000)
        android.util.Log.i("BearingCrash", "shot=$name bytes=${out.length()}")
        return out
    }

    @Test
    fun bearingAlone_noScreenshot_survivesCardinals() {
        waitStyle()
        for (deg in listOf(10.0, 45.0, 90.0, 180.0, 270.0)) {
            android.util.Log.e("BearingCrash", "bearing-alone set=$deg")
            setBearing(deg)
            Thread.sleep(3_000)
            android.util.Log.e("BearingCrash", "bearing-alone still alive at=$deg")
        }
        // Return to 0 without screenshot.
        setBearing(0.0)
        Thread.sleep(2_000)
        android.util.Log.e("BearingCrash", "bearing-alone PASS")
    }

    @Test
    fun bearingThenScreenshot_allFourHeadings() {
        waitStyle()
        val shots = mutableListOf<File>()
        for (deg in listOf(0.0, 90.0, 180.0, 270.0)) {
            android.util.Log.e("BearingCrash", "bearing+shot set=$deg")
            setBearing(deg)
            Thread.sleep(2_000)
            shots += shot("bearing_iso_${deg.toInt()}.png")
            android.util.Log.e("BearingCrash", "bearing+shot survived=$deg")
        }
        assertEquals(4, shots.size)
        // Frames at different bearings must differ (map actually rotated).
        assertFalse(
            "0 vs 90 screenshots must differ",
            shots[0].readBytes().contentEquals(shots[1].readBytes()),
        )
        assertFalse(
            "90 vs 180 screenshots must differ",
            shots[1].readBytes().contentEquals(shots[2].readBytes()),
        )
        android.util.Log.e("BearingCrash", "bearing+shot PASS")
    }
}
