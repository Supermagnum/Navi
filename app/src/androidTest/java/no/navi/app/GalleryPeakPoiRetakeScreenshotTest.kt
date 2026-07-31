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
import java.io.File
import kotlin.math.abs

/**
 * Retake peak gallery docs so basemap POIs are visible where the style has
 * them. Uses [NaviMapTestHooks.pendingCamera] rather than Route-search keyboard.
 */
@RunWith(AndroidJUnit4::class)
class GalleryPeakPoiRetakeScreenshotTest {
    @get:Rule
    val activityRule = ActivityTestRule(MainActivity::class.java, false, false)

    private lateinit var context: android.content.Context

    @Before
    fun setUp() {
        context = InstrumentationRegistry.getInstrumentation().targetContext
        val auto = InstrumentationRegistry.getInstrumentation().uiAutomation
        auto.grantRuntimePermission(context.packageName, android.Manifest.permission.ACCESS_FINE_LOCATION)
        auto.grantRuntimePermission(context.packageName, android.Manifest.permission.ACCESS_COARSE_LOCATION)
        OstlandetOfflineFixtures.ensureInstalled(NaviAppData.resolve(context))
        shell("mkdir -p $OUT && chmod 777 $OUT")
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = true
        NaviMapTestHooks.disableGpsFollow = true
        NaviMapTestHooks.followGps = false
        NaviMapTestHooks.styleReady = false
        MapHudPrefs.saveOptIn3d(context, true)
        MapHudPrefs.saveCameraTiltDeg(context, 45.0)
    }

    @After
    fun tearDown() {
        NaviMapTestHooks.hideSearchChrome = false
        NaviMapTestHooks.forceOnlineBasemap = false
        NaviMapTestHooks.disableGpsFollow = false
        NaviMapTestHooks.followGps = true
        NaviMapTestHooks.requestOptIn3d = null
        MapHudPrefs.saveOptIn3d(context, false)
        MapHudPrefs.saveCameraTiltDeg(context, 0.0)
        setWifi(true)
        runCatching { activityRule.finishActivity() }
    }

    @Test
    fun capture_peak_pois_at_z16() {
        activityRule.launchActivity(null)
        shoot("poi_galdhopiggen_online.png", GALDHOPIGGEN.first, GALDHOPIGGEN.second, online = true)
        shoot("poi_elgpiggen.png", ELGPIGGEN.first, ELGPIGGEN.second, online = false)
        // Liberty has no mountain_peak layer; Elgpiggen is often absent from OMT
        // poi ranks. Keep the documented online-3D framing (DEM + Liberty).
        shoot(
            "poi_elgpiggen_online.png",
            ELGPIGGEN.first,
            ELGPIGGEN.second,
            online = true,
            tilt = 45.0,
            zoom = 13.0,
            want3d = true,
        )
        shoot("poi_prekestolen.png", PREKESTOLEN.first, PREKESTOLEN.second, online = true)
    }

    private fun shoot(
        name: String,
        lat: Double,
        lon: Double,
        online: Boolean,
        tilt: Double = 45.0,
        zoom: Double = 16.0,
        want3d: Boolean = true,
    ) {
        setWifi(online)
        NaviMapTestHooks.styleReady = false
        NaviMapTestHooks.forceOnlineBasemap = online
        NaviMapTestHooks.requestOptIn3d = want3d
        NaviMapTestHooks.requestCameraTiltDeg = tilt
        MapHudPrefs.saveOptIn3d(context, want3d)
        MapHudPrefs.saveCameraTiltDeg(context, tilt)
        NaviMapTestHooks.disableGpsFollow = true
        NaviMapTestHooks.followGps = false
        NaviMapTestHooks.hideSearchChrome = true
        NaviMapTestHooks.pendingCurrentStreet = ""
        NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)

        val deadline = System.currentTimeMillis() + 120_000
        var lastKind = ""
        while (System.currentTimeMillis() < deadline) {
            lastKind = NaviMapTestHooks.lastBasemapKind
            val kindOk =
                if (online) {
                    lastKind.startsWith("Online")
                } else {
                    lastKind.startsWith("Offline")
                }
            val zoomOk = abs(NaviMapTestHooks.lastCameraZoom - zoom) < 0.4
            val pitchOk =
                if (want3d) {
                    NaviMapTestHooks.lastCameraPitch >= (tilt - 5.0).coerceAtLeast(30.0)
                } else {
                    NaviMapTestHooks.lastCameraPitch <= 2.0
                }
            if (NaviMapTestHooks.styleReady && kindOk && zoomOk && pitchOk) break
            NaviMapTestHooks.forceOnlineBasemap = online
            NaviMapTestHooks.requestOptIn3d = want3d
            NaviMapTestHooks.requestCameraTiltDeg = tilt
            NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
            Thread.sleep(400)
        }

        assertTrue(
            "$name style/kind zoom=${NaviMapTestHooks.lastCameraZoom} " +
                "pitch=${NaviMapTestHooks.lastCameraPitch} kind=$lastKind",
            NaviMapTestHooks.styleReady &&
                abs(NaviMapTestHooks.lastCameraZoom - zoom) < 0.5 &&
                if (want3d) {
                    NaviMapTestHooks.lastCameraPitch >= 30.0
                } else {
                    NaviMapTestHooks.lastCameraPitch <= 3.0
                },
        )
        if (online) {
            assertTrue(
                "$name expected Online basemap, kind=$lastKind",
                lastKind.startsWith("Online"),
            )
        } else {
            assertTrue(
                "$name expected Offline basemap, kind=$lastKind",
                lastKind.startsWith("Offline"),
            )
        }

        repeat(16) {
            NaviMapTestHooks.requestCameraTiltDeg = tilt
            NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
            Thread.sleep(500)
        }
        if (online) {
            Thread.sleep(8_000)
            NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
            Thread.sleep(2_000)
        } else {
            Thread.sleep(3_000)
        }

        val path = "$OUT/$name"
        shell("screencap -p $path")
        shell("chmod 644 $path")
        val f = File(path)
        val minBytes = if (online) 200_000L else 60_000L
        assertTrue("$name missing/small (${f.length()})", f.isFile && f.length() > minBytes)
        Log.i(
            TAG,
            "SHOT $name bytes=${f.length()} zoom=${NaviMapTestHooks.lastCameraZoom} " +
                "pitch=${NaviMapTestHooks.lastCameraPitch} kind=$lastKind",
        )
    }

    private fun setWifi(enabled: Boolean) {
        shell(if (enabled) "svc wifi enable" else "svc wifi disable")
        Thread.sleep(if (enabled) 5_000 else 2_000)
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
        private const val TAG = "GalleryPeakRetake"
        private const val OUT = "/data/local/tmp/navi_gallery_docs"
        private val GALDHOPIGGEN = 61.6364721 to 8.3124426
        private val ELGPIGGEN = 62.1592913 to 11.3584086
        private val PREKESTOLEN = 58.9870777 to 6.1887732
    }
}
