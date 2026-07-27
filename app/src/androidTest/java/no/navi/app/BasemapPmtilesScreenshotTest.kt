package no.navi.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.rule.ActivityTestRule
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.pmtilesPlanetUrl
import uniffi.navi.pmtilesQueueRegion
import uniffi.navi.pmtilesRunJob
import java.io.File

/**
 * Visual evidence: offline Protomaps / Mapterhorn hillshade 3D / coverage boundary
 * with drive HUD bars visible.
 */
@RunWith(AndroidJUnit4::class)
class BasemapPmtilesScreenshotTest {
    @get:Rule
    val activityRule = ActivityTestRule(MainActivity::class.java, false, false)

    private val osloLat = 59.91
    private val osloLon = 10.75

    @Test
    fun offlineBasemap_3d_and_fallback_screenshots() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val dataDir = NaviAppData.resolve(context)

        File(dataDir, "pmtiles/test_oslo.pmtiles").delete()
        val job = pmtilesQueueRegion(dataDir.absolutePath, "test/oslo", pmtilesPlanetUrl())
        assertTrue(job.id.isNotBlank())
        val done = pmtilesRunJob(dataDir.absolutePath, job.id)
        assertEquals("completed", done.status)
        assertTrue(File(done.localPath).length() > 1000)

        MapHudPrefs.saveOptIn3d(context, false)
        MapHudPrefs.saveCameraTiltDeg(context, 0.0)
        // Keep drive HUD bars; only tuck search/tools panels.
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = true
        // Emulator GPS often reports Alt 0 m; inject a real-ish Oslo elevation.
        NaviMapTestHooks.gpsAltitudeM = 47.0

        activityRule.launchActivity(null)
        assertTrue(activityRule.activity.isFinishing.not())
        Thread.sleep(4_000)

        fun shell(cmd: String) {
            val pfd = InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand(cmd)
            java.io.FileInputStream(pfd.fileDescriptor).use { input ->
                val buf = ByteArray(4096)
                while (input.read(buf) >= 0) {
                }
            }
            pfd.close()
        }

