package no.navi.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.rule.ActivityTestRule
import org.junit.After
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import kotlin.math.abs

/**
 * Low-zoom overview: World.mwm / WorldCoasts at the MWM root must paint Norway
 * when the camera is fully zoomed out (CoMaps drape path).
 */
@RunWith(AndroidJUnit4::class)
class MwmLowZoomNorwayScreenshotTest {
    @get:Rule
    val activityRule = ActivityTestRule(MainActivity::class.java, false, false)

    private lateinit var outDir: File

    @Before
    fun setUp() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        outDir =
            File(context.cacheDir, "navi_low_zoom_norway").also {
                it.mkdirs()
                it.listFiles()?.forEach { f -> f.delete() }
            }
        shell("mkdir -p /data/local/tmp/navi_low_zoom_norway && chmod 777 /data/local/tmp/navi_low_zoom_norway")
        NaviMapTestHooks.hideUiChrome = true
        NaviMapTestHooks.disableGpsFollow = true
        NaviMapTestHooks.followGps = false
        MapHudPrefs.saveOptIn3d(context, false)
        MapHudPrefs.saveCameraTiltDeg(context, 0.0)
    }

    @After
    fun tearDown() {
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.disableGpsFollow = false
        runCatching { activityRule.finishActivity() }
    }

    @Test
    fun norway_visible_at_zoom_4() {
        val root = File(NaviAppData.resolve(InstrumentationRegistry.getInstrumentation().targetContext), "mwm")
        assertTrue("World.mwm missing at ${root.absolutePath}", File(root, "World.mwm").isFile)
        assertTrue("WorldCoasts.mwm missing", File(root, "WorldCoasts.mwm").isFile)

        NaviMapTestHooks.pendingCamera = Triple(NORWAY_LAT, NORWAY_LON, ZOOM)
        activityRule.launchActivity(null)

        val bootDeadline = System.currentTimeMillis() + 90_000
        while (System.currentTimeMillis() < bootDeadline) {
            if (NaviMapTestHooks.drapeState == "READY" &&
                abs(NaviMapTestHooks.lastCameraZoom - ZOOM) < 0.6 &&
                abs(NaviMapTestHooks.lastCameraLat - NORWAY_LAT) < 2.0
            ) {
                break
            }
            NaviMapTestHooks.disableGpsFollow = true
            NaviMapTestHooks.followGps = false
            NaviMapTestHooks.pendingCamera = Triple(NORWAY_LAT, NORWAY_LON, ZOOM)
            Thread.sleep(400)
        }
        assertTrue(
            "drape not ready or camera not at low zoom " +
                "state=${NaviMapTestHooks.drapeState} z=${NaviMapTestHooks.lastCameraZoom} " +
                "lat=${NaviMapTestHooks.lastCameraLat}",
            NaviMapTestHooks.drapeState == "READY" &&
                abs(NaviMapTestHooks.lastCameraZoom - ZOOM) < 0.6,
        )

        Thread.sleep(2_500)
        val devicePath = "/data/local/tmp/navi_low_zoom_norway/norway_z4.png"
        shell("screencap -p $devicePath")
        shell("chmod 644 $devicePath")
        val bytes = shellBytes("cat $devicePath")
        assertTrue("screenshot empty", bytes.size > 50_000)
        File(outDir, "norway_z4.png").writeBytes(bytes)
        android.util.Log.i(
            TAG,
            "LOW_ZOOM_SHOT path=$devicePath bytes=${bytes.size} z=${NaviMapTestHooks.lastCameraZoom} " +
                "lat=${NaviMapTestHooks.lastCameraLat} lon=${NaviMapTestHooks.lastCameraLon}",
        )
    }

    private fun shell(cmd: String): String {
        val pfd = InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand(cmd)
        return java.io
            .FileInputStream(pfd.fileDescriptor)
            .use { it.readBytes().decodeToString() }
            .also { pfd.close() }
    }

    private fun shellBytes(cmd: String): ByteArray {
        val pfd = InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand(cmd)
        return java.io
            .FileInputStream(pfd.fileDescriptor)
            .use { it.readBytes() }
            .also { pfd.close() }
    }

    companion object {
        private const val TAG = "MwmLowZoomNorway"
        private const val NORWAY_LAT = 64.0
        private const val NORWAY_LON = 12.0
        private const val ZOOM = 4.0
    }
}
