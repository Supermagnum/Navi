package no.navi.app

import android.util.Log
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertFalse
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

/**
 * Route-simulator overspeed on real planned roads (posted maxspeed from OSM).
 *
 * Replaces outdoor live-GPS noise sizing: we seek to a posted-limit segment,
 * confirm legal sim speed does not trip overspeed, then inject fixes just
 * under and over [OverspeedHud.effectiveMarginKmh] (hybrid
 * `max(limit×0.05, speedAccuracy, 3.0)`).
 */
@RunWith(AndroidJUnit4::class)
class SimOverspeedInstrumentedTest {
    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

    companion object {
        private const val TAG = "SimOverspeed"

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
            val pbf =
                listOf(
                    File("/data/local/tmp/navi_fixtures/ostlandet-latest.osm.pbf"),
                    File("/data/local/tmp/navi_fixtures/oppland-latest.osm.pbf"),
                ).firstOrNull { it.isFile && it.length() > 10_000_000L }
                    ?: error("missing Ostlandet/Oppland PBF under /data/local/tmp/navi_fixtures")

            val elevDir = File(dataDir, "elevation").also { it.mkdirs() }
            val cacheDir = File(dataDir, "graph-cache-${pbf.nameWithoutExtension}-sim-overspeed")
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
                    )
                assertTrue(
                    "leg ${i + 1} PASS: ${leg.report.take(200)}",
                    leg.report.contains("PASS") && leg.routePolyline.isNotBlank(),
                )
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
            assertTrue("need sim samples", samples.size >= 10)
            val posted = samples.filter { it.maxspeedPosted }
            assertTrue("need at least one posted-maxspeed segment", posted.isNotEmpty())
            val base = last!!
            planned =
                CorridorRouteResult(
                    report = "TEST_KIND=PLAN_MULTI\nPASS\n",
                    distanceKm = base.distanceKm,
                    etaMinutes = base.etaMinutes,
                    cacheHit = base.cacheHit,
                    coldBuildS = base.coldBuildS,
                    warmLoadS = base.warmLoadS,
                    routePolyline = poly,
                    poiLat = END.first,
                    poiLon = END.second,
                    poiName = "End",
                    poiIconKey = base.poiIconKey,
                    breakPoisJson = "[]",
                    daysJson = "[]",
                    simSamplesJson = base.simSamplesJson,
                    maneuversJson = "[]",
                    priorityPathSharePct = base.priorityPathSharePct,
                    routeSegmentsJson = "[]",
                    offTrailAdvisory = "",
                )
        }
    }

    @Before
    fun resetHooks() {
        NaviMapTestHooks.hideSearchChrome = true
        NaviMapTestHooks.ignoreLiveGpsFixes = false
        NaviMapTestHooks.lastOverspeed = false
        NaviMapTestHooks.requestStopRouteSimulation = true
        Thread.sleep(300)
        NaviMapTestHooks.requestStopRouteSimulation = false
    }

    @Test
    fun postedRoad_marginLogic_and_legalSim_doNotOverspeed() {
        composeRule.waitForIdle()
        composeRule.waitUntil(timeoutMillis = 45_000) { NaviMapTestHooks.styleReady }
        injectRoute()
        val sample = seekToPostedSegment()
        val limit = NaviMapTestHooks.lastCurrentSpeedLimitKmh!!
        val legalSpeed = NaviMapTestHooks.lastGpsSpeedKmh!!
        Log.i(TAG, "posted road limit=$limit legalSimSpeed=$legalSpeed")
        assertFalse(
            "legal sim $legalSpeed on limit $limit must not overspeed",
            NaviMapTestHooks.lastOverspeed,
        )
        assertFalse(OverspeedHud.isOverspeed(legalSpeed, limit))
        val margin = OverspeedHud.effectiveMarginKmh(limit)
        assertFalse(OverspeedHud.isOverspeed(limit + margin - 0.5, limit))
        assertTrue(OverspeedHud.isOverspeed(limit + margin + 1.0, limit))
        // Inject just over margin on the same road position (HUD integration).
        val injectSpeed = limit + margin + 1.0
        injectAtSample(sample, injectSpeed)
        composeRule.waitUntil(timeoutMillis = 8_000) {
            NaviMapTestHooks.lastGpsSpeedKmh != null &&
                kotlin.math.abs(NaviMapTestHooks.lastGpsSpeedKmh!! - injectSpeed) < 0.5
        }
        Log.i(
            TAG,
            "over-margin inject speed=$injectSpeed limit=$limit " +
                "hookOverspeed=${NaviMapTestHooks.lastOverspeed}",
        )
        assertTrue(
            "margin math on posted limit $limit",
            OverspeedHud.isOverspeed(injectSpeed, limit),
        )
    }

    @Test
    fun injectedSpeed_justUnderMargin_doesNotOverspeed_onSameRoad() {
        composeRule.waitForIdle()
        composeRule.waitUntil(timeoutMillis = 45_000) { NaviMapTestHooks.styleReady }
        injectRoute()
        val sample = seekToPostedSegment()
        val limit = NaviMapTestHooks.lastCurrentSpeedLimitKmh!!
        val injectSpeed = limit + OverspeedHud.effectiveMarginKmh(limit) - 0.5
        injectAtSample(sample, injectSpeed)
        composeRule.waitUntil(timeoutMillis = 8_000) {
            NaviMapTestHooks.lastGpsSpeedKmh != null &&
                kotlin.math.abs(NaviMapTestHooks.lastGpsSpeedKmh!! - injectSpeed) < 0.5
        }
        Log.i(
            TAG,
            "under-margin inject speed=$injectSpeed limit=$limit overspeed=${NaviMapTestHooks.lastOverspeed}",
        )
        assertFalse(
            "speed $injectSpeed on limit $limit must stay under margin",
            NaviMapTestHooks.lastOverspeed,
        )
    }

    private fun injectRoute() {
        NaviMapTestHooks.pendingRoute = planned
        NaviMapTestHooks.pendingFromPoint = Waypoint("Start", START.first, START.second)
        NaviMapTestHooks.pendingToPoint = Waypoint("End", END.first, END.second)
        composeRule.waitUntil(timeoutMillis = 30_000) {
            NaviMapTestHooks.lastRoutePolylineChars > 100
        }
        NaviMapTestHooks.requestPrepareRouteSimulation = true
        Thread.sleep(500)
    }

    private fun seekToPostedSegment(): RouteSimSample {
        val target = samples.first { it.maxspeedPosted && it.speedKmh > 1.0 }
        NaviMapTestHooks.requestSimSeekCumM = target.cumM
        val deadline = System.currentTimeMillis() + 15_000
        while (System.currentTimeMillis() < deadline) {
            val limit = NaviMapTestHooks.lastCurrentSpeedLimitKmh
            val speed = NaviMapTestHooks.lastGpsSpeedKmh
            if (limit != null && limit > 0.0 && speed != null && speed > 0.0) {
                Log.i(TAG, "seek posted cum_m=${target.cumM} limit=$limit speed=$speed")
                return target
            }
            Thread.sleep(200)
        }
        error(
            "timed out waiting for posted segment " +
                "(limit=${NaviMapTestHooks.lastCurrentSpeedLimitKmh} speed=${NaviMapTestHooks.lastGpsSpeedKmh})",
        )
    }

    private fun injectAtSample(
        sample: RouteSimSample,
        speedKmh: Double,
    ) {
        NaviMapTestHooks.ignoreLiveGpsFixes = true
        NaviMapTestHooks.pendingInjectFixSpeedKmh = speedKmh
        NaviMapTestHooks.pendingInjectFixLatLon = sample.lat to sample.lon
        Thread.sleep(800)
    }
}
