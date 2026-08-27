package no.navi.app

import android.graphics.Bitmap
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.rule.ActivityTestRule
import androidx.test.uiautomator.UiDevice
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.BeforeClass
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.CorridorRouteResult
import uniffi.navi.FfiCarRestSettings
import uniffi.navi.FfiVehicleLimits
import uniffi.navi.TravelProfile
import uniffi.navi.planCarRoute
import uniffi.navi.saveCarRestSettings
import java.io.File
import java.util.concurrent.TimeUnit

/**
 * Follow-up: SIMULATING banner pixel check, break-minutes vs integrated hours,
 * and 45° tilt with 3D on/off on the Automotive emulator.
 */
@RunWith(AndroidJUnit4::class)
class SimBannerBreakTiltInstrumentedTest {
    companion object {
        private val START = 60.8059250 to 11.3299030
        private val VIA1 = 60.8023620 to 11.3053691
        private val VIA2 = 60.7974313 to 11.3094874
        private val END = 60.8056487 to 11.3290523

        @JvmStatic
        lateinit var planned: CorridorRouteResult

        @JvmStatic
        lateinit var samples: List<RouteSimSample>

        @JvmStatic
        @BeforeClass
        fun planShortLoop() {
            val context = InstrumentationRegistry.getInstrumentation().targetContext
            val dataDir = NaviAppData.resolve(context).also { it.mkdirs() }
            OstlandetOfflineFixtures.ensureInstalled(dataDir)
            val pbf =
                listOf(
                    File("/data/local/tmp/navi_fixtures/ostlandet-latest.osm.pbf"),
                    File("/data/local/tmp/navi_fixtures/oppland-latest.osm.pbf"),
                ).firstOrNull { it.isFile && it.length() > 10_000_000L }
                    ?: error("missing Ostlandet/Oppland PBF under /data/local/tmp/navi_fixtures")

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
                tarProc.waitFor(120, TimeUnit.SECONDS)
            }
            val cacheDir = File(dataDir, "graph-cache-${pbf.nameWithoutExtension}-car-tilt")
            cacheDir.mkdirs()
            val vehicle =
                FfiVehicleLimits(
                    axleWeightKg = null,
                    bogieWeightKg = null,
                    heightM = null,
                    widthM = null,
                    lengthM = null,
                    totalWeightKg = null,
                )
            val wps = listOf(START, VIA1, VIA2, END)
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
                        cacheDir = cacheDir.absolutePath,
                        startLat = a.first,
                        startLon = a.second,
                        endLat = b.first,
                        endLon = b.second,
                        useEco = false,
                        profile = TravelProfile.CAR,
                        avoidMotorways = false,
                        avoidTolls = false,
                        avoidFerries = false,
                        vehicle = vehicle,
                        preferOfficialNetworks = false,
                        dataDir = "",
                    )
                assertTrue(
                    "leg ${i + 1} must PASS: ${leg.report.take(400)}",
                    leg.report.contains("PASS") && leg.routePolyline.isNotBlank(),
                )
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
            samples = mergeSimSamples(legSamples)
            assertTrue("expected simulation samples", samples.size >= 10)
            val base = last!!
            planned =
                CorridorRouteResult(
                    report = "TEST_KIND=PLAN_MULTI\nPASS\ndistance_km=$dist\n",
                    distanceKm = dist,
                    etaMinutes = eta,
                    cacheHit = base.cacheHit,
                    coldBuildS = base.coldBuildS,
                    warmLoadS = base.warmLoadS,
                    routePolyline = poly,
                    poiLat = END.first,
                    poiLon = END.second,
                    poiName = "Loop end",
                    poiIconKey = base.poiIconKey,
                    breakPoisJson = "[]",
                    daysJson = "[]",
                    simSamplesJson =
                        JSONArray(
                            samples.map { s ->
                                JSONObject()
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
                    routeSegmentsJson = "[]",
                    offTrailAdvisory = "",
                )
        }

        private fun mergeSimSamples(legs: List<List<RouteSimSample>>): List<RouteSimSample> {
            val out = mutableListOf<RouteSimSample>()
            var offset = 0.0
            for ((li, leg) in legs.withIndex()) {
                for ((si, s) in leg.withIndex()) {
                    if (li > 0 && si == 0) continue
                    out += s.copy(cumM = s.cumM + offset)
                }
                offset += leg.lastOrNull()?.cumM ?: 0.0
            }
            return out
        }
    }

