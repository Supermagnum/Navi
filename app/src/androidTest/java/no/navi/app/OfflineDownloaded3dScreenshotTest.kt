package no.navi.app

import android.graphics.Bitmap
import android.graphics.Color
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.rule.ActivityTestRule
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import kotlin.math.abs
import kotlin.math.sqrt

/**
 * SM-P613: opt-in 3D must render from **downloaded** Protomaps + local
 * Mapterhorn DEM (loopback HTTP tiles). Checks DEM tiles are actually fetched,
 * land is not the olive encoding wash, and luminance varies from hillshade.
 */
@RunWith(AndroidJUnit4::class)
class OfflineDownloaded3dScreenshotTest {
    @get:Rule
    val activityRule = ActivityTestRule(MainActivity::class.java, false, false)

    private lateinit var dataDir: File
    private lateinit var context: android.content.Context

    private val lat = 61.493
    private val lon = 8.351
    private val zoom = 12.0

    @Before
    fun setUp() {
        context = InstrumentationRegistry.getInstrumentation().targetContext
        dataDir = NaviAppData.resolve(context)
        val auto = InstrumentationRegistry.getInstrumentation().uiAutomation
        auto.grantRuntimePermission(context.packageName, android.Manifest.permission.ACCESS_FINE_LOCATION)
        auto.grantRuntimePermission(context.packageName, android.Manifest.permission.ACCESS_COARSE_LOCATION)
        OstlandetOfflineFixtures.ensureInstalled(dataDir)
        val dem = File(dataDir, "pmtiles/europe_norway_ostlandet_dem.pmtiles")
        assertTrue("local DEM required: ${dem.absolutePath}", dem.isFile && dem.length() > 1_000_000)
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = true
        NaviMapTestHooks.forceOnlineBasemap = false
        NaviMapTestHooks.disableGpsFollow = true
        NaviMapTestHooks.followGps = false
        NaviMapTestHooks.gpsAltitudeM = 1000.0
        MapHudPrefs.saveOptIn3d(context, true)
        MapHudPrefs.saveCameraTiltDeg(context, 45.0)
        // Do not enable airplane mode: on SM-P613 MapLibre's HTTP stack does not
        // fetch 127.0.0.1 DEM tiles while airplane mode is on (hitsOk stays 0),
        // which previously false-passed this test on cream earth with no hillshade.
        // Fixtures are local PMTiles; network is unused when OfflineProtomaps resolves.
        LocalDemTileServer.resetHitCounts()
    }

    @After
    fun tearDown() {
        NaviMapTestHooks.localDemMapboxConversion = false
        NaviMapTestHooks.disableGpsFollow = false
        NaviMapTestHooks.followGps = true
        MapHudPrefs.saveOptIn3d(context, false)
        MapHudPrefs.saveCameraTiltDeg(context, 0.0)
    }

    private fun cameraAtGjendebu(): Boolean =
        abs(NaviMapTestHooks.lastCameraLat - lat) <= 0.05 &&
            abs(NaviMapTestHooks.lastCameraLon - lon) <= 0.05

    private fun waitReady() {
        NaviMapTestHooks.disableGpsFollow = true
        NaviMapTestHooks.followGps = false
        NaviMapTestHooks.requestOptIn3d = true
        NaviMapTestHooks.requestCameraTiltDeg = 45.0
        NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
        val deadline = System.currentTimeMillis() + 60_000
        while (System.currentTimeMillis() < deadline) {
            val kind = NaviMapTestHooks.lastBasemapKind
            val pitchOk = abs(NaviMapTestHooks.lastCameraPitch - 45.0) <= 2.5
            if (kind == "OfflineProtomaps" &&
                NaviMapTestHooks.lastTerrainAttached &&
                pitchOk &&
                cameraAtGjendebu() &&
                NaviMapTestHooks.styleReady
            ) {
                return
            }
            NaviMapTestHooks.disableGpsFollow = true
            NaviMapTestHooks.followGps = false
            NaviMapTestHooks.requestOptIn3d = true
            NaviMapTestHooks.requestCameraTiltDeg = 45.0
            NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
            Thread.sleep(400)
        }
    }

