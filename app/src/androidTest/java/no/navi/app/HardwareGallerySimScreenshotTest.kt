package no.navi.app

import android.graphics.Bitmap
import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.rule.ActivityTestRule
import androidx.test.uiautomator.By
import androidx.test.uiautomator.UiDevice
import androidx.test.uiautomator.Until
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
 * Real-hardware (SM-P613) gallery recapture: route-simulation scenes must show
 * the red **SIMULATING** banner. Emulator AVDs are not used.
 *
 * Host pull after run:
 * adb pull /data/local/tmp/navi_gallery_hw/ docs/images/
 */
@RunWith(AndroidJUnit4::class)
class HardwareGallerySimScreenshotTest {
    @get:Rule
    val activityRule = ActivityTestRule(MainActivity::class.java, false, false)

    private lateinit var context: android.content.Context
    private lateinit var dataDir: File
    private lateinit var device: UiDevice
    private lateinit var outDir: File

    companion object {
        private const val TAG = "NaviGalleryHw"
        private const val DEVICE_OUT = "/data/local/tmp/navi_gallery_hw"

        // Finnstad → Søndre Ommang → Rosenlund (60.8030670, 11.3283020).
        private val START = 60.8059250 to 11.3299030
        private val VIA = 60.8024498 to 11.3049950
        private val END = 60.8030670 to 11.3283020

        private const val SHOT_3D = "finnstad_sondre_ommang_3d.png"
        private const val SHOT_FLAT = "finnstad_sondre_ommang_flat.png"

        @JvmStatic
        lateinit var planned: CorridorRouteResult

        @JvmStatic
        @BeforeClass
        fun planCorridor() {
            val context = InstrumentationRegistry.getInstrumentation().targetContext
            val dataDir = NaviAppData.resolve(context).also { it.mkdirs() }
            OstlandetOfflineFixtures.ensureInstalled(dataDir)
            val auto = InstrumentationRegistry.getInstrumentation().uiAutomation
            auto.grantRuntimePermission(context.packageName, android.Manifest.permission.ACCESS_FINE_LOCATION)
            auto.grantRuntimePermission(context.packageName, android.Manifest.permission.ACCESS_COARSE_LOCATION)

            val pbf =
                listOf(
                    File("/data/local/tmp/navi_fixtures/espa-atnbrufossen-corridor.osm.pbf"),
                    File("/data/local/tmp/navi_fixtures/ostlandet-latest.osm.pbf"),
                ).firstOrNull { it.isFile && it.length() > 1_000_000L }
                    ?: error("missing corridor/Ostlandet PBF under /data/local/tmp/navi_fixtures")

            val elevDir = File(dataDir, "elevation").also { it.mkdirs() }
            val stagedTar = File("/data/local/tmp/navi_fixtures/elevation-corridor.tar")
            if (stagedTar.isFile && !File(elevDir, "copernicus").exists()) {
                val tarProc =
                    ProcessBuilder(
                        "tar",
                        "-xf",
                        stagedTar.absolutePath,
                        "-C",
                        dataDir.absolutePath,
                    ).redirectErrorStream(true).start()
                check(tarProc.waitFor(180, TimeUnit.SECONDS)) { "elevation tar timeout" }
                check(tarProc.exitValue() == 0) { "elevation tar failed" }
            }
            val cache = File(dataDir, "graph-cache-gallery-hw-car").also { it.mkdirs() }
            val vehicle =
                FfiVehicleLimits(
                    axleWeightKg = null,
                    bogieWeightKg = null,
                    heightM = null,
                    widthM = null,
                    lengthM = null,
                    totalWeightKg = null,
                )
            val wps = listOf(START, VIA, END)
            val legSamples = mutableListOf<List<RouteSimSample>>()
            var poly = ""
            var dist = 0.0
            var eta = 0.0
            var last: CorridorRouteResult? = null
            for (i in 0 until wps.lastIndex) {
                val a = wps[i]
                val b = wps[i + 1]
                val leg =
                    planCarRoute(
                        pbfPath = pbf.absolutePath,
                        elevDir = elevDir.absolutePath,
                        cacheDir = cache.absolutePath,
                        startLat = a.first,
                        startLon = a.second,
                        endLat = b.first,
                        endLon = b.second,
                        useEco = false,
                        profile = TravelProfile.CAR,
                        avoidMajor = false,
                        avoidTolls = false,
                        avoidFerries = false,
                        vehicle = vehicle,
                        preferOfficialNetworks = false,
                    )
                check(leg.report.contains("PASS") && leg.routePolyline.isNotBlank()) {
                    "leg ${i + 1} failed: ${leg.report.take(400)}"
                }
                dist += leg.distanceKm
                eta += leg.etaMinutes
                poly =
                    if (poly.isEmpty()) {
                        leg.routePolyline
                    } else {
                        poly + ";" + leg.routePolyline.substringAfter(';')
                    }
                legSamples.add(parseRouteSimSamples(leg.simSamplesJson))
                last = leg
            }
            val samples = mergeSimSamples(legSamples)
            val base = last!!
            planned =
                CorridorRouteResult(
                    report = "TEST_KIND=GALLERY_FINNSTAD_OMMANG\nPASS\ndistance_km=$dist\n",
                    distanceKm = dist,
                    etaMinutes = eta,
                    cacheHit = base.cacheHit,
                    coldBuildS = base.coldBuildS,
                    warmLoadS = base.warmLoadS,
                    routePolyline = poly,
                    poiLat = END.first,
                    poiLon = END.second,
                    poiName = "Rosenlund",
                    poiIconKey = base.poiIconKey,
                    breakPoisJson = "[]",
                    daysJson = "[]",
                    simSamplesJson =
                        org.json
                            .JSONArray(
                                samples.map { s ->
                                    org.json
                                        .JSONObject()
                                        .put("lat", s.lat)
                                        .put("lon", s.lon)
                                        .put("cum_m", s.cumM)
                                        .put("speed_kmh", s.speedKmh)
                                        .put("highway", s.highway)
                                        .put("maxspeed_posted", s.maxspeedPosted)
                                },
                            ).toString(),
                    maneuversJson = "[]",
                    priorityPathSharePct = base.priorityPathSharePct,
                )
            check(planned.routePolyline.isNotBlank()) { "empty polyline" }
            check(planned.simSamplesJson.length > 10) { "no sim samples" }
            Log.i(TAG, "PLANNED dist=${planned.distanceKm} samples=${samples.size}")
        }
    }

