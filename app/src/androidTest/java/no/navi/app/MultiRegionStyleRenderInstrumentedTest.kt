package no.navi.app

import android.graphics.Bitmap
import android.graphics.Color
import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.rule.ActivityTestRule
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.geofabrikLatestPbfUrl
import uniffi.navi.initNativeLogging
import java.io.File
import kotlin.math.abs

/**
 * On-device visual confirmation of the multi-region offline style URI fix.
 *
 * Downloads Østlandet → Vestlandet → Trøndelag sequentially into the live
 * app data dir, then flies the camera across each region (and revisits) at
 * low/medium/high zoom, asserting OfflineProtomaps + distinct style URIs and
 * capturing screencaps for visual review.
 *
 * Third region is Trøndelag (central Norway — different shape from the two
 * southern landsdeler; adjacent to Østlandet for seam coverage). Nordland is
 * not a Geofabrik leaf extract (stub ~9 KB), so it cannot be used here.
 */
@RunWith(AndroidJUnit4::class)
class MultiRegionStyleRenderInstrumentedTest {
    @get:Rule
    val activityRule = ActivityTestRule(MainActivity::class.java, false, false)

    private lateinit var context: android.content.Context
    private lateinit var dataDir: File
    private val report = StringBuilder()

    private companion object {
        const val TAG = "MultiRegionStyleVis"
        const val OST = "europe/norway/ostlandet"
        const val VEST = "europe/norway/vestlandet"
        const val TROND = "europe/norway/trondelag"
        const val PER_REGION_TIMEOUT_MS = 4L * 60L * 60L * 1000L
        const val START_GRACE_MS = 180_000L
        const val SHOT_DIR = "/data/local/tmp/navi_multi_region_style"

        // Distinct interiors (not on shared borders).
        val OST_CAM = Triple(59.9139, 10.7522, 0.0) // Oslo
        val VEST_CAM = Triple(60.3913, 5.3221, 0.0) // Bergen
        val TROND_CAM = Triple(63.4305, 10.3951, 0.0) // Trondheim

        // Approx Østlandet / Vestlandet mountain seam (Filefjell corridor).
        val SEAM_OST_VEST = Triple(61.15, 8.05, 0.0)
    }

    private lateinit var reportPath: String
    private lateinit var shotDir: File

    @Before
    fun setUp() {
        initNativeLogging()
        context = InstrumentationRegistry.getInstrumentation().targetContext
        dataDir = NaviAppData.resolve(context)
        reportPath = File(context.cacheDir, "navi_multi_region_style_report.txt").absolutePath
        shotDir = File(context.cacheDir, "navi_multi_region_style").also { it.mkdirs() }
        // World-readable mirror for host adb pull when possible.
        runCatching {
            shell("mkdir -p $SHOT_DIR")
            shell("chmod 777 $SHOT_DIR")
        }
        val auto = InstrumentationRegistry.getInstrumentation().uiAutomation
        auto.grantRuntimePermission(context.packageName, android.Manifest.permission.ACCESS_FINE_LOCATION)
        auto.grantRuntimePermission(context.packageName, android.Manifest.permission.ACCESS_COARSE_LOCATION)
        NaviMapTestHooks.hideUiChrome = true
        NaviMapTestHooks.disableGpsFollow = true
        NaviMapTestHooks.forceOnlineBasemap = false
        NaviMapTestHooks.styleReady = false
        MapHudPrefs.saveOptIn3d(context, false)
        MapHudPrefs.saveCameraTiltDeg(context, 0.0)
        report.appendLine("dataDir=${dataDir.absolutePath}")
        report.appendLine("started=${System.currentTimeMillis()}")
    }

    @After
    fun tearDown() {
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.disableGpsFollow = false
        NaviMapTestHooks.forceOnlineBasemap = false
        setWifi(true)
        runCatching { activityRule.finishActivity() }
        runCatching { File(reportPath).writeText(report.toString()) }
        runCatching {
            shell("cp $reportPath /data/local/tmp/navi_multi_region_style_report.txt")
            shell("chmod 644 /data/local/tmp/navi_multi_region_style_report.txt")
        }
        Log.i(TAG, "REPORT_WRITTEN $reportPath\n$report")
    }

