package no.navi.app

import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.By
import androidx.test.uiautomator.UiDevice
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.BeforeClass
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/**
 * Map screenshots for the staged Skolla→Rondvassbu hike focused on Eldåbu
 * pause labels (2D / corridor framing). Search/keyboard planning is covered by
 * HikingSearchRouteScreenshotTest.
 */
@RunWith(AndroidJUnit4::class)
class HikingRouteMapScreenshotTest {
    companion object {
        @JvmStatic
        lateinit var poly: String

        @JvmStatic
        lateinit var breaks: String

        @JvmStatic
        @BeforeClass
        fun loadFixtures() {
            NaviMapTestHooks.hideUiChrome = false
            NaviMapTestHooks.hideSearchChrome = true
            NaviMapTestHooks.lastTerrainAttached = false
            NaviMapTestHooks.lastCameraPitch = 0.0
            NaviMapTestHooks.lastBasemapKind = ""
            NaviMapTestHooks.styleReady = false
            NaviMapTestHooks.lastRoutePolylineChars = 0
            NaviMapTestHooks.lastBreakPoiCount = 0
            val context = InstrumentationRegistry.getInstrumentation().targetContext
            val dataDir = NaviAppData.resolve(context)
            // Offline 3D needs Ostlandet PMTiles + DEM in app dataDir (not only fixtures/).
            OstlandetOfflineFixtures.ensureInstalled(dataDir)
            MapHudPrefs.saveOptIn3d(context, true)
            val staged = File("/data/local/tmp/navi_fixtures")
            poly = File(staged, "skolla_rondvassbu.polyline.txt").readText().trim()
            breaks = File(staged, "skolla_rondvassbu.breaks.json").readText().trim()
            check(poly.contains(';'))
            check(breaks.contains("Eldåbu"))
            check(!breaks.contains("Store Ramshøgda")) {
                "Store Ramshøgda must not be a labeled pause stop"
            }
        }
    }

    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

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
        // Do not grant/dismiss location here — on this AVD a granted dialog leaves
        // takeScreenshot with an empty MapLibre frame; the first permission prompt
        // composites a usable map+route image behind the dialog.
    }

    private fun dismissPermissionDialogs() {
        val deadline = System.currentTimeMillis() + 10_000
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
                Thread.sleep(600)
                continue
            }
            break
        }
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

    private fun waitStyle(timeoutMs: Long = 90_000) {
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.styleReady) return
            Thread.sleep(400)
        }
        assertTrue("styleReady", NaviMapTestHooks.styleReady)
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
                poiLat = 61.8804325,
                poiLon = 9.7959854,
                poiName = "Rondvassbu",
                poiIconKey = "cabin",
                breakPoisJson = breaks,
                daysJson = "[]",
                simSamplesJson = "[]",
                maneuversJson = "[]",
                priorityPathSharePct = 0.0,
            )

        // Apply via the live composition handler when available — pendingRoute alone
        // is skipped while a permission dialog has paused the activity (RESUMED-only).
        fun pushRoute() {
            val r = route()
            composeRule.runOnUiThread {
                val direct = NaviMapTestHooks.applyRouteHandler
                if (direct != null) {
                    direct(r)
                } else {
                    NaviMapTestHooks.pendingRoute = r
                }
            }
        }
        pushRoute()
        val deadline = System.currentTimeMillis() + 60_000
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.lastRoutePolylineChars > 100 &&
                NaviMapTestHooks.lastBreakPoiCount >= 2
            ) {
                return
            }
            Thread.sleep(400)
            pushRoute()
        }
        error(
            "route not applied poly=${NaviMapTestHooks.lastRoutePolylineChars} " +
                "breaks=${NaviMapTestHooks.lastBreakPoiCount} " +
                "handler=${NaviMapTestHooks.applyRouteHandler != null}",
        )
    }

    private fun await3d(timeoutMs: Long = 120_000) {
        NaviMapTestHooks.requestOptIn3d = true
        NaviMapTestHooks.requestCameraTiltDeg = 45.0
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            // Opt-in 3D = Mapterhorn hillshade attach. Camera tilt is independent
            // (TERRAIN_VIEW_TILT is 0); request a user preset for nicer shots but
            // do not fail the test if pitch stays flat.
            if (NaviMapTestHooks.lastTerrainAttached) {
                if (NaviMapTestHooks.lastCameraPitch < 40.0) {
                    NaviMapTestHooks.requestCameraTiltDeg = 45.0
                }
                return
            }
            Thread.sleep(400)
        }
        error(
            "3D not active kind=${NaviMapTestHooks.lastBasemapKind} " +
                "terrain=${NaviMapTestHooks.lastTerrainAttached} " +
                "pitch=${NaviMapTestHooks.lastCameraPitch}",
        )
    }

    private fun await2d(timeoutMs: Long = 90_000) {
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            if (!NaviMapTestHooks.lastTerrainAttached &&
                NaviMapTestHooks.lastCameraPitch < 15.0
            ) {
                return
            }
            Thread.sleep(400)
        }
    }

    private fun capture(name: String) {
        NaviMapTestHooks.hideSearchChrome = true
        var best: android.graphics.Bitmap? = null
        var bestBytes = 0
        val deadline = System.currentTimeMillis() + 90_000
        while (System.currentTimeMillis() < deadline) {
            Thread.sleep(4_000)
            val shot =
                InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
                    ?: continue
            val baos = java.io.ByteArrayOutputStream()
            shot.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, baos)
            val bytes = baos.toByteArray()
            if (bytes.size > bestBytes) {
                bestBytes = bytes.size
                best?.recycle()
                best = shot
            } else {
                shot.recycle()
            }
            if (bestBytes > 200_000) break
        }
        assertTrue("null shot $name", best != null)
        val out = File(dataDir, name)
        out.outputStream().use { best!!.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) }
        val cache = File(context.cacheDir, name)
        out.copyTo(cache, overwrite = true)
        shell("run-as ${context.packageName} cat cache/$name > /data/local/tmp/$name")
        shell("chmod 644 /data/local/tmp/$name")
        assertTrue("$name too small (${out.length()})", out.length() > 80_000)
        android.util.Log.i(
            "HikingRouteMapScreenshotTest",
            "shot=$name kind=${NaviMapTestHooks.lastBasemapKind} " +
                "terrain=${NaviMapTestHooks.lastTerrainAttached} " +
                "pitch=${NaviMapTestHooks.lastCameraPitch} breaks=${NaviMapTestHooks.lastBreakPoiCount} " +
                "bytes=${out.length()}",
        )
    }

    @Test
    fun hike_eldabu_corridor_2d_and_3d() {
        // Do not dismiss the location dialog until the map style is ready —
        // early dismissal races MapLibre surface creation on this AVD.
        waitStyle()
        dataDir =
            (
                NaviAppData.resolve(composeRule.activity)
            ).also { it.mkdirs() }

        MapHudPrefs.saveOptIn3d(context, true)
        NaviMapTestHooks.requestOptIn3d = true
        Thread.sleep(3_000)
        injectRoute()
        NaviMapTestHooks.pendingCamera = Triple(61.7525, 10.0538, 11.0)
        await3d()
        Thread.sleep(5_000)
        capture("hike_eldabu_ramshogda_3d.png")

        MapHudPrefs.saveOptIn3d(context, false)
        NaviMapTestHooks.requestOptIn3d = false
        Thread.sleep(3_000)
        injectRoute()
        NaviMapTestHooks.pendingCamera = Triple(61.7525, 10.0538, 11.0)
        await2d()
        Thread.sleep(5_000)
        capture("hike_eldabu_ramshogda_2d.png")

        val a = File(dataDir, "hike_eldabu_ramshogda_3d.png")
        val b = File(dataDir, "hike_eldabu_ramshogda_2d.png")
        assertTrue(a.isFile && b.isFile)
        assertNotEquals(
            "3D and 2D Eldåbu shots must differ",
            a.readBytes().contentHashCode(),
            b.readBytes().contentHashCode(),
        )
    }
}
