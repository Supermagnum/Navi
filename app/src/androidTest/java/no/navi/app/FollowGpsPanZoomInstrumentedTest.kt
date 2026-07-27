package no.navi.app

import android.graphics.Bitmap
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.UiDevice
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.BeforeClass
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.CorridorRouteResult
import uniffi.navi.FfiVehicleLimits
import uniffi.navi.TravelProfile
import uniffi.navi.planCarRoute
import java.io.File
import java.util.concurrent.TimeUnit

/**
 * Manual pan must not be undone by zoom while a simulated GPS fix is moving.
 *
 * Sequence: plan + simulate → pan away → zoom in/out → camera stays → Recenter.
 */
@RunWith(AndroidJUnit4::class)
class FollowGpsPanZoomInstrumentedTest {

    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

    companion object {
        // Short Hamar-area hop (same corridor family as LiveRouteSimulation).
        private val START = 60.8059250 to 11.3299030
        private val END = 60.8023620 to 11.3053691

        @JvmStatic
        @BeforeClass
        fun beforeClass() {
            val pkg = InstrumentationRegistry.getInstrumentation().targetContext.packageName
            runCatching {
                InstrumentationRegistry.getInstrumentation().uiAutomation
                    .grantRuntimePermission(pkg, android.Manifest.permission.ACCESS_FINE_LOCATION)
            }
            NaviMapTestHooks.hideUiChrome = false
            NaviMapTestHooks.hideSearchChrome = true
            NaviMapTestHooks.simulationTimeScale = 40.0
        }
    }

    @Before
    fun setUp() {
        NaviMapTestHooks.requestStopRouteSimulation = true
        Thread.sleep(300)
        NaviMapTestHooks.requestStopRouteSimulation = false
        NaviMapTestHooks.followGps = true
        NaviMapTestHooks.requestRecenterGps = false
        NaviMapTestHooks.lastSimAlongM = 0.0
        NaviMapTestHooks.simulatingActive = false
        composeRule.waitForIdle()
        Thread.sleep(200)
    }

    @Test
    fun pan_then_zoom_keeps_camera_recenter_returns_to_gps() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val dataDir = NaviAppData.resolve(context).also { it.mkdirs() }
        val shotDir = File(dataDir, "follow_gps_shots").also { it.mkdirs() }

        composeRule.waitForIdle()
        waitUntil(45_000) {
            NaviMapTestHooks.styleReady || NaviMapTestHooks.lastReportedLayerCount >= 1
        }

        val route = planShortHop(dataDir)
        assertTrue(route.report.contains("PASS"))
        assertTrue(route.routePolyline.isNotBlank())
        assertTrue(route.simSamplesJson.length > 10)

        NaviMapTestHooks.lastRoutePolylineChars = 0
        NaviMapTestHooks.simulatingActive = false
        NaviMapTestHooks.lastSimAlongM = 0.0
        NaviMapTestHooks.pendingRoute = route
        waitUntil(60_000) { NaviMapTestHooks.lastRoutePolylineChars > 50 }
        Thread.sleep(800)

        // Keep requesting start until the host accepts (needs routeSamples >= 2).
        val simStartDeadline = System.currentTimeMillis() + 30_000
        while (System.currentTimeMillis() < simStartDeadline && !NaviMapTestHooks.simulatingActive) {
            NaviMapTestHooks.requestStartRouteSimulation = true
            Thread.sleep(250)
        }
        assertTrue(
            "SIMULATING must become active (polylineChars=${NaviMapTestHooks.lastRoutePolylineChars})",
            NaviMapTestHooks.simulatingActive,
        )
        // Seek into the corridor so the live fix is mid-route (moving GPS under camera).
        NaviMapTestHooks.requestSimSeekCumM = 400.0
        Thread.sleep(800)
        waitUntil(15_000) {
            NaviMapTestHooks.lastSimAlongM > 50.0 ||
                (NaviMapTestHooks.lastCameraLat != 0.0 && NaviMapTestHooks.simulatingActive)
        }
        Thread.sleep(800)
        shot(shotDir, "01_simulating_follow.png")

        InstrumentationRegistry.getInstrumentation().runOnMainSync {
            NaviMapTestHooks.requestRecenterGps = true
        }
        waitUntil(5_000) { NaviMapTestHooks.followGps }
        Thread.sleep(600)
        val gpsLat = NaviMapTestHooks.lastCameraLat
        val gpsLon = NaviMapTestHooks.lastCameraLon
        assertTrue("expected GPS-centered camera", kotlin.math.abs(gpsLat) > 1.0)

