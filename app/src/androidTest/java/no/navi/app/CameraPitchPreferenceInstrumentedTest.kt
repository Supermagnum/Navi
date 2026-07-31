package no.navi.app

import android.graphics.Bitmap
import android.graphics.Color
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.rule.ActivityTestRule
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import kotlin.math.abs

/**
 * Prefs / UI tilt must match the **rendered** MapLibre camera pitch — the gap
 * found when `camera_tilt_deg=60` in prefs while the live map stayed flat.
 *
 * Asserts both [NaviMapTestHooks.lastCameraPitch] (from MapLibre camera state)
 * and that 0° vs 45° screenshots differ (so a stale hook cannot fake a pass).
 */
@RunWith(AndroidJUnit4::class)
class CameraPitchPreferenceInstrumentedTest {
    @get:Rule
    val activityRule = ActivityTestRule(MainActivity::class.java, false, false)

    private lateinit var context: android.content.Context
    private lateinit var dataDir: File

    private val lat = 61.493
    private val lon = 8.351
    private val zoom = 12.0
    private val targetTilt = 45.0

    @Before
    fun setUp() {
        context = InstrumentationRegistry.getInstrumentation().targetContext
        dataDir = NaviAppData.resolve(context)
        val auto = InstrumentationRegistry.getInstrumentation().uiAutomation
        auto.grantRuntimePermission(context.packageName, android.Manifest.permission.ACCESS_FINE_LOCATION)
        auto.grantRuntimePermission(context.packageName, android.Manifest.permission.ACCESS_COARSE_LOCATION)
        OstlandetOfflineFixtures.ensureInstalled(dataDir)
        MapHudPrefs.rememberDownloadedPmtilesRegion(context, "europe_norway_ostlandet")
        NaviMapTestHooks.hideSearchChrome = true
        NaviMapTestHooks.forceOnlineBasemap = false
        NaviMapTestHooks.gpsAltitudeM = 1000.0
        MapHudPrefs.saveOptIn3d(context, true)
        MapHudPrefs.saveCameraTiltDeg(context, 0.0)
    }

    private fun awaitRenderedPitch(
        want: Double,
        timeoutMs: Long = 45_000,
    ) {
        NaviMapTestHooks.requestOptIn3d = true
        NaviMapTestHooks.requestCameraTiltDeg = want
        NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.styleReady &&
                abs(NaviMapTestHooks.lastCameraPitch - want) <= 2.0
            ) {
                return
            }
            NaviMapTestHooks.requestOptIn3d = true
            NaviMapTestHooks.requestCameraTiltDeg = want
            NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
            Thread.sleep(400)
        }
        assertEquals(
            "rendered camera pitch must match preference",
            want,
            NaviMapTestHooks.lastCameraPitch,
            2.0,
        )
    }

    private fun capture(name: String): Bitmap {
        assertTrue(InstrumentedMapCapture.awaitRenderSettled(25_000))
        Thread.sleep(2_000)
        assertTrue(InstrumentedMapCapture.awaitRenderSettled(10_000))
        val shot = InstrumentedMapCapture.takeScreenshotAfterSettle(8_000)
        require(shot != null && shot.width > 0)
        val out = File(dataDir, name)
        out.outputStream().use { shot.compress(Bitmap.CompressFormat.PNG, 100, it) }
        InstrumentedMapCapture.screencapAfterSettle("/data/local/tmp/$name", 3_000)
        assertTrue(out.length() > 8_000)
        return shot
    }

    /** Mean absolute pixel delta in the central map band (skip chrome). */
    private fun mapBandDelta(
        a: Bitmap,
        b: Bitmap,
    ): Double {
        val w = minOf(a.width, b.width)
        val h = minOf(a.height, b.height)
        var sum = 0.0
        var n = 0
        val y0 = (h * 0.18).toInt()
        val y1 = (h * 0.75).toInt()
        val x0 = (w * 0.08).toInt()
        val x1 = (w * 0.92).toInt()
        var y = y0
        while (y < y1) {
            var x = x0
            while (x < x1) {
                val ca = a.getPixel(x, y)
                val cb = b.getPixel(x, y)
                sum += abs(Color.red(ca) - Color.red(cb))
                sum += abs(Color.green(ca) - Color.green(cb))
                sum += abs(Color.blue(ca) - Color.blue(cb))
                n += 3
                x += 10
            }
            y += 10
        }
        return sum / n.coerceAtLeast(1)
    }

    @Test
    fun live_apply_and_restore_rendered_pitch_matches_prefs() {
        activityRule.launchActivity(null)
        Thread.sleep(2_000)
        assertTrue(InstrumentedMapCapture.awaitStyleReady(60_000))

        // Flat reference.
        awaitRenderedPitch(0.0)
        val flat = capture("camera_pitch_live_0.png")

        // Live-apply 45°.
        awaitRenderedPitch(targetTilt)
        assertEquals(targetTilt, MapHudPrefs.loadCameraTiltDeg(context), 0.01)
        assertEquals(targetTilt, NaviMapTestHooks.lastCameraPitch, 2.0)
        val pitched = capture("camera_pitch_live_45.png")
        val delta = mapBandDelta(flat, pitched)
        android.util.Log.i("CameraPitchPref", "flat_vs_45_delta=$delta")
        assertTrue(
            "0° vs 45° screenshots must differ (delta=$delta) — pitch not only in prefs/hooks",
            delta >= 4.0,
        )

        // Restore-on-load from prefs only (no requestCameraTiltDeg).
        activityRule.finishActivity()
        Thread.sleep(1_000)
        NaviMapTestHooks.lastCameraPitch = -1.0
        NaviMapTestHooks.styleReady = false
        MapHudPrefs.saveCameraTiltDeg(context, targetTilt)
        MapHudPrefs.saveOptIn3d(context, true)

        activityRule.launchActivity(null)
        Thread.sleep(2_000)
        assertTrue(InstrumentedMapCapture.awaitStyleReady(60_000))
        NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
        val deadline = System.currentTimeMillis() + 45_000
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.styleReady &&
                abs(NaviMapTestHooks.lastCameraPitch - targetTilt) <= 2.0
            ) {
                break
            }
            NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
            Thread.sleep(400)
        }
        assertEquals(
            "restored camera pitch from prefs",
            targetTilt,
            NaviMapTestHooks.lastCameraPitch,
            2.0,
        )
        val restored = capture("camera_pitch_restore_45.png")
        val restoreDelta = mapBandDelta(flat, restored)
        android.util.Log.i("CameraPitchPref", "flat_vs_restore45_delta=$restoreDelta")
        assertTrue(
            "restored 45° screenshot must differ from flat (delta=$restoreDelta)",
            restoreDelta >= 4.0,
        )
    }
}
