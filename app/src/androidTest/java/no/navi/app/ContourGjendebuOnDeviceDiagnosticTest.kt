package no.navi.app

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
import kotlin.math.abs

/**
 * On-device diagnostic: contours at Gjendebu (Jotunheimen) with pan/revisit
 * to verify DEM + contour generation caches warm on real hardware.
 *
 * Not a CI release gate — run manually when a physical device is attached.
 */
@RunWith(AndroidJUnit4::class)
class ContourGjendebuOnDeviceDiagnosticTest {
    @get:Rule
    val activityRule = ActivityTestRule(MainActivity::class.java, false, false)

    private lateinit var context: android.content.Context

    private val lat = 61.493
    private val lon = 8.351
    private val zoom = 13.0

    @Before
    fun setUp() {
        context = InstrumentationRegistry.getInstrumentation().targetContext
        val auto = InstrumentationRegistry.getInstrumentation().uiAutomation
        auto.grantRuntimePermission(context.packageName, android.Manifest.permission.ACCESS_FINE_LOCATION)
        auto.grantRuntimePermission(context.packageName, android.Manifest.permission.ACCESS_COARSE_LOCATION)
        NaviMapTestHooks.hideUiChrome = true
        NaviMapTestHooks.hideSearchChrome = true
        NaviMapTestHooks.forceOnlineBasemap = true
        NaviMapTestHooks.disableGpsFollow = true
        NaviMapTestHooks.followGps = false
        MapHudPrefs.saveContoursEnabled(context, true)
        MapHudPrefs.saveOptIn3d(context, true)
        MapHudPrefs.saveCameraTiltDeg(context, 35.0)
        NaviMapTestHooks.contourDemCacheHits = 0
        NaviMapTestHooks.contourDemCacheMiss = 0
        NaviMapTestHooks.contourGenCacheHits = 0
        NaviMapTestHooks.contourGenCacheMiss = 0
    }

    @After
    fun tearDown() {
        NaviMapTestHooks.forceOnlineBasemap = false
        NaviMapTestHooks.disableGpsFollow = false
        NaviMapTestHooks.followGps = true
        MapHudPrefs.saveContoursEnabled(context, false)
        MapHudPrefs.saveOptIn3d(context, false)
        MapHudPrefs.saveCameraTiltDeg(context, 0.0)
    }

    private fun cameraNear(
        tLat: Double,
        tLon: Double,
    ): Boolean =
        abs(NaviMapTestHooks.lastCameraLat - tLat) <= 0.04 &&
            abs(NaviMapTestHooks.lastCameraLon - tLon) <= 0.04

    private fun injectCamera(
        tLat: Double,
        tLon: Double,
        tZoom: Double,
    ) {
        NaviMapTestHooks.forceOnlineBasemap = true
        NaviMapTestHooks.disableGpsFollow = true
        NaviMapTestHooks.followGps = false
        NaviMapTestHooks.pendingCamera = Triple(tLat, tLon, tZoom)
    }

    private fun waitContoursReady(
        tLat: Double,
        tLon: Double,
        timeoutMs: Long = 90_000,
    ) {
        injectCamera(tLat, tLon, zoom)
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.styleReady &&
                NaviMapTestHooks.lastContoursAttached &&
                NaviMapTestHooks.lastBasemapKind.startsWith("Online") &&
                cameraNear(tLat, tLon)
            ) {
                return
            }
            injectCamera(tLat, tLon, zoom)
            Thread.sleep(400)
        }
        error(
            "contour ready timeout contours=${NaviMapTestHooks.lastContoursAttached} " +
                "kind=${NaviMapTestHooks.lastBasemapKind} " +
                "cam=${NaviMapTestHooks.lastCameraLat},${NaviMapTestHooks.lastCameraLon}",
        )
    }

    private fun logCache(label: String) {
        Log.i(
            TAG,
            "$label demHits=${NaviMapTestHooks.contourDemCacheHits} " +
                "demMiss=${NaviMapTestHooks.contourDemCacheMiss} " +
                "genHits=${NaviMapTestHooks.contourGenCacheHits} " +
                "genMiss=${NaviMapTestHooks.contourGenCacheMiss} " +
                "terrain=${NaviMapTestHooks.lastTerrainAttached} " +
                "contours=${NaviMapTestHooks.lastContoursAttached}",
        )
    }

    @Test
    fun gjendebu_contourPan_revisitCacheWarms() {
        activityRule.launchActivity(null)
        waitContoursReady(lat, lon)
        InstrumentedMapCapture.awaitRenderSettled(30_000)
        logCache("after_initial_settle")

        val missAfterFirst = NaviMapTestHooks.contourGenCacheMiss
        assertTrue(
            "expected contour generation on first view (miss=$missAfterFirst)",
            missAfterFirst > 0L,
        )

        // Pan east into adjacent mountainous tiles.
        val panLat = lat + 0.025
        val panLon = lon + 0.04
        injectCamera(panLat, panLon, zoom)
        val panDeadline = System.currentTimeMillis() + 45_000
        while (System.currentTimeMillis() < panDeadline && !cameraNear(panLat, panLon)) {
            injectCamera(panLat, panLon, zoom)
            Thread.sleep(350)
        }
        InstrumentedMapCapture.awaitRenderSettled(25_000)
        logCache("after_pan")

        // Return to original view — caches should hit on DEM + contour tiles.
        injectCamera(lat, lon, zoom)
        val backDeadline = System.currentTimeMillis() + 45_000
        while (System.currentTimeMillis() < backDeadline && !cameraNear(lat, lon)) {
            injectCamera(lat, lon, zoom)
            Thread.sleep(350)
        }
        InstrumentedMapCapture.awaitRenderSettled(25_000)
        logCache("after_revisit")

        assertTrue(
            "expected contour gen cache hits on revisit (hits=${NaviMapTestHooks.contourGenCacheHits})",
            NaviMapTestHooks.contourGenCacheHits > 0L,
        )
        assertTrue(
            "expected DEM grid cache hits on revisit (hits=${NaviMapTestHooks.contourDemCacheHits})",
            NaviMapTestHooks.contourDemCacheHits > 0L,
        )
    }

    companion object {
        private const val TAG = "ContourGjendebuDiag"
    }
}
