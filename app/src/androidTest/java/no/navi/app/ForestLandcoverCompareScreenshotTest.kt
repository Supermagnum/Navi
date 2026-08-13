package no.navi.app

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Color
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
import uniffi.navi.pmtilesDeleteJob
import uniffi.navi.pmtilesListJobs
import uniffi.navi.pmtilesPlanetUrl
import uniffi.navi.pmtilesQueueRegion
import uniffi.navi.pmtilesRunJob
import java.io.File
import kotlin.math.abs

/**
 * Espa forest/wood landcover green fraction — restore vs fresh download.
 *
 * Staged Ostlandet tiles use mostly landuse.kind=`wood` (not `forest`). The
 * offline style must paint `wood` (and landcover `forest`) or land stays cream.
 *
 * Prefer host `am instrument` after `:app:installDebug` /
 * `:app:installDebugAndroidTest`. Older AGP runs of `connectedDebugAndroidTest`
 * could uninstall and wipe app data; this repo now keeps APKs installed after
 * connected tests (`leaveApksInstalledAfterRun`).
 */
@RunWith(AndroidJUnit4::class)
class ForestLandcoverCompareScreenshotTest {
    @get:Rule
    val activityRule = ActivityTestRule(MainActivity::class.java, false, false)

    private lateinit var context: android.content.Context
    private lateinit var dataDir: File