    /** Land-pixel stats: enough chroma + luminance spread => colors + hillshade. */
    private fun analyzeLand(bmp: Bitmap): Triple<Double, Double, Int> {
        var n = 0
        var sumL = 0.0
        var sumL2 = 0.0
        var colorful = 0
        val step = 8
        val w = bmp.width
        val h = bmp.height
        // Skip chrome: sample central map band.
        val y0 = (h * 0.18).toInt()
        val y1 = (h * 0.82).toInt()
        val x0 = (w * 0.08).toInt()
        val x1 = (w * 0.92).toInt()
        var y = y0
        while (y < y1) {
            var x = x0
            while (x < x1) {
                val c = bmp.getPixel(x, y)
                val r = Color.red(c)
                val g = Color.green(c)
                val b = Color.blue(c)
                // Skip near-white UI / near-pure water blue.
                val isWater = b > 140 && b > r + 25 && b > g + 15
                val isUiChrome = r > 220 && g > 220 && b > 220
                if (!isWater && !isUiChrome) {
                    val l = (0.2126 * r + 0.7152 * g + 0.0722 * b)
                    sumL += l
                    sumL2 += l * l
                    n++
                    val chroma = maxOf(r, g, b) - minOf(r, g, b)
                    if (chroma >= 12) colorful++
                }
                x += step
            }
            y += step
        }
        require(n > 200) { "too few land samples: $n" }
        val mean = sumL / n
        val varL = (sumL2 / n) - mean * mean
        val std = sqrt(varL.coerceAtLeast(0.0))
        return Triple(std, colorful.toDouble() / n, n)
    }

