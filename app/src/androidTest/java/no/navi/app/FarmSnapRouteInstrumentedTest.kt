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
 * Esso Myklegard (Løten) → Nedre Bårdset farm (Ringsaker).
 *
 * The farm sits on a leftover `highway=track` island after private drives are
 * stripped. Snap must use the public network so car planning succeeds.
 */
@RunWith(AndroidJUnit4::class)
class FarmSnapRouteInstrumentedTest {
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

    private fun waitStyle(timeoutMs: Long = 60_000) {
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.styleReady || NaviMapTestHooks.lastReportedLayerCount >= 1) {
                return
            }
            Thread.sleep(400)
        }
    }

    private fun injectRoute(route: CorridorRouteResult) {
        NaviMapTestHooks.routeStartLabel = "Esso Myklegard"
        NaviMapTestHooks.routeEndLabel = "Nedre Bårdset"
        NaviMapTestHooks.lastRoutePolylineChars = 0
        NaviMapTestHooks.pendingRoute = route
        val deadline = System.currentTimeMillis() + 60_000
        while (System.currentTimeMillis() < deadline) {
            Thread.sleep(400)
            if (NaviMapTestHooks.lastRoutePolylineChars > 100) {
                return
            }
            NaviMapTestHooks.pendingRoute = route
        }
        error(
            "route overlay not applied (chars=${NaviMapTestHooks.lastRoutePolylineChars})",
        )
    }

    @Test
    fun essoMyklegard_to_nedreBaardset_farm_plans_and_screenshot() {
        val pbf = File(dataDir, "ostlandet-latest.osm.pbf")
        assertTrue("missing Ostlandet PBF at ${pbf.absolutePath}", pbf.isFile)

        val route =
            planCarRoute(
                pbfPath = pbf.absolutePath,
                elevDir = File(dataDir, "elevation").absolutePath,
                cacheDir = File(dataDir, "graph-cache-farm-snap").absolutePath.also { File(it).mkdirs() },
                startLat = 60.849476,
                startLon = 11.368314,
                endLat = 60.9527528,
                endLon = 10.7794485,
                useEco = false,
                profile = TravelProfile.CAR,
                avoidMotorways = false,
                tollPolicy = uniffi.navi.FfiTollPolicy.ALLOW,
                avoidFerries = false,
                vehicle = FfiVehicleLimits(null, null, null, null, null, null),
                preferOfficialNetworks = false,
                dataDir = "",
            )
        android.util.Log.i("FarmSnapRoute", route.report)
        assertTrue("plan failed:\n${route.report}", route.report.contains("PASS"))
        assertTrue("empty polyline:\n${route.report}", route.routePolyline.contains(';'))
        assertTrue("too short (${route.distanceKm} km):\n${route.report}", route.distanceKm > 20.0)

        activityRule.launchActivity(null)
        waitStyle()
        injectRoute(route)
        NaviMapTestHooks.pendingCamera = Triple(60.90, 11.07, 9.5)
        Thread.sleep(2_500)
        NaviMapTestHooks.pendingCamera = Triple(60.90, 11.07, 9.5)
        InstrumentedMapCapture.screencapAfterSettle(
            "/data/local/tmp/esso_myklegard_nedre_bardset.png",
            timeoutMs = 12_000,
        )
        val shot = File("/data/local/tmp/esso_myklegard_nedre_bardset.png")
        assertTrue("missing screencap", shot.isFile && shot.length() > 20_000)
        shot.copyTo(File(dataDir, "esso_myklegard_nedre_bardset.png"), overwrite = true)
    }
}