    @Test
    fun download_three_regions_then_visual_style_switch_check() {
        // Drop leftover queue/sidecar from prior aborted runs so drain starts clean.
        File(dataDir, RegionDownloadBackground.QUEUE_FILE).delete()
        File(dataDir, RegionDownloadBackground.JOB_FILE).delete()
        File(dataDir, "region-download-queue-loc.json").delete()

        log("BEGIN sequential downloads")
        val finishedPaths = mutableListOf<String>()
        downloadRegion(OST, "ostlandet-latest.osm.pbf", finishedPaths)
        visualSweep("after_ost", OST, OST_CAM, expectKey = "ostlandet")

        downloadRegion(VEST, "vestlandet-latest.osm.pbf", finishedPaths)
        visualSweep("after_vest", VEST, VEST_CAM, expectKey = "vestlandet")
        visualSweep("revisit_ost_after_vest", OST, OST_CAM, expectKey = "ostlandet")

        downloadRegion(TROND, "trondelag-latest.osm.pbf", finishedPaths)
        visualSweep("after_trond", TROND, TROND_CAM, expectKey = "trondelag")
        visualSweep("revisit_ost_after_trond", OST, OST_CAM, expectKey = "ostlandet")
        visualSweep("revisit_vest_after_trond", VEST, VEST_CAM, expectKey = "vestlandet")

        assertEquals(
            "each region must finish exactly once (no queue re-drain)",
            listOf(OST, VEST, TROND),
            finishedPaths,
        )

        // Adjacent-pair seam (Østlandet / Vestlandet).
        seamCheck()

        assertStyleFilesDistinctOnDisk()
        assertSimultaneousSingleStyleDesign()
        report.appendLine("OVERALL=PASS")
        log("PASS overall")
    }

    private fun downloadRegion(
        path: String,
        filename: String,
        finishedPaths: MutableList<String>,
    ) {
        awaitWifiConnected()
        if (PlaceIndexReady.isReady(dataDir, path) &&
            PackRegionAvailability.localPmtilesReady(dataDir, path) &&
            PackRegionAvailability.localBakeReady(dataDir, path)
        ) {
            log("DOWNLOAD_SKIP already ready path=$path")
            report.appendLine("download_skip path=$path reason=already_ready")
            finishedPaths.add(path)
            MapHudPrefs.rememberDownloadedPmtilesRegion(
                context,
                PackRegionAvailability.geofabrikPathToRegionKey(path),
            )
            return
        }
        val url = geofabrikLatestPbfUrl(path)
        log("DOWNLOAD_START path=$path")
        report.appendLine("download_start path=$path")
        RegionDownloadBackground.ensureStarted(context, dataDir, url, filename, path)

        val t0 = System.currentTimeMillis()
        var sawRunning = false
        while (System.currentTimeMillis() - t0 < START_GRACE_MS) {
            if (RegionDownloadBackground.isRunning()) {
                sawRunning = true
                break
            }
            Thread.sleep(250)
        }
        assertTrue("download did not start for $path", sawRunning)

        var finished = false
        while (System.currentTimeMillis() - t0 < PER_REGION_TIMEOUT_MS) {
            val running = RegionDownloadBackground.isRunning()
            val status = RegionDownloadBackground.statusLine()
            val ui = RegionDownloadBackground.uiLine()
            val elapsed = System.currentTimeMillis() - t0
            if (elapsed % 60_000L < 3_000L) {
                log("progress path=$path running=$running status=$status ui=$ui elapsed_s=${elapsed / 1000}")
            }
            if (sawRunning && !running && (status == "done" || status.startsWith("failed") || status.startsWith("done "))) {
                finished = true
                log("DOWNLOAD_END path=$path status=$status")
                report.appendLine("download_end path=$path status=$status elapsed_s=${elapsed / 1000}")
                break
            }
            Thread.sleep(3_000)
        }
        assertTrue("timed out downloading $path", finished)
        val finalStatus = RegionDownloadBackground.statusLine()
        assertTrue(
            "expected done for $path, got $finalStatus",
            finalStatus == "done" || finalStatus.startsWith("done "),
        )
        assertTrue(
            "place-index ready missing for $path",
            PlaceIndexReady.isReady(dataDir, path),
        )
        assertTrue(
            "basemap missing for $path",
            PackRegionAvailability.localPmtilesReady(dataDir, path),
        )
        // Queue must be empty of this path after a single successful drain.
        assertTrue(
            "queue must not still contain $path after completion",
            RegionDownloadBackground.loadQueue(dataDir).none {
                PackRegionAvailability.regionIdsMatchForCatalog(it.geofabrikPath, path)
            },
        )
        finishedPaths.add(path)
        MapHudPrefs.rememberDownloadedPmtilesRegion(
            context,
            PackRegionAvailability.geofabrikPathToRegionKey(path),
        )
        report.appendLine("download_ok path=$path")
    }

