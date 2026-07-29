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
 * Evidence: does offline Protomaps render the Dystingbo farm label across the
 * extract maxzoom=12 / feature min_zoom=13 boundary?
 *
 * Single activity session — camera changes only — avoids flaky relaunches while
 * Wi‑Fi is off.
 */
@RunWith(AndroidJUnit4::class)
class FarmLabelZoomScreenshotTest {
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
            File(context.cacheDir, "navi_farm_zoom").also {
                it.mkdirs()
                it.listFiles()?.forEach { f -> f.delete() }
            }
        shell("mkdir -p /data/local/tmp/navi_farm_zoom && chmod 777 /data/local/tmp/navi_farm_zoom")
        NaviMapTestHooks.hideUiChrome = true
        NaviMapTestHooks.styleReady = false
        NaviMapTestHooks.forceOnlineBasemap = false
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
    fun capture_dystingbo_offline_zoom_ladder() {
        ensureOfflineBasemapInstalled(dataDir)
        setNetworkEnabled(false)
        MapHudPrefs.saveOptIn3d(context, false)

        NaviMapTestHooks.styleReady = false
        NaviMapTestHooks.lastBasemapKind = ""
        NaviMapTestHooks.forceOnlineBasemap = false
        NaviMapTestHooks.pendingCamera = Triple(FARM_LAT, FARM_LON, 11.0)
        activityRule.launchActivity(null)

        val bootDeadline = System.currentTimeMillis() + 90_000
        while (System.currentTimeMillis() < bootDeadline) {
            if (NaviMapTestHooks.styleReady &&
                NaviMapTestHooks.lastBasemapKind == "OfflineProtomaps"
            ) {
                break
            }
            NaviMapTestHooks.pendingCamera = Triple(FARM_LAT, FARM_LON, 11.0)
            Thread.sleep(400)
        }
        assertTrue(
            "offline basemap did not load kind=${NaviMapTestHooks.lastBasemapKind}",
            NaviMapTestHooks.styleReady &&
                NaviMapTestHooks.lastBasemapKind == "OfflineProtomaps",
        )

        val zooms = listOf(11.0, 12.0, 12.9, 13.0, 13.5, 14.0, 15.0)
        for (z in zooms) {
            val label = "offline_dystingbo_z" + z.toString().replace('.', '_')
            shootInPlace(label, FARM_LAT, FARM_LON, z)
        }

        setNetworkEnabled(true)
        val n = outDir.listFiles()?.count { it.extension == "png" } ?: 0
        assertTrue("expected farm zoom shots in $outDir, got $n", n >= zooms.size)
    }

    private fun shootInPlace(
        label: String,
        lat: Double,
        lon: Double,
        zoom: Double,
    ) {
        NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
        val camDeadline = System.currentTimeMillis() + 25_000
        while (System.currentTimeMillis() < camDeadline) {
            if (abs(NaviMapTestHooks.lastCameraZoom - zoom) < 0.05 &&
                abs(NaviMapTestHooks.lastCameraLat - lat) < 0.0005 &&
                abs(NaviMapTestHooks.lastCameraLon - lon) < 0.0005
            ) {
                break
            }
            NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
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
        // Extra idle so symbol collision / overzoom labels finish.
        Thread.sleep(1_200)
        assertTrue(
            "second settle failed for $label",
            InstrumentedMapCapture.awaitRenderSettled(20_000),
        )

        val png = File(outDir, "$label.png")
        val req = NaviMapTestHooks.snapshotRequestId + 1
        NaviMapTestHooks.lastSnapshotPng = null
        NaviMapTestHooks.snapshotRequestId = req
        val snapDeadline = System.currentTimeMillis() + 25_000
        while (System.currentTimeMillis() < snapDeadline) {
            val bytes = NaviMapTestHooks.lastSnapshotPng
            if (NaviMapTestHooks.lastSnapshotId >= req && bytes != null && bytes.size > 1_000) {
                png.writeBytes(bytes)
                break
            }
            Thread.sleep(250)
        }
        if (!png.isFile || png.length() < 1_000) {
            InstrumentedMapCapture.screencapAfterSettle(png.absolutePath, timeoutMs = 5_000)
        }
        assertTrue(
            "missing $label kind=${NaviMapTestHooks.lastBasemapKind} " +
                "zoomHook=${NaviMapTestHooks.lastCameraZoom}",
            png.isFile && png.length() > 1_000,
        )
        InstrumentedMapCapture.screencapAfterSettle(
            "/data/local/tmp/navi_farm_zoom/$label.png",
            timeoutMs = 5_000,
        )
        android.util.Log.i(
            TAG,
            "FARM_SHOT label=$label kind=${NaviMapTestHooks.lastBasemapKind} " +
                "zoom=$zoom cameraZoom=${NaviMapTestHooks.lastCameraZoom} " +
                "lat=${NaviMapTestHooks.lastCameraLat} lon=${NaviMapTestHooks.lastCameraLon} " +
                "path=/data/local/tmp/navi_farm_zoom/$label.png",
        )
    }

    private fun ensureOfflineBasemapInstalled(dataDir: File) {
        val staged =
            File("/data/local/tmp/navi_fixtures/europe_norway_ostlandet.pmtiles")
        check(staged.isFile) { "missing ${staged.absolutePath}" }
        val pmDir = File(dataDir, "pmtiles").also { it.mkdirs() }
        val dst = File(pmDir, "europe_norway_ostlandet.pmtiles")
        if (!dst.isFile || dst.length() != staged.length()) {
            staged.copyTo(dst, overwrite = true)
        }
        val job = uniffi.navi.pmtilesQueueRegion(dataDir.absolutePath, REGION, null)
        check(job.id.isNotBlank()) { "pmtilesQueueRegion returned empty id" }
        val done = uniffi.navi.pmtilesRunJob(dataDir.absolutePath, job.id)
        check(done.status == "completed") {
            "expected completed Ostlandet job, got ${done.status}"
        }
        check(dst.isFile && dst.length() > 1_000L) {
            "basemap missing under ${pmDir.absolutePath}"
        }
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
        private const val TAG = "NaviFarmZoom"
        private const val REGION = "europe/norway/ostlandet"
        private const val FARM_LAT = 60.8022727
        private const val FARM_LON = 11.1389560
    }
}
