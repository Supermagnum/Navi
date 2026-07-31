package no.navi.app

import android.graphics.Bitmap
import android.graphics.Color
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
import kotlin.math.sqrt

/**
 * Diagnostic only (not a release gate): online Liberty + remote Mapterhorn
 * terrarium at Gjendebu — same camera as [OfflineDownloaded3dScreenshotTest].
 * Logs wash/cream metrics; does not assert pass/fail on hillshade quality.
 */
@RunWith(AndroidJUnit4::class)
class OnlineGjendebu3dHillshadeDiagnosticTest {
    @get:Rule
    val activityRule = ActivityTestRule(MainActivity::class.java, false, false)

    private lateinit var context: android.content.Context

    private val lat = 61.493
    private val lon = 8.351
    private val zoom = 12.0

    @Before
    fun setUp() {
        context = InstrumentationRegistry.getInstrumentation().targetContext
        val auto = InstrumentationRegistry.getInstrumentation().uiAutomation
        auto.grantRuntimePermission(context.packageName, android.Manifest.permission.ACCESS_FINE_LOCATION)
        auto.grantRuntimePermission(context.packageName, android.Manifest.permission.ACCESS_COARSE_LOCATION)
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = true
        NaviMapTestHooks.forceOnlineBasemap = true
        NaviMapTestHooks.disableGpsFollow = true
        NaviMapTestHooks.followGps = false
        NaviMapTestHooks.gpsAltitudeM = 1000.0
        MapHudPrefs.saveOptIn3d(context, true)
        MapHudPrefs.saveCameraTiltDeg(context, 45.0)
    }

    @After
    fun tearDown() {
        NaviMapTestHooks.hillshadeExaggerationOverride = null
        NaviMapTestHooks.forceOnlineBasemap = false
        NaviMapTestHooks.disableGpsFollow = false
        NaviMapTestHooks.followGps = true
        MapHudPrefs.saveOptIn3d(context, false)
        MapHudPrefs.saveCameraTiltDeg(context, 0.0)
    }

    private fun cameraAtGjendebu(): Boolean =
        abs(NaviMapTestHooks.lastCameraLat - lat) <= 0.05 &&
            abs(NaviMapTestHooks.lastCameraLon - lon) <= 0.05

    private fun waitReady() {
        NaviMapTestHooks.forceOnlineBasemap = true
        NaviMapTestHooks.disableGpsFollow = true
        NaviMapTestHooks.followGps = false
        NaviMapTestHooks.requestOptIn3d = true
        NaviMapTestHooks.requestCameraTiltDeg = 45.0
        NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
        val deadline = System.currentTimeMillis() + 90_000
        while (System.currentTimeMillis() < deadline) {
            val kind = NaviMapTestHooks.lastBasemapKind
            val pitchOk = abs(NaviMapTestHooks.lastCameraPitch - 45.0) <= 2.5
            if (kind.startsWith("Online") &&
                NaviMapTestHooks.lastTerrainAttached &&
                pitchOk &&
                cameraAtGjendebu() &&
                NaviMapTestHooks.styleReady
            ) {
                return
            }
            NaviMapTestHooks.forceOnlineBasemap = true
            NaviMapTestHooks.disableGpsFollow = true
            NaviMapTestHooks.followGps = false
            NaviMapTestHooks.requestOptIn3d = true
            NaviMapTestHooks.requestCameraTiltDeg = 45.0
            NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
            Thread.sleep(400)
        }
        error(
            "timeout kind=${NaviMapTestHooks.lastBasemapKind} terrain=${NaviMapTestHooks.lastTerrainAttached} " +
                "pitch=${NaviMapTestHooks.lastCameraPitch} cam=${NaviMapTestHooks.lastCameraLat}," +
                NaviMapTestHooks.lastCameraLon,
        )
    }

