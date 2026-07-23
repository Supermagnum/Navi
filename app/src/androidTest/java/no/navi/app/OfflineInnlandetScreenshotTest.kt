package no.navi.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.rule.ActivityTestRule
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.BeforeClass
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.CorridorRouteResult
import uniffi.navi.runCarCorridorPipeline
import java.io.File

/**
 * Online vs fully offline (no Wi‑Fi / mobile data) Innlandet coverage:
 * Protomaps basemap + Mapterhorn DEM, corridor routing with POI + breaks.
 *
 * Staging (host → `/data/local/tmp/navi_fixtures/`):
 * - `europe_norway_ostlandet.pmtiles`
 * - `europe_norway_ostlandet_dem.pmtiles`
 * - `espa-atnbrufossen-corridor.osm.pbf`
 * - `elevation-corridor.tar`
 */
@RunWith(AndroidJUnit4::class)
class OfflineInnlandetScreenshotTest {
    companion object {
        @JvmStatic
        lateinit var carRoute: CorridorRouteResult

        @JvmStatic
        @BeforeClass
        fun provisionInnlandet() {
            val context = InstrumentationRegistry.getInstrumentation().targetContext
            val dataDir = NaviAppData.resolve(context)
            val staged = File("/data/local/tmp/navi_fixtures")

            val pbf = File(staged, "espa-atnbrufossen-corridor.osm.pbf")
            val elevTar = File(staged, "elevation-corridor.tar")
            check(pbf.isFile) { "missing ${pbf.absolutePath}" }
            check(elevTar.isFile) { "missing ${elevTar.absolutePath}" }

            OstlandetOfflineFixtures.ensureInstalled(dataDir)

            val localPbf = File(dataDir, "espa-atnbrufossen-corridor.osm.pbf")
            pbf.copyTo(localPbf, overwrite = true)
            File(dataDir, "elevation").deleteRecursively()
            val tarProc = ProcessBuilder(
                "tar", "-xf", elevTar.absolutePath, "-C", dataDir.absolutePath,
            ).redirectErrorStream(true).start()
            val tarOut = tarProc.inputStream.bufferedReader().readText()
            check(tarProc.waitFor() == 0) { "tar failed: $tarOut" }

            carRoute = runCarCorridorPipeline(
                pbfPath = localPbf.absolutePath,
                elevDir = File(dataDir, "elevation").absolutePath,
                cacheDir = File(dataDir, "graph-cache").absolutePath,
                breakIntervalHours = 1.0,
            )
            check(carRoute.routePolyline.contains(';')) { carRoute.report }
            check(carRoute.distanceKm > 5.0) { "distance ${carRoute.distanceKm}" }
            check(carRoute.report.contains("PASS")) { carRoute.report }
            // POI discovery on corridor PBF
            check(carRoute.poiName.isNotBlank() || carRoute.poiIconKey.isNotBlank()) {
                "expected POI from corridor: ${carRoute.report}"
            }
        }
    }

    @get:Rule
    val activityRule = ActivityTestRule(MainActivity::class.java, false, false)

    private lateinit var dataDir: File
    private lateinit var context: android.content.Context

    @Before
    fun setUp() {
        context = InstrumentationRegistry.getInstrumentation().targetContext
        dataDir = NaviAppData.resolve(context)
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = true
        NaviMapTestHooks.gpsAltitudeM = 412.0
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

    private fun setNetworkEnabled(enabled: Boolean) {
        if (enabled) {
            shell("svc wifi enable")
            shell("cmd connectivity airplane-mode disable")
        } else {
            shell("svc wifi disable")
            shell("cmd connectivity airplane-mode enable")
        }
        Thread.sleep(2_500)
    }

    private fun capture(name: String) {
        Thread.sleep(2_500)
        val shot = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
        assertTrue("null shot $name", shot != null)
        assertNotEquals(0, shot!!.width)
        val out = File(dataDir, name)
        out.outputStream().use { shot.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) }
        assertTrue("$name too small (${out.length()})", out.length() > 10_000)
        shell("screencap -p /data/local/tmp/$name")
        shell("chmod 644 /data/local/tmp/$name")
        android.util.Log.i(
            "OfflineInnlandetScreenshotTest",
            "shot=$name kind=${NaviMapTestHooks.lastBasemapKind} " +
                "terrain=${NaviMapTestHooks.lastTerrainAttached} " +
                "pitch=${NaviMapTestHooks.lastCameraPitch} " +
                "break=${NaviMapTestHooks.lastBreakHudVisible} bytes=${out.length()}",
        )
    }

