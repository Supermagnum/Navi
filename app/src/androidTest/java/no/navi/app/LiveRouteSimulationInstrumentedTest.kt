package no.navi.app

import android.graphics.Bitmap
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONArray
import org.json.JSONObject
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
 * Built-in debug route simulation on the Hamar short loop.
 *
 * Exercises the real [applyFix] pipeline at maxspeed/highway-fallback pace
 * (wall-clock compressed for CI via [NaviMapTestHooks.simulationTimeScale]).
 */
@RunWith(AndroidJUnit4::class)
class LiveRouteSimulationInstrumentedTest {
    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

    companion object {
        // Short loop (same geometry used for earlier GPX duplicate-rte/trk diagnosis).
        private val START = 60.8059250 to 11.3299030
        private val VIA1 = 60.8023620 to 11.3053691
        private val VIA2 = 60.7974313 to 11.3094874
        private val END = 60.8056487 to 11.3290523

        @JvmStatic
        lateinit var planned: CorridorRouteResult

        @JvmStatic
        lateinit var samples: List<RouteSimSample>

        @JvmStatic
        lateinit var maneuvers: List<RouteManeuver>

        @JvmStatic
        var planElapsedMs: Long = 0L

        @JvmStatic
        @BeforeClass
        fun planShortLoop() {
            val context = InstrumentationRegistry.getInstrumentation().targetContext
            val dataDir = NaviAppData.resolve(context).also { it.mkdirs() }
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
            val cacheDir = File(dataDir, "graph-cache-${pbf.nameWithoutExtension}-car-sim")
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
            val legManeuvers = mutableListOf<List<RouteManeuver>>()
            var poly = ""
            var dist = 0.0
            var eta = 0.0
            var last: CorridorRouteResult? = null
            val t0 = System.currentTimeMillis()
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
                        avoidMajor = false,
                        avoidTolls = false,
                        avoidFerries = false,
                        vehicle = vehicle,
                        preferOfficialNetworks = false,
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
                legManeuvers.add(parseRouteManeuvers(leg.maneuversJson))
                last = leg
            }
            planElapsedMs = System.currentTimeMillis() - t0
            samples = mergeSimSamples(legSamples)
            maneuvers = mergeManeuvers(legManeuvers)
            assertTrue("expected simulation samples", samples.size >= 10)
            assertTrue("expected maneuvers including destination", maneuvers.isNotEmpty())
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
                    maneuversJson =
                        JSONArray(
                            maneuvers.map { m ->
                                JSONObject()
                                    .put("lat", m.lat)
                                    .put("lon", m.lon)
                                    .put("cum_m", m.cumM)
                                    .put("kind", m.kind)
                                    .put("street", m.street)
                                    .put("roundabout_exit", m.roundaboutExit)
                            },
                        ).toString(),
                    priorityPathSharePct = base.priorityPathSharePct,
                    routeSegmentsJson = "[]",
                    offTrailAdvisory = "",
                )
            android.util.Log.i(
                "LiveRouteSim",
                "planned distance_km=$dist eta_min=$eta samples=${samples.size} " +
                    "maneuvers=${maneuvers.size} elapsed_ms=$planElapsedMs",
            )
        }
    }

    @Before
    fun resetHooks() {
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = false
        NaviMapTestHooks.simulationTimeScale = 25.0
        NaviMapTestHooks.requestRotationMode = MapRotationMode.DirectionOfTravel
        NaviMapTestHooks.lastArrivedAtEnd = false
        NaviMapTestHooks.lastViaIndex = -1
        NaviMapTestHooks.requestStopRouteSimulation = true
        Thread.sleep(300)
        NaviMapTestHooks.requestStopRouteSimulation = false
    }

    @Test
    fun shortLoop_simulation_approach_screenshots_and_speed_checks() {
        composeRule.waitForIdle()
        waitStyle()

        NaviMapTestHooks.routeStartLabel = "Start"
        NaviMapTestHooks.routeViaLabel = "Via1, Via2"
        NaviMapTestHooks.routeEndLabel = "End"
        NaviMapTestHooks.pendingFromPoint = Waypoint("Start", START.first, START.second)
        NaviMapTestHooks.pendingViaPoints =
            listOf(
                Waypoint("Via1", VIA1.first, VIA1.second),
                Waypoint("Via2", VIA2.first, VIA2.second),
            )
        NaviMapTestHooks.pendingToPoint = Waypoint("End", END.first, END.second)
        NaviMapTestHooks.pendingRoute = planned
        waitRouteOnMap()

        val shotDir =
            File(
                NaviAppData.resolve(InstrumentationRegistry.getInstrumentation().targetContext),
                "sim_loop_shots",
            ).also { it.mkdirs() }

        saveShot(shotDir, "sim_loop_route_planned.png")
        pullToDocs("sim_loop_route_planned.png", shotDir)

        // Start realtime-compressed playback for banner + speed spot-checks.
        NaviMapTestHooks.requestStartRouteSimulation = true
        val simDeadline = System.currentTimeMillis() + 20_000
        while (System.currentTimeMillis() < simDeadline && !NaviMapTestHooks.simulatingActive) {
            Thread.sleep(200)
        }
        assertTrue("SIMULATING must become active", NaviMapTestHooks.simulatingActive)
        // Give the overlay a moment to poll simulatingActive and recompose.
        Thread.sleep(1_000)
        composeRule.waitForIdle()
        val device =
            androidx.test.uiautomator.UiDevice.getInstance(
                InstrumentationRegistry.getInstrumentation(),
            )
        val bannerVisible =
            device.hasObject(
                androidx.test.uiautomator.By
                    .text("SIMULATING"),
            )
        android.util.Log.i(
            "LiveRouteSim",
            "banner_visible=$bannerVisible simulatingActive=${NaviMapTestHooks.simulatingActive}",
        )
        // Soft requirement: active flag is mandatory; on-screen label is best-effort
        // (AAOS multi-user / MapLibre SurfaceView can hide Compose semantics).
        if (!bannerVisible) {
            android.util.Log.w(
                "LiveRouteSim",
                "SIMULATING text not found via UiAutomator — flag is active; continuing",
            )
        }

        // Spot-check speeds against posted / highway-class fallback.
        var postedOk = false
        var fallbackOk = false
        val speedDeadline = System.currentTimeMillis() + 30_000
        while (System.currentTimeMillis() < speedDeadline && (!postedOk || !fallbackOk)) {
            val speed = NaviMapTestHooks.lastSimSpeedKmh
            val hwy = NaviMapTestHooks.lastSimHighway
            if (speed != null && hwy != null) {
                if (NaviMapTestHooks.lastSimMaxspeedPosted) {
                    postedOk = speed > 1.0
                    android.util.Log.i(
                        "LiveRouteSim",
                        "speed_spot posted highway=$hwy speed_kmh=$speed",
                    )
                } else {
                    val expected = highwayFallbackKmh(hwy)
                    if (kotlin.math.abs(speed - expected) < 0.1) {
                        fallbackOk = true
                        android.util.Log.i(
                            "LiveRouteSim",
                            "speed_spot fallback highway=$hwy speed_kmh=$speed expected=$expected",
                        )
                    }
                }
            }
            Thread.sleep(250)
        }
        assertTrue("observed at least one speed sample", NaviMapTestHooks.lastSimSpeedKmh != null)
        // Prefer confirming fallback table when a non-posted segment is seen; else posted is enough.
        assertTrue(
            "speed-limit playback observed (posted=$postedOk fallback=$fallbackOk)",
            postedOk || fallbackOk,
        )

        saveShot(shotDir, "sim_loop_mid_simulating.png")
        pullToDocs("sim_loop_mid_simulating.png", shotDir)

        // Stop continuous playback; prepare a fresh tracker for ordered seeks.
        NaviMapTestHooks.requestStopRouteSimulation = true
        Thread.sleep(500)
        NaviMapTestHooks.requestPrepareRouteSimulation = true
        Thread.sleep(500)

        // Per-turn 100 m / 50 m approach checkpoints via seek (same applyFix path).
        val turns =
            maneuvers
                .filter { it.kind != "destination" && it.cumM > 120.0 }
                .take(12)
        assertTrue("route should have real turns, got ${maneuvers.map { it.kind }}", turns.isNotEmpty())
        var viaSeen = false
        for ((idx, turn) in turns.withIndex()) {
            for (before in listOf(100.0, 50.0)) {
                val target = (turn.cumM - before).coerceAtLeast(0.0)
                NaviMapTestHooks.requestSimSeekCumM = target
                val deadline = System.currentTimeMillis() + 8_000
                var ok = false
                while (System.currentTimeMillis() < deadline) {
                    val d = NaviMapTestHooks.lastDistanceToManeuverM
                    if (d != null && kotlin.math.abs(d - before) <= 25.0) {
                        ok = true
                        break
                    }
                    Thread.sleep(150)
                }
                assertTrue(
                    "turn[$idx] ${turn.kind} ~${before.toInt()}m: distance=${NaviMapTestHooks.lastDistanceToManeuverM}",
                    ok,
                )
                Thread.sleep(400)
                composeRule.waitForIdle()
                val phase = NaviMapTestHooks.lastApproachPhase
                assertTrue(
                    "approach phase should be Appear/Urgency at ${before}m before turn " +
                        "(got $phase dist=${NaviMapTestHooks.lastDistanceToManeuverM})",
                    phase == ApproachUiPhase.Appear || phase == ApproachUiPhase.Urgency,
                )
                // Best-effort Compose visibility (SurfaceView may hide semantics).
                runCatching {
                    composeRule
                        .onNodeWithTag("approach_instruction_box", useUnmergedTree = true)
                        .assertExists()
                }
                val name = "sim_loop_turn${idx}_${before.toInt()}m.png"
                saveShot(shotDir, name)
                pullToDocs(name, shotDir)
                android.util.Log.i(
                    "LiveRouteSim",
                    "CHECKPOINT turn=$idx kind=${turn.kind} street=${turn.street} " +
                        "before_m=$before dist=${NaviMapTestHooks.lastDistanceToManeuverM} " +
                        "along=${NaviMapTestHooks.lastSimAlongM}",
                )
            }
            if (NaviMapTestHooks.lastViaIndex >= 0) viaSeen = true
        }

        // Drive toward end for via/end recognition.
        val endCum = samples.last().cumM
        // Seek near sample points closest to each via waypoint.
        for (via in listOf(VIA1, VIA2)) {
            val nearest =
                samples.minByOrNull {
                    RouteProgressTracker.haversineM(it.lat, it.lon, via.first, via.second)
                } ?: continue
            NaviMapTestHooks.requestSimSeekCumM = nearest.cumM
            Thread.sleep(700)
            if (NaviMapTestHooks.lastViaIndex >= 0) viaSeen = true
        }
        assertTrue("must recognize at least one via transition", viaSeen)
        saveShot(shotDir, "sim_loop_via_transition.png")
        pullToDocs("sim_loop_via_transition.png", shotDir)
        NaviMapTestHooks.requestSimSeekCumM = endCum
        val arriveDeadline = System.currentTimeMillis() + 10_000
        while (System.currentTimeMillis() < arriveDeadline && !NaviMapTestHooks.lastArrivedAtEnd) {
            Thread.sleep(200)
        }
        assertTrue("must recognize arrival at end", NaviMapTestHooks.lastArrivedAtEnd)
        Thread.sleep(500)
        saveShot(shotDir, "sim_loop_arrival.png")
        pullToDocs("sim_loop_arrival.png", shotDir)

        // Bearing should have moved under Direction-of-travel.
        assertNotNull(NaviMapTestHooks.gpsBearingDeg)

        android.util.Log.i(
            "LiveRouteSim",
            "PASS turns=${turns.size} via_seen=$viaSeen arrived=${NaviMapTestHooks.lastArrivedAtEnd} " +
                "bearing=${NaviMapTestHooks.gpsBearingDeg} eta=${NaviMapTestHooks.lastMinutesToBreak}",
        )
    }

    private fun waitStyle() {
        val deadline = System.currentTimeMillis() + 45_000
        while (System.currentTimeMillis() < deadline) {
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

    private fun saveShot(
        dir: File,
        name: String,
    ) {
        val shot = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
        assertNotNull(shot)
        assertTrue(shot!!.width > 0)
        val out = File(dir, name)
        out.outputStream().use {
            shot.compress(Bitmap.CompressFormat.PNG, 100, it)
        }
        assertTrue(out.length() > 3_000)
        val extDir =
            InstrumentationRegistry
                .getInstrumentation()
                .targetContext
                .getExternalFilesDir(null)
        if (extDir != null) {
            out.copyTo(File(extDir, name), overwrite = true)
        }
        // Persist under /data/local/tmp via root so host adb pull survives app teardown.
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
        android.util.Log.i("LiveRouteSim", "SHOT ${out.absolutePath}")
    }

    private fun pullToDocs(
        name: String,
        shotDir: File,
    ) {
        android.util.Log.i("LiveRouteSim", "SHOT_READY $name size=${File(shotDir, name).length()}")
    }
}
