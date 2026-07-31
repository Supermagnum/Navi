package no.navi.app

import android.graphics.Bitmap
import android.graphics.Color
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.rule.ActivityTestRule
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/**
 * Diagnostic: compare land pixel colors for offline Protomaps at mountain vs
 * lowland, with opt-in 3D on vs off. Does not assert pass/fail thresholds beyond
 * capturing evidence for the land-color investigation.
 */
@RunWith(AndroidJUnit4::class)
class OfflineLandColorProbeTest {
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
        OstlandetOfflineFixtures.ensureInstalled(dataDir)
        NaviMapTestHooks.hideSearchChrome = true
        NaviMapTestHooks.forceOnlineBasemap = false
        NaviMapTestHooks.gpsAltitudeM = 1000.0
    }

    private fun awaitKind(
        want3d: Boolean,
        lat: Double,
        lon: Double,
        zoom: Double,
    ) {
        MapHudPrefs.saveOptIn3d(context, want3d)
        MapHudPrefs.saveCameraTiltDeg(context, 0.0)
        NaviMapTestHooks.requestOptIn3d = want3d
        NaviMapTestHooks.requestCameraTiltDeg = 0.0
        NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
        val deadline = System.currentTimeMillis() + 60_000
        while (System.currentTimeMillis() < deadline) {
            val kind = NaviMapTestHooks.lastBasemapKind
            val terrain = NaviMapTestHooks.lastTerrainAttached
            if (kind == "OfflineProtomaps" &&
                NaviMapTestHooks.styleReady &&
                terrain == want3d
            ) {
                return
            }
            NaviMapTestHooks.requestOptIn3d = want3d
            NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
            Thread.sleep(400)
        }
        error(
            "timeout kind=${NaviMapTestHooks.lastBasemapKind} terrain=${NaviMapTestHooks.lastTerrainAttached} want3d=$want3d",
        )
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
        val best = counts.maxByOrNull { it.value }!!.key
        return best
    }

    private fun colorfulFrac(bmp: Bitmap): Double {
        var n = 0
        var colorful = 0
        val step = 8
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
                    if (maxOf(r, g, b) - minOf(r, g, b) >= 12) colorful++
                }
                x += step
            }
            y += step
        }
        return if (n == 0) 0.0 else colorful.toDouble() / n
    }

    private fun capture(
        label: String,
        lat: Double,
        lon: Double,
        zoom: Double,
        want3d: Boolean,
    ): String {
        awaitKind(want3d, lat, lon, zoom)
        if (want3d) {
            LocalDemTileServer.resetHitCounts()
        }
        assertTrue(InstrumentedMapCapture.awaitRenderSettled(25_000))
        Thread.sleep(3_000)
        assertTrue(InstrumentedMapCapture.awaitRenderSettled(15_000))
        val shot = InstrumentedMapCapture.takeScreenshotAfterSettle(8_000)!!
        val out = File(dataDir, "$label.png")
        out.outputStream().use { shot.compress(Bitmap.CompressFormat.PNG, 100, it) }
        InstrumentedMapCapture.screencapAfterSettle("/data/local/tmp/$label.png", 3_000)
        val mode = landModeRgb(shot)
        val frac = colorfulFrac(shot)
        val line =
            "PROBE $label kind=${NaviMapTestHooks.lastBasemapKind} terrain=${NaviMapTestHooks.lastTerrainAttached} " +
                "mode_rgb=${mode.first},${mode.second},${mode.third} colorful_frac=$frac " +
                "demHitsOk=${LocalDemTileServer.hitsOk} demHitsMiss=${LocalDemTileServer.hitsMiss}"
        android.util.Log.i("OfflineLandColor", line)
        return line
    }

    @Test
    fun probe_mountain_and_lowland_2d_and_3d() {
        activityRule.launchActivity(null)
        Thread.sleep(2_000)
        assertTrue(InstrumentedMapCapture.awaitStyleReady(60_000))

        val lines =
            listOf(
                // Gjendebu mountain
                capture("land_probe_gjende_2d", 61.493, 8.351, 12.0, want3d = false),
                capture("land_probe_gjende_3d", 61.493, 8.351, 12.0, want3d = true),
                // Hamar lowland
                capture("land_probe_hamar_2d", 60.794, 11.068, 12.0, want3d = false),
                capture("land_probe_hamar_3d", 60.794, 11.068, 12.0, want3d = true),
            )
        File(dataDir, "land_color_probe.txt").writeText(lines.joinToString("\n"))
        lines.forEach { android.util.Log.i("OfflineLandColor", it) }
    }
}