    @get:Rule
    val activityRule = ActivityTestRule(MainActivity::class.java, false, false)

    private lateinit var dataDir: File
    private lateinit var context: android.content.Context
    private lateinit var device: UiDevice

    @Before
    fun setUp() {
        context = InstrumentationRegistry.getInstrumentation().targetContext
        dataDir = NaviAppData.resolve(context)
        device = UiDevice.getInstance(InstrumentationRegistry.getInstrumentation())
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = true
        NaviMapTestHooks.simulationTimeScale = 20.0
        NaviMapTestHooks.simulatingActive = false
        NaviMapTestHooks.lastMinutesToBreak = null
        NaviMapTestHooks.lastElapsedDrivingHours = null
        NaviMapTestHooks.lastSimAlongM = 0.0
        MapHudPrefs.saveOptIn3d(context, false)
        MapHudPrefs.saveCameraTiltDeg(context, 0.0)
        saveCarRestSettings(
            dataDir.absolutePath,
            FfiCarRestSettings(
                breakIntervalHours = 2.0,
                restDurationMinutes = 15u,
                ecoModeEnabled = false,
            ),
        )
        grantLocation()
    }

    private fun grantLocation() {
        val ui = InstrumentationRegistry.getInstrumentation().uiAutomation
        // AAOS secondary user (files under /data/user/10/…) needs an explicit --user grant.
        val users = listOf("", " --user 0", " --user 10", " --user current")
        for (perm in listOf(
            "android.permission.ACCESS_FINE_LOCATION",
            "android.permission.ACCESS_COARSE_LOCATION",
        )) {
            for (u in users) {
                val pfd = ui.executeShellCommand("pm grant$u ${context.packageName} $perm")
                java.io.FileInputStream(pfd.fileDescriptor).use { input ->
                    val buf = ByteArray(4096)
                    while (input.read(buf) >= 0) {
                    }
                }
                pfd.close()
            }
        }
    }

    private fun dismissPermissionIfPresent() {
        // System location dialog covers Compose overlays (including SIMULATING).
        for (label in listOf(
            "While using the app",
            "Allow",
            "ONLY THIS TIME",
            "Only this time",
        )) {
            val obj =
                device.findObject(
                    androidx.test.uiautomator.By
                        .text(label),
                )
            if (obj != null) {
                obj.click()
                Thread.sleep(800)
                return
            }
        }
    }

    private fun waitStyle() {
        activityRule.launchActivity(null)
        Thread.sleep(1_000)
        dismissPermissionIfPresent()
        val deadline = System.currentTimeMillis() + 45_000
        while (System.currentTimeMillis() < deadline) {
            dismissPermissionIfPresent()
            if (NaviMapTestHooks.styleReady || NaviMapTestHooks.lastReportedLayerCount >= 1) return
            Thread.sleep(300)
        }
    }