        panMapHorizontal()
        waitUntil(8_000) {
            !NaviMapTestHooks.followGps &&
                haversineM(
                    gpsLat,
                    gpsLon,
                    NaviMapTestHooks.lastCameraLat,
                    NaviMapTestHooks.lastCameraLon,
                ) > 80.0
        }
        assertFalse("pan must disable follow mode", NaviMapTestHooks.followGps)
        val panLat = NaviMapTestHooks.lastCameraLat
        val panLon = NaviMapTestHooks.lastCameraLon
        shot(shotDir, "02_after_pan.png")

        composeRule.onNodeWithTag("btn_recenter_gps", useUnmergedTree = true).assertIsDisplayed()

        val zoomBefore = NaviMapTestHooks.lastCameraZoom
        composeRule.onNodeWithTag("zoom_in", useUnmergedTree = true).performClick()
        composeRule.waitForIdle()
        Thread.sleep(700)
        waitUntil(4_000) { NaviMapTestHooks.lastCameraZoom > zoomBefore + 0.4 }
        val afterZoomInLat = NaviMapTestHooks.lastCameraLat
        val afterZoomInLon = NaviMapTestHooks.lastCameraLon
        val driftIn = haversineM(panLat, panLon, afterZoomInLat, afterZoomInLon)
        assertTrue(
            "zoom in must not snap back to GPS (drift=${driftIn}m)",
            driftIn < 60.0,
        )
        assertFalse(NaviMapTestHooks.followGps)
        shot(shotDir, "03_after_zoom_in.png")

        val z2 = NaviMapTestHooks.lastCameraZoom
        composeRule.onNodeWithTag("zoom_out", useUnmergedTree = true).performClick()
        composeRule.waitForIdle()
        Thread.sleep(700)
        waitUntil(4_000) { NaviMapTestHooks.lastCameraZoom < z2 - 0.4 }
        val afterZoomOutLat = NaviMapTestHooks.lastCameraLat
        val afterZoomOutLon = NaviMapTestHooks.lastCameraLon
        val driftOut = haversineM(panLat, panLon, afterZoomOutLat, afterZoomOutLon)
        assertTrue("zoom out must keep panned center (drift=${driftOut}m)", driftOut < 80.0)
        shot(shotDir, "04_after_zoom_out.png")

        // Confirm the live fix is still updating while the camera stays panned away:
        // seek further along the corridor and check along-route progress moves.
        val alongBefore = NaviMapTestHooks.lastSimAlongM
        NaviMapTestHooks.requestSimSeekCumM = (alongBefore + 250.0).coerceAtLeast(650.0)
        waitUntil(10_000) { NaviMapTestHooks.lastSimAlongM > alongBefore + 50.0 }

        composeRule.onNodeWithTag("btn_recenter_gps", useUnmergedTree = true).performClick()
        composeRule.waitForIdle()
        waitUntil(5_000) { NaviMapTestHooks.followGps }
        Thread.sleep(700)
        assertTrue(NaviMapTestHooks.followGps)
        val leftPan = haversineM(
            panLat,
            panLon,
            NaviMapTestHooks.lastCameraLat,
            NaviMapTestHooks.lastCameraLon,
        )
        assertTrue("recenter should leave the panned view (moved ${leftPan}m)", leftPan > 50.0)
        shot(shotDir, "05_after_recenter.png")

        for (mode in listOf(
            MapRotationMode.NorthUp,
            MapRotationMode.Compass,
            MapRotationMode.DirectionOfTravel,
        )) {
            InstrumentationRegistry.getInstrumentation().runOnMainSync {
                NaviMapTestHooks.requestRotationMode = mode
            }
            Thread.sleep(400)
            panMapHorizontal()
            waitUntil(6_000) { !NaviMapTestHooks.followGps }
            val latM = NaviMapTestHooks.lastCameraLat
            val lonM = NaviMapTestHooks.lastCameraLon
            composeRule.onNodeWithTag("zoom_in", useUnmergedTree = true).performClick()
            composeRule.waitForIdle()
            Thread.sleep(500)
            val d = haversineM(latM, lonM, NaviMapTestHooks.lastCameraLat, NaviMapTestHooks.lastCameraLon)
            assertTrue("mode=$mode zoom must keep pan (drift=${d}m)", d < 80.0)
            InstrumentationRegistry.getInstrumentation().runOnMainSync {
                NaviMapTestHooks.requestRecenterGps = true
            }
            waitUntil(5_000) { NaviMapTestHooks.followGps }
        }
        shot(shotDir, "06_rotation_modes_ok.png")

