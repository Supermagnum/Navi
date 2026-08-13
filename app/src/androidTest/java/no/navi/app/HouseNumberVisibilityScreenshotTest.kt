package no.navi.app

import android.util.Log
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
 * House-number labels on Hamar downtown — Liberty (online) vs Protomaps (offline).
 *
 * Pass criteria (visual + log report):
 * - Liberty: numbers visible at z14+ (OpenFreeMap housenumber floor); absent below
 *   is N/A here (ladder starts at z14).
 * - Protomaps: numbers visible at z15+; absent at z12–14 (no address points in tiles).
 * - Street / POI labels and building fills remain present (no regression).
 *
 * Evidence lands under the app cache dir and `/data/local/tmp/navi_housenumber_zoom/`.
 */
@RunWith(AndroidJUnit4::class)
class HouseNumberVisibilityScreenshotTest {
    @get:Rule
    val activityRule = ActivityTestRule(MainActivity::class.java, false, false)

    private lateinit var context: android.content.Context
    private lateinit var dataDir: File
    private lateinit var outDir: File

    // Hamar sentrum — dense addr:housenumber coverage in OSM.
    private val lat = 60.79448
    private val lon = 11.06799

    @Before
    fun setUp() {
        context = InstrumentationRegistry.getInstrumentation().targetContext
        dataDir = NaviAppData.resolve(context)
        outDir =
            File(context.cacheDir, "navi_housenumber_zoom").also {
                it.mkdirs()
                it.listFiles()?.forEach { f -> f.delete() }
            }
        shell("mkdir -p /data/local/tmp/navi_housenumber_zoom && chmod 777 /data/local/tmp/navi_housenumber_zoom")
        OstlandetOfflineFixtures.ensureInstalled(dataDir)
        NaviMapTestHooks.hideUiChrome = true
        NaviMapTestHooks.hideSearchChrome = true
        NaviMapTestHooks.disableGpsFollow = true
        NaviMapTestHooks.followGps = false
        NaviMapTestHooks.styleReady = false
        MapHudPrefs.saveOptIn3d(context, false)
        MapHudPrefs.saveCameraTiltDeg(context, 0.0)
    }

    @After
    fun tearDown() {
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = false
        NaviMapTestHooks.forceOnlineBasemap = false
        NaviMapTestHooks.disableGpsFollow = false
        NaviMapTestHooks.followGps = true
        MapHudPrefs.saveOptIn3d(context, false)
        setWifi(true)
        runCatching { activityRule.finishActivity() }
    }

    @Test
    fun capture_hamar_housenumber_zoom_ladders() {
        activityRule.launchActivity(null)

        // Online Liberty
        setWifi(true)
        NaviMapTestHooks.forceOnlineBasemap = true
        waitKind("Online")
        for (z in listOf(14.0, 15.0, 16.0, 17.0, 18.0)) {
            shoot("online_liberty_z${z.toInt()}", lat, lon, z)
        }

        // Offline Protomaps
        setWifi(false)
        NaviMapTestHooks.forceOnlineBasemap = false
        NaviMapTestHooks.styleReady = false
        waitKind("OfflineProtomaps")
        for (z in listOf(12.0, 13.0, 14.0, 15.0, 16.0, 17.0)) {
            shoot("offline_pm_z${z.toInt()}", lat, lon, z)
        }

        setWifi(true)
        val n = outDir.listFiles()?.count { it.extension == "png" } ?: 0
        assertTrue("expected housenumber zoom shots, got $n in $outDir", n >= 10)
    }

    private fun waitKind(prefix: String) {
        val deadline = System.currentTimeMillis() + 90_000
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.styleReady &&
                NaviMapTestHooks.lastBasemapKind.startsWith(prefix)
            ) {
                return
            }
            NaviMapTestHooks.pendingCamera = Triple(lat, lon, 14.0)
            Thread.sleep(400)
        }
        assertTrue(
            "basemap kind prefix=$prefix got=${NaviMapTestHooks.lastBasemapKind}",
            NaviMapTestHooks.styleReady &&
                NaviMapTestHooks.lastBasemapKind.startsWith(prefix),
        )
    }

    private fun shoot(
        name: String,
        lat: Double,
        lon: Double,
        zoom: Double,
    ) {
        NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
        val camDeadline = System.currentTimeMillis() + 25_000
        while (System.currentTimeMillis() < camDeadline) {
            if (abs(NaviMapTestHooks.lastCameraZoom - zoom) < 0.15 &&
                abs(NaviMapTestHooks.lastCameraLat - lat) < 0.0005 &&
                abs(NaviMapTestHooks.lastCameraLon - lon) < 0.0005
            ) {
                break
            }
            NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
            Thread.sleep(300)
        }
        Thread.sleep(4_000)
        val shot = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
        assertTrue("screenshot null for $name", shot != null)
        val dest = File(outDir, "$name.png")
        dest.outputStream().use { shot!!.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) }
        assertTrue(dest.isFile && dest.length() > 10_000)
        shell("cp ${dest.absolutePath} /data/local/tmp/navi_housenumber_zoom/$name.png || true")
        shell("screencap -p /data/local/tmp/navi_housenumber_zoom/${name}_screencap.png")
        val line =
            "$name kind=${NaviMapTestHooks.lastBasemapKind} z=${NaviMapTestHooks.lastCameraZoom} bytes=${dest.length()}"
        File(outDir, "report.txt").appendText(line + "\n")
        Log.i(TAG, line)
    }

    private fun setWifi(enabled: Boolean) {
        shell(if (enabled) "svc wifi enable" else "svc wifi disable")
        Thread.sleep(1_500)
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

    companion object {
        private const val TAG = "HouseNumberVis"
    }
}