    private fun analyzeLand(bmp: Bitmap): Triple<Double, Double, Int> {
        var n = 0
        var sumL = 0.0
        var sumL2 = 0.0
        var colorful = 0
        val step = 8
        val w = bmp.width
        val h = bmp.height
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
                    val darkSlab = l < 118.0 && chroma < 25
                    if (oliveSig || darkSlab) wash++
                }
                x += step
            }
            y += step
        }
        return if (n == 0) 1.0 else wash.toDouble() / n
    }

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
                    val dist = abs(r - 236) + abs(g - 228) + abs(b - 216)
                    if (dist < 45) cream++
                }
                x += step
            }
            y += step
        }
        return if (n == 0) 0.0 else cream.toDouble() / n
    }

    private fun landModeRgb(bmp: Bitmap): Triple<Int, Int, Int> {
        val counts = HashMap<Triple<Int, Int, Int>, Int>()
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
                    val key = Triple(r / 4 * 4, g / 4 * 4, b / 4 * 4)
                    counts[key] = (counts[key] ?: 0) + 1
                }
                x += step
            }
            y += step
        }
        return counts.maxByOrNull { it.value }!!.key
    }

    @Test
    fun gjendebu_online_3d_metrics_and_screencap() {
        val resolved =
            BasemapStyleResolver.resolve(
                context = context,
                dataDir = NaviAppData.resolve(context),
                lat = lat,
                lon = lon,
                prefer3d = true,
                vulkanAvailable = true,
                forceOnline2d = true,
            )
        assertTrue(resolved.kind.name.startsWith("Online"))
        assertTrue(resolved.attachMapterhornTerrain)
        assertTrue(resolved.demSourceUri?.contains("tilejson") == true)

        activityRule.launchActivity(null)
        Thread.sleep(3_000)
        assertTrue(InstrumentedMapCapture.awaitStyleReady(60_000))
        waitReady()

        assertTrue(InstrumentedMapCapture.awaitRenderSettled(35_000))
        Thread.sleep(8_000)
        assertTrue(InstrumentedMapCapture.awaitRenderSettled(20_000))
        val shot = InstrumentedMapCapture.takeScreenshotAfterSettle(10_000)
        require(shot != null && shot.width > 0) { "null screenshot" }

        InstrumentedMapCapture.screencapAfterSettle(
            "/data/local/tmp/online_gjendebu_3d.png",
            5_000,
        )

        val (std, colorfulFrac, n) = analyzeLand(shot)
        val washFrac = washFraction(shot)
        val creamFrac = creamFraction(shot)
        val mode = landModeRgb(shot)
        android.util.Log.i(
            "OnlineGjendebu3d",
            "kind=${NaviMapTestHooks.lastBasemapKind} terrain=true " +
                "pitch=${NaviMapTestHooks.lastCameraPitch} " +
                "camLat=${NaviMapTestHooks.lastCameraLat} camLon=${NaviMapTestHooks.lastCameraLon} " +
                "land_n=$n lum_std=$std colorful_frac=$colorfulFrac washFrac=$washFrac creamFrac=$creamFrac " +
                "mode_rgb=${mode.first},${mode.second},${mode.third} dem=${resolved.demSourceUri}",
        )
    }

    @Test
    fun gjendebu_online_3d_exag03_metrics_and_screencap() {
        NaviMapTestHooks.hillshadeExaggerationOverride = 0.3f
        val resolved =
            BasemapStyleResolver.resolve(
                context = context,
                dataDir = NaviAppData.resolve(context),
                lat = lat,
                lon = lon,
                prefer3d = true,
                vulkanAvailable = true,
                forceOnline2d = true,
            )
        assertTrue(resolved.kind.name.startsWith("Online"))
        activityRule.launchActivity(null)
        Thread.sleep(3_000)
        assertTrue(InstrumentedMapCapture.awaitStyleReady(60_000))
        waitReady()
        assertTrue(InstrumentedMapCapture.awaitRenderSettled(35_000))
        Thread.sleep(8_000)
        assertTrue(InstrumentedMapCapture.awaitRenderSettled(20_000))
        val shot = InstrumentedMapCapture.takeScreenshotAfterSettle(10_000)
        require(shot != null && shot.width > 0) { "null screenshot" }
        InstrumentedMapCapture.screencapAfterSettle(
            "/data/local/tmp/online_gjendebu_3d_exag0.3.png",
            5_000,
        )
        val (std, colorfulFrac, n) = analyzeLand(shot)
        val washFrac = washFraction(shot)
        val creamFrac = creamFraction(shot)
        val mode = landModeRgb(shot)
        android.util.Log.i(
            "OnlineGjendebu3dExag03",
            "exag=0.3 kind=${NaviMapTestHooks.lastBasemapKind} land_n=$n lum_std=$std " +
                "colorful_frac=$colorfulFrac washFrac=$washFrac creamFrac=$creamFrac " +
                "mode_rgb=${mode.first},${mode.second},${mode.third}",
        )
    }
}