        // Pull shots to host docs if the instrumented process can see the path
        // (otherwise adb pull after the test).
        val docs = File(
            "/mnt/2e9a1e9f-2097-408c-ab9a-a01b32f11d28/github-projects/Navi/docs/images/follow_gps",
        )
        runCatching {
            docs.mkdirs()
            shotDir.listFiles()?.filter { it.extension == "png" }?.forEach { f ->
                f.copyTo(File(docs, f.name), overwrite = true)
            }
        }

        InstrumentationRegistry.getInstrumentation().runOnMainSync {
            NaviMapTestHooks.requestStopRouteSimulation = true
        }
    }

    private fun planShortHop(dataDir: File): CorridorRouteResult {
        val pbf = listOf(
            File("/data/local/tmp/navi_fixtures/ostlandet-latest.osm.pbf"),
            File("/data/local/tmp/navi_fixtures/oppland-latest.osm.pbf"),
            File("/data/local/tmp/navi_fixtures/espa-atnbrufossen-corridor.osm.pbf"),
            File(dataDir, "ostlandet-latest.osm.pbf"),
            File(dataDir, "espa-atnbrufossen-corridor.osm.pbf"),
        ).firstOrNull { it.isFile && it.length() > 1_000_000L }
            ?: error("missing Ostlandet/Oppland/Espa PBF under /data/local/tmp/navi_fixtures")

        val elevDir = File(dataDir, "elevation").also { it.mkdirs() }
        val stagedTar = File("/data/local/tmp/navi_fixtures/elevation-corridor.tar")
        if (stagedTar.isFile && !File(elevDir, "copernicus").exists()) {
            val tarProc = ProcessBuilder(
                "tar", "-xf", stagedTar.absolutePath, "-C", dataDir.absolutePath,
            ).redirectErrorStream(true).start()
            tarProc.waitFor(120, TimeUnit.SECONDS)
        }
        val cache = File(dataDir, "graph-cache-follow-gps").also { it.mkdirs() }
        return planCarRoute(
            pbfPath = pbf.absolutePath,
            elevDir = elevDir.absolutePath,
            cacheDir = cache.absolutePath,
            startLat = START.first,
            startLon = START.second,
            endLat = END.first,
            endLon = END.second,
            useEco = false,
            profile = TravelProfile.CAR,
            avoidMajor = false,
            avoidTolls = false,
            avoidFerries = false,
            vehicle = FfiVehicleLimits(
                axleWeightKg = null,
                bogieWeightKg = null,
                heightM = null,
                widthM = null,
                lengthM = null,
                totalWeightKg = null,
            ),
            preferOfficialNetworks = false,
        )
    }

    private fun panMapHorizontal() {
        val device = UiDevice.getInstance(InstrumentationRegistry.getInstrumentation())
        val w = device.displayWidth
        val h = device.displayHeight
        device.swipe(w * 3 / 4, h / 2, w / 4, h / 2, 40)
        Thread.sleep(500)
        composeRule.waitForIdle()
    }

    private fun shot(dir: File, name: String) {
        val shot = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
        assertNotNull("screenshot $name", shot)
        assertTrue(shot!!.width > 0)
        val out = File(dir, name)
        out.outputStream().use {
            shot.compress(Bitmap.CompressFormat.PNG, 100, it)
        }
        assertTrue("$name too small", out.length() > 3_000)
        val pfd = InstrumentationRegistry.getInstrumentation().uiAutomation
            .executeShellCommand("su 0 cp ${out.absolutePath} /data/local/tmp/$name")
        java.io.FileInputStream(pfd.fileDescriptor).use { input ->
            val buf = ByteArray(4096)
            while (input.read(buf) >= 0) {
            }
        }
        pfd.close()
    }

    private fun waitUntil(timeoutMs: Long, pred: () -> Boolean) {
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            if (pred()) return
            Thread.sleep(100)
        }
        assertTrue("timeout ${timeoutMs}ms", pred())
    }

    private fun haversineM(lat1: Double, lon1: Double, lat2: Double, lon2: Double): Double {
        val r = 6_378_100.0
        val dLat = Math.toRadians(lat2 - lat1)
        val dLon = Math.toRadians(lon2 - lon1)
        val a = Math.sin(dLat / 2) * Math.sin(dLat / 2) +
            Math.cos(Math.toRadians(lat1)) * Math.cos(Math.toRadians(lat2)) *
            Math.sin(dLon / 2) * Math.sin(dLon / 2)
        return 2 * r * Math.asin(Math.sqrt(a))
    }
}
