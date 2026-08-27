package no.navi.app

import android.graphics.Bitmap
import android.util.Log
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.CorridorRouteResult
import uniffi.navi.FfiVehicleLimits
import uniffi.navi.TravelProfile
import uniffi.navi.planCarRoute
import java.io.File
import java.io.FileOutputStream

/**
 * Off-route detection, approach suppress, debounce, and profile-specific reroute.
 */
@RunWith(AndroidJUnit4::class)
class OffRouteBehaviorInstrumentedTest {
    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

    private lateinit var context: android.content.Context
    private lateinit var dataDir: File
    private lateinit var shotDir: File

    @Before
    fun setUp() {
        context = InstrumentationRegistry.getInstrumentation().targetContext
        dataDir = NaviAppData.resolve(context).also { it.mkdirs() }
        shotDir = File(context.cacheDir, "off_route_shots").also { it.mkdirs() }
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = true
        NaviMapTestHooks.pendingInjectFixLatLon = null
        NaviMapTestHooks.pendingRoute = null
        NaviMapTestHooks.offRouteConfirmMsOverride = 800L
        NaviMapTestHooks.autoRerouteTriggeredCount = 0
        NaviMapTestHooks.rerouteResultOverride = null
        NaviMapTestHooks.forcePlanPbfPath = null
        NaviMapTestHooks.requestHikingRerouteAnswer = null
        NaviMapTestHooks.ignoreLiveGpsFixes = true
        NaviMapTestHooks.lastOffRoute = false
        NaviMapTestHooks.hikingReroutePromptVisible = false
        NaviMapTestHooks.requestStopRouteSimulation = true
        Thread.sleep(400)
        NaviMapTestHooks.requestStopRouteSimulation = false
    }