    private fun visualSweep(
        label: String,
        path: String,
        cam: Triple<Double, Double, Double>,
        expectKey: String,
    ) {
        setWifi(false)
        val zooms = listOf(6.0 to "low", 10.0 to "mid", 14.0 to "high")
        val uris = mutableListOf<String>()
        for ((zoom, zName) in zooms) {
            val shotPath = File(shotDir, "${label}_$zName.png").absolutePath
            val mirrorPath = "$SHOT_DIR/${label}_$zName.png"
            val (resolved, variance) =
                shoot(
                    lat = cam.first,
                    lon = cam.second,
                    zoom = zoom,
                    devicePath = shotPath,
                    mirrorPath = mirrorPath,
                )
            assertEquals(
                "basemap kind at $label/$zName",
                "OfflineProtomaps",
                NaviMapTestHooks.lastBasemapKind,
            )
            assertTrue(
                "style URI must mention $expectKey at $label/$zName: ${resolved.styleUri}",
                resolved.styleUri.contains(expectKey, ignoreCase = true),
            )
            assertTrue(
                "style must point at $expectKey pmtiles",
                styleJsonPointsAtRegion(resolved.styleUri, expectKey),
            )
            assertSpritePathsPresent(resolved.styleUri)
            assertTrue(
                "map looks blank/uniform at $label/$zName variance=$variance",
                variance > 80.0,
            )
            uris.add(resolved.styleUri)
            report.appendLine(
                "shot=$label/$zName uri=${resolved.styleUri} variance=$variance path=$shotPath",
            )
            log("SHOT_OK $label/$zName variance=$variance uri=${resolved.styleUri}")
        }
        assertEquals("style URI stable across zooms for $label", 1, uris.toSet().size)
        report.appendLine("visual_pass label=$label path=$path")
    }

    private fun seamCheck() {
        setWifi(false)
        val shotPath = File(shotDir, "seam_ost_vest_mid.png").absolutePath
        val (resolved, variance) =
            shoot(
                lat = SEAM_OST_VEST.first,
                lon = SEAM_OST_VEST.second,
                zoom = 9.0,
                devicePath = shotPath,
                mirrorPath = "$SHOT_DIR/seam_ost_vest_mid.png",
            )
        assertEquals("OfflineProtomaps", NaviMapTestHooks.lastBasemapKind)
        // Single active style by design — whichever covering archive wins.
        val key =
            when {
                resolved.styleUri.contains("ostlandet", true) -> "ostlandet"
                resolved.styleUri.contains("vestlandet", true) -> "vestlandet"
                else -> "other"
            }
        report.appendLine(
            "seam_ost_vest uri=${resolved.styleUri} winner=$key variance=$variance " +
                "note=single_active_style_by_design",
        )
        assertTrue("seam shot looks blank variance=$variance", variance > 80.0)
        log("SEAM_OK winner=$key uri=${resolved.styleUri}")
    }

    private fun assertStyleFilesDistinctOnDisk() {
        val styleDir = File(context.filesDir, "map-styles/protomaps-light")
        val styles =
            styleDir
                .listFiles()
                ?.filter {
                    it.name.startsWith("style.local.v3.") && it.name.endsWith(".json")
                }.orEmpty()
        report.appendLine("style_files=${styles.map { it.name }}")
        assertTrue(
            "expected >=3 unique offline style files, got ${styles.map { it.name }}",
            styles.size >= 3,
        )
        val stems = styles.map { it.name }.toSet()
        assertTrue(stems.any { it.contains("ostlandet") })
        assertTrue(stems.any { it.contains("vestlandet") })
        assertTrue(stems.any { it.contains("trondelag") })
    }

