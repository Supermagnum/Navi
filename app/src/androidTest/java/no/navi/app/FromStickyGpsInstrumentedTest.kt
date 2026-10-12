package no.navi.app

import android.graphics.Bitmap
import android.os.SystemClock
import android.util.Log
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.rule.GrantPermissionRule
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.CorridorRouteResult
import uniffi.navi.PlaceHit
import uniffi.navi.TravelProfile
import java.io.File
import java.io.FileOutputStream

/**
 * Emulator proof for #158 / PR #160:
 * 1) Typed/picked Hamar From ~1 km from the GPS fix stays Hamar after plan +
 *    sustained off-route (does not auto-replan From to GPS).
 * 2) Use GPS / My-position From follows GPS on auto-replan.
 */
@RunWith(AndroidJUnit4::class)
class FromStickyGpsInstrumentedTest {
    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

    @get:Rule
    val permissionRule: GrantPermissionRule =
        GrantPermissionRule.grant(
            android.Manifest.permission.ACCESS_FINE_LOCATION,
            android.Manifest.permission.ACCESS_COARSE_LOCATION,
        )

    private lateinit var shotDir: File
    private lateinit var evidenceDir: File

    @Before
    fun setUp() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        shotDir = File(context.cacheDir, "pr160_from_sticky_shots").also { it.mkdirs() }
        evidenceDir =
            File(context.getExternalFilesDir(null), "pr160-from-sticky").also { it.mkdirs() }
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = false
        NaviMapTestHooks.ignoreLiveGpsFixes = true
        NaviMapTestHooks.pendingInjectFixLatLon = null
        NaviMapTestHooks.pendingRoute = null
        NaviMapTestHooks.offRouteConfirmMsOverride = 800L
        NaviMapTestHooks.autoRerouteTriggeredCount = 0
        NaviMapTestHooks.rerouteResultOverride = null
        NaviMapTestHooks.forcePlanPbfPath = null
        NaviMapTestHooks.requestHikingRerouteAnswer = null
        NaviMapTestHooks.lastOffRoute = false
        NaviMapTestHooks.lastFromFollowsGps = true
        NaviMapTestHooks.lastFromName = ""
        NaviMapTestHooks.pendingFromFollowsGps = null
        NaviMapTestHooks.lastGpsLat = Double.NaN
        NaviMapTestHooks.lastGpsLon = Double.NaN
        NaviMapTestHooks.requestStopRouteSimulation = true
        Thread.sleep(400)
        NaviMapTestHooks.requestStopRouteSimulation = false
        NaviMapTestHooks.requestTravelProfile = TravelProfile.CAR
        composeRule.waitForIdle()
        // Hook poll + applyFixRef are live once the route applier is registered.
        waitUntil(30_000) { NaviMapTestHooks.applyRouteHandler != null }
        Thread.sleep(400)
    }

    @After
    fun tearDown() {
        NaviMapTestHooks.ignoreLiveGpsFixes = false
        NaviMapTestHooks.pendingInjectFixLatLon = null
        NaviMapTestHooks.rerouteResultOverride = null
        NaviMapTestHooks.offRouteConfirmMsOverride = null
        NaviMapTestHooks.hideSearchChrome = false
        NaviMapTestHooks.pendingFromFollowsGps = null
        OnlinePlaceSearch.reverseOverrideForTests = null
    }

    @Test
    fun typedHamarFrom_1kmFromGps_staysStickyAfterPlanAndOffRoute() {
        injectFix(GPS_NEAR_HAMAR_LAT, GPS_NEAR_HAMAR_LON)
        assertTrue(
            "GPS fix must land near Hamar offset",
            !NaviMapTestHooks.lastGpsLat.isNaN(),
        )
        val startToGpsM =
            RouteProgressTracker.haversineM(
                HAMAR_LAT,
                HAMAR_LON,
                GPS_NEAR_HAMAR_LAT,
                GPS_NEAR_HAMAR_LON,
            )
        assertTrue("GPS should be ~1km from Hamar, got ${startToGpsM}m", startToGpsM in 700.0..1200.0)

        // Typed/picked From (sticky): apply via pendingFromFollowsGps=false on plan.
        val route = syntheticHamarHop()
        NaviMapTestHooks.rerouteResultOverride =
            route.copy(
                report = route.report + "reroute_override=true\n",
                distanceKm = route.distanceKm + 1.2,
                routePolyline = route.routePolyline + ";11.09,60.80",
            )
        pushRoute(route, fromName = "Hamar", fromFollowsGps = false)
        composeRule.waitForIdle()
        runCatching { composeRule.mainClock.advanceTimeBy(500) }
        waitUntil(15_000) {
            NaviMapTestHooks.lastFromName.contains("Hamar", ignoreCase = true) &&
                !NaviMapTestHooks.lastFromFollowsGps
        }
        assertFalse(
            "typed Hamar must not GPS-follow",
            NaviMapTestHooks.lastFromFollowsGps,
        )
        assertTrue(NaviMapTestHooks.lastFromName.contains("Hamar", ignoreCase = true))

        val samples = parseRouteSimSamples(route.simSamplesJson)
        assertTrue(samples.size >= 8)
        val mid = samples[samples.size / 3]
        prepareAndSeek(mid.cumM)

        val countBefore = NaviMapTestHooks.autoRerouteTriggeredCount
        val polyBefore = NaviMapTestHooks.lastRoutePolylineChars
        val fromBefore = NaviMapTestHooks.lastFromName
        val offLat = mid.lat + 0.06
        val offLon = mid.lon
        assertTrue(
            RouteProgressTracker.haversineM(offLat, offLon, mid.lat, mid.lon) > 3_000.0,
        )

        val deadline = System.currentTimeMillis() + 18_000
        while (System.currentTimeMillis() < deadline) {
            injectOffRoute(offLat, offLon)
            if (NaviMapTestHooks.autoRerouteTriggeredCount > countBefore) break
            if (NaviMapTestHooks.reroutingActive) break
            Thread.sleep(250)
        }

        assertEquals(
            "typed From must NOT auto-replan from GPS",
            countBefore,
            NaviMapTestHooks.autoRerouteTriggeredCount,
        )
        assertFalse("must not enter rerouting for sticky From", NaviMapTestHooks.reroutingActive)
        assertEquals(polyBefore, NaviMapTestHooks.lastRoutePolylineChars)
        assertTrue(
            "From label must stay Hamar after off-route (got=${NaviMapTestHooks.lastFromName})",
            NaviMapTestHooks.lastFromName.contains("Hamar", ignoreCase = true),
        )
        assertEquals(fromBefore, NaviMapTestHooks.lastFromName)
        assertFalse(NaviMapTestHooks.lastFromFollowsGps)

        // Keep search chrome visible so the From summary is in the shot.
        NaviMapTestHooks.hideSearchChrome = false
        Thread.sleep(400)
        saveShot("01_typed_hamar_sticky_after_offroute.png")
        Log.i(
            TAG,
            "PASS sticky Hamar fromFollowsGps=${NaviMapTestHooks.lastFromFollowsGps} " +
                "from=${NaviMapTestHooks.lastFromName} summary=${waypointSummaryText()} " +
                "gps=${NaviMapTestHooks.lastGpsLat},${NaviMapTestHooks.lastGpsLon} " +
                "startToGpsM≈${
                    RouteProgressTracker.haversineM(
                        HAMAR_LAT,
                        HAMAR_LON,
                        GPS_NEAR_HAMAR_LAT,
                        GPS_NEAR_HAMAR_LON,
                    )
                }",
        )
    }

    @Test
    fun useGpsFrom_followsGpsOnAutoReplan() {
        injectFix(GPS_NEAR_HAMAR_LAT, GPS_NEAR_HAMAR_LON)
        OnlinePlaceSearch.reverseOverrideForTests = { lat, lon ->
            PlaceHit(
                osmId = 2L,
                name = "My position",
                kind = "online/place/house",
                lat = lat,
                lon = lon,
                subArea = "",
                municipality = "Hamar",
                regionId = "europe/norway/ostlandet",
            )
        }

        composeRule.waitForIdle()
        composeRule.onNodeWithTag("chip_from", useUnmergedTree = true).performClick()
        composeRule.waitForIdle()
        composeRule.onNodeWithTag("btn_use_gps", useUnmergedTree = true).performClick()

        waitUntil(8_000) {
            !waypointSummaryText().contains("From: (unset)") &&
                NaviMapTestHooks.lastFromFollowsGps
        }
        assertTrue(
            "Use GPS must enable GPS-follow",
            NaviMapTestHooks.lastFromFollowsGps,
        )
        val summaryGps = waypointSummaryText()
        assertTrue("From set after Use GPS: $summaryGps", summaryGps.contains("From:"))

        val route = syntheticHamarHop()
        NaviMapTestHooks.rerouteResultOverride =
            route.copy(
                report = route.report + "reroute_override=true\n",
                distanceKm = route.distanceKm + 2.0,
                routePolyline = route.routePolyline + ";11.10,60.81",
            )
        // Keep GPS-follow; do not overwrite From name with sticky Hamar.
        pushRoute(route, fromName = NaviMapTestHooks.lastFromName.ifBlank { "My position" }, fromFollowsGps = true)
        assertTrue(NaviMapTestHooks.lastFromFollowsGps)

        val samples = parseRouteSimSamples(route.simSamplesJson)
        val mid = samples[samples.size / 3]
        prepareAndSeek(mid.cumM)

        val countBefore = NaviMapTestHooks.autoRerouteTriggeredCount
        val polyBefore = NaviMapTestHooks.lastRoutePolylineChars
        val offLat = mid.lat + 0.06
        val offLon = mid.lon

        val deadline = System.currentTimeMillis() + 20_000
        while (System.currentTimeMillis() < deadline) {
            injectOffRoute(offLat, offLon)
            if (NaviMapTestHooks.autoRerouteTriggeredCount > countBefore) break
            if (NaviMapTestHooks.reroutingActive) break
            if (NaviMapTestHooks.lastRoutePolylineChars != polyBefore) break
            Thread.sleep(250)
        }

        assertTrue(
            "Use GPS From must auto-replan from live fix " +
                "(count=${NaviMapTestHooks.autoRerouteTriggeredCount} poly=$polyBefore→${NaviMapTestHooks.lastRoutePolylineChars})",
            NaviMapTestHooks.autoRerouteTriggeredCount > countBefore ||
                NaviMapTestHooks.reroutingActive ||
                NaviMapTestHooks.lastRoutePolylineChars != polyBefore,
        )
        waitUntil(10_000) { !NaviMapTestHooks.reroutingActive }
        assertTrue(
            "GPS-follow flag remains after replan",
            NaviMapTestHooks.lastFromFollowsGps,
        )

        saveShot("02_use_gps_follows_after_replan.png")
        Log.i(
            TAG,
            "PASS use-gps followsGps=${NaviMapTestHooks.lastFromFollowsGps} " +
                "from=${NaviMapTestHooks.lastFromName} " +
                "reroutes=${NaviMapTestHooks.autoRerouteTriggeredCount} " +
                "summary=${waypointSummaryText()}",
        )
    }

    private fun syntheticHamarHop(): CorridorRouteResult {
        val startLat = HAMAR_LAT
        val startLon = HAMAR_LON
        val endLat = HAMAR_LAT + 0.012
        val endLon = HAMAR_LON + 0.018
        val n = 24
        val samples = ArrayList<String>(n)
        val poly = StringBuilder()
        var cum = 0.0
        var prevLat = startLat
        var prevLon = startLon
        for (i in 0 until n) {
            val t = i / (n - 1).toDouble()
            val lat = startLat + (endLat - startLat) * t
            val lon = startLon + (endLon - startLon) * t
            if (i > 0) {
                cum += RouteProgressTracker.haversineM(prevLat, prevLon, lat, lon)
                poly.append(';')
            }
            poly.append("%.6f,%.6f".format(java.util.Locale.US, lon, lat))
            samples.add(
                """{"lat":$lat,"lon":$lon,"cum_m":$cum,"speed_kmh":50.0,"highway":"primary","maxspeed":50}""",
            )
            prevLat = lat
            prevLon = lon
        }
        return CorridorRouteResult(
            report = "TEST_KIND=FROM_STICKY_HAMAR\nprofile=Car\nPASS\n",
            distanceKm = cum / 1000.0,
            etaMinutes = (cum / 1000.0) / 50.0 * 60.0,
            cacheHit = true,
            coldBuildS = 0.0,
            warmLoadS = 0.0,
            routePolyline = poly.toString(),
            poiLat = endLat,
            poiLon = endLon,
            poiName = "Near Hamar",
            poiIconKey = "city",
            breakPoisJson = "[]",
            daysJson = "[]",
            simSamplesJson = "[" + samples.joinToString(",") + "]",
            maneuversJson = "[]",
            priorityPathSharePct = 100.0,
            routeSegmentsJson = "[]",
            offTrailAdvisory = "",
            tollPolicy = "allow",
            padAttemptsJson = "[]",
            searchExpansions = 0u,
            searchTerminateReason = "ok",
            tollAvoidanceIncomplete = false,
            routeUsesTolls = false,
        )
    }

    private fun pushRoute(
        result: CorridorRouteResult,
        fromName: String,
        fromFollowsGps: Boolean,
    ) {
        val samples = parseRouteSimSamples(result.simSamplesJson)
        val first = samples.first()
        val last = samples.last()
        NaviMapTestHooks.routeStartLabel = fromName
        NaviMapTestHooks.routeEndLabel = "Near Hamar"
        NaviMapTestHooks.pendingFromPoint = Waypoint(fromName, first.lat, first.lon)
        NaviMapTestHooks.pendingFromFollowsGps = fromFollowsGps
        NaviMapTestHooks.pendingToPoint = Waypoint("Near Hamar", last.lat, last.lon)
        NaviMapTestHooks.lastRoutePolylineChars = 0

        fun push() {
            composeRule.runOnUiThread {
                val direct = NaviMapTestHooks.applyRouteHandler
                if (direct != null) direct(result) else NaviMapTestHooks.pendingRoute = result
            }
        }
        push()
        val deadline = System.currentTimeMillis() + 60_000
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.lastRoutePolylineChars > 50) {
                composeRule.waitForIdle()
                return
            }
            Thread.sleep(250)
            push()
        }
        error("route not applied")
    }

    private fun prepareAndSeek(cumM: Double) {
        NaviMapTestHooks.requestPrepareRouteSimulation = true
        Thread.sleep(800)
        NaviMapTestHooks.requestStartRouteSimulation = true
        val simDeadline = System.currentTimeMillis() + 20_000
        while (System.currentTimeMillis() < simDeadline && !NaviMapTestHooks.simulatingActive) {
            Thread.sleep(200)
            NaviMapTestHooks.requestStartRouteSimulation = true
        }
        assertTrue("simulation must start", NaviMapTestHooks.simulatingActive)
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

    private fun injectFix(
        lat: Double,
        lon: Double,
    ) {
        NaviMapTestHooks.ignoreLiveGpsFixes = true
        composeRule.waitForIdle()
        val deadline = SystemClock.elapsedRealtime() + 30_000
        var pendingSeenNull = false
        while (SystemClock.elapsedRealtime() < deadline) {
            if (NaviMapTestHooks.pendingInjectFixLatLon == null) {
                pendingSeenNull = true
            }
            if (!NaviMapTestHooks.lastGpsLat.isNaN() &&
                kotlin.math.abs(NaviMapTestHooks.lastGpsLat - lat) < 0.001 &&
                kotlin.math.abs(NaviMapTestHooks.lastGpsLon - lon) < 0.001
            ) {
                return
            }
            NaviMapTestHooks.pendingInjectFixLatLon = lat to lon
            runCatching { composeRule.mainClock.advanceTimeBy(250) }
            Thread.sleep(250)
        }
        error(
            "inject fix failed lat=$lat lon=$lon got=${NaviMapTestHooks.lastGpsLat},${NaviMapTestHooks.lastGpsLon} " +
                "pendingConsumed=$pendingSeenNull pendingNow=${NaviMapTestHooks.pendingInjectFixLatLon} " +
                "handler=${NaviMapTestHooks.applyRouteHandler != null}",
        )
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
                kotlin.math.abs(NaviMapTestHooks.lastGpsLat - lat) < 1e-4
            ) {
                return
            }
            Thread.sleep(50)
            if (NaviMapTestHooks.pendingInjectFixLatLon == null) {
                NaviMapTestHooks.pendingInjectFixLatLon = lat to lon
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
        assertTrue("waitUntil timed out (${timeoutMs}ms)", pred())
    }

    private fun waypointSummaryText(): String =
        runCatching {
            composeRule
                .onNodeWithTag("search_waypoints_summary", useUnmergedTree = true)
                .fetchSemanticsNode()
                .config[SemanticsProperties.Text]
                .joinToString(" ") { it.text }
        }.getOrDefault("")

    private fun saveShot(name: String) {
        val bmp =
            InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
                ?: return
        FileOutputStream(File(shotDir, name)).use { out ->
            bmp.compress(Bitmap.CompressFormat.PNG, 90, out)
        }
        runCatching {
            File(shotDir, name).copyTo(File(evidenceDir, name), overwrite = true)
        }
        runCatching {
            File(shotDir, name).copyTo(
                File("/sdcard/Documents/debug/navi_$name"),
                overwrite = true,
            )
        }
        // Host-pull path for evidence commits.
        val pfd =
            InstrumentationRegistry
                .getInstrumentation()
                .uiAutomation
                .executeShellCommand("cp ${File(shotDir, name).absolutePath} /data/local/tmp/$name")
        java.io.FileInputStream(pfd.fileDescriptor).use { input ->
            val buf = ByteArray(4096)
            while (input.read(buf) >= 0) {
            }
        }
        pfd.close()
        bmp.recycle()
        Log.i(TAG, "SHOT $name bytes=${File(shotDir, name).length()}")
    }

    companion object {
        private const val TAG = "FromStickyGps"
        private const val HAMAR_LAT = 60.79446
        private const val HAMAR_LON = 11.07872

        /** ~0.9 km west of Hamar centre — within the old 1 km "joined" window. */
        private const val GPS_NEAR_HAMAR_LAT = 60.79446
        private const val GPS_NEAR_HAMAR_LON = 11.0621
    }
}
