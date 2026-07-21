package no.navi.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.rule.ActivityTestRule
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/**
 * Multi-zoom map screenshots centered on a fixed POI (lat/lon).
 * Uses [NaviMapTestHooks.pendingCamera] so Compose camera state updates in-process.
 */
@RunWith(AndroidJUnit4::class)
class ZoomPoiScreenshotTest {
    @get:Rule
    val activityRule = ActivityTestRule(MainActivity::class.java, false, false)

    private val centerLat = 58.991547
    private val centerLon = 6.138377

    @Test
    fun multiZoomScreenshots_centeredOnPoi() {
        activityRule.launchActivity(null)
        val activity = activityRule.activity
        assertTrue(activity.isFinishing.not())

        // Allow MapLibre style to load before zoom sweep.
        Thread.sleep(5_000)

        fun shell(cmd: String) {
            val pfd = InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand(cmd)
            java.io.FileInputStream(pfd.fileDescriptor).use { input ->
                val buf = ByteArray(4096)
                while (input.read(buf) >= 0) {
                }
            }
            pfd.close()
        }

        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val dataDir = (context.getExternalFilesDir(null) ?: context.filesDir).also { it.mkdirs() }

        val zooms = listOf(
            6.5 to "navi_zoom_6_5.png",
            11.0 to "navi_zoom_11.png",
            16.0 to "navi_zoom_16.png",
        )

        for ((zoom, deviceName) in zooms) {
            NaviMapTestHooks.hideUiChrome = true
            NaviMapTestHooks.pendingCamera = Triple(centerLat, centerLon, zoom)
            // Wait for Compose poll + tile fetch at this zoom.
            Thread.sleep(4_500)

            val shot = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
            assertTrue("screenshot null at zoom=$zoom", shot != null)
            assertNotEquals("screenshot width at zoom=$zoom", 0, shot!!.width)
            assertNotEquals("screenshot height at zoom=$zoom", 0, shot.height)

            val appOut = File(dataDir, deviceName)
            appOut.outputStream().use { os ->
                shot.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, os)
            }
            assertTrue("wrote ${appOut.absolutePath}", appOut.isFile && appOut.length() > 5_000)

            val tmpPath = "/data/local/tmp/$deviceName"
            shell("screencap -p $tmpPath")
            shell("ls -la $tmpPath")
            android.util.Log.i(
                "ZoomPoiScreenshotTest",
                "zoom=$zoom appBytes=${appOut.length()} path=${appOut.absolutePath} tmp=$tmpPath",
            )
            Thread.sleep(500)
        }
    }
}