    @Before
    fun setUp() {
        context = InstrumentationRegistry.getInstrumentation().targetContext
        dataDir = NaviAppData.resolve(context)
        device = UiDevice.getInstance(InstrumentationRegistry.getInstrumentation())
        outDir = File(DEVICE_OUT).also { it.mkdirs() }
        shell("mkdir -p $DEVICE_OUT && chmod 777 $DEVICE_OUT")
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = true
        NaviMapTestHooks.simulationTimeScale = 30.0
        NaviMapTestHooks.requestStopRouteSimulation = true
        Thread.sleep(300)
        NaviMapTestHooks.requestStopRouteSimulation = false
        NaviMapTestHooks.simulatingActive = false
        MapHudPrefs.saveOptIn3d(context, false)
        MapHudPrefs.saveCameraTiltDeg(context, 0.0)
    }

    @Test
    fun capture_finnstad_docs_only() {
        activityRule.launchActivity(null)
        waitStyle()

        NaviMapTestHooks.hideSearchChrome = true
        NaviMapTestHooks.disableGpsFollow = true
        NaviMapTestHooks.followGps = false
        NaviMapTestHooks.requestOptIn3d = true
        MapHudPrefs.saveOptIn3d(context, true)
        NaviMapTestHooks.requestCameraTiltDeg = 0.0
        NaviMapTestHooks.routeStartLabel = "Finnstad"
        NaviMapTestHooks.routeViaLabel = "Søndre Ommang"
        NaviMapTestHooks.routeEndLabel = "Rosenlund"
        NaviMapTestHooks.pendingRoute = planned
        waitUntil(60_000) { NaviMapTestHooks.lastRoutePolylineChars > 50 }
        Thread.sleep(600)
        startSimulationMidRoute()
        NaviMapTestHooks.followGps = false
        frameFullRoute(planned.routePolyline, pad = 1.45)
        Thread.sleep(3_000)
        assertSimulatingBannerVisible()
        shot(SHOT_3D, requireSimulating = true, framePad = 1.45)

        NaviMapTestHooks.requestOptIn3d = false
        MapHudPrefs.saveOptIn3d(context, false)
        frameFullRoute(planned.routePolyline, pad = 1.45)
        Thread.sleep(2_500)
        assertSimulatingBannerVisible()
        shot(SHOT_FLAT, requireSimulating = true, framePad = 1.45)

        NaviMapTestHooks.requestStopRouteSimulation = true
        Log.i(TAG, "finnstad docs done")
    }

