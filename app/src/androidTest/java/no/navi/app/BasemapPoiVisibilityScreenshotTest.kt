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

/**
 * Evidence shots for everyday basemap POIs (schools, fuel, shops, bus stops, …)
 * — distinct from Navi [docs/poi.md] rest/overnight [PoiIndex] categories.
 *
 * Centers on Hamar downtown where OSM has dense amenity coverage. Captures
 * online Liberty and offline Protomaps, flat and opt-in 3D, at street zoom.
 */
@RunWith(AndroidJUnit4::class)
class BasemapPoiVisibilityScreenshotTest {
    @get:Rule
    val activityRule = ActivityTestRule(MainActivity::class.java, false, false)

    private lateinit var context: android.content.Context
    private lateinit var dataDir: File

    // Hamar sentrum — dense schools / shops / parking / transit in OSM.
    private val lat = 60.79448
    private val lon = 11.06799
    private val zoom = 16.0

    @Before
    fun setUp() {
        context = InstrumentationRegistry.getInstrumentation().targetContext
        dataDir = NaviAppData.resolve(context)
        val auto = InstrumentationRegistry.getInstrumentation().uiAutomation
        auto.grantRuntimePermission(context.packageName, android.Manifest.permission.ACCESS_FINE_LOCATION)
        auto.grantRuntimePermission(context.packageName, android.Manifest.permission.ACCESS_COARSE_LOCATION)
        OstlandetOfflineFixtures.ensureInstalled(dataDir)
        NaviMapTestHooks.hideUiChrome = true
        NaviMapTestHooks.hideSearchChrome = true
        NaviMapTestHooks.disableGpsFollow = true
        NaviMapTestHooks.followGps = false
        NaviMapTestHooks.styleReady = false
    }

    @After
    fun tearDown() {
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = false
        NaviMapTestHooks.forceOnlineBasemap = false
        NaviMapTestHooks.disableGpsFollow = false
        NaviMapTestHooks.followGps = true
        NaviMapTestHooks.requestOptIn3d = null
        MapHudPrefs.saveOptIn3d(context, false)
        MapHudPrefs.saveCameraTiltDeg(context, 0.0)
        setWifi(true)
        runCatching { activityRule.finishActivity() }
    }

    @Test
    fun capture_hamar_online_and_offline_flat_and_3d() {
        activityRule.launchActivity(null)
        val outDir = File("/data/local/tmp/basemap_poi_shots").also { it.mkdirs() }

        // Online Liberty: Wi-Fi on, force online style.
        setWifi(true)
        shoot(
            outDir = outDir,
            name = "online_liberty_flat_z16",
            forceOnline = true,
            optIn3d = false,
            tilt = 0.0,
            expectKindPrefix = "Online",
        )
        shoot(
            outDir = outDir,
            name = "online_liberty_3d_z16",
            forceOnline = true,
            optIn3d = true,
            tilt = 45.0,
            expectKindPrefix = "Online",
        )

        // Offline Protomaps: Wi-Fi off after fixtures installed.
        setWifi(false)
        shoot(
            outDir = outDir,
            name = "offline_protomaps_flat_z16",
            forceOnline = false,
            optIn3d = false,
            tilt = 0.0,
            expectKindPrefix = "Offline",
        )
        shoot(
            outDir = outDir,
            name = "offline_protomaps_3d_z16",
            forceOnline = false,
            optIn3d = true,
            tilt = 45.0,
            expectKindPrefix = "Offline",
        )

        val report =
            buildString {
                appendLine("Basemap POI visibility shots (Hamar $lat,$lon z=$zoom)")
                appendLine("Not the same as docs/poi.md PoiIndex rest categories.")
                File(outDir, "report.txt").takeIf { it.isFile }?.let { append(it.readText()) }
            }
        File(outDir, "summary.txt").writeText(report)
        Log.i(TAG, report)
    }

    private fun shoot(
        outDir: File,
        name: String,
        forceOnline: Boolean,
        optIn3d: Boolean,
        tilt: Double,
        expectKindPrefix: String,
    ) {
        NaviMapTestHooks.styleReady = false
        NaviMapTestHooks.forceOnlineBasemap = forceOnline
        NaviMapTestHooks.requestOptIn3d = optIn3d
        NaviMapTestHooks.requestCameraTiltDeg = tilt
        MapHudPrefs.saveOptIn3d(context, optIn3d)
        MapHudPrefs.saveCameraTiltDeg(context, tilt)
        NaviMapTestHooks.disableGpsFollow = true
        NaviMapTestHooks.followGps = false
        NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)

        val deadline = System.currentTimeMillis() + 90_000
        var lastKind = ""
        while (System.currentTimeMillis() < deadline) {
            lastKind = NaviMapTestHooks.lastBasemapKind
            val kindOk = lastKind.startsWith(expectKindPrefix)
            val pitchOk =
                if (optIn3d) {
                    kotlin.math.abs(NaviMapTestHooks.lastCameraPitch - tilt) <= 3.0
                } else {
                    NaviMapTestHooks.lastCameraPitch <= 2.0
                }
            if (NaviMapTestHooks.styleReady && kindOk && pitchOk) break
            NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
            NaviMapTestHooks.requestOptIn3d = optIn3d
            NaviMapTestHooks.requestCameraTiltDeg = tilt
            Thread.sleep(400)
        }
        // Settle tiles / icons.
        Thread.sleep(5_000)

        val shot = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
        assertTrue("screenshot null for $name", shot != null)
        val dest = File(outDir, "$name.png")
        dest.outputStream().use { shot!!.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) }
        assertTrue(dest.isFile && dest.length() > 20_000)

        val line =
            "$name kind=$lastKind styleReady=${NaviMapTestHooks.styleReady} " +
                "pitch=${NaviMapTestHooks.lastCameraPitch} bytes=${dest.length()}"
        File(outDir, "report.txt").appendText(line + "\n")
        Log.i(TAG, line)
        shell("screencap -p /data/local/tmp/basemap_poi_shots/$name.png")
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
        private const val TAG = "BasemapPoiVisibility"
    }
}
