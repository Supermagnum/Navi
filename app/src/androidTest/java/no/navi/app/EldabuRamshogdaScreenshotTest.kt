package no.navi.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.rule.ActivityTestRule
import androidx.test.uiautomator.By
import androidx.test.uiautomator.UiDevice
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.BeforeClass
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/**
 * Re-captures Eldåbu corridor with 3D on, no permission dialog
 * (README image; peaks stay unlabeled).
 */
@RunWith(AndroidJUnit4::class)
class EldabuRamshogdaScreenshotTest {
    companion object {
        // Camera near Eldåbu (61.756, 9.979); terrain toward Store Ramshøgda stays unlabeled.
        private const val CAM_LAT = 61.7525
        private const val CAM_LON = 10.0538
        private const val CAM_ZOOM = 12.0

        @JvmStatic
        lateinit var poly: String

        @JvmStatic
        lateinit var breaks: String

        @JvmStatic
        @BeforeClass
        fun loadFixtures() {
            val staged = File("/data/local/tmp/navi_fixtures")
            poly = File(staged, "skolla_rondvassbu.polyline.txt").readText().trim()
            breaks = File(staged, "skolla_rondvassbu.breaks.json").readText().trim()
            check(poly.contains(';'))
            check(breaks.contains("Eldåbu"))
            check(!breaks.contains("Store Ramshøgda")) {
                "Store Ramshøgda must not be a labeled pause stop"
            }
            val context = InstrumentationRegistry.getInstrumentation().targetContext
            val dataDir = NaviAppData.resolve(context)
            OstlandetOfflineFixtures.ensureInstalled(dataDir)
            MapHudPrefs.saveOptIn3d(context, true)
            NaviMapTestHooks.hideUiChrome = false
            NaviMapTestHooks.hideSearchChrome = true
            runCatching {
                InstrumentationRegistry
                    .getInstrumentation()
                    .uiAutomation
                    .grantRuntimePermission(
                        context.packageName,
                        android.Manifest.permission.ACCESS_FINE_LOCATION,
                    )
            }
            runCatching {
                InstrumentationRegistry
                    .getInstrumentation()
                    .uiAutomation
                    .grantRuntimePermission(
                        context.packageName,
                        android.Manifest.permission.ACCESS_COARSE_LOCATION,
                    )
            }
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
        NaviMapTestHooks.gpsAltitudeM = 980.0
        MapHudPrefs.saveOptIn3d(context, true)
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

    private fun dismissPermissionDialogs() {
        val deadline = System.currentTimeMillis() + 12_000
        while (System.currentTimeMillis() < deadline) {
            val allow =
                device.findObject(By.text("While using the app"))
                    ?: device.findObject(By.text("Allow"))
                    ?: device.findObject(By.text("ALLOW"))
                    ?: device.findObject(
                        By.res("com.android.permissioncontroller", "permission_allow_button"),
                    )
                    ?: device.findObject(
                        By.res(
                            "com.android.permissioncontroller",
                            "permission_allow_foreground_only_button",
                        ),
                    )
            if (allow != null) {
                allow.click()
                Thread.sleep(700)
                continue
            }
            break
        }
    }

    private fun waitStyle(timeoutMs: Long = 90_000) {
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.styleReady || NaviMapTestHooks.lastReportedLayerCount >= 1) return
            Thread.sleep(400)
        }
    }

    private fun injectRoute() {
        NaviMapTestHooks.routeStartLabel = "Skolla"
        NaviMapTestHooks.routeEndLabel = "Rondvassbu"
        NaviMapTestHooks.routeViaLabel = "Harlandshytta, Eldåbu"

        fun route() =
            uniffi.navi.CorridorRouteResult(
                report = "PASS\ndistance_km=112.5\n",
                distanceKm = 112.5,
                etaMinutes = 1800.0,
                cacheHit = true,
                coldBuildS = 0.0,
                warmLoadS = 0.0,
                routePolyline = poly,
                poiLat = 61.7562897,
                poiLon = 9.9793564,
                poiName = "Eldåbu",
                poiIconKey = "cabin",
                breakPoisJson = breaks,
                daysJson = "[]",
                simSamplesJson = "[]",
                maneuversJson = "[]",
                priorityPathSharePct = 0.0,
            )
        NaviMapTestHooks.pendingRoute = route()
        val deadline = System.currentTimeMillis() + 60_000
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.lastRoutePolylineChars > 100 &&
                NaviMapTestHooks.lastBreakPoiCount >= 2
            ) {
                return
            }
            Thread.sleep(400)
            NaviMapTestHooks.pendingRoute = route()
        }
        error("route not applied")
    }

    private fun await3d(timeoutMs: Long = 120_000) {
        NaviMapTestHooks.requestOptIn3d = true
        NaviMapTestHooks.requestCameraTiltDeg = 45.0
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            // Opt-in 3D is hillshade attach; kind may be Online3d or OfflineProtomaps.
            // Camera tilt is a separate user preset (no longer forced by 3D).
            if (NaviMapTestHooks.lastTerrainAttached) {
                if (NaviMapTestHooks.lastCameraPitch < 40.0) {
                    NaviMapTestHooks.requestCameraTiltDeg = 45.0
                } else {
                    return
                }
            }
            Thread.sleep(400)
        }
        error(
            "3D not active kind=${NaviMapTestHooks.lastBasemapKind} " +
                "terrain=${NaviMapTestHooks.lastTerrainAttached} " +
                "pitch=${NaviMapTestHooks.lastCameraPitch}",
        )
    }

    private fun captureClean(name: String) {
        dismissPermissionDialogs()
        NaviMapTestHooks.hideSearchChrome = true
        // Keep re-centering while tiles settle.
        repeat(8) {
            NaviMapTestHooks.pendingCamera = Triple(CAM_LAT, CAM_LON, CAM_ZOOM)
            Thread.sleep(2_500)
            dismissPermissionDialogs()
        }
        val shot = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
        assertTrue("null shot", shot != null)
        val out = File(dataDir, name)
        out.outputStream().use { shot!!.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) }
        val cache = File(context.cacheDir, name)
        out.copyTo(cache, overwrite = true)
        shell("run-as ${context.packageName} cat cache/$name > /data/local/tmp/$name")
        shell("chmod 644 /data/local/tmp/$name")
        // Reject permission-dialog composites (~90–100KB purple/empty or dialog-heavy).
        assertTrue("$name too small (${out.length()})", out.length() > 180_000)
        android.util.Log.i(
            "EldabuRamshogdaScreenshotTest",
            "shot=$name bytes=${out.length()} kind=${NaviMapTestHooks.lastBasemapKind} " +
                "terrain=${NaviMapTestHooks.lastTerrainAttached} pitch=${NaviMapTestHooks.lastCameraPitch} " +
                "breaks=${NaviMapTestHooks.lastBreakPoiCount}",
        )
    }

    @Test
    fun eldabu_and_store_ramshogda_3d() {
        activityRule.launchActivity(null)
        dismissPermissionDialogs()
        waitStyle()
        dismissPermissionDialogs()

        NaviMapTestHooks.requestOptIn3d = true
        Thread.sleep(2_000)
        injectRoute()
        NaviMapTestHooks.pendingCamera = Triple(CAM_LAT, CAM_LON, CAM_ZOOM)
        await3d()
        Thread.sleep(8_000)
        captureClean("hike_eldabu_ramshogda_3d.png")
    }
}