        fun waitCamera(
            lat: Double,
            lon: Double,
            zoom: Double,
            timeoutMs: Long = 20_000,
        ) {
            NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
            val deadline = System.currentTimeMillis() + timeoutMs
            while (System.currentTimeMillis() < deadline) {
                val clat = NaviMapTestHooks.lastCameraLat
                val clon = NaviMapTestHooks.lastCameraLon
                if (kotlin.math.abs(clat - lat) < 0.15 &&
                    kotlin.math.abs(clon - lon) < 0.15
                ) {
                    return
                }
                Thread.sleep(400)
                NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoom)
            }
        }

        fun waitHudAltitude(
            expected: Double,
            timeoutMs: Long = 10_000,
        ) {
            val deadline = System.currentTimeMillis() + timeoutMs
            while (System.currentTimeMillis() < deadline) {
                val got = NaviMapTestHooks.lastHudAltitudeM
                if (got != null && kotlin.math.abs(got - expected) < 0.5) return
                Thread.sleep(200)
            }
        }

        fun capture(name: String) {
            NaviMapTestHooks.hideUiChrome = false
            NaviMapTestHooks.hideSearchChrome = true
            Thread.sleep(2_500)
            assertNotNull("HUD altitude should be set before $name", NaviMapTestHooks.lastHudAltitudeM)
            assertTrue(
                "altitude must not be the emulator 0 sentinel before $name " +
                    "(was ${NaviMapTestHooks.lastHudAltitudeM})",
                kotlin.math.abs(NaviMapTestHooks.lastHudAltitudeM!!) > 0.5,
            )
            val shot = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
            assertTrue("null shot $name", shot != null)
            assertNotEquals(0, shot!!.width)
            val out = File(dataDir, name)
            out.outputStream().use { shot.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) }
            assertTrue("$name too small (${out.length()})", out.length() > 5_000)
            shell("screencap -p /data/local/tmp/$name")
            shell("chmod 644 /data/local/tmp/$name")
            android.util.Log.i(
                "BasemapPmtilesScreenshotTest",
                "kind=${NaviMapTestHooks.lastBasemapKind} pitch=${NaviMapTestHooks.lastCameraPitch} " +
                    "terrain=${NaviMapTestHooks.lastTerrainAttached} " +
                    "alt=${NaviMapTestHooks.lastHudAltitudeM} " +
                    "cam=${NaviMapTestHooks.lastCameraLat},${NaviMapTestHooks.lastCameraLon} " +
                    "wrote ${out.absolutePath} bytes=${out.length()}",
            )
        }

        waitHudAltitude(47.0)
        waitCamera(osloLat, osloLon, 12.0)
        var kind = ""
        repeat(40) {
            Thread.sleep(500)
            kind = NaviMapTestHooks.lastBasemapKind
            if (kind == "OfflineProtomaps") return@repeat
            if (it % 8 == 7) {
                NaviMapTestHooks.pendingCamera = Triple(osloLat, osloLon, 12.0)
            }
        }
        val direct =
            BasemapStyleResolver.resolve(
                context = context,
                dataDir = dataDir,
                lat = osloLat,
                lon = osloLon,
                prefer3d = false,
                vulkanAvailable = true,
            )
        assertEquals(BasemapStyleResolver.StyleKind.OfflineProtomaps, direct.kind)
        assertTrue("map never switched to OfflineProtomaps (was $kind)", kind == "OfflineProtomaps")
        capture("basemap_offline_protomaps.png")

        NaviMapTestHooks.gpsAltitudeM = 12.0
        waitCamera(69.65, 18.96, 10.0)
        waitHudAltitude(12.0)
        var boundaryKind = ""
        repeat(30) {
            Thread.sleep(500)
            boundaryKind = NaviMapTestHooks.lastBasemapKind
            if (boundaryKind == "OnlineLiberty" || boundaryKind == "Online3d") return@repeat
        }
        assertTrue(
            "boundary map stayed offline (was $boundaryKind)",
            boundaryKind == "OnlineLiberty" || boundaryKind == "Online3d",
        )
        capture("basemap_coverage_boundary_tromso.png")

        // Gjendebu (Jotunheimen) — DEM hillshade is visually obvious in the valley.
        // Toggle 3D via hooks (no finishActivity/relaunch): destroying MapLibre's
        // Vulkan MapRenderer from ActivityTestRule mid-suite has crashed the
        // instrumentation process in AndroidVulkanRendererBackend::~… on the
        // FinalizerDaemon.
        val gjendeLat = 61.493
        val gjendeLon = 8.351
        NaviMapTestHooks.gpsAltitudeM = 1000.0
        waitHudAltitude(1000.0)
        waitCamera(gjendeLat, gjendeLon, 12.0)
        // Opt-in hillshade is independent of camera tilt (TERRAIN_VIEW_TILT=0).
        // Request a user tilt preset so the screenshot shows perspective + DEM.
        NaviMapTestHooks.requestOptIn3d = true
        NaviMapTestHooks.requestCameraTiltDeg = 45.0
        var kind3d = ""
        repeat(40) {
            Thread.sleep(500)
            kind3d = NaviMapTestHooks.lastBasemapKind
            if (kind3d == "Online3d" &&
                NaviMapTestHooks.lastTerrainAttached &&
                NaviMapTestHooks.lastCameraPitch >= 40.0
            ) {
                return@repeat
            }
            if (it % 8 == 7) {
                NaviMapTestHooks.pendingCamera = Triple(gjendeLat, gjendeLon, 12.0)
                NaviMapTestHooks.requestOptIn3d = true
                NaviMapTestHooks.requestCameraTiltDeg = 45.0
            }
        }
        assertEquals("Online3d", kind3d)
        assertTrue(
            "Mapterhorn hillshade must attach (terrain=${NaviMapTestHooks.lastTerrainAttached})",
            NaviMapTestHooks.lastTerrainAttached,
        )
        assertTrue(
            "user tilt preset should apply with Vulkan (pitch=${NaviMapTestHooks.lastCameraPitch})",
            NaviMapTestHooks.lastCameraPitch >= 40.0,
        )
        capture("basemap_3d_mapterhorn_hillshade.png")

        val fallback =
            BasemapStyleResolver.resolve(
                context = context,
                dataDir = dataDir,
                lat = gjendeLat,
                lon = gjendeLon,
                prefer3d = true,
                vulkanAvailable = false,
                forceOnline2d = true,
            )
        assertEquals(BasemapStyleResolver.StyleKind.OnlineLiberty, fallback.kind)
        NaviMapTestHooks.requestOptIn3d = false
        NaviMapTestHooks.requestCameraTiltDeg = 0.0
        waitCamera(gjendeLat, gjendeLon, 12.0)
        repeat(30) {
            Thread.sleep(400)
            if (NaviMapTestHooks.lastBasemapKind == "OnlineLiberty" &&
                NaviMapTestHooks.lastCameraPitch < 5.0 &&
                !NaviMapTestHooks.lastTerrainAttached
            ) {
                return@repeat
            }
            if (it % 8 == 7) {
                NaviMapTestHooks.requestOptIn3d = false
                NaviMapTestHooks.requestCameraTiltDeg = 0.0
            }
        }
        assertEquals("OnlineLiberty", NaviMapTestHooks.lastBasemapKind)
        assertTrue(
            "terrain must detach when 3D off (terrain=${NaviMapTestHooks.lastTerrainAttached})",
            !NaviMapTestHooks.lastTerrainAttached,
        )
        capture("basemap_3d_fallback_liberty.png")
    }
}
