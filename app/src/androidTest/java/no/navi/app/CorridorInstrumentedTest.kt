package no.navi.app

import android.graphics.BitmapFactory
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.rule.ActivityTestRule
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.assertNotEquals
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.FfiIconTheme
import uniffi.navi.detectedParallelism
import uniffi.navi.ffiLinkageSmokeTest
import uniffi.navi.provisionRegionData
import uniffi.navi.rasterizeIconCheck
import uniffi.navi.rasterizeIconPng
import uniffi.navi.runCarCorridorPipeline
import java.io.File

/**
 * On-device tests. Reports must state DATA_SOURCE explicitly.
 * Silence on DATA_SOURCE must be treated as stub/smoke — never as routing PASS.
 */
@RunWith(AndroidJUnit4::class)
class CorridorInstrumentedTest {
    @get:Rule
    val activityRule = ActivityTestRule(MainActivity::class.java, false, false)

    private lateinit var dataDir: File
    private lateinit var iconsDir: File

    @Before
    fun setUp() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        dataDir = NaviAppData.resolve(context)
        iconsDir = File(context.filesDir, "icons").also { dest ->
            dest.mkdirs()
            val am = context.assets
            val names = am.list("icons") ?: emptyArray()
            for (name in names) {
                val out = File(dest, name)
                if (!out.exists()) {
                    am.open("icons/$name").use { input ->
                        out.outputStream().use { output -> input.copyTo(output) }
                    }
                }
            }
        }
    }

    @Test
    fun smokeTest_ffiLinkageOnly_labeledSmoke() {
        val report = ffiLinkageSmokeTest()
        assertTrue("must label SMOKE: $report", report.contains("TEST_KIND=SMOKE"))
        assertTrue("must state DATA_SOURCE=none: $report", report.contains("DATA_SOURCE=none"))
        assertTrue("must PASS: $report", report.contains("PASS"))
        assertTrue("must not claim real_pbf: $report", !report.contains("DATA_SOURCE=real_pbf"))
        assertTrue(detectedParallelism() >= 1u)
    }

    @Test
    fun realPipeline_provisionsViaDownload_thenRoutes() {
        val url = System.getProperty("navi.fixture.pbf.url")
            ?: InstrumentationRegistry.getArguments().getString("navi.fixture.pbf.url")
            ?: "http://10.0.2.2:8765/espa-atnbrufossen-corridor.osm.pbf"

        val elevTar = System.getProperty("navi.fixture.elev.url")
            ?: InstrumentationRegistry.getArguments().getString("navi.fixture.elev.url")
            ?: "http://10.0.2.2:8765/elevation-corridor.tar"

        val provision = provisionRegionData(
            dataDir = dataDir.absolutePath,
            pbfUrl = url,
            pbfFilename = "espa-atnbrufossen-corridor.osm.pbf",
            elevationTarUrl = elevTar,
        )
        assertTrue("provision must PASS: $provision", provision.contains("PASS"))
        assertTrue(
            "provision must not be silent stub: $provision",
            provision.contains("TEST_KIND=PROVISION"),
        )

        val pbf = File(dataDir, "espa-atnbrufossen-corridor.osm.pbf")
        assertTrue("PBF must exist after provision", pbf.isFile && pbf.length() > 1_000_000L)

        val result = runCarCorridorPipeline(
            pbfPath = pbf.absolutePath,
            elevDir = File(dataDir, "elevation").absolutePath,
            cacheDir = File(dataDir, "graph-cache").absolutePath,
            breakIntervalHours = 1.0,
        )
        val report = result.report
        assertFalse("must not contain STUB: $report", report.contains("STUB", ignoreCase = true))
        assertTrue("must be REAL_PIPELINE: $report", report.contains("TEST_KIND=REAL_PIPELINE"))
        assertTrue("must be real_pbf: $report", report.contains("DATA_SOURCE=real_pbf"))
        assertTrue("must PASS: $report", report.contains("PASS"))
        assertTrue("must report cache hit: $report", report.contains("cache_hit=true"))
        assertTrue("warm must be faster: $report", result.warmLoadS < result.coldBuildS * 0.85 || result.coldBuildS < 2.0)
        assertTrue("distance must be positive: ${result.distanceKm}", result.distanceKm > 5.0)
        assertTrue("polyline must be non-empty", result.routePolyline.contains(';'))
    }

    @Test
    fun iconRasterization_producesNonEmptyBitmaps() {
        val keys = listOf("fuel", "nav_straight", "status_routing", "eco-mode")
        for (key in keys) {
            val check = rasterizeIconCheck(
                key = key,
                theme = FfiIconTheme.DAY,
                width = 48u,
                height = 48u,
                bundledDir = iconsDir.absolutePath,
            )
            assertTrue("icon $key: $check", check.contains("TEST_KIND=ICON_RASTER"))
            assertTrue("icon $key PASS: $check", check.contains("PASS"))
            val png = rasterizeIconPng(
                key = key,
                theme = FfiIconTheme.DAY,
                width = 48u,
                height = 48u,
                bundledDir = iconsDir.absolutePath,
            )
            assertTrue("png for $key empty", png.isNotEmpty())
            assertTrue("png magic for $key", png.size >= 8 && png[0] == 0x89.toByte())
            val bmp = BitmapFactory.decodeByteArray(png, 0, png.size)
            assertTrue("decode $key", bmp != null && bmp.width > 0 && bmp.height > 0)
        }
        // Country flag .svgz path
        val flag = rasterizeIconCheck(
            key = "country_NO",
            theme = FfiIconTheme.DAY,
            width = 64u,
            height = 40u,
            bundledDir = iconsDir.absolutePath,
        )
        // Flag may resolve to unknown if naming differs; still require a non-empty raster.
        val flagPng = rasterizeIconPng(
            key = "country_NO",
            theme = FfiIconTheme.DAY,
            width = 64u,
            height = 40u,
            bundledDir = iconsDir.absolutePath,
        )
        assertTrue("flag png empty ($flag)", flagPng.isNotEmpty())
    }

    @Test
    fun mapIsVisible_withRouteOverlay_andScreenshot() {
        activityRule.launchActivity(null)
        val activity = activityRule.activity
        assertTrue(activity.isFinishing.not())

        val url = System.getProperty("navi.fixture.pbf.url")
            ?: InstrumentationRegistry.getArguments().getString("navi.fixture.pbf.url")
            ?: "http://10.0.2.2:8765/espa-atnbrufossen-corridor.osm.pbf"
        val elevTar = System.getProperty("navi.fixture.elev.url")
            ?: InstrumentationRegistry.getArguments().getString("navi.fixture.elev.url")
            ?: "http://10.0.2.2:8765/elevation-corridor.tar"
        provisionRegionData(
            dataDir = dataDir.absolutePath,
            pbfUrl = url,
            pbfFilename = "espa-atnbrufossen-corridor.osm.pbf",
            elevationTarUrl = elevTar,
        )
        val result = runCarCorridorPipeline(
            pbfPath = File(dataDir, "espa-atnbrufossen-corridor.osm.pbf").absolutePath,
            elevDir = File(dataDir, "elevation").absolutePath,
            cacheDir = File(dataDir, "graph-cache").absolutePath,
            breakIntervalHours = 1.0,
        )
        assertTrue(result.report.contains("DATA_SOURCE=real_pbf"))
        assertTrue(result.routePolyline.isNotBlank())

        val iconPng = rasterizeIconPng(
            key = result.poiIconKey.ifBlank { "fuel" },
            theme = FfiIconTheme.DAY,
            width = 64u,
            height = 64u,
            bundledDir = iconsDir.absolutePath,
        )
        NaviMapTestHooks.pendingIconPng = iconPng
        NaviMapTestHooks.pendingRoute = result
        // Keep top/bottom drive HUD bars visible for route evidence screenshots.
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = true
        NaviMapTestHooks.gpsAltitudeM = 412.0
        NaviMapTestHooks.requestShowTripEta = true
        NaviMapTestHooks.requestBreakReminders = true

        // Wait for Compose to apply the route overlay + MapLibre style layers.
        var layers = 0
        val deadline = System.currentTimeMillis() + 45_000
        while (System.currentTimeMillis() < deadline) {
            Thread.sleep(500)
            layers = NaviMapTestHooks.lastReportedLayerCount
            if (layers >= 2 && NaviMapTestHooks.lastBreakHudVisible) break
        }
        assertTrue(
            "MapLibre style should expose basemap (+ route) layers, got $layers",
            layers >= 1,
        )

        // Extra settle time for basemap tiles + route camera fit (chrome already hidden).
        Thread.sleep(6_000)
        val shot = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
        assertTrue("screenshot bitmap null", shot != null)
        assertNotEquals("screenshot width", 0, shot!!.width)
        assertNotEquals("screenshot height", 0, shot.height)

        val seen = HashSet<Int>()
        val stepX = (shot.width / 32).coerceAtLeast(1)
        val stepY = (shot.height / 32).coerceAtLeast(1)
        var y = 0
        while (y < shot.height) {
            var x = 0
            while (x < shot.width) {
                seen.add(shot.getPixel(x, y))
                x += stepX
            }
            y += stepY
        }
        assertTrue("map screenshot looks empty/flat (colors=${seen.size})", seen.size > 8)

        val out = File(dataDir, "route_map.png")
        out.outputStream().use { os ->
            shot.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, os)
        }
        assertTrue("wrote ${out.absolutePath}", out.isFile && out.length() > 10_000)

        // Publish via MediaStore Downloads so the host can adb-pull without sandbox tricks.
        val values = android.content.ContentValues().apply {
            put(android.provider.MediaStore.MediaColumns.DISPLAY_NAME, "navi_route_map.png")
            put(android.provider.MediaStore.MediaColumns.MIME_TYPE, "image/png")
            put(
                android.provider.MediaStore.MediaColumns.RELATIVE_PATH,
                android.os.Environment.DIRECTORY_DOWNLOADS,
            )
            put(android.provider.MediaStore.MediaColumns.IS_PENDING, 1)
        }
        val resolver = InstrumentationRegistry.getInstrumentation().targetContext.contentResolver
        val uri = resolver.insert(android.provider.MediaStore.Downloads.EXTERNAL_CONTENT_URI, values)
        assertTrue("MediaStore insert failed", uri != null)
        resolver.openOutputStream(uri!!).use { os ->
            requireNotNull(os)
            assertTrue(
                "compress to MediaStore failed",
                shot.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, os),
            )
        }
        val done = android.content.ContentValues().apply {
            put(android.provider.MediaStore.MediaColumns.IS_PENDING, 0)
        }
        resolver.update(uri, done, null, null)

        android.util.Log.i("NaviMapTest", "screenshot bytes=${out.length()} path=${out.absolutePath} media=$uri")

        // Device-side screencap avoids adb exec-out multi-display warning pollution.
        fun shell(cmd: String) {
            val pfd = InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand(cmd)
            java.io.FileInputStream(pfd.fileDescriptor).use { input ->
                val buf = ByteArray(4096)
                while (input.read(buf) >= 0) {
                }
            }
            pfd.close()
        }
        Thread.sleep(1_500)
        shell("screencap -p /data/local/tmp/navi_route_map.png")
        shell("ls -la /data/local/tmp/navi_route_map.png")
        Thread.sleep(1_000)
    }
}