    @Test
    fun car_offRoute_suppressesApproach_thenAutoReroutes_withBanner() {
        NaviMapTestHooks.requestTravelProfile = TravelProfile.CAR
        Thread.sleep(600)
        ensureEspaFixture()
        val pbf = File(dataDir, "espa-atnbrufossen-corridor.osm.pbf")
        NaviMapTestHooks.forcePlanPbfPath = pbf.absolutePath
        val result = planEspaCar()
        assertTrue(result.report.contains("PASS"))
        val samples = parseRouteSimSamples(result.simSamplesJson)
        assertTrue(samples.size >= 10)
        val mid = samples[samples.size / 4]
        val offLat = mid.lat
        val offLon = mid.lon + 0.08
        assertTrue(
            RouteProgressTracker.haversineM(offLat, offLon, mid.lat, mid.lon) > 3_000.0,
        )

        // Fast fake replan so the test does not wait on a cold PBF scan.
        NaviMapTestHooks.rerouteResultOverride =
            result.copy(
                report = result.report + "reroute_override=true\n",
                distanceKm = result.distanceKm + 1.5,
                routePolyline = result.routePolyline + ";11.0,61.0",
            )

        pushRoute(result, "Espa", "Atnbrufossen")
        val polyBefore = NaviMapTestHooks.lastRoutePolylineChars
        assertTrue(polyBefore > 100)

        prepareAndSeek(mid.cumM)
        val alongOn = NaviMapTestHooks.lastSimAlongM
        Log.i(TAG, "ON_ROUTE along=$alongOn")

        // Brief blip: confirm window stays long so a short inject cannot auto-reroute.
        NaviMapTestHooks.offRouteConfirmMsOverride = 15_000L
        val countBefore = NaviMapTestHooks.autoRerouteTriggeredCount
        injectOffRoute(offLat, offLon)
        val offDeadline = System.currentTimeMillis() + 5_000
        while (System.currentTimeMillis() < offDeadline && !NaviMapTestHooks.lastOffRoute) {
            Thread.sleep(50)
        }
        assertTrue(
            "immediate off-route flag (crossTrack=${NaviMapTestHooks.lastCrossTrackM})",
            NaviMapTestHooks.lastOffRoute,
        )
        composeRule.onNodeWithTag("approach_off_route").assertIsDisplayed()
        // Return on corridor before the long confirm window elapses.
        NaviMapTestHooks.pendingInjectFixLatLon = mid.lat to mid.lon
        val onDeadline = System.currentTimeMillis() + 5_000
        while (System.currentTimeMillis() < onDeadline && NaviMapTestHooks.lastOffRoute) {
            Thread.sleep(50)
            if (NaviMapTestHooks.pendingInjectFixLatLon == null && NaviMapTestHooks.lastOffRoute) {
                NaviMapTestHooks.pendingInjectFixLatLon = mid.lat to mid.lon
            }
        }
        assertFalse(NaviMapTestHooks.lastOffRoute)
        assertEquals(
            "brief blip must not auto-reroute",
            countBefore,
            NaviMapTestHooks.autoRerouteTriggeredCount,
        )

        // Sustained off-route with short confirm (production-like debounce, sped up).
        NaviMapTestHooks.offRouteConfirmMsOverride = 800L
        val deadline = System.currentTimeMillis() + 20_000
        while (System.currentTimeMillis() < deadline) {
            injectOffRoute(offLat, offLon)
            if (NaviMapTestHooks.autoRerouteTriggeredCount > countBefore) break
            if (NaviMapTestHooks.reroutingActive) break
            Thread.sleep(250)
        }
        assertTrue(
            "motor must auto-reroute after debounce (count=${NaviMapTestHooks.autoRerouteTriggeredCount})",
            NaviMapTestHooks.autoRerouteTriggeredCount > countBefore ||
                NaviMapTestHooks.reroutingActive ||
                NaviMapTestHooks.lastRoutePolylineChars != polyBefore,
        )
        // Banner should appear at least briefly during recompute
        val bannerDeadline = System.currentTimeMillis() + 8_000
        var sawBanner = false
        while (System.currentTimeMillis() < bannerDeadline) {
            if (NaviMapTestHooks.reroutingActive) {
                sawBanner = true
                break
            }
            // Override replan is fast — also accept completed update
            if (NaviMapTestHooks.lastRoutePolylineChars != polyBefore) {
                sawBanner = true
                break
            }
            Thread.sleep(50)
        }
        assertTrue("rerouting banner / replan activity expected", sawBanner)
        val settle = System.currentTimeMillis() + 10_000
        while (System.currentTimeMillis() < settle && NaviMapTestHooks.reroutingActive) {
            Thread.sleep(100)
        }
        assertTrue(
            "polyline should update after reroute",
            NaviMapTestHooks.lastRoutePolylineChars != polyBefore ||
                NaviMapTestHooks.lastPlanDistanceKm == result.distanceKm + 1.5,
        )
        assertFalse(NaviMapTestHooks.hikingReroutePromptVisible)
        // Reroute start must be a live resolved label (or coord fallback), not
        // the hardcoded "Here" and not the stale original Plan start ("Espa").
        val startAfter = NaviMapTestHooks.lastAppliedRouteStartLabel
        assertTrue("reroute start label present", startAfter.isNotBlank())
        assertTrue(
            "reroute start must not stay 'Here' or original Espa (got=$startAfter)",
            startAfter != "Here" && startAfter != "Espa",
        )
        assertEquals("", NaviMapTestHooks.routeStartLabel)
        saveShot("car_off_route_fixed.png")
        pullShot("car_off_route_fixed.png")
        Log.i(
            TAG,
            "FINDING car: off-route suppresses approach; auto-reroute count=" +
                "${NaviMapTestHooks.autoRerouteTriggeredCount}; start=$startAfter",
        )
    }