    @Test
    fun gjendebu_offline_3d_colors_and_hillshade_from_downloads_only() {
        val direct =
            BasemapStyleResolver.resolve(
                context = context,
                dataDir = dataDir,
                lat = lat,
                lon = lon,
                prefer3d = true,
                vulkanAvailable = true,
            )
        assertEquals(BasemapStyleResolver.StyleKind.OfflineProtomaps, direct.kind)
        assertTrue(
            "offline 3D uses baked style JSON, not runtime attach",
            !direct.attachMapterhornTerrain,
        )
        assertTrue(
            "DEM must be loopback TileJSON (terrarium), got ${direct.demSourceUri}",
            direct.demSourceUri != null &&
                direct.demSourceUri!!.startsWith("http://127.0.0.1:") &&
                direct.demSourceUri!!.contains("/tilejson.json"),
        )
        val styleFile = File(direct.styleUri.removePrefix("file://"))
        val styleText = styleFile.readText()
        assertTrue(styleText.contains(MapterhornTerrain.HILLS_LAYER_ID))
        assertTrue(styleText.contains("\"encoding\"") && styleText.contains("terrarium"))

        activityRule.launchActivity(null)
        Thread.sleep(3_000)
        assertTrue(InstrumentedMapCapture.awaitStyleReady(60_000))
        waitReady()

        assertEquals("OfflineProtomaps", NaviMapTestHooks.lastBasemapKind)
        assertTrue(NaviMapTestHooks.lastTerrainAttached)
        assertTrue(
            "camera must stay at Gjendebu before capture (lat=${NaviMapTestHooks.lastCameraLat} " +
                "lon=${NaviMapTestHooks.lastCameraLon})",
            cameraAtGjendebu(),
        )

        assertTrue(InstrumentedMapCapture.awaitRenderSettled(35_000))
        // Local DEM tiles need a beat after attach before hillshade is visible.
        Thread.sleep(8_000)
        assertTrue(InstrumentedMapCapture.awaitRenderSettled(20_000))
        val shot = InstrumentedMapCapture.takeScreenshotAfterSettle(10_000)
        require(shot != null && shot.width > 0) { "null screenshot" }

        val out = File(dataDir, "offline_downloaded_3d_gjendebu.png")
        out.outputStream().use {
            shot.compress(Bitmap.CompressFormat.PNG, 100, it)
        }
        InstrumentedMapCapture.screencapAfterSettle(
            "/data/local/tmp/offline_downloaded_3d_gjendebu.png",
            5_000,
        )

        val (std, colorfulFrac, n) = analyzeLand(shot)
        val washFrac = washFraction(shot)
        val creamFrac = creamFraction(shot)
        val demHitsOk =
            maxOf(LocalDemTileServer.hitsOk, NaviMapTestHooks.localDemHitsOk)
        val demHitsMiss =
            maxOf(LocalDemTileServer.hitsMiss, NaviMapTestHooks.localDemHitsMiss)
        android.util.Log.i(
            "OfflineDownloaded3d",
            "PASS kind=${NaviMapTestHooks.lastBasemapKind} terrain=true " +
                "pitch=${NaviMapTestHooks.lastCameraPitch} " +
                "camLat=${NaviMapTestHooks.lastCameraLat} camLon=${NaviMapTestHooks.lastCameraLon} " +
                "land_n=$n lum_std=$std " +
                "colorful_frac=$colorfulFrac washFrac=$washFrac creamFrac=$creamFrac " +
                "dem=${direct.demSourceUri} bytes=${out.length()} " +
                "demHitsOk=$demHitsOk demHitsMiss=$demHitsMiss " +
                "elev=[${LocalDemTileServer.lastElevMin},${LocalDemTileServer.lastElevMax}] " +
                "tile=${LocalDemTileServer.lastDecodedWidth}x${LocalDemTileServer.lastDecodedHeight} " +
                "rtMaxErr=${LocalDemTileServer.lastRoundtripMaxError} " +
                "pngRtMaxErr=${LocalDemTileServer.lastPngRoundtripMaxError}",
        )

        assertTrue(
            "MapLibre must fetch local DEM tiles (hitsOk=$demHitsOk miss=$demHitsMiss)",
            demHitsOk >= 1,
        )
        assertEquals(
            "decoded terrarium tile must be 512x512",
            LocalDemTileServer.TILE_SIZE,
            LocalDemTileServer.lastDecodedWidth,
        )
        assertEquals(LocalDemTileServer.TILE_SIZE, LocalDemTileServer.lastDecodedHeight)
        if (NaviMapTestHooks.localDemMapboxConversion) {
            assertTrue(
                "mapbox pack round-trip max error must be <= 1m (got ${LocalDemTileServer.lastRoundtripMaxError})",
                !LocalDemTileServer.lastRoundtripMaxError.isNaN() &&
                    LocalDemTileServer.lastRoundtripMaxError <= 1.0,
            )
        }
        assertTrue(
            "converted DEM elev must be plausible Norway ground, got " +
                "[${LocalDemTileServer.lastElevMin},${LocalDemTileServer.lastElevMax}]",
            !LocalDemTileServer.lastElevMin.isNaN() &&
                LocalDemTileServer.lastElevMin > -500 &&
                LocalDemTileServer.lastElevMax < 5000 &&
                LocalDemTileServer.lastElevMax > LocalDemTileServer.lastElevMin,
        )
        // Hillshade / relief: land luminance must vary (flat wash fails).
        assertTrue(
            "hillshade should produce land luminance spread (std=$std n=$n)",
            std >= 10.0,
        )
        // Protomaps light colors (tan land, greens, labels) — not a single brown slab.
        assertTrue(
            "map colors should vary (colorful_frac=$colorfulFrac)",
            colorfulFrac >= 0.08,
        )
        // Reject olive DEM-encoding wash and cream-less checkerboard (Protomaps tan).
        assertTrue(
            "land must show Protomaps cream earth (creamFrac=$creamFrac) and not olive/dark wash " +
                "(washFrac=$washFrac)",
            creamFrac >= 0.12 && washFrac < 0.45,
        )
        assertTrue(out.length() > 8_000)
    }

