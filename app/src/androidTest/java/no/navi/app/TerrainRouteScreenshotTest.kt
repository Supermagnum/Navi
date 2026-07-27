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
 * Espa→Atnbrufossen (car) with opt-in 3D hillshade off.
 *
 * [OstlandetOfflineFixtures.ensureInstalled] installs Østlandet PMTiles into the
 * app dataDir; offline-first resolution ([BasemapStyleResolver]) therefore
 * selects OfflineProtomaps covering this corridor — not OnlineLiberty.
 */
@RunWith(AndroidJUnit4::class)
class TerrainRouteScreenshotTest {
    companion object {
        @JvmStatic
        lateinit var carRoute: CorridorRouteResult

        @JvmStatic
        @BeforeClass
        fun provisionRoutes() {
            val context = InstrumentationRegistry.getInstrumentation().targetContext
            val dataDir = NaviAppData.resolve(context)
            OstlandetOfflineFixtures.ensureInstalled(dataDir)

            val stagedPbf = File("/data/local/tmp/navi_fixtures/espa-atnbrufossen-corridor.osm.pbf")
            val stagedTar = File("/data/local/tmp/navi_fixtures/elevation-corridor.tar")
            check(stagedPbf.isFile) { "missing ${stagedPbf.absolutePath}" }
            check(stagedTar.isFile) { "missing ${stagedTar.absolutePath}" }

            val pbf = File(dataDir, "espa-atnbrufossen-corridor.osm.pbf")
            stagedPbf.copyTo(pbf, overwrite = true)
            File(dataDir, "elevation").deleteRecursively()
            val tarProc =
                ProcessBuilder(
                    "tar",
                    "-xf",
                    stagedTar.absolutePath,
                    "-C",
                    dataDir.absolutePath,
                ).redirectErrorStream(true).start()
            val tarOut = tarProc.inputStream.bufferedReader().readText()
            check(tarProc.waitFor() == 0) { "tar failed: $tarOut" }

            carRoute =
                runCarCorridorPipeline(
                    pbfPath = pbf.absolutePath,
                    elevDir = File(dataDir, "elevation").absolutePath,
                    cacheDir = File(dataDir, "graph-cache").absolutePath,
                    breakIntervalHours = 1.0,
                )
            check(carRoute.routePolyline.contains(';')) { carRoute.report }
            check(carRoute.distanceKm > 5.0) { "car distance ${carRoute.distanceKm}" }
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
        MapHudPrefs.saveOptIn3d(context, false)
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
            "TerrainRouteScreenshotTest",
            "shot=$name kind=${NaviMapTestHooks.lastBasemapKind} " +
                "terrain=${NaviMapTestHooks.lastTerrainAttached} " +
                "pitch=${NaviMapTestHooks.lastCameraPitch} bytes=${out.length()}",
        )
    }

    private fun waitStyle(timeoutMs: Long = 45_000) {
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.styleReady || NaviMapTestHooks.lastReportedLayerCount >= 1) return
            Thread.sleep(400)
        }
    }

    private fun injectRoute(
        route: CorridorRouteResult,
        startLabel: String = "Espa",
        endLabel: String = "Atnbrufossen",
        viaLabel: String = "",
    ) {
        NaviMapTestHooks.routeStartLabel = startLabel
        NaviMapTestHooks.routeEndLabel = endLabel
        NaviMapTestHooks.routeViaLabel = viaLabel
        NaviMapTestHooks.pendingRoute = route
        val deadline = System.currentTimeMillis() + 60_000
        while (System.currentTimeMillis() < deadline) {
            Thread.sleep(500)
            if (NaviMapTestHooks.lastReportedLayerCount >= 1) return
            NaviMapTestHooks.pendingRoute = route
        }
    }

    private fun awaitKind(
        kind: String,
        terrain: Boolean?,
        timeoutMs: Long = 45_000,
    ) {
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            val okKind = NaviMapTestHooks.lastBasemapKind == kind
            val okTerrain = terrain == null || NaviMapTestHooks.lastTerrainAttached == terrain
            if (okKind && okTerrain) return
            Thread.sleep(400)
        }
        assertEquals(kind, NaviMapTestHooks.lastBasemapKind)
        if (terrain != null) {
            assertEquals(terrain, NaviMapTestHooks.lastTerrainAttached)
        }
    }

    @Test
    fun car_espa_atnbrufossen_3d_off() {
        activityRule.launchActivity(null)
        waitStyle()
        injectRoute(carRoute)

        MapHudPrefs.saveOptIn3d(context, false)
        // Force style re-resolve via activity restart so 3D state is clean.
        activityRule.finishActivity()
        Thread.sleep(800)
        activityRule.launchActivity(null)
        waitStyle()
        injectRoute(carRoute)
        // Offline-first: local Østlandet PMTiles cover Espa→Atnbrufossen.
        // With opt-in 3D off, expect OfflineProtomaps without hillshade attach.
        awaitKind("OfflineProtomaps", terrain = false)
        assertFalse(NaviMapTestHooks.lastTerrainAttached)
        capture("route_car_espa_atnbrufossen_3d_off.png")
    }
}
