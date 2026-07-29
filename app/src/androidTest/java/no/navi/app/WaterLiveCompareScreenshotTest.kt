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
 * Live-framebuffer evidence: Liberty vs offline Protomaps water at Mjøsa/Hamar.
 * Uses shell screencap (not MapLibre snapshot) after render settle.
 */
@RunWith(AndroidJUnit4::class)
class WaterLiveCompareScreenshotTest {
    @get:Rule
    val activityRule = ActivityTestRule(MainActivity::class.java, false, false)

    private lateinit var context: android.content.Context
    private lateinit var dataDir: File

    @Before
    fun setUp() {
        context = InstrumentationRegistry.getInstrumentation().targetContext
        dataDir = NaviAppData.resolve(context)
        shell("mkdir -p /data/local/tmp/navi_water_live && chmod 777 /data/local/tmp/navi_water_live")
        NaviMapTestHooks.hideUiChrome = true
        NaviMapTestHooks.styleReady = false
        MapHudPrefs.saveOptIn3d(context, false)
        MapHudPrefs.saveCameraTiltDeg(context, 0.0)
    }

    @After
    fun tearDown() {
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.forceOnlineBasemap = false
        MapHudPrefs.saveOptIn3d(context, false)
        setNetworkEnabled(true)
        runCatching { activityRule.finishActivity() }
    }

    @Test
    fun capture_mjosa_liberty_vs_offline() {
        ensureOfflineBasemapInstalled(dataDir)

        // Online Liberty first (network on).
        setNetworkEnabled(true)
        shoot("live_liberty_z10", LAKE_LAT, LAKE_LON, 10.0, forceOnline = true)
        shoot("live_liberty_z11", LAKE_LAT, LAKE_LON, 11.0, forceOnline = true)
        shoot("live_liberty_z13", LAKE_LAT, LAKE_LON, 13.0, forceOnline = true)

        activityRule.finishActivity()
        Thread.sleep(800)
        setNetworkEnabled(false)
        shoot("live_offline_pm_z10", LAKE_LAT, LAKE_LON, 10.0, forceOnline = false)
        shoot("live_offline_pm_z11", LAKE_LAT, LAKE_LON, 11.0, forceOnline = false)
        shoot("live_offline_pm_z13", LAKE_LAT, LAKE_LON, 13.0, forceOnline = false)

        setNetworkEnabled(true)
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
            "style not ready for $label kind=${NaviMapTestHooks.lastBasemapKind}",
            NaviMapTestHooks.styleReady,
        )
        // Hold camera; wait for idle composite (same path as interactive after settle).
        NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
        val camDeadline = System.currentTimeMillis() + 20_000
        while (System.currentTimeMillis() < camDeadline) {
            if (abs(NaviMapTestHooks.lastCameraZoom - zoom) < 0.1) break
            NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
            Thread.sleep(250)
        }
        assertTrue(InstrumentedMapCapture.awaitRenderSettled(30_000))
        Thread.sleep(2_000)
        assertTrue(InstrumentedMapCapture.awaitRenderSettled(20_000))

        val path = "/data/local/tmp/navi_water_live/$label.png"
        InstrumentedMapCapture.screencapAfterSettle(path, timeoutMs = 8_000)
        android.util.Log.i(
            TAG,
            "WATER_LIVE label=$label kind=${NaviMapTestHooks.lastBasemapKind} " +
                "zoom=${NaviMapTestHooks.lastCameraZoom} path=$path",
        )
    }

    private fun ensureOfflineBasemapInstalled(dataDir: File) {
        val staged = File("/data/local/tmp/navi_fixtures/europe_norway_ostlandet.pmtiles")
        check(staged.isFile) { "missing ${staged.absolutePath}" }
        val pmDir = File(dataDir, "pmtiles").also { it.mkdirs() }
        val dst = File(pmDir, "europe_norway_ostlandet.pmtiles")
        if (!dst.isFile || dst.length() != staged.length()) {
            staged.copyTo(dst, overwrite = true)
        }
        val job = uniffi.navi.pmtilesQueueRegion(dataDir.absolutePath, REGION, null)
        val done = uniffi.navi.pmtilesRunJob(dataDir.absolutePath, job.id)
        check(done.status == "completed") { "job ${done.status}" }
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
        private const val TAG = "NaviWaterLive"
        private const val REGION = "europe/norway/ostlandet"
        private const val LAKE_LAT = 60.7945
        private const val LAKE_LON = 11.0680
    }
}