    /** Exp B: raw terrarium WebP + style encoding terrarium @ hillshade 0.5 (metrics only). */
    @Test
    fun expB_gjendebu_raw_terrarium_hillshade_metrics() {
        NaviMapTestHooks.localDemMapboxConversion = false
        LocalDemTileServer.stop()
        LocalDemTileServer.resetHitCounts()

        val direct =
            BasemapStyleResolver.resolve(
                context = context,
                dataDir = dataDir,
                lat = lat,
                lon = lon,
                prefer3d = true,
                vulkanAvailable = true,
            )
        assertTrue(direct.demSourceUri?.contains("/tilejson.json") == true)
        val styleText = File(direct.styleUri.removePrefix("file://")).readText()
        assertTrue(styleText.contains("terrarium") || styleText.contains("webp"))

        activityRule.launchActivity(null)
        Thread.sleep(3_000)
        assertTrue(InstrumentedMapCapture.awaitStyleReady(60_000))
        waitReady()
        assertTrue(InstrumentedMapCapture.awaitRenderSettled(35_000))
        Thread.sleep(8_000)
        assertTrue(InstrumentedMapCapture.awaitRenderSettled(20_000))
        val shot = InstrumentedMapCapture.takeScreenshotAfterSettle(10_000)
        require(shot != null && shot.width > 0)

        val out =
            File(
                dataDir,
                "offline_downloaded_3d_gjendebu_expB_raw_terrarium.png",
            )
        out.outputStream().use { shot.compress(Bitmap.CompressFormat.PNG, 100, it) }
        InstrumentedMapCapture.screencapAfterSettle(
            "/data/local/tmp/offline_downloaded_3d_gjendebu_expB.png",
            5_000,
        )

        val (std, colorfulFrac, n) = analyzeLand(shot)
        val washFrac = washFraction(shot)
        val creamFrac = creamFraction(shot)
        android.util.Log.i(
            "OfflineDownloaded3dExpB",
            "expB raw terrarium dem=${direct.demSourceUri} land_n=$n lum_std=$std " +
                "colorful_frac=$colorfulFrac washFrac=$washFrac creamFrac=$creamFrac " +
                "demHitsOk=${LocalDemTileServer.hitsOk} elev=" +
                "[${LocalDemTileServer.lastElevMin},${LocalDemTileServer.lastElevMax}] " +
                "tile=${LocalDemTileServer.lastDecodedWidth}x${LocalDemTileServer.lastDecodedHeight}",
        )
    }

    private fun washFraction(bmp: Bitmap): Double {
        var n = 0
        var wash = 0
        val step = 6
        val y0 = (bmp.height * 0.18).toInt()
        val y1 = (bmp.height * 0.75).toInt()
        val x0 = (bmp.width * 0.1).toInt()
        val x1 = (bmp.width * 0.9).toInt()
        var y = y0
        while (y < y1) {
            var x = x0
            while (x < x1) {
                val c = bmp.getPixel(x, y)
                val r = Color.red(c)
                val g = Color.green(c)
                val b = Color.blue(c)
                val water = b > 140 && b > r + 25 && b > g + 15
                val ui = r > 220 && g > 220 && b > 220
                if (!water && !ui) {
                    n++
                    val l = 0.2126 * r + 0.7152 * g + 0.0722 * b
                    val chroma = maxOf(r, g, b) - minOf(r, g, b)
                    val oliveSig =
                        kotlin.math.abs(r - 88) + kotlin.math.abs(g - 80) + kotlin.math.abs(b - 60) < 35
                    // Uniform dark slab (mapbox-DEM hillshade wash): low cream, not classic olive.
                    val darkSlab = l < 118.0 && chroma < 25
                    if (oliveSig || darkSlab) wash++
                }
                x += step
            }
            y += step
        }
        return if (n == 0) 1.0 else wash.toDouble() / n
    }

    /** Protomaps light land ~RGB(236,228,216) / cream #f8f4f0 — rejects olive slabs. */
    private fun creamFraction(bmp: Bitmap): Double {
        var n = 0
        var cream = 0
        val step = 6
        val y0 = (bmp.height * 0.18).toInt()
        val y1 = (bmp.height * 0.75).toInt()
        val x0 = (bmp.width * 0.1).toInt()
        val x1 = (bmp.width * 0.9).toInt()
        var y = y0
        while (y < y1) {
            var x = x0
            while (x < x1) {
                val c = bmp.getPixel(x, y)
                val r = Color.red(c)
                val g = Color.green(c)
                val b = Color.blue(c)
                val water = b > 140 && b > r + 25 && b > g + 15
                val ui = r > 220 && g > 220 && b > 220
                if (!water && !ui) {
                    n++
                    val dist =
                        abs(r - 236) + abs(g - 228) + abs(b - 216)
                    if (dist < 45) cream++
                }
                x += step
            }
            y += step
        }
        return if (n == 0) 0.0 else cream.toDouble() / n
    }
}
