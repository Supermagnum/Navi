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
 * Offline Protomaps: townhall labels (Hamar / Gjøvik rådhus) after whitelist fix.
 * `kind=building` is out of scope — empty in these tiles; generic named footprints
 * are not in the `pois` layer.
 */
@RunWith(AndroidJUnit4::class)
class TownhallPoiLabelScreenshotTest {
    @get:Rule
    val activityRule = ActivityTestRule(MainActivity::class.java, false, false)

    private lateinit var context: android.content.Context
    private lateinit var dataDir: File
    private lateinit var outDir: File

    @Before
    fun setUp() {
        context = InstrumentationRegistry.getInstrumentation().targetContext
        dataDir = NaviAppData.resolve(context)
        outDir =
            File(context.cacheDir, "navi_townhall_poi").also {
                it.mkdirs()
                it.listFiles()?.forEach { f -> f.delete() }
            }
        shell("mkdir -p /data/local/tmp/navi_townhall_poi && chmod 777 /data/local/tmp/navi_townhall_poi")
        OstlandetOfflineFixtures.ensureInstalled(dataDir)
        NaviMapTestHooks.hideUiChrome = true
        NaviMapTestHooks.hideSearchChrome = true
        NaviMapTestHooks.disableGpsFollow = true
        NaviMapTestHooks.followGps = false
        NaviMapTestHooks.styleReady = false
        NaviMapTestHooks.forceOnlineBasemap = false
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
        setWifi(true)
        runCatching { activityRule.finishActivity() }
    }

    @Test
    fun capture_radhus_offline_protomaps() {
        setWifi(false)
        NaviMapTestHooks.forceOnlineBasemap = false
        NaviMapTestHooks.pendingCamera = Triple(HAMAR_LAT, HAMAR_LON, 15.0)
        activityRule.launchActivity(null)

        waitKind("OfflineProtomaps")
        // Feature min_zoom is 15; also shoot z14 (should lack townhall label) and z16.
        for (shot in listOf(
            Quad("hamar_z14", HAMAR_LAT, HAMAR_LON, 14.0),
            Quad("hamar_z15", HAMAR_LAT, HAMAR_LON, 15.0),
            Quad("hamar_z16", HAMAR_LAT, HAMAR_LON, 16.0),
            Quad("gjovik_z15", GJOVIK_LAT, GJOVIK_LON, 15.0),
            Quad("gjovik_z16", GJOVIK_LAT, GJOVIK_LON, 16.0),
        )) {
            shoot(shot.name, shot.lat, shot.lon, shot.z)
        }

        setWifi(true)
        val n = outDir.listFiles()?.count { it.extension == "png" } ?: 0
        assertTrue("expected townhall shots, got $n", n >= 5)
    }

    private data class Quad(
        val name: String,
        val lat: Double,
        val lon: Double,
        val z: Double,
    )

    private fun waitKind(prefix: String) {
        val deadline = System.currentTimeMillis() + 90_000
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.styleReady &&
                NaviMapTestHooks.lastBasemapKind.startsWith(prefix)
            ) {
                return
            }
            NaviMapTestHooks.pendingCamera = Triple(HAMAR_LAT, HAMAR_LON, 15.0)
            Thread.sleep(400)
        }
        assertTrue(
            "basemap kind=$prefix got=${NaviMapTestHooks.lastBasemapKind}",
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
        shell("cp ${dest.absolutePath} /data/local/tmp/navi_townhall_poi/$name.png || true")
        shell("screencap -p /data/local/tmp/navi_townhall_poi/${name}_screencap.png")
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
        private const val TAG = "TownhallPoiLabel"
        private const val HAMAR_LAT = 60.79446
        private const val HAMAR_LON = 11.07872
        private const val GJOVIK_LAT = 60.79474
        private const val GJOVIK_LON = 10.69278
    }
}
