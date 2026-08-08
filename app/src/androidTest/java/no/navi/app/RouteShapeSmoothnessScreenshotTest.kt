package no.navi.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.rule.ActivityTestRule
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

/**
 * Evidence that pack-hit car overlays follow OSM road shape (not junction chords).
 *
 * Plans Espa→Atnbrufossen against the on-device Østlandet PBF + indexed packs,
 * frames a curved stretch, and writes:
 * - `/data/local/tmp/route_shape_smoothness.png`
 * - app files `route_shape_smoothness.png`
 *
 * Expect `pack_hit=true` in the plan report when packs are present. Vertex count
 * on the polyline should be much higher than junction-only chords after graph
 * pack v2 (edge shapes).
 */
@RunWith(AndroidJUnit4::class)
class RouteShapeSmoothnessScreenshotTest {
    @get:Rule
    val activityRule = ActivityTestRule(MainActivity::class.java, false, false)

    private lateinit var dataDir: File
    private lateinit var context: android.content.Context

    @Before
    fun setUp() {
        context = InstrumentationRegistry.getInstrumentation().targetContext
        val auto = InstrumentationRegistry.getInstrumentation().uiAutomation
        auto.grantRuntimePermission(context.packageName, android.Manifest.permission.ACCESS_FINE_LOCATION)
        auto.grantRuntimePermission(context.packageName, android.Manifest.permission.ACCESS_COARSE_LOCATION)
        dataDir = NaviAppData.resolve(context)
        NaviMapTestHooks.hideUiChrome = true
        NaviMapTestHooks.hideSearchChrome = true
        NaviMapTestHooks.disableGpsFollow = true
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

    private fun waitStyle(timeoutMs: Long = 60_000) {
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.styleReady || NaviMapTestHooks.lastReportedLayerCount >= 1) return
            Thread.sleep(400)
        }
    }

    private fun injectRoute(route: CorridorRouteResult) {
        NaviMapTestHooks.routeStartLabel = "Espa"
        NaviMapTestHooks.routeEndLabel = "Atnbrufossen"
        NaviMapTestHooks.lastRoutePolylineChars = 0
        // Prefer pendingRoute so MainActivity consumes on the main/UI thread
        // (Compose mapState). Direct handler calls from the test thread can
        // update hooks without painting the #C62828 overlay.
        NaviMapTestHooks.pendingRoute = route
        val deadline = System.currentTimeMillis() + 60_000
        while (System.currentTimeMillis() < deadline) {
            Thread.sleep(400)
            if (NaviMapTestHooks.lastRoutePolylineChars > 100) return
            // Re-arm in case a non-resumed activity stole/cleared the pending value.
            NaviMapTestHooks.pendingRoute = route
        }
        error(
            "route overlay not applied (chars=${NaviMapTestHooks.lastRoutePolylineChars}); " +
                "styleReady=${NaviMapTestHooks.styleReady}",
        )
    }

    @Test
    fun packHit_carRoute_followsRoadShape_screenshot() {
        val pbf = File(dataDir, "ostlandet-latest.osm.pbf")
        assertTrue("missing Ostlandet PBF at ${pbf.absolutePath}", pbf.isFile)

        val elev = File(dataDir, "elevation").absolutePath
        val cache = File(dataDir, "graph-cache-shape-shot").absolutePath
        File(cache).mkdirs()

        // Espa → Atnbrufossen (same corridor used elsewhere).
        val route =
            planCarRoute(
                pbfPath = pbf.absolutePath,
                elevDir = elev,
                cacheDir = cache,
                startLat = 60.5621914,
                startLon = 11.2561239,
                endLat = 61.85125,
                endLon = 10.233842,
                useEco = false,
                profile = TravelProfile.CAR,
                avoidMotorways = false,
                avoidTolls = false,
                avoidFerries = false,
                vehicle = FfiVehicleLimits(null, null, null, null, null, null),
                preferOfficialNetworks = false,
            )
        assertTrue("plan failed: ${route.report}", route.routePolyline.contains(';'))
        assertTrue("expected distance: ${route.distanceKm}", route.distanceKm > 50.0)
        val verts = route.routePolyline.split(';').size
        android.util.Log.i(
            "RouteShapeSmoothness",
            "verts=$verts pack_hit=${route.report.contains("pack_hit=true")} " +
                "report=${route.report.take(400)}",
        )
        // With shape-enabled packs, a ~150 km corridor has thousands of verts;
        // junction chords alone are typically a few hundred.
        File(dataDir, "route_shape_smoothness_meta.txt").writeText(
            "verts=$verts\npack_hit=${route.report.contains("pack_hit=true")}\n" +
                "distance_km=${route.distanceKm}\nreport=${route.report}\n",
        )

        activityRule.launchActivity(null)
        waitStyle()
        injectRoute(route)
        assertTrue(
            "route polyline not on map (chars=${NaviMapTestHooks.lastRoutePolylineChars})",
            NaviMapTestHooks.lastRoutePolylineChars > 500,
        )
        // Zoom a curved Mjøsa shore stretch where junction chords leave the road.
        // Apply twice: fit-to-corridor after inject can race the first camera set.
        NaviMapTestHooks.pendingCamera = Triple(60.98, 10.70, 11.5)
        Thread.sleep(2_500)
        NaviMapTestHooks.pendingCamera = Triple(60.98, 10.70, 11.5)
        Thread.sleep(4_000)
        InstrumentedMapCapture.screencapAfterSettle(
            "/data/local/tmp/route_shape_smoothness.png",
            timeoutMs = 12_000,
        )
        shell("chmod 644 /data/local/tmp/route_shape_smoothness.png")
        val shot = File("/data/local/tmp/route_shape_smoothness.png")
        assertTrue("missing screencap", shot.isFile && shot.length() > 20_000)
        shot.copyTo(File(dataDir, "route_shape_smoothness.png"), overwrite = true)

        // After graph-pack v2 (edge shapes), expect dense overlay verts.
        // Set -Pandroid.testInstrumentationRunnerArguments.expectSmooth=true
        // (or system property navi.route_shape.expect_smooth=true) for the
        // post-fix gate; omit for a chord baseline capture.
        val expectSmooth =
            System
                .getProperty("navi.route_shape.expect_smooth", "false")
                .equals("true", ignoreCase = true) ||
                InstrumentationRegistry
                    .getArguments()
                    .getString("expectSmooth", "false")
                    .equals("true", ignoreCase = true)
        if (expectSmooth) {
            assertTrue(
                "polyline still looks junction-sparse (verts=$verts) — packs may lack shape",
                verts >= 800,
            )
        } else {
            android.util.Log.i(
                "RouteShapeSmoothness",
                "baseline capture verts=$verts (expectSmooth=false)",
            )
        }
    }
}