    @Test
    fun capture_follow_gps_tilt_route_idle_with_simulating_where_required() {
        activityRule.launchActivity(null)
        waitStyle()

        NaviMapTestHooks.routeStartLabel = "Finnstad"
        NaviMapTestHooks.routeViaLabel = "Søndre Ommang"
        NaviMapTestHooks.routeEndLabel = "Rosenlund"
        NaviMapTestHooks.pendingRoute = planned
        waitUntil(60_000) { NaviMapTestHooks.lastRoutePolylineChars > 50 }
        Thread.sleep(800)

        startSimulationMidRoute()
        assertSimulatingBannerVisible()
        shot("follow_gps/01_simulating_follow.png", requireSimulating = true)
        shot("route_map.png", requireSimulating = true)

        // Pan / zoom / recenter while still simulating.
        val gpsLat = NaviMapTestHooks.lastCameraLat
        val gpsLon = NaviMapTestHooks.lastCameraLon
        panMapHorizontal()
        waitUntil(8_000) { !NaviMapTestHooks.followGps }
        assertSimulatingBannerVisible()
        shot("follow_gps/02_after_pan.png", requireSimulating = true)

        val zoomBefore = NaviMapTestHooks.lastCameraZoom
        clickTag("zoom_in")
        waitUntil(4_000) { NaviMapTestHooks.lastCameraZoom > zoomBefore + 0.3 }
        assertSimulatingBannerVisible()
        shot("follow_gps/03_after_zoom_in.png", requireSimulating = true)

        val z2 = NaviMapTestHooks.lastCameraZoom
        clickTag("zoom_out")
        waitUntil(4_000) { NaviMapTestHooks.lastCameraZoom < z2 - 0.3 }
        assertSimulatingBannerVisible()
        shot("follow_gps/04_after_zoom_out.png", requireSimulating = true)

        clickTag("btn_recenter_gps")
        waitUntil(5_000) { NaviMapTestHooks.followGps }
        Thread.sleep(600)
        assertTrue(
            "recenter should leave pan",
            haversineM(gpsLat, gpsLon, NaviMapTestHooks.lastCameraLat, NaviMapTestHooks.lastCameraLon) > 30.0 ||
                NaviMapTestHooks.followGps,
        )
        assertSimulatingBannerVisible()
        shot("follow_gps/05_after_recenter.png", requireSimulating = true)

        for (mode in listOf(
            MapRotationMode.NorthUp,
            MapRotationMode.Compass,
            MapRotationMode.DirectionOfTravel,
        )) {
            NaviMapTestHooks.requestRotationMode = mode
            Thread.sleep(350)
        }
        assertSimulatingBannerVisible()
        shot("follow_gps/06_rotation_modes_ok.png", requireSimulating = true)

        // 45° tilt while still simulating (3D off, then on).
        NaviMapTestHooks.requestCameraTiltDeg = 45.0
        NaviMapTestHooks.requestOptIn3d = false
        awaitPitch(45.0)
        Thread.sleep(1_500)
        assertSimulatingBannerVisible()
        shot("tilt45_3d_off.png", requireSimulating = true)

        NaviMapTestHooks.requestOptIn3d = true
        MapHudPrefs.saveOptIn3d(context, true)
        val terrainDeadline = System.currentTimeMillis() + 90_000
        while (System.currentTimeMillis() < terrainDeadline) {
            if (NaviMapTestHooks.lastTerrainAttached ||
                NaviMapTestHooks.lastBasemapKind == "OfflineProtomaps"
            ) {
                break
            }
            if (kotlin.math.abs(NaviMapTestHooks.lastCameraPitch - 45.0) > 2.0) {
                NaviMapTestHooks.requestCameraTiltDeg = 45.0
            }
            Thread.sleep(300)
        }
        NaviMapTestHooks.requestCameraTiltDeg = 45.0
        awaitPitch(45.0, timeoutMs = 20_000)
        Thread.sleep(2_000)
        assertSimulatingBannerVisible()
        shot("tilt45_3d_on.png", requireSimulating = true)

        // Idle both bars: stop simulation and clear route (honest idle — no SIMULATING).
        NaviMapTestHooks.requestStopRouteSimulation = true
        Thread.sleep(500)
        NaviMapTestHooks.requestStopRouteSimulation = false
        NaviMapTestHooks.requestClearRoute = true
        Thread.sleep(800)
        NaviMapTestHooks.requestClearRoute = false
        assertFalse(NaviMapTestHooks.simulatingActive)
        NaviMapTestHooks.hideSearchChrome = true
        NaviMapTestHooks.requestCloseTools = true
        NaviMapTestHooks.requestCameraTiltDeg = 0.0
        NaviMapTestHooks.requestOptIn3d = false
        MapHudPrefs.saveOptIn3d(context, false)
        Thread.sleep(1_000)
        shot("hud/hud_idle_both_bars.png", requireSimulating = false)

        // Full-route overview: re-apply corridor after idle clear, then frame Start→End
        // (3D on, then flat). Do not fly the camera to an unrelated valley.
        NaviMapTestHooks.disableGpsFollow = true
        NaviMapTestHooks.followGps = false
        NaviMapTestHooks.requestOptIn3d = true
        MapHudPrefs.saveOptIn3d(context, true)
        NaviMapTestHooks.requestCameraTiltDeg = 0.0
        NaviMapTestHooks.routeStartLabel = "Finnstad"
        NaviMapTestHooks.routeViaLabel = "Søndre Ommang"
        NaviMapTestHooks.routeEndLabel = "Rosenlund"
        NaviMapTestHooks.pendingRoute = planned
        waitUntil(60_000) { NaviMapTestHooks.lastRoutePolylineChars > 50 }
        Thread.sleep(600)
        startSimulationMidRoute()
        NaviMapTestHooks.followGps = false
        frameFullRoute(planned.routePolyline, pad = 1.45)
        Thread.sleep(3_000)
        assertSimulatingBannerVisible()
        shot(SHOT_3D, requireSimulating = true, framePad = 1.45)

        NaviMapTestHooks.requestOptIn3d = false
        MapHudPrefs.saveOptIn3d(context, false)
        frameFullRoute(planned.routePolyline, pad = 1.45)
        Thread.sleep(2_500)
        assertSimulatingBannerVisible()
        shot(SHOT_FLAT, requireSimulating = true, framePad = 1.45)

        // Tighter full-route frame (still Start→End visible) on online Liberty.
        NaviMapTestHooks.forceOnlineBasemap = true
        NaviMapTestHooks.hideUiChrome = false
        frameFullRoute(planned.routePolyline, pad = 1.2)
        Thread.sleep(5_000)
        assertSimulatingBannerVisible()
        shot("zoom_z16.png", requireSimulating = true, framePad = 1.2)

        NaviMapTestHooks.requestStopRouteSimulation = true
        NaviMapTestHooks.forceOnlineBasemap = false
        NaviMapTestHooks.disableGpsFollow = false
        Log.i(TAG, "DONE files=${outDir.walkTopDown().filter { it.isFile }.map { it.name }.toList()}")
    }