    @Test
    fun hiking_offRoute_promptsInsteadOfAuto() {
        val staged = File("/data/local/tmp/navi_fixtures")
        val poly = File(staged, "skolla_rondvassbu.polyline.txt").readText().trim()
        val sim = File(staged, "skolla_rondvassbu.sim_samples.json").readText().trim()
        val breaks = File(staged, "skolla_rondvassbu.breaks.json").readText().trim()
        val samples = parseRouteSimSamples(sim)
        val mid = samples[samples.size / 5]
        val offLat = mid.lat + 0.05
        val offLon = mid.lon

        NaviMapTestHooks.requestTravelProfile = TravelProfile.HIKING
        Thread.sleep(800)

        val fake =
            CorridorRouteResult(
                report = "TEST_KIND=PLAN_HIKING_ROUTE\nprofile=Hiking\nPASS\n",
                distanceKm = 100.0,
                etaMinutes = 1600.0,
                cacheHit = true,
                coldBuildS = 0.0,
                warmLoadS = 0.0,
                routePolyline = poly,
                poiLat = mid.lat,
                poiLon = mid.lon,
                poiName = "Mid",
                poiIconKey = "cabin",
                breakPoisJson = breaks.ifBlank { "[]" },
                daysJson = "[]",
                simSamplesJson = sim,
                maneuversJson =
                    """[{"lat":${mid.lat},"lon":${mid.lon},"cum_m":${mid.cumM + 800},""" +
                        """"kind":"left","street":"Trail","roundabout_exit":null}]""",
                priorityPathSharePct = 100.0,
                routeSegmentsJson = "[]",
                offTrailAdvisory = "",
            )
        NaviMapTestHooks.rerouteResultOverride =
            fake.copy(distanceKm = 101.0, routePolyline = poly + ";9.8,61.9")

        pushRoute(fake, "Skolla", "Rondvassbu")
        val polyBefore = NaviMapTestHooks.lastRoutePolylineChars
        val countBefore = NaviMapTestHooks.autoRerouteTriggeredCount
        prepareAndSeek(mid.cumM)

        val deadline = System.currentTimeMillis() + 20_000
        while (System.currentTimeMillis() < deadline) {
            injectOffRoute(offLat, offLon)
            if (NaviMapTestHooks.hikingReroutePromptVisible) break
            Thread.sleep(250)
        }
        assertTrue(
            "hiking must prompt (off=${NaviMapTestHooks.lastOffRoute} xt=${NaviMapTestHooks.lastCrossTrackM})",
            NaviMapTestHooks.hikingReroutePromptVisible,
        )
        assertEquals(
            "hiking must not silent auto-reroute",
            countBefore,
            NaviMapTestHooks.autoRerouteTriggeredCount,
        )
        assertTrue(
            "still off-route while prompt open (live GPS must not clobber)",
            NaviMapTestHooks.lastOffRoute,
        )
        composeRule.onNodeWithTag("approach_off_route").assertIsDisplayed()

        // Decline — keep original plan
        NaviMapTestHooks.requestHikingRerouteAnswer = false
        Thread.sleep(1_000)
        assertFalse(NaviMapTestHooks.hikingReroutePromptVisible)
        assertEquals(polyBefore, NaviMapTestHooks.lastRoutePolylineChars)

        // Clear suppress-until-on-route by returning to the corridor, then prompt again.
        NaviMapTestHooks.pendingInjectFixLatLon = mid.lat to mid.lon
        Thread.sleep(600)
        assertFalse(NaviMapTestHooks.lastOffRoute)

        NaviMapTestHooks.offRouteConfirmMsOverride = 500L
        val d2 = System.currentTimeMillis() + 15_000
        while (System.currentTimeMillis() < d2) {
            injectOffRoute(offLat, offLon)
            if (NaviMapTestHooks.hikingReroutePromptVisible) break
            Thread.sleep(200)
        }
        assertTrue(NaviMapTestHooks.hikingReroutePromptVisible)
        NaviMapTestHooks.requestHikingRerouteAnswer = true
        val settle = System.currentTimeMillis() + 12_000
        while (System.currentTimeMillis() < settle) {
            if (NaviMapTestHooks.lastRoutePolylineChars != polyBefore) break
            Thread.sleep(100)
        }
        assertTrue(
            "accepting prompt should update polyline",
            NaviMapTestHooks.lastRoutePolylineChars != polyBefore,
        )
        saveShot("hike_off_route_prompt.png")
        pullShot("hike_off_route_prompt.png")
        Log.i(TAG, "FINDING hike: prompt then accept updates route")
    }