    @Before
    fun setUp() {
        context = InstrumentationRegistry.getInstrumentation().targetContext
        dataDir = NaviAppData.resolve(context)
        val auto = InstrumentationRegistry.getInstrumentation().uiAutomation
        auto.grantRuntimePermission(context.packageName, android.Manifest.permission.ACCESS_FINE_LOCATION)
        auto.grantRuntimePermission(context.packageName, android.Manifest.permission.ACCESS_COARSE_LOCATION)
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
    fun capture_espa_staged_restore_before() {
        val report = OfflinePmtilesBootstrap.restoreOstlandetFromStaging(dataDir)
        check(report.startsWith("OK:")) { report }
        val basemap = File(dataDir, "pmtiles/$BASEMAP")
        val staged = File(OfflineDataIntegrity.STAGED_FIXTURES_DIR, BASEMAP)
        check(basemap.isFile && basemap.length() == staged.length()) {
            "basemap size ${basemap.length()} != staged ${staged.length()}"
        }
        log(
            "RESTORE_READY basemapBytes=${basemap.length()} stagedBytes=${staged.length()} " +
                "report=$report",
        )

        val green = shootEspa(DEVICE_RESTORE_BEFORE)
        log(
            "FOREST_RESTORE_BEFORE greenFrac=$green path=$DEVICE_RESTORE_BEFORE " +
                "basemapBytes=${basemap.length()} kind=${NaviMapTestHooks.lastBasemapKind}",
        )
        File(dataDir, "forest_restore_before_green.txt").writeText(
            "greenFrac=$green\nbasemapBytes=${basemap.length()}\npath=$DEVICE_RESTORE_BEFORE\n",
        )
    }

    /**
     * Deletes app-private Ostlandet basemap (+ related jobs), then downloads from
     * network without copying staged fixtures. Keeps DEM file when present.
     * May take 10–30+ minutes.
     */
    @Test
    fun wipe_fresh_download_espa_before() {
        setNetworkEnabled(true)
        wipeOstlandetBasemapKeepDem()

        val basemap = File(dataDir, "pmtiles/$BASEMAP")
        check(!basemap.exists()) { "basemap still present after wipe: ${basemap.absolutePath}" }
        val staged = File(OfflineDataIntegrity.STAGED_FIXTURES_DIR, BASEMAP)
        check(staged.isFile) { "must keep staged fixture ${staged.absolutePath}" }

        val planet = pmtilesPlanetUrl()
        log("FRESH_QUEUE region=$REGION planet=$planet")
        val job = pmtilesQueueRegion(dataDir.absolutePath, REGION, planet)
        check(job.id.isNotBlank() && !job.status.startsWith("failed")) {
            "queue failed id=${job.id} status=${job.status}"
        }
        log("FRESH_QUEUED id=${job.id} path=${job.localPath}")

        val started = System.currentTimeMillis()
        val done = pmtilesRunJob(dataDir.absolutePath, job.id)
        val elapsedMs = System.currentTimeMillis() - started
        log(
            "FRESH_DONE status=${done.status} bytes=${done.bytesReceived} " +
                "total=${done.totalBytes} elapsedMs=$elapsedMs path=${done.localPath}",
        )
        check(done.status == "completed") {
            "fresh download not completed: status=${done.status} elapsedMs=$elapsedMs"
        }
        check(basemap.isFile && basemap.length() > 1_000_000L) {
            "fresh basemap missing/small bytes=${basemap.length()}"
        }
        log(
            "FRESH_SIZE freshBytes=${basemap.length()} stagedBytes=${staged.length()} " +
                "match=${basemap.length() == staged.length()}",
        )

        val green = shootEspa(DEVICE_FRESH_BEFORE)
        log(
            "FOREST_FRESH_BEFORE greenFrac=$green path=$DEVICE_FRESH_BEFORE " +
                "basemapBytes=${basemap.length()} kind=${NaviMapTestHooks.lastBasemapKind}",
        )
        File(dataDir, "forest_fresh_before_green.txt").writeText(
            "greenFrac=$green\nbasemapBytes=${basemap.length()}\nstagedBytes=${staged.length()}\n" +
                "path=$DEVICE_FRESH_BEFORE\nelapsedMs=$elapsedMs\n",
        )
    }

    @Test
    fun capture_espa_staged_restore_after_style_fix() {
        val report = OfflinePmtilesBootstrap.restoreOstlandetFromStaging(dataDir)
        check(report.startsWith("OK:")) { report }
        val green = shootEspa(DEVICE_RESTORE_AFTER)
        log(
            "FOREST_RESTORE_AFTER greenFrac=$green path=$DEVICE_RESTORE_AFTER " +
                "kind=${NaviMapTestHooks.lastBasemapKind}",
        )
        File(dataDir, "forest_restore_after_green.txt").writeText(
            "greenFrac=$green\npath=$DEVICE_RESTORE_AFTER\n",
        )
        assertTrue(
            "expected forest/wood green after style fix (greenFrac=$green)",
            green >= 0.05,
        )
    }

    @Test
    fun capture_espa_fresh_download_after_style_fix() {
        val basemap = File(dataDir, "pmtiles/$BASEMAP")
        check(basemap.isFile && basemap.length() > 1_000_000L) {
            "need existing fresh Ostlandet basemap; got ${basemap.length()}"
        }
        val staged = File(OfflineDataIntegrity.STAGED_FIXTURES_DIR, BASEMAP)
        // Fresh extract differs in size from staged fixture.
        check(basemap.length() != staged.length()) {
            "basemap size matches staged — run wipe_fresh_download first " +
                "(fresh=${basemap.length()} staged=${staged.length()})"
        }
        val green = shootEspa(DEVICE_FRESH_AFTER)
        log(
            "FOREST_FRESH_AFTER greenFrac=$green path=$DEVICE_FRESH_AFTER " +
                "basemapBytes=${basemap.length()} kind=${NaviMapTestHooks.lastBasemapKind}",
        )
        File(dataDir, "forest_fresh_after_green.txt").writeText(
            "greenFrac=$green\nbasemapBytes=${basemap.length()}\npath=$DEVICE_FRESH_AFTER\n",
        )
        assertTrue(
            "expected forest/wood green after style fix (greenFrac=$green)",
            green >= 0.05,
        )
    }

    private fun wipeOstlandetBasemapKeepDem() {
        val pm = File(dataDir, "pmtiles").also { it.mkdirs() }
        val jobs = pmtilesListJobs(dataDir.absolutePath)
        for (j in jobs) {
            val path = j.localPath
            val isBasemap =
                path.endsWith(BASEMAP) ||
                    (j.regionKey.contains("ostlandet") && !path.contains("_dem"))
            if (isBasemap) {
                log("DELETE_JOB id=${j.id} status=${j.status} path=$path")
                runCatching { pmtilesDeleteJob(dataDir.absolutePath, j.id) }
            }
        }
        File(pm, BASEMAP).delete()
        File(pm, "$BASEMAP.partial").delete()
        File(pm, "$BASEMAP.chunks").deleteRecursively()
        // Partial artifacts with region-key naming.
        pm.listFiles()?.forEach { f ->
            if (f.name.startsWith("europe_norway_ostlandet") &&
                !f.name.contains("_dem") &&
                f.name != DEM
            ) {
                if (f.isDirectory) f.deleteRecursively() else f.delete()
            }
        }
        val dem = File(pm, DEM)
        log(
            "WIPE_DONE basemapGone=${!File(pm, BASEMAP).exists()} demKept=${dem.isFile} " +
                "demBytes=${if (dem.isFile) dem.length() else 0L}",
        )
    }

    private fun shootEspa(devicePath: String): Double {
        runCatching { activityRule.finishActivity() }
        Thread.sleep(500)
        MapHudPrefs.saveOptIn3d(context, false)
        MapHudPrefs.saveCameraTiltDeg(context, 0.0)
        NaviMapTestHooks.requestOptIn3d = false
        NaviMapTestHooks.requestCameraTiltDeg = 0.0
        NaviMapTestHooks.styleReady = false
        NaviMapTestHooks.lastBasemapKind = ""
        NaviMapTestHooks.forceOnlineBasemap = false
        NaviMapTestHooks.hideUiChrome = true
        NaviMapTestHooks.pendingCamera = Triple(ESPA_LAT, ESPA_LON, ESPA_ZOOM)
        activityRule.launchActivity(null)

        val deadline = System.currentTimeMillis() + 90_000
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.styleReady &&
                NaviMapTestHooks.lastBasemapKind == "OfflineProtomaps"
            ) {
                break
            }
            NaviMapTestHooks.pendingCamera = Triple(ESPA_LAT, ESPA_LON, ESPA_ZOOM)
            NaviMapTestHooks.requestOptIn3d = false
            Thread.sleep(400)
        }
        assertTrue(
            "offline not ready kind=${NaviMapTestHooks.lastBasemapKind} " +
                "styleReady=${NaviMapTestHooks.styleReady}",
            NaviMapTestHooks.styleReady &&
                NaviMapTestHooks.lastBasemapKind == "OfflineProtomaps",
        )