    private fun frameFullRoute(
        polyline: String,
        pad: Double,
    ) {
        val cam = RouteCameraFit.fromPolyline(polyline, pad = pad)
        NaviMapTestHooks.disableGpsFollow = true
        NaviMapTestHooks.followGps = false
        NaviMapTestHooks.pendingCamera = cam
        Thread.sleep(400)
        NaviMapTestHooks.pendingCamera = cam
        Log.i(TAG, "frameFullRoute lat=${cam.first} lon=${cam.second} z=${cam.third} pad=$pad")
    }

    private fun startSimulationMidRoute() {
        if (!NaviMapTestHooks.disableGpsFollow) {
            NaviMapTestHooks.followGps = true
        }
        NaviMapTestHooks.lastSimAlongM = 0.0
        NaviMapTestHooks.simulationTimeScale = 1.0
        NaviMapTestHooks.requestPrepareRouteSimulation = true
        Thread.sleep(700)
        val mid = (planned.distanceKm * 1000.0 * 0.4).coerceAtLeast(200.0)
        NaviMapTestHooks.requestSimSeekCumM = mid
        val deadline = System.currentTimeMillis() + 20_000
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.simulatingActive && NaviMapTestHooks.lastSimAlongM > 50.0) break
            Thread.sleep(150)
        }
        assertTrue(
            "simulatingActive after prepare+seek (along=${NaviMapTestHooks.lastSimAlongM})",
            NaviMapTestHooks.simulatingActive,
        )
        // Crawl so the short hop does not finish mid-gallery.
        NaviMapTestHooks.simulationTimeScale = 0.25
        Thread.sleep(500)
    }

    /** Re-seek if playback ended so SIMULATING stays up for the next shot. */
    private fun holdSimulation() {
        if (!NaviMapTestHooks.simulatingActive || NaviMapTestHooks.lastArrivedAtEnd) {
            startSimulationMidRoute()
        } else {
            val mid = (planned.distanceKm * 1000.0 * 0.4).coerceAtLeast(200.0)
            NaviMapTestHooks.requestSimSeekCumM = mid
            NaviMapTestHooks.simulationTimeScale = 0.25
            Thread.sleep(400)
        }
        assertTrue("hold simulatingActive", NaviMapTestHooks.simulatingActive)
    }

    private fun assertSimulatingBannerVisible() {
        holdSimulation()
        assertTrue("hook simulatingActive", NaviMapTestHooks.simulatingActive)
        val seen =
            device.wait(Until.hasObject(By.text("SIMULATING")), 5_000) ||
                device.hasObject(By.desc("SIMULATING"))
        assertTrue("SIMULATING banner must be on screen", seen)
    }

    private fun shot(
        relative: String,
        requireSimulating: Boolean,
        framePad: Double? = null,
    ) {
        if (requireSimulating) assertSimulatingBannerVisible()
        if (framePad != null) {
            frameFullRoute(planned.routePolyline, pad = framePad)
            Thread.sleep(900)
        }
        dismissPermissionIfPresent()
        Thread.sleep(400)
        val path = "$DEVICE_OUT/$relative"
        shell("mkdir -p $(dirname $path)")
        // Prefer shell screencap (live framebuffer) over UiAutomation bitmap.
        shell("screencap -p $path")
        shell("chmod 644 $path")
        val f = File(path)
        // Fallback if screencap path not readable from host process yet
        if (!f.isFile || f.length() < 3_000) {
            val bmp = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
            assertNotNull(bmp)
            f.parentFile?.mkdirs()
            f.outputStream().use { bmp!!.compress(Bitmap.CompressFormat.PNG, 100, it) }
        }
        assertTrue("$relative too small (${f.length()})", f.isFile && f.length() > 3_000)
        Log.i(
            TAG,
            "SHOT $relative bytes=${f.length()} sim=${NaviMapTestHooks.simulatingActive} " +
                "kind=${NaviMapTestHooks.lastBasemapKind} pitch=${NaviMapTestHooks.lastCameraPitch}",
        )
    }

    private fun waitStyle() {
        waitUntil(90_000) { NaviMapTestHooks.styleReady }
        assertTrue(NaviMapTestHooks.styleReady)
    }

    private fun awaitPitch(
        target: Double,
        timeoutMs: Long = 25_000,
    ) {
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            if (kotlin.math.abs(NaviMapTestHooks.lastCameraPitch - target) <= 2.0) return
            NaviMapTestHooks.requestCameraTiltDeg = target
            Thread.sleep(200)
        }
        assertTrue(
            "pitch want=$target got=${NaviMapTestHooks.lastCameraPitch}",
            kotlin.math.abs(NaviMapTestHooks.lastCameraPitch - target) <= 2.0,
        )
    }

    private fun clickTag(tag: String) {
        runCatching {
            device.wait(Until.hasObject(By.res(".*$tag.*".toPattern())), 3_000)
        }
        // Compose test tags are not always resource ids — use instrumentation click via hooks where needed.
        when (tag) {
            "zoom_in" -> {
                // DriveHud zoom buttons: try content description / text fallbacks
                if (!device.hasObject(By.text("+"))) {
                    // Coordinate tap near bottom-left zoom on portrait tablet
                    val w = device.displayWidth
                    val h = device.displayHeight
                    device.click((w * 0.08).toInt(), (h * 0.92).toInt())
                } else {
                    device.findObject(By.text("+"))?.click()
                }
            }
            "zoom_out" -> {
                if (!device.hasObject(By.text("-"))) {
                    val w = device.displayWidth
                    val h = device.displayHeight
                    device.click((w * 0.04).toInt(), (h * 0.92).toInt())
                } else {
                    device.findObject(By.text("-"))?.click()
                }
            }
            "btn_recenter_gps" -> {
                if (device.hasObject(By.text("Recenter"))) {
                    device.findObject(By.text("Recenter"))?.click()
                } else {
                    NaviMapTestHooks.requestRecenterGps = true
                }
            }
        }
        Thread.sleep(400)
    }

    private fun panMapHorizontal() {
        val w = device.displayWidth
        val h = device.displayHeight
        device.swipe(w * 3 / 4, h / 2, w / 4, h / 2, 40)
        Thread.sleep(500)
    }

    private fun dismissPermissionIfPresent() {
        for (label in listOf("While using the app", "Allow", "ONLY THIS TIME", "While using the app")) {
            val obj = device.findObject(By.text(label))
            if (obj != null) {
                obj.click()
                Thread.sleep(400)
            }
        }
    }

    private fun waitUntil(
        timeoutMs: Long,
        pred: () -> Boolean,
    ) {
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            if (pred()) return
            Thread.sleep(100)
        }
        assertTrue("timeout ${timeoutMs}ms", pred())
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

    private fun haversineM(
        lat1: Double,
        lon1: Double,
        lat2: Double,
        lon2: Double,
    ): Double {
        val r = 6_371_000.0
        val p1 = Math.toRadians(lat1)
        val p2 = Math.toRadians(lat2)
        val dLat = Math.toRadians(lat2 - lat1)
        val dLon = Math.toRadians(lon2 - lon1)
        val a =
            kotlin.math.sin(dLat / 2) * kotlin.math.sin(dLat / 2) +
                kotlin.math.cos(p1) * kotlin.math.cos(p2) *
                kotlin.math.sin(dLon / 2) * kotlin.math.sin(dLon / 2)
        return 2 * r * kotlin.math.atan2(kotlin.math.sqrt(a), kotlin.math.sqrt(1 - a))
    }
}