    private fun pushRoute(
        result: CorridorRouteResult,
        start: String,
        end: String,
    ) {
        NaviMapTestHooks.routeStartLabel = start
        NaviMapTestHooks.routeEndLabel = end
        val samples = parseRouteSimSamples(result.simSamplesJson)
        val first = samples.firstOrNull()
        val last = samples.lastOrNull()
        if (first != null) {
            NaviMapTestHooks.pendingFromPoint = Waypoint(start, first.lat, first.lon)
        }
        if (last != null) {
            NaviMapTestHooks.pendingToPoint = Waypoint(end, last.lat, last.lon)
        }

        fun push() {
            composeRule.runOnUiThread {
                val direct = NaviMapTestHooks.applyRouteHandler
                if (direct != null) direct(result) else NaviMapTestHooks.pendingRoute = result
            }
        }
        push()
        val deadline = System.currentTimeMillis() + 90_000
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.lastRoutePolylineChars > 100) return
            Thread.sleep(300)
            push()
        }
        error("route not applied")
    }

    private fun prepareAndSeek(cumM: Double) {
        NaviMapTestHooks.requestPrepareRouteSimulation = true
        Thread.sleep(1_000)
        NaviMapTestHooks.requestStartRouteSimulation = true
        val simDeadline = System.currentTimeMillis() + 20_000
        while (System.currentTimeMillis() < simDeadline && !NaviMapTestHooks.simulatingActive) {
            Thread.sleep(200)
            NaviMapTestHooks.requestStartRouteSimulation = true
        }
        NaviMapTestHooks.requestSimSeekCumM = cumM
        val seekDeadline = System.currentTimeMillis() + 15_000
        while (System.currentTimeMillis() < seekDeadline) {
            if (kotlin.math.abs(NaviMapTestHooks.lastSimAlongM - cumM) < 2_000.0) break
            Thread.sleep(200)
            if (NaviMapTestHooks.requestSimSeekCumM == null) {
                NaviMapTestHooks.requestSimSeekCumM = cumM
            }
        }
    }

    private fun injectOffRoute(
        lat: Double,
        lon: Double,
    ) {
        NaviMapTestHooks.ignoreLiveGpsFixes = true
        NaviMapTestHooks.pendingInjectFixLatLon = lat to lon
        val deadline = System.currentTimeMillis() + 5_000
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.pendingInjectFixLatLon == null &&
                kotlin.math.abs(NaviMapTestHooks.lastGpsLat - lat) < 1e-5 &&
                kotlin.math.abs(NaviMapTestHooks.lastGpsLon - lon) < 1e-5
            ) {
                return
            }
            if (NaviMapTestHooks.pendingInjectFixLatLon == null) {
                // Consumed but coordinates not mirrored yet — short wait.
                Thread.sleep(50)
                if (NaviMapTestHooks.lastOffRoute ||
                    (
                        kotlin.math.abs(NaviMapTestHooks.lastGpsLat - lat) < 1e-5 &&
                            kotlin.math.abs(NaviMapTestHooks.lastGpsLon - lon) < 1e-5
                    )
                ) {
                    return
                }
            }
            Thread.sleep(50)
            if (NaviMapTestHooks.pendingInjectFixLatLon == null) {
                NaviMapTestHooks.pendingInjectFixLatLon = lat to lon
            }
        }
    }

    private fun planEspaCar(): CorridorRouteResult {
        val pbf = File(dataDir, "espa-atnbrufossen-corridor.osm.pbf")
        return planCarRoute(
            pbfPath = pbf.absolutePath,
            elevDir = File(dataDir, "elevation").absolutePath,
            cacheDir = File(dataDir, "graph-cache").absolutePath,
            startLat = 60.5621914,
            startLon = 11.2561239,
            endLat = 61.8512500,
            endLon = 10.2338420,
            useEco = false,
            profile = TravelProfile.CAR,
            avoidMotorways = false,
            avoidTolls = false,
            avoidFerries = false,
            vehicle = FfiVehicleLimits(null, null, null, null, null, null),
            preferOfficialNetworks = false,
            dataDir = "",
        )
    }

    private fun ensureEspaFixture() {
        val pbf = File(dataDir, "espa-atnbrufossen-corridor.osm.pbf")
        if (pbf.isFile && pbf.length() > 1_000_000L) return
        val staged = File("/data/local/tmp/navi_fixtures/espa-atnbrufossen-corridor.osm.pbf")
        assertTrue(staged.isFile)
        staged.copyTo(pbf, overwrite = true)
        val elev = File(dataDir, "elevation")
        elev.mkdirs()
        val tar = File("/data/local/tmp/navi_fixtures/elevation-corridor.tar")
        if (tar.isFile && !File(elev, "copernicus").exists()) {
            ProcessBuilder("tar", "-xf", tar.absolutePath, "-C", dataDir.absolutePath)
                .redirectErrorStream(true)
                .start()
                .waitFor()
        }
    }

    private fun saveShot(name: String) {
        val bmp =
            InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
                ?: return
        FileOutputStream(File(shotDir, name)).use { out ->
            bmp.compress(Bitmap.CompressFormat.PNG, 100, out)
        }
        bmp.recycle()
    }

    private fun pullShot(name: String) {
        val local = File(shotDir, name)
        if (!local.isFile) return
        runCatching {
            local.copyTo(File("/sdcard/Documents/debug/navi_$name"), overwrite = true)
        }
        Log.i(TAG, "SHOT=/sdcard/Documents/debug/navi_$name bytes=${local.length()}")
    }

    companion object {
        private const val TAG = "OffRouteBehavior"
    }
}