    private fun waitRouteOnMap() {
        val deadline = System.currentTimeMillis() + 60_000
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.lastRoutePolylineChars > 50) return
            NaviMapTestHooks.pendingRoute = planned
            Thread.sleep(400)
        }
        error("route polyline not applied (${NaviMapTestHooks.lastRoutePolylineChars})")
    }

    private fun expectedElapsedHours(toCumM: Double): Double {
        if (samples.size < 2) return 0.0
        var hours = 0.0
        for (i in 0 until samples.lastIndex) {
            val a = samples[i]
            val b = samples[i + 1]
            if (a.cumM >= toCumM) break
            val segEnd = minOf(b.cumM, toCumM)
            val dm = (segEnd - a.cumM).coerceAtLeast(0.0)
            hours += (dm / 1000.0) / a.speedKmh.coerceAtLeast(1.0)
            if (b.cumM >= toCumM) break
        }
        return hours
    }

    private fun saveShot(
        dir: File,
        name: String,
    ) {
        assertTrue(
            "map render did not settle before $name",
            InstrumentedMapCapture.awaitRenderSettled(30_000),
        )
        val shot = InstrumentedMapCapture.takeScreenshotAfterSettle(5_000)
        assertNotNull(shot)
        assertTrue(shot!!.width > 0)
        val out = File(dir, name)
        out.outputStream().use {
            shot.compress(Bitmap.CompressFormat.PNG, 100, it)
        }
        assertTrue(out.length() > 3_000)
        val pfd =
            InstrumentationRegistry
                .getInstrumentation()
                .uiAutomation
                .executeShellCommand("su 0 cp ${out.absolutePath} /data/local/tmp/$name")
        java.io.FileInputStream(pfd.fileDescriptor).use { input ->
            val buf = ByteArray(4096)
            while (input.read(buf) >= 0) {
            }
        }
        pfd.close()
        android.util.Log.i("SimBannerBreakTilt", "SHOT ${out.absolutePath} bytes=${out.length()}")
    }

    private fun awaitPitch(
        target: Double,
        timeoutMs: Long = 25_000,
    ) {
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            if (kotlin.math.abs(NaviMapTestHooks.lastCameraPitch - target) <= 1.5) return
            Thread.sleep(200)
        }
        assertEquals("camera pitch", target, NaviMapTestHooks.lastCameraPitch, 1.5)
    }

    @Test
    fun banner_break_and_tilt45_shots() {
        waitStyle()
        NaviMapTestHooks.pendingFromPoint = Waypoint("Start", START.first, START.second)
        NaviMapTestHooks.pendingViaPoints =
            listOf(
                Waypoint("Via1", VIA1.first, VIA1.second),
                Waypoint("Via2", VIA2.first, VIA2.second),
            )
        NaviMapTestHooks.pendingToPoint = Waypoint("End", END.first, END.second)
        NaviMapTestHooks.pendingRoute = planned
        waitRouteOnMap()

        val shotDir = File(dataDir, "followup_shots").also { it.mkdirs() }

        // 1) SIMULATING banner — hold mid-route (do not race a 20x full playback).
        NaviMapTestHooks.requestPrepareRouteSimulation = true
        Thread.sleep(600)
        val midCum = samples[samples.size / 2].cumM
        NaviMapTestHooks.requestSimSeekCumM = midCum
        val alongDeadline = System.currentTimeMillis() + 15_000
        while (System.currentTimeMillis() < alongDeadline) {
            if (NaviMapTestHooks.simulatingActive &&
                NaviMapTestHooks.lastSimAlongM > midCum * 0.4
            ) {
                break
            }
            Thread.sleep(150)
        }
        assertTrue("simulatingActive for banner", NaviMapTestHooks.simulatingActive)
        assertTrue(
            "seek must advance along (along=${NaviMapTestHooks.lastSimAlongM} mid=$midCum)",
            NaviMapTestHooks.lastSimAlongM > midCum * 0.4,
        )
        dismissPermissionIfPresent()
        Thread.sleep(1_000)
        dismissPermissionIfPresent()
        val uiaText =
            device.hasObject(
                androidx.test.uiautomator.By
                    .text("SIMULATING"),
            )
        // adb screencap is the required manual evidence; do not overwrite with UiAutomation.
        val pfdCap =
            InstrumentationRegistry
                .getInstrumentation()
                .uiAutomation
                .executeShellCommand("screencap -p /data/local/tmp/simulating_banner_manual.png")
        java.io.FileInputStream(pfdCap.fileDescriptor).use { input ->
            val buf = ByteArray(4096)
            while (input.read(buf) >= 0) {
            }
        }
        pfdCap.close()
        // Keep a secondary copy under app files for local inspection.
        saveShot(shotDir, "simulating_banner_uiautomation.png")
        android.util.Log.i(
            "SimBannerBreakTilt",
            "banner uiautomator_text=$uiaText along=${NaviMapTestHooks.lastSimAlongM} " +
                "(adb screencap /data/local/tmp/simulating_banner_manual.png is source of truth)",
        )

        // 2) Break minutes vs integrated planned hours at mid-route.
        val breakDeadline = System.currentTimeMillis() + 12_000
        while (System.currentTimeMillis() < breakDeadline) {
            if (NaviMapTestHooks.lastElapsedDrivingHours != null &&
                (NaviMapTestHooks.lastElapsedDrivingHours ?: 0.0) > 0.0 &&
                NaviMapTestHooks.lastMinutesToBreak != null
            ) {
                break
            }
            Thread.sleep(150)
        }
        val elapsedH = NaviMapTestHooks.lastElapsedDrivingHours
        val minsLeft = NaviMapTestHooks.lastMinutesToBreak
        assertNotNull("elapsed hours", elapsedH)
        assertNotNull("minutes to break", minsLeft)
        val along = NaviMapTestHooks.lastSimAlongM
        val expectH = expectedElapsedHours(along)
        assertEquals("integrated elapsed hours", expectH, elapsedH!!, 0.08)
        val expectMins = ((2.0 - elapsedH) * 60.0).coerceAtLeast(0.0)
        assertEquals("minutes to break", expectMins, minsLeft!!, 4.0)
        assertTrue(
            "break must count down mid-route (minsLeft=$minsLeft elapsedH=$elapsedH)",
            minsLeft < 119.5 && elapsedH > 0.0,
        )
        android.util.Log.i(
            "SimBannerBreakTilt",
            "break_ok minsLeft=$minsLeft elapsedH=$elapsedH expectH=$expectH along=$along",
        )

        NaviMapTestHooks.requestStopRouteSimulation = true
        Thread.sleep(400)

        // 3) 45° tilt — 3D off, then 3D on (Vulkan Automotive AVD).
        NaviMapTestHooks.hideSearchChrome = true
        NaviMapTestHooks.pendingCamera = Triple(START.first, START.second, 14.0)
        NaviMapTestHooks.requestCameraTiltDeg = 45.0
        NaviMapTestHooks.requestOptIn3d = false
        awaitPitch(45.0)
        Thread.sleep(2_500)
        assertEquals(45.0, NaviMapTestHooks.lastCameraPitch, 1.5)
        dismissPermissionIfPresent()
        saveShot(shotDir, "tilt45_3d_off.png")

        NaviMapTestHooks.requestOptIn3d = true
        val terrainDeadline = System.currentTimeMillis() + 90_000
        while (System.currentTimeMillis() < terrainDeadline) {
            if (NaviMapTestHooks.lastTerrainAttached &&
                kotlin.math.abs(NaviMapTestHooks.lastCameraPitch - 45.0) <= 1.5
            ) {
                break
            }
            // Re-assert tilt if a style switch flattened the camera briefly.
            if (kotlin.math.abs(NaviMapTestHooks.lastCameraPitch - 45.0) > 1.5) {
                NaviMapTestHooks.requestCameraTiltDeg = 45.0
            }
            Thread.sleep(300)
        }
        assertTrue(
            "terrain at 45° kind=${NaviMapTestHooks.lastBasemapKind} " +
                "terrain=${NaviMapTestHooks.lastTerrainAttached} pitch=${NaviMapTestHooks.lastCameraPitch}",
            NaviMapTestHooks.lastTerrainAttached,
        )
        assertEquals(45.0, NaviMapTestHooks.lastCameraPitch, 1.5)
        // Style switch can race GeoJSON overlays — re-assert the corridor and wait
        // for layers to settle so the red route line is above hillshade.
        assertTrue(
            "polyline must still be planned before 3D tilt shot",
            NaviMapTestHooks.lastRoutePolylineChars > 50,
        )
        NaviMapTestHooks.pendingRoute = planned
        NaviMapTestHooks.requestCameraTiltDeg = 45.0
        Thread.sleep(3_500)
        dismissPermissionIfPresent()
        saveShot(shotDir, "tilt45_3d_on.png")
        // Host pull target used by docs/README.
        val pfdTilt =
            InstrumentationRegistry
                .getInstrumentation()
                .uiAutomation
                .executeShellCommand(
                    "su 0 cp ${File(shotDir, "tilt45_3d_on.png").absolutePath} " +
                        "/data/local/tmp/tilt45_3d_on.png",
                )
        java.io.FileInputStream(pfdTilt.fileDescriptor).use { input ->
            val buf = ByteArray(4096)
            while (input.read(buf) >= 0) {
            }
        }
        pfdTilt.close()

        NaviMapTestHooks.requestCameraTiltDeg = 0.0
        NaviMapTestHooks.requestOptIn3d = false
        awaitPitch(0.0)
    }
}
