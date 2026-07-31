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
 * Online Liberty label/POI zoom ladder centered on a Hamar gas station.
 * Uses [NaviMapTestHooks.forceOnlineBasemap] so covering Ostlandet PMTiles
 * do not win over Liberty.
 */
@RunWith(AndroidJUnit4::class)
class GasStationLibertyLabelZoomScreenshotTest {
    @get:Rule
    val activityRule = ActivityTestRule(MainActivity::class.java, false, false)

    private lateinit var context: android.content.Context

    @Before
    fun setUp() {
        context = InstrumentationRegistry.getInstrumentation().targetContext
        shell("mkdir -p /data/local/tmp/navi_label_zoom_liberty && chmod 777 /data/local/tmp/navi_label_zoom_liberty")
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
        setNetworkEnabled(true)
        runCatching { activityRule.finishActivity() }
    }

    @Test
    fun capture_gas_station_liberty_zoom_ladder() {
        setNetworkEnabled(true)

        NaviMapTestHooks.styleReady = false
        NaviMapTestHooks.lastBasemapKind = ""
        NaviMapTestHooks.forceOnlineBasemap = true
        NaviMapTestHooks.pendingCamera = Triple(GAS_LAT, GAS_LON, 12.0)
        activityRule.launchActivity(null)

        val bootDeadline = System.currentTimeMillis() + 90_000
        while (System.currentTimeMillis() < bootDeadline) {
            if (NaviMapTestHooks.styleReady &&
                NaviMapTestHooks.lastBasemapKind.startsWith("Online")
            ) {
                break
            }
            NaviMapTestHooks.forceOnlineBasemap = true
            NaviMapTestHooks.pendingCamera = Triple(GAS_LAT, GAS_LON, 12.0)
            Thread.sleep(400)
        }
        assertTrue(
            "Liberty online did not load kind=${NaviMapTestHooks.lastBasemapKind}",
            NaviMapTestHooks.styleReady &&
                NaviMapTestHooks.lastBasemapKind.startsWith("Online"),
        )

        for (z in listOf(12.0, 13.0, 14.0, 15.0, 16.0)) {
            val label = "liberty_gas_z${z.toInt()}"
            shootInPlace(label, z)
        }
    }

    private fun shootInPlace(
        label: String,
        zoom: Double,
    ) {
        NaviMapTestHooks.forceOnlineBasemap = true
        NaviMapTestHooks.pendingCamera = Triple(GAS_LAT, GAS_LON, zoom)
        val camDeadline = System.currentTimeMillis() + 25_000
        while (System.currentTimeMillis() < camDeadline) {
            if (abs(NaviMapTestHooks.lastCameraZoom - zoom) < 0.05 &&
                abs(NaviMapTestHooks.lastCameraLat - GAS_LAT) < 0.0005 &&
                abs(NaviMapTestHooks.lastCameraLon - GAS_LON) < 0.0005
            ) {
                break
            }
            NaviMapTestHooks.pendingCamera = Triple(GAS_LAT, GAS_LON, zoom)
            Thread.sleep(300)
        }
        assertTrue(
            "camera did not reach z=$zoom (got ${NaviMapTestHooks.lastCameraZoom} " +
                "@ ${NaviMapTestHooks.lastCameraLat},${NaviMapTestHooks.lastCameraLon})",
            abs(NaviMapTestHooks.lastCameraZoom - zoom) < 0.15,
        )
        assertTrue(
            "render settle failed for $label",
            InstrumentedMapCapture.awaitRenderSettled(30_000),
        )
        Thread.sleep(1_200)
        assertTrue(
            "second settle failed for $label",
            InstrumentedMapCapture.awaitRenderSettled(20_000),
        )

        val path = "/data/local/tmp/navi_label_zoom_liberty/$label.png"
        InstrumentedMapCapture.screencapAfterSettle(path, timeoutMs = 8_000)
        android.util.Log.i(
            TAG,
            "LIBERTY_LABEL label=$label kind=${NaviMapTestHooks.lastBasemapKind} " +
                "zoom=$zoom cameraZoom=${NaviMapTestHooks.lastCameraZoom} " +
                "lat=${NaviMapTestHooks.lastCameraLat} lon=${NaviMapTestHooks.lastCameraLon} " +
                "path=$path",
        )
        val f = File(path)
        assertTrue("missing $label at $path", f.isFile && f.length() > 1_000)
    }

    private fun setNetworkEnabled(enabled: Boolean) {
        shell(if (enabled) "svc wifi enable" else "svc wifi disable")
        shell(if (enabled) "svc data enable" else "svc data disable")
        Thread.sleep(1_200)
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
        private const val TAG = "NaviLibertyLabelZoom"
        private const val GAS_LAT = 60.7897832
        private const val GAS_LON = 11.0920954
    }
}