    private fun assertSimultaneousSingleStyleDesign() {
        // Document design: resolve() picks one covering vector job.
        val coveringOst =
            BasemapStyleResolver.resolve(
                context,
                dataDir,
                OST_CAM.first,
                OST_CAM.second,
                prefer3d = false,
                vulkanAvailable = true,
            )
        val coveringSeam =
            BasemapStyleResolver.resolve(
                context,
                dataDir,
                SEAM_OST_VEST.first,
                SEAM_OST_VEST.second,
                prefer3d = false,
                vulkanAvailable = true,
            )
        report.appendLine(
            "design_single_style ostUri=${coveringOst.styleUri} seamUri=${coveringSeam.styleUri} " +
                "composited_multi_source=false",
        )
        assertNotEquals(
            "interior Oslo and seam should not both be Liberty",
            BasemapStyleResolver.LIBERTY_URL,
            coveringOst.styleUri,
        )
    }

    private fun shoot(
        lat: Double,
        lon: Double,
        zoom: Double,
        devicePath: String,
        mirrorPath: String? = null,
    ): Pair<BasemapStyleResolver.ResolvedStyle, Double> {
        runCatching { activityRule.finishActivity() }
        Thread.sleep(400)
        NaviMapTestHooks.styleReady = false
        NaviMapTestHooks.lastBasemapKind = ""
        NaviMapTestHooks.lastStyleLoadError = null
        NaviMapTestHooks.forceOnlineBasemap = false
        NaviMapTestHooks.hideUiChrome = true
        NaviMapTestHooks.disableGpsFollow = true
        NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
        activityRule.launchActivity(null)

        val deadline = System.currentTimeMillis() + 120_000
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.styleReady &&
                NaviMapTestHooks.lastBasemapKind == "OfflineProtomaps"
            ) {
                break
            }
            NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
            Thread.sleep(400)
        }
        assertTrue(
            "offline style not ready kind=${NaviMapTestHooks.lastBasemapKind} " +
                "err=${NaviMapTestHooks.lastStyleLoadError}",
            NaviMapTestHooks.styleReady &&
                NaviMapTestHooks.lastBasemapKind == "OfflineProtomaps",
        )

        val camDeadline = System.currentTimeMillis() + 30_000
        while (System.currentTimeMillis() < camDeadline) {
            if (abs(NaviMapTestHooks.lastCameraZoom - zoom) < 0.15 &&
                abs(NaviMapTestHooks.lastCameraLat - lat) < 0.02 &&
                abs(NaviMapTestHooks.lastCameraLon - lon) < 0.02
            ) {
                break
            }
            NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
            Thread.sleep(300)
        }
        assertTrue(InstrumentedMapCapture.awaitRenderSettled(35_000))
        Thread.sleep(1_500)
        assertTrue(InstrumentedMapCapture.awaitRenderSettled(20_000))
        InstrumentedMapCapture.screencapAfterSettle(devicePath, timeoutMs = 8_000)
        val live =
            InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
        if (live != null) {
            val out = File(devicePath)
            out.parentFile?.mkdirs()
            out.outputStream().use { live.compress(Bitmap.CompressFormat.PNG, 100, it) }
            if (mirrorPath != null) {
                shell("cp ${out.absolutePath} $mirrorPath")
                shell("chmod 644 $mirrorPath")
            }
        }

        val resolved =
            BasemapStyleResolver.resolve(
                context = context,
                dataDir = dataDir,
                lat = lat,
                lon = lon,
                prefer3d = false,
                vulkanAvailable = true,
            )
        assertEquals(BasemapStyleResolver.StyleKind.OfflineProtomaps, resolved.kind)
        return resolved to (live?.let { sampleVariance(it) } ?: bitmapVariance(devicePath))
    }

    private fun styleJsonPointsAtRegion(
        styleUri: String,
        expectKey: String,
    ): Boolean {
        val f = File(styleUri.removePrefix("file://"))
        if (!f.isFile) return false
        // JSONObject.toString() escapes slashes (pmtiles:\/\/file:\/\/\/…), so
        // do not require a literal "pmtiles://file://" substring.
        val text = f.readText()
        val unescaped = text.replace("\\/", "/")
        return unescaped.contains(expectKey, ignoreCase = true) &&
            unescaped.contains(".pmtiles") &&
            (unescaped.contains("pmtiles://") || unescaped.contains("\"protomaps\""))
    }

    private fun assertSpritePathsPresent(styleUri: String) {
        val f = File(styleUri.removePrefix("file://"))
        assertTrue(f.isFile)
        val text = f.readText()
        assertTrue("sprite path missing in $styleUri", text.contains("sprites"))
        // Sprites are shared prepared assets (same atlas for all regions by design).
        val spriteDir = File(context.filesDir, "map-styles/protomaps-light/sprites/light")
        assertTrue(
            "shared sprite atlas missing under ${spriteDir.absolutePath}",
            spriteDir.exists() ||
                File(context.filesDir, "map-styles/protomaps-light/sprites").exists(),
        )
        report.appendLine(
            "sprites=shared_prepared_assets atlas_dir_exists=${spriteDir.parentFile?.exists()} " +
                "style=$styleUri",
        )
    }

    private fun bitmapVariance(devicePath: String): Double {
        val bytes = adbPullBytes(devicePath) ?: return 0.0
        val bmp =
            android.graphics.BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
                ?: return 0.0
        return sampleVariance(bmp)
    }

    private fun sampleVariance(bmp: Bitmap): Double {
        var n = 0
        var sum = 0.0
        var sumSq = 0.0
        val stepX = (bmp.width / 48).coerceAtLeast(1)
        val stepY = (bmp.height / 48).coerceAtLeast(1)
        var y = 0
        while (y < bmp.height) {
            var x = 0
            while (x < bmp.width) {
                val c = bmp.getPixel(x, y)
                val lum =
                    (0.2126 * Color.red(c)) +
                        (0.7152 * Color.green(c)) +
                        (0.0722 * Color.blue(c))
                sum += lum
                sumSq += lum * lum
                n++
                x += stepX
            }
            y += stepY
        }
        if (n == 0) return 0.0
        val mean = sum / n
        return (sumSq / n) - (mean * mean)
    }

    private fun adbPullBytes(devicePath: String): ByteArray? {
        // Screencap is already on device; read via UiAutomation shell cat is awkward for binary.
        // Prefer copying into app-accessible cache via shell.
        val local = File(context.cacheDir, File(devicePath).name)
        shell("cp $devicePath ${local.absolutePath}")
        // cp into app cache may fail (SELinux); fall back to /data/local/tmp read via File if world-readable.
        val fromTmp = File(devicePath)
        val src =
            when {
                local.isFile && local.length() > 1000L -> local
                fromTmp.isFile && fromTmp.length() > 1000L -> fromTmp
                else -> {
                    // Last resort: UiAutomation screenshot instead of screencap file.
                    return null
                }
            }
        return src.readBytes()
    }

    private fun setWifi(enabled: Boolean) {
        shell(if (enabled) "svc wifi enable" else "svc wifi disable")
        Thread.sleep(1_000)
    }

    /** Wait until Wi-Fi is up enough for pack-server / Geofabrik HTTP. */
    private fun awaitWifiConnected(timeoutMs: Long = 90_000) {
        setWifi(true)
        shell("cmd connectivity airplane-mode disable")
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            val wifi =
                shellOut("dumpsys wifi | grep -m1 'mNetworkInfo'")
                    .ifBlank { shellOut("dumpsys wifi | grep -m1 'Wifi is enabled'") }
            val connected =
                wifi.contains("CONNECTED", ignoreCase = true) ||
                    shellOut("ping -c 1 -W 2 8.8.8.8").contains("1 received") ||
                    shellOut("ping -c 1 -W 2 download.geofabrik.de").contains("1 received")
            if (connected) {
                log("wifi_ready info=${wifi.take(120)}")
                Thread.sleep(2_000)
                return
            }
            Thread.sleep(2_000)
        }
        // Last-chance: still proceed; download will fail loudly if offline.
        log("wifi_wait_timeout continuing anyway")
    }

    private fun shellOut(cmd: String): String {
        val pfd =
            InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand(cmd)
        return java.io
            .FileInputStream(pfd.fileDescriptor)
            .use { input ->
                input.readBytes().toString(Charsets.UTF_8)
            }.also { pfd.close() }
    }

    private fun shell(cmd: String) {
        val pfd =
            InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand(cmd)
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
}
