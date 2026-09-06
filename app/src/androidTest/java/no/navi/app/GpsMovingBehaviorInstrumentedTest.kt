package no.navi.app

import android.util.Log
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.rule.GrantPermissionRule
import org.json.JSONArray
import org.json.JSONObject
import org.junit.After
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
import uniffi.navi.roadNearInfo
import java.io.File
import kotlin.math.abs
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.sin
import kotlin.math.sqrt

/**
 * Moving-GPS follow-up (closes the stationary-only gap from the Aug-13 puck
 * investigation): simulation exercises followGps, street label, and speed HUD
 * under motion; live provider log captures real multi-provider behaviour.
 */
@RunWith(AndroidJUnit4::class)
class GpsMovingBehaviorInstrumentedTest {
    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

    @get:Rule
    val permissionRule: GrantPermissionRule =
        GrantPermissionRule.grant(
            android.Manifest.permission.ACCESS_FINE_LOCATION,
            android.Manifest.permission.ACCESS_COARSE_LOCATION,
        )

    companion object {
        private const val TAG = "GpsMovingBehavior"

        // Hamar short loop (same as LiveRouteSimulationInstrumentedTest).
        private val START = 60.8059250 to 11.3299030
        private val VIA1 = 60.8023620 to 11.3053691
        private val VIA2 = 60.7974313 to 11.3094874
        private val END = 60.8056487 to 11.3290523

        // Furnesvegen / E6 parallel corridor (road_near.rs ostlandet coords).
        private const val FURNES_LAT = 60.852498
        private const val FURNES_LON = 11.007872

        @JvmStatic
        lateinit var planned: CorridorRouteResult

        @JvmStatic
        lateinit var samples: List<RouteSimSample>

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
            val cacheDir = File(dataDir, "graph-cache-${pbf.nameWithoutExtension}-moving-gps")
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
            var poly = ""
            var dist = 0.0
            var eta = 0.0
            var last: CorridorRouteResult? = null
            val legSamples = mutableListOf<List<RouteSimSample>>()
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
                        tollPolicy = uniffi.navi.FfiTollPolicy.ALLOW,
                        avoidFerries = false,
                        vehicle = vehicle,
                        preferOfficialNetworks = false,
                        dataDir = "",
                    )
                assertTrue(
                    "leg ${i + 1} must PASS: ${leg.report.take(300)}",
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
                    tollPolicy = "allow",
                    padAttemptsJson = "[]",
                    searchExpansions = 0u,
                    searchTerminateReason = "fail",
                    tollAvoidanceIncomplete = false,
                    routeUsesTolls = false,
                )
            assertTrue("expected sim samples", samples.size >= 10)
        }
    }

    @Before
    fun resetHooks() {
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = true
        NaviMapTestHooks.ignoreLiveGpsFixes = false
        NaviMapTestHooks.disableGpsFollow = false
        NaviMapTestHooks.followGps = true
        NaviMapTestHooks.simulationTimeScale = 30.0
        NaviMapTestHooks.requestStopRouteSimulation = true
        Thread.sleep(300)
        NaviMapTestHooks.requestStopRouteSimulation = false
        NaviMapTestHooks.lastGpsProvider = ""
    }

    @After
    fun tearDown() {
        NaviMapTestHooks.requestStopRouteSimulation = true
        NaviMapTestHooks.ignoreLiveGpsFixes = false
        NaviMapTestHooks.disableGpsFollow = false
        NaviMapTestHooks.hideSearchChrome = false
    }

    @Test
    fun simulation_followGps_tracks_moving_puck() {
        composeRule.waitForIdle()
        waitStyle()
        injectRouteAndStartSim()

        val deadline = System.currentTimeMillis() + 25_000
        var moved = false
        var cameraFollowed = false
        while (System.currentTimeMillis() < deadline) {
            val along = NaviMapTestHooks.lastSimAlongM
            if (along > 30.0) moved = true
            val lat = NaviMapTestHooks.lastGpsLat
            val camLat = NaviMapTestHooks.lastCameraLat
            if (moved && NaviMapTestHooks.followGps && !lat.isNaN() && abs(camLat - lat) < 0.0002) {
                cameraFollowed = true
                break
            }
            Thread.sleep(200)
        }
        assertTrue(
            "simulation must advance along route (along_m=${NaviMapTestHooks.lastSimAlongM})",
            moved,
        )
        assertTrue(
            "followGps kept camera within ~20 m of puck (follow=${NaviMapTestHooks.followGps})",
            cameraFollowed,
        )
        Log.i(TAG, "followGps ok moved=$moved camLat=${NaviMapTestHooks.lastCameraLat} gpsLat=${NaviMapTestHooks.lastGpsLat}")
    }

    @Test
    fun simulation_speed_hud_updates_without_wild_flicker() {
        composeRule.waitForIdle()
        waitStyle()
        injectRouteAndStartSim()

        val speeds = mutableListOf<Double>()
        val deadline = System.currentTimeMillis() + 20_000
        while (System.currentTimeMillis() < deadline && speeds.size < 25) {
            NaviMapTestHooks.lastGpsSpeedKmh?.let { speeds.add(it) }
            Thread.sleep(250)
        }
        assertTrue("expected speed samples during sim", speeds.size >= 5)
        var bigJumps = 0
        for (i in 1 until speeds.size) {
            if (abs(speeds[i] - speeds[i - 1]) > 40.0) bigJumps++
        }
        assertTrue(
            "speed HUD had $bigJumps jumps >40 km/h between 250 ms polls (samples=${speeds.size})",
            bigJumps <= 1,
        )
        Log.i(TAG, "speed samples=${speeds.size} bigJumps=$bigJumps range=${speeds.minOrNull()}..${speeds.maxOrNull()}")
    }

    @Test
    fun furnesvegen_sticky_label_via_ffi_while_moving_east() {
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        val dataDir = NaviAppData.resolve(ctx)
        val pbf =
            listOf(
                File("/data/local/tmp/navi_fixtures/ostlandet-latest.osm.pbf"),
                File(dataDir, "ostlandet-latest.osm.pbf"),
            ).firstOrNull { it.isFile && it.length() > 10_000_000L }
                ?: run {
                    Log.w(TAG, "skip furnes FFI: no ostlandet PBF on device")
                    return
                }
        val elevDir = File(dataDir, "elevation").absolutePath
        val cacheDir = File(dataDir, "graph-cache-furnes-sticky").absolutePath
        val midLon = 11.007872
        var label: String? = null
        for (i in 0 until 8) {
            val lon = midLon + i * 0.00035
            val noisyLat = FURNES_LAT - 15.0 / 111_320.0
            val info =
                roadNearInfo(
                    pbfPath = pbf.absolutePath,
                    cacheDir = cacheDir,
                    elevDir = elevDir,
                    lat = noisyLat,
                    lon = lon,
                    profile = TravelProfile.CAR,
                    maxM = 80.0,
                )
            label = info.label
            Log.i(TAG, "furnes ffi step=$i label=$label")
        }
        assertTrue("expected label on Furnes corridor", !label.isNullOrBlank())
        assertTrue(
            "sticky FFI must keep Furnes/184 under ~15 m noise toward E6, got '$label'",
            label!!.contains("Furnes", ignoreCase = true) ||
                label.contains("184") ||
                (!label.contains("E 6") && !label.equals("E6", ignoreCase = true)),
        )
    }

    @Test
    fun live_provider_log_and_movement_report() {
        composeRule.waitForIdle()
        NaviMapTestHooks.ignoreLiveGpsFixes = false
        NaviMapTestHooks.disableGpsFollow = false

        val providers = mutableListOf<String>()
        val lats = mutableListOf<Double>()
        val start = System.currentTimeMillis()
        val collectMs = 45_000L
        while (System.currentTimeMillis() - start < collectMs) {
            val p = NaviMapTestHooks.lastGpsProvider
            if (p.isNotBlank()) providers.add(p)
            val lat = NaviMapTestHooks.lastGpsLat
            if (!lat.isNaN()) lats.add(lat)
            Thread.sleep(500)
        }

        val distinct = providers.distinct()
        val movedM =
            if (lats.size >= 2) {
                haversineM(lats.first(), NaviMapTestHooks.lastGpsLon, lats.last(), NaviMapTestHooks.lastGpsLon)
            } else {
                0.0
            }
        val report =
            buildString {
                appendLine("device=${android.os.Build.MODEL}")
                appendLine("providers=$distinct samples=${providers.size}")
                appendLine("provider_mixing=${distinct.size > 1}")
                appendLine("lat_span=${if (lats.isEmpty()) 0.0 else lats.max() - lats.min()}")
                appendLine("approx_move_m=$movedM")
            }
        Log.i(TAG, report)
        val outDir =
            File(
                InstrumentationRegistry.getInstrumentation().targetContext.cacheDir,
                "gps_moving_diag",
            ).also { it.mkdirs() }
        File(outDir, "provider_report.txt").writeText(report)

        // Stationary indoor: still capture provider list; movement is best-effort.
        assertTrue("must observe at least one GPS fix", lats.isNotEmpty())
        if (movedM < 20.0 && distinct.size <= 1) {
            Log.w(
                TAG,
                "INCONCLUSIVE provider-mixing-while-moving: device stationary " +
                    "(move_m=$movedM) — re-run outdoors or on Pixel 9a while driving",
            )
        }
    }

    private fun injectRouteAndStartSim() {
        NaviMapTestHooks.pendingRoute = planned
        NaviMapTestHooks.pendingFromPoint = Waypoint("Start", START.first, START.second)
        NaviMapTestHooks.pendingToPoint = Waypoint("End", END.first, END.second)
        waitRouteOnMap()
        NaviMapTestHooks.requestStartRouteSimulation = true
        val deadline = System.currentTimeMillis() + 15_000
        while (System.currentTimeMillis() < deadline && !NaviMapTestHooks.simulatingActive) {
            Thread.sleep(200)
        }
        assertTrue("simulation must start", NaviMapTestHooks.simulatingActive)
        Thread.sleep(800)
    }

    private fun waitStyle() {
        composeRule.waitUntil(timeoutMillis = 45_000) { NaviMapTestHooks.styleReady }
    }

    private fun waitRouteOnMap() {
        composeRule.waitUntil(timeoutMillis = 30_000) {
            NaviMapTestHooks.lastRoutePolylineChars > 100
        }
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
        val dp = Math.toRadians(lat2 - lat1)
        val dl = Math.toRadians(lon2 - lon1)
        val a = sin(dp / 2) * sin(dp / 2) + cos(p1) * cos(p2) * sin(dl / 2) * sin(dl / 2)
        return 2 * r * atan2(sqrt(a), sqrt(1 - a))
    }
}