        val camDeadline = System.currentTimeMillis() + 25_000
        while (System.currentTimeMillis() < camDeadline) {
            if (abs(NaviMapTestHooks.lastCameraZoom - ESPA_ZOOM) < 0.08 &&
                abs(NaviMapTestHooks.lastCameraLat - ESPA_LAT) < 0.001 &&
                abs(NaviMapTestHooks.lastCameraLon - ESPA_LON) < 0.001
            ) {
                break
            }
            NaviMapTestHooks.pendingCamera = Triple(ESPA_LAT, ESPA_LON, ESPA_ZOOM)
            Thread.sleep(300)
        }
        assertTrue(InstrumentedMapCapture.awaitRenderSettled(30_000))
        Thread.sleep(2_500)
        assertTrue(InstrumentedMapCapture.awaitRenderSettled(20_000))

        InstrumentedMapCapture.screencapAfterSettle(devicePath, timeoutMs = 8_000)
        val shot =
            InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
                ?: decodeDevicePng(devicePath)
        val green = greenFrac(shot)
        log(
            "SHOT path=$devicePath greenFrac=$green " +
                "camera=${NaviMapTestHooks.lastCameraLat},${NaviMapTestHooks.lastCameraLon}," +
                "${NaviMapTestHooks.lastCameraZoom} kind=${NaviMapTestHooks.lastBasemapKind} " +
                "terrain=${NaviMapTestHooks.lastTerrainAttached}",
        )
        return green
    }

    private fun decodeDevicePng(path: String): Bitmap {
        val bytes = File(path).readBytes()
        return BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
            ?: error("failed to decode $path")
    }

    /**
     * Fraction of center-70% map pixels that look green (forest/wood blend).
     * Wood fill `#a8d090` at 0.5 opacity over cream earth often lands near
     * (175,190,161) where g-r == 15 — use >= so muted greens count.
     */
    private fun greenFrac(bmp: Bitmap): Double {
        val x0 = (bmp.width * 0.15).toInt()
        val x1 = (bmp.width * 0.85).toInt()
        val y0 = (bmp.height * 0.15).toInt()
        val y1 = (bmp.height * 0.85).toInt()
        var n = 0
        var green = 0
        val step = 4
        var y = y0
        while (y < y1) {
            var x = x0
            while (x < x1) {
                val c = bmp.getPixel(x, y)
                val r = Color.red(c)
                val g = Color.green(c)
                val b = Color.blue(c)
                n++
                if (g >= r + 15 && g > b + 8 && g > 90) {
                    green++
                }
                x += step
            }
            y += step
        }
        return if (n == 0) 0.0 else green.toDouble() / n
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

    private fun log(msg: String) {
        Log.i(TAG, msg)
    }

    companion object {
        private const val TAG = "NaviForestLandcover"
        private const val REGION = "europe/norway/ostlandet"
        private const val BASEMAP = "europe_norway_ostlandet.pmtiles"
        private const val DEM = "europe_norway_ostlandet_dem.pmtiles"
        private const val ESPA_LAT = 60.617
        private const val ESPA_LON = 11.167
        private const val ESPA_ZOOM = 11.0
        private const val DEVICE_RESTORE_BEFORE = "/data/local/tmp/forest_restore_before.png"
        private const val DEVICE_FRESH_BEFORE = "/data/local/tmp/forest_fresh_before.png"
        private const val DEVICE_RESTORE_AFTER = "/data/local/tmp/forest_restore_after.png"
        private const val DEVICE_FRESH_AFTER = "/data/local/tmp/forest_fresh_after.png"
    }
}