    private fun waitStyle(timeoutMs: Long = 60_000) {
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.styleReady || NaviMapTestHooks.lastReportedLayerCount >= 1) return
            Thread.sleep(400)
        }
    }

    private fun injectRoute() {
        NaviMapTestHooks.routeStartLabel = "Espa"
        NaviMapTestHooks.routeEndLabel = "Atnbrufossen"
        NaviMapTestHooks.routeViaLabel = ""
        NaviMapTestHooks.requestBreakReminders = true
        NaviMapTestHooks.pendingRoute = carRoute
        val deadline = System.currentTimeMillis() + 90_000
        while (System.currentTimeMillis() < deadline) {
            Thread.sleep(500)
            if (NaviMapTestHooks.lastBreakHudVisible && NaviMapTestHooks.lastReportedLayerCount >= 1) {
                return
            }
            NaviMapTestHooks.pendingRoute = carRoute
            NaviMapTestHooks.requestBreakReminders = true
        }
    }

    @Test
    fun online_then_offline_with_3d_routing_poi_breaks() {
        // --- Online shot (Liberty / live tiles OK) ---
        setNetworkEnabled(true)
        MapHudPrefs.saveOptIn3d(context, false)
        activityRule.launchActivity(null)
        waitStyle()
        injectRoute()
        assertTrue(
            "break countdown missing online",
            NaviMapTestHooks.lastBreakHudVisible,
        )
        assertTrue(carRoute.poiName.isNotBlank() || carRoute.poiIconKey.isNotBlank())
        capture("innlandet_online_route.png")

        // --- Offline + 3D: airplane mode, local Protomaps + DEM ---
        activityRule.finishActivity()
        Thread.sleep(1_000)
        setNetworkEnabled(false)
        MapHudPrefs.saveOptIn3d(context, true)

        // Espa / corridor center inside Ostlandet bbox
        NaviMapTestHooks.pendingCamera = Triple(61.0, 10.7, 8.5)
        activityRule.launchActivity(null)
        waitStyle()
        injectRoute()

        val deadline = System.currentTimeMillis() + 60_000
        var kind = ""
        while (System.currentTimeMillis() < deadline) {
            kind = NaviMapTestHooks.lastBasemapKind
            if (kind == "OfflineProtomaps" && NaviMapTestHooks.lastTerrainAttached) break
            Thread.sleep(500)
            NaviMapTestHooks.pendingCamera = Triple(61.0, 10.7, 8.5)
        }
        assertEquals("OfflineProtomaps", kind)
        assertTrue(
            "offline 3D hillshade must attach without network",
            NaviMapTestHooks.lastTerrainAttached,
        )
        assertTrue(NaviMapTestHooks.lastCameraPitch >= 40.0)
        assertTrue(
            "breaks must work offline",
            NaviMapTestHooks.lastBreakHudVisible,
        )
        // Resolver must not think we have internet for Liberty fallback.
        assertFalse(
            "device should report no internet in airplane mode",
            BasemapStyleResolver.hasNetwork(context),
        )
        // Offline 3D gallery shot removed from docs/images; keep local evidence only.
        capture("tmp_innlandet_offline_3d_route.png")

        // Restore network for subsequent tests on the same AVD.
        setNetworkEnabled(true)
        MapHudPrefs.saveOptIn3d(context, false)
    }
}
