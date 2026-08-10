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
import kotlin.math.abs

/**
 * Lake / river / wetland name labels: Liberty + offline Protomaps screenshots.
 *
 * Cases: Mjøsa (lake), Glomma (river), Bårdsæterkjølen (wetland pois).
 * Labeling-only — water fill/line geometry layers must stay untouched.
 */
@RunWith(AndroidJUnit4::class)
class WaterWetlandNameLabelScreenshotTest {
    @get:Rule
    val activityRule = ActivityTestRule(MainActivity::class.java, false, false)

    private lateinit var context: android.content.Context

    @Before
    fun setUp() {
        context = InstrumentationRegistry.getInstrumentation().targetContext
        val auto = InstrumentationRegistry.getInstrumentation().uiAutomation
        auto.grantRuntimePermission(context.packageName, android.Manifest.permission.ACCESS_FINE_LOCATION)
        auto.grantRuntimePermission(context.packageName, android.Manifest.permission.ACCESS_COARSE_LOCATION)
        shell("mkdir -p /data/local/tmp/navi_water_wetland_names && chmod 777 /data/local/tmp/navi_water_wetland_names")
        NaviMapTestHooks.hideUiChrome = true
        NaviMapTestHooks.hideSearchChrome = true
        NaviMapTestHooks.disableGpsFollow = true
        NaviMapTestHooks.followGps = false
        NaviMapTestHooks.styleReady = false
        NaviMapTestHooks.requestOptIn3d = false
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
        setNetworkEnabled(true)
        runCatching { activityRule.finishActivity() }
    }

    @Test
    fun captureLakeRiverWetlandNameLadder() {
        OstlandetOfflineFixtures.ensureInstalled(NaviAppData.resolve(context))

        setNetworkEnabled(true)
        // Liberty: lake/river should already label; wetland is upstream-limited.
        shoot("liberty_mjosa_z10", MJOSA_LAT, MJOSA_LON, 10.0, forceOnline = true)
        shoot("liberty_glomma_z12", GLOMMA_LAT, GLOMMA_LON, 12.0, forceOnline = true)
        shoot("liberty_bardsater_z14", WETLAND_LAT, WETLAND_LON, 14.0, forceOnline = true)

        activityRule.finishActivity()
        Thread.sleep(800)
        setNetworkEnabled(false)
        shoot("offline_pm_mjosa_z10", MJOSA_LAT, MJOSA_LON, 10.0, forceOnline = false)
        shoot("offline_pm_glomma_z12", GLOMMA_LAT, GLOMMA_LON, 12.0, forceOnline = false)
        shoot("offline_pm_bardsater_z12", WETLAND_LAT, WETLAND_LON, 12.0, forceOnline = false)
        shoot("offline_pm_bardsater_z14", WETLAND_LAT, WETLAND_LON, 14.0, forceOnline = false)
        setNetworkEnabled(true)

        val n =
            java.io
                .File("/data/local/tmp/navi_water_wetland_names")
                .listFiles()
                ?.count { it.extension == "png" } ?: 0
        assertTrue("expected water/wetland name shots, got $n", n >= 7)
    }

    private fun shoot(
        label: String,
        lat: Double,
        lon: Double,
        zoom: Double,
        forceOnline: Boolean,
    ) {
        runCatching { activityRule.finishActivity() }
        Thread.sleep(500)
        NaviMapTestHooks.styleReady = false
        NaviMapTestHooks.lastBasemapKind = ""
        NaviMapTestHooks.forceOnlineBasemap = forceOnline
        NaviMapTestHooks.requestOptIn3d = false
        NaviMapTestHooks.requestCameraTiltDeg = 0.0
        MapHudPrefs.saveOptIn3d(context, false)
        NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
        activityRule.launchActivity(null)

        val deadline = System.currentTimeMillis() + 90_000
        while (System.currentTimeMillis() < deadline) {
            val kind = NaviMapTestHooks.lastBasemapKind
            val kindOk =
                if (forceOnline) kind.startsWith("Online") else kind == "OfflineProtomaps"
            if (NaviMapTestHooks.styleReady && kindOk) break
            NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
            Thread.sleep(400)
        }
        assertTrue(
            "styleReady $label kind=${NaviMapTestHooks.lastBasemapKind}",
            NaviMapTestHooks.styleReady,
        )
        if (!forceOnline) {
            assertTrue(NaviMapTestHooks.lastBasemapKind == "OfflineProtomaps")
        }
        NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
        val camDeadline = System.currentTimeMillis() + 20_000
        while (System.currentTimeMillis() < camDeadline) {
            if (abs(NaviMapTestHooks.lastCameraZoom - zoom) < 0.15 &&
                abs(NaviMapTestHooks.lastCameraLat - lat) < 0.08
            ) {
                break
            }
            NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
            Thread.sleep(250)
        }
        assertTrue(InstrumentedMapCapture.awaitRenderSettled(30_000))
        Thread.sleep(2_000)
        assertTrue(InstrumentedMapCapture.awaitRenderSettled(20_000))
        val path = "/data/local/tmp/navi_water_wetland_names/$label.png"
        InstrumentedMapCapture.screencapAfterSettle(path, timeoutMs = 8_000)
        android.util.Log.i(
            TAG,
            "NAME_SHOT $label kind=${NaviMapTestHooks.lastBasemapKind} z=${NaviMapTestHooks.lastCameraZoom}",
        )
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
        private const val TAG = "WaterWetlandNames"

        private const val MJOSA_LAT = 60.7553816
        private const val MJOSA_LON = 10.8501628
        private const val GLOMMA_LAT = 61.0657135
        private const val GLOMMA_LON = 11.3546579
        private const val WETLAND_LAT = 60.9272464
        private const val WETLAND_LON = 11.2547322
    }
}
