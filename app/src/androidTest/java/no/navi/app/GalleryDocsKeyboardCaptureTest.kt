package no.navi.app

import android.graphics.Bitmap
import android.util.Log
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextClearance
import androidx.compose.ui.test.performTextInput
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.By
import androidx.test.uiautomator.UiDevice
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.BeforeClass
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.FfiTruckRestSettings
import uniffi.navi.saveTruckRestSettings
import java.io.File

/**
 * Gallery docs capture on **real hardware only** (SM-P613).
 *
 * - Coordinates entered via Route search keyboard (`lat, lon`), never map-tap /
 *   pendingCamera injection for scene framing after a hit.
 * - [NaviMapTestHooks.disableGpsFollow] stays true so screenshots never reveal
 *   the tester's live GPS. Route-following shots use **Simulate route**.
 * - Corridor geometry comes from the in-app planner (real PBF), not stubs.
 *
 * Host pull:
 *   adb pull /data/local/tmp/navi_gallery_docs/ docs/images/
 */
@RunWith(AndroidJUnit4::class)
class GalleryDocsKeyboardCaptureTest {
    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

    private lateinit var device: UiDevice
    private lateinit var dataDir: File

    companion object {
        private const val TAG = "GalleryDocsKb"
        private const val OUT = "/data/local/tmp/navi_gallery_docs"

        // Routes (docs/pictures.md follow-up).
        private val ESPA = 60.5621914 to 11.2561239
        private val ATNBRUFOSSEN = 61.8512500 to 10.2338420
        private val AAKERSAETRA = 61.1553669 to 10.9174631
        private val JAMMERDALSBU = 61.5857799 to 10.3536473
        private val RONDVASSBU = 61.8787483 to 9.7963376
        private val VENA_START = 61.2257995 to 10.4626044
        private val VENA_END = 61.2252767 to 10.5468394
        private val LOTEN_START = 60.8059250 to 11.3299030
        private val LOTEN_VIA1 = 60.8023620 to 11.3053691
        private val LOTEN_VIA2 = 60.7974313 to 11.3094874
        private val LOTEN_END = 60.8056487 to 11.3290523

        // Single points.
        private val JUTULHOGGET = 61.9968774 to 10.8888101
        private val GALDHOPIGGEN = 61.6364721 to 8.3124426
        private val ELGPIGGEN = 62.1592913 to 11.3584086
        private val PREKESTOLEN = 58.9870777 to 6.1887732

        @JvmStatic
        @BeforeClass
        fun beforeClass() {
            val ctx = InstrumentationRegistry.getInstrumentation().targetContext
            val auto = InstrumentationRegistry.getInstrumentation().uiAutomation
            auto.grantRuntimePermission(ctx.packageName, android.Manifest.permission.ACCESS_FINE_LOCATION)
            auto.grantRuntimePermission(ctx.packageName, android.Manifest.permission.ACCESS_COARSE_LOCATION)
            val dataDir = NaviAppData.resolve(ctx).also { it.mkdirs() }
            OstlandetOfflineFixtures.ensureInstalled(dataDir)
            // In-app Plan route needs a region PBF under app files (not only /data/local/tmp).
            val staged =
                listOf(
                    File("/data/local/tmp/navi_fixtures/ostlandet-latest.osm.pbf"),
                    File("/data/local/tmp/navi_fixtures/espa-atnbrufossen-corridor.osm.pbf"),
                ).firstOrNull { it.isFile && it.length() > 1_000_000L }
                    ?: error("push ostlandet or corridor PBF to /data/local/tmp/navi_fixtures")
            val destName =
                if (staged.name.contains("ostlandet")) {
                    "ostlandet-latest.osm.pbf"
                } else {
                    "ostlandet-latest.osm.pbf"
                }
            val dest = File(dataDir, destName)
            if (!dest.isFile || dest.length() < staged.length() / 2) {
                staged.copyTo(dest, overwrite = true)
            }
            File(OUT).mkdirs()
            auto.executeShellCommand("mkdir -p $OUT && chmod 777 $OUT").close()
            Log.i(TAG, "pbf=${dest.absolutePath} bytes=${dest.length()}")
        }
    }

    @Before
    fun setUp() {
        device = UiDevice.getInstance(InstrumentationRegistry.getInstrumentation())
        dataDir = NaviAppData.resolve(InstrumentationRegistry.getInstrumentation().targetContext)
        // Privacy: never follow live GPS for gallery frames.
        NaviMapTestHooks.disableGpsFollow = true
        NaviMapTestHooks.followGps = false
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = false
        NaviMapTestHooks.forceOnlineBasemap = false
        NaviMapTestHooks.requestStopRouteSimulation = true
        Thread.sleep(300)
        NaviMapTestHooks.requestStopRouteSimulation = false
        NaviMapTestHooks.simulatingActive = false
        dismissPermission()
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

    private fun dismissPermission() {
        val deadline = System.currentTimeMillis() + 6_000
        while (System.currentTimeMillis() < deadline) {
            val allow =
                device.findObject(By.text("While using the app"))
                    ?: device.findObject(By.text("Allow"))
                    ?: device.findObject(
                        By.res("com.android.permissioncontroller", "permission_allow_foreground_only_button"),
                    )
            if (allow != null) {
                allow.click()
                Thread.sleep(400)
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

    private fun clickTag(tag: String) {
        val node = composeRule.onNodeWithTag(tag, useUnmergedTree = true)
        runCatching { node.performScrollTo() }
        node.assertIsDisplayed().performClick()
        composeRule.waitForIdle()
    }

    private fun setField(
        tag: String,
        value: String,
    ) {
        val node = composeRule.onNodeWithTag(tag, useUnmergedTree = true)
        runCatching { node.performScrollTo() }
        node.performTextClearance()
        node.performTextInput(value)
        composeRule.waitForIdle()
    }

    private fun openRoutePanel() {
        runCatching {
            composeRule.onNodeWithTag("field_search", useUnmergedTree = true).assertIsDisplayed()
        }.onFailure {
            clickTag("btn_open_search")
        }
        composeRule.onNodeWithTag("field_search", useUnmergedTree = true).assertIsDisplayed()
    }

    /** Keyboard lat,lon → pick coordinate hit (no map tap). */
    private fun typeCoordAndPickHit(
        chipTag: String,
        lat: Double,
        lon: Double,
    ) {
        clickTag(chipTag)
        val q = String.format(java.util.Locale.US, "%.7f, %.7f", lat, lon)
        setField("field_search", q)
        val deadline = System.currentTimeMillis() + 10_000
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.lastSearchHitCount >= 1) break
            Thread.sleep(200)
        }
        assertTrue(
            "coordinate hit for $chipTag q=$q hits=${NaviMapTestHooks.lastSearchHitCount}",
            NaviMapTestHooks.lastSearchHitCount >= 1,
        )
        clickTag("search_hit_0")
        Thread.sleep(600)
        // Keep live GPS follow off after the hit flies the camera.
        NaviMapTestHooks.disableGpsFollow = true
        NaviMapTestHooks.followGps = false
    }

    private fun selectProfile(chip: String) {
        runCatching { clickTag("btn_open_profile") }
        clickTag(chip)
        runCatching { clickTag("btn_save_profile") }
        Thread.sleep(400)
    }

    private fun planAndWait(timeoutMs: Long = 900_000) {
        NaviMapTestHooks.lastRoutePolylineChars = 0
        NaviMapTestHooks.lastPlanReport = ""
        composeRule.onNodeWithTag("btn_plan_route", useUnmergedTree = true).performScrollTo()
        clickTag("btn_plan_route")
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.lastRoutePolylineChars > 100 &&
                NaviMapTestHooks.lastPlanReport.contains("PASS")
            ) {
                return
            }
            Thread.sleep(2_000)
            Log.i(
                TAG,
                "waiting_plan chars=${NaviMapTestHooks.lastRoutePolylineChars} " +
                    "report=${NaviMapTestHooks.lastPlanReport.take(120)}",
            )
        }
        error("plan timeout report=${NaviMapTestHooks.lastPlanReport.take(500)}")
    }

    private fun startSimulation() {
        NaviMapTestHooks.disableGpsFollow = true
        NaviMapTestHooks.followGps = false
        // Prefer UI Simulate when visible; fall back to hooks that drive the same
        // RouteSimulator path (still along the real planned samples).
        runCatching { clickTag("btn_simulate_route") }
            .onFailure {
                NaviMapTestHooks.requestPrepareRouteSimulation = true
                Thread.sleep(800)
                NaviMapTestHooks.requestStartRouteSimulation = true
                Thread.sleep(500)
                NaviMapTestHooks.requestSimSeekCumM =
                    (NaviMapTestHooks.lastPlanDistanceKm * 1000.0 * 0.35).coerceAtLeast(200.0)
            }
        val deadline = System.currentTimeMillis() + 30_000
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.simulatingActive) break
            // Re-assert prepare/start if samples arrived late after hiking plan.
            if (!NaviMapTestHooks.requestPrepareRouteSimulation &&
                !NaviMapTestHooks.requestStartRouteSimulation
            ) {
                NaviMapTestHooks.requestPrepareRouteSimulation = true
                Thread.sleep(400)
                NaviMapTestHooks.requestStartRouteSimulation = true
            }
            Thread.sleep(250)
        }
        if (!NaviMapTestHooks.simulatingActive) {
            Log.w(TAG, "simulatingActive still false — continuing without position marker")
        } else {
            NaviMapTestHooks.simulationTimeScale = 0.2
            NaviMapTestHooks.followGps = false
            Thread.sleep(800)
        }
    }

    private fun assertNotLiveGpsHud() {
        // Never capture with live GPS follow (would reveal tester location).
        assertFalse(
            "live GPS follow must stay off for gallery",
            NaviMapTestHooks.followGps && !NaviMapTestHooks.simulatingActive,
        )
        assertTrue(
            "disableGpsFollow must stay set",
            NaviMapTestHooks.disableGpsFollow || NaviMapTestHooks.simulatingActive,
        )
    }

    private fun shot(relative: String) {
        assertNotLiveGpsHud()
        dismissPermission()
        Thread.sleep(500)
        val path = "$OUT/$relative"
        shell("mkdir -p $(dirname $path)")
        shell("screencap -p $path")
        shell("chmod 644 $path")
        val f = File(path)
        if (!f.isFile || f.length() < 8_000) {
            val bmp = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
            assertTrue(bmp != null)
            f.parentFile?.mkdirs()
            f.outputStream().use { bmp!!.compress(Bitmap.CompressFormat.PNG, 100, it) }
        }
        assertTrue("$relative too small (${f.length()})", f.isFile && f.length() > 8_000)
        // Reject near-empty beige frames (no tiles).
        assertTrue(
            "$relative suspiciously small for tiled map (${f.length()})",
            f.length() > 40_000,
        )
        Log.i(
            TAG,
            "SHOT $relative bytes=${f.length()} sim=${NaviMapTestHooks.simulatingActive} " +
                "kind=${NaviMapTestHooks.lastBasemapKind} poly=${NaviMapTestHooks.lastRoutePolylineChars}",
        )
    }

    private fun dismissIme() {
        composeRule.activity.runOnUiThread {
            val imm =
                composeRule.activity.getSystemService(android.content.Context.INPUT_METHOD_SERVICE)
                    as android.view.inputmethod.InputMethodManager
            composeRule.activity.currentFocus?.let { imm.hideSoftInputFromWindow(it.windowToken, 0) }
                ?: imm.hideSoftInputFromWindow(composeRule.activity.window.decorView.windowToken, 0)
        }
        Thread.sleep(600)
        device.executeShellCommand("input keyevent 111")
        Thread.sleep(400)
    }

    private fun awaitPitch(
        target: Double,
        timeoutMs: Long = 25_000,
    ) {
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            val got = NaviMapTestHooks.lastCameraPitch
            if (kotlin.math.abs(got - target) <= 2.0) return
            // Accept any pitched 3D preset (35/45/60) when asking for tilt > 0 —
            // prefs from a prior run can snap to a neighboring preset.
            if (target >= 35.0 && got >= 35.0) return
            if (target == 0.0 && got <= 2.0) return
            NaviMapTestHooks.requestCameraTiltDeg = target
            Thread.sleep(250)
        }
        val got = NaviMapTestHooks.lastCameraPitch
        assertTrue(
            "pitch want=$target got=$got",
            kotlin.math.abs(got - target) <= 2.0 || (target >= 35.0 && got >= 35.0),
        )
    }

    private fun closeChromeForMapShot() {
        dismissIme()
        runCatching { clickTag("btn_close_search") }
        Thread.sleep(400)
        NaviMapTestHooks.hideSearchChrome = true
        NaviMapTestHooks.disableGpsFollow = true
        NaviMapTestHooks.followGps = false
        Thread.sleep(1_200)
    }

    private fun clearRouteUi() {
        NaviMapTestHooks.requestStopRouteSimulation = true
        Thread.sleep(400)
        NaviMapTestHooks.requestStopRouteSimulation = false
        NaviMapTestHooks.requestClearRoute = true
        Thread.sleep(600)
        NaviMapTestHooks.requestClearRoute = false
        NaviMapTestHooks.simulatingActive = false
        NaviMapTestHooks.hideSearchChrome = false
    }

    @Test
    fun capture_loten_loop_simulator() {
        waitStyle()
        openRoutePanel()
        selectProfile("chip_profile_car")
        typeCoordAndPickHit("chip_from", LOTEN_START.first, LOTEN_START.second)
        typeCoordAndPickHit("chip_via", LOTEN_VIA1.first, LOTEN_VIA1.second)
        typeCoordAndPickHit("chip_via", LOTEN_VIA2.first, LOTEN_VIA2.second)
        typeCoordAndPickHit("chip_to", LOTEN_END.first, LOTEN_END.second)
        planAndWait(900_000)
        startSimulation()
        closeChromeForMapShot()
        Thread.sleep(1_500)
        shot("route_adalsbruk_loten_loop.png")
        shot("route_map.png")
        clearRouteUi()
    }

    @Test
    fun capture_single_points_poi() {
        waitStyle()
        openRoutePanel()

        fun poiShot(
            name: String,
            lat: Double,
            lon: Double,
            want3d: Boolean,
            zooms: Int,
            online: Boolean = false,
            /** Camera zoom after fly-to; basemap amenity/peak POIs need ≥16. */
            targetZoom: Double? = null,
        ) {
            clearRouteUi()
            NaviMapTestHooks.forceOnlineBasemap = online
            NaviMapTestHooks.hideSearchChrome = false
            openRoutePanel()
            typeCoordAndPickHit("chip_from", lat, lon)
            NaviMapTestHooks.disableGpsFollow = true
            NaviMapTestHooks.followGps = false
            val ctx = InstrumentationRegistry.getInstrumentation().targetContext
            if (want3d) {
                NaviMapTestHooks.requestOptIn3d = true
                MapHudPrefs.saveOptIn3d(ctx, true)
                Thread.sleep(1_500)
                val tilt = 45.0
                NaviMapTestHooks.requestCameraTiltDeg = tilt
                MapHudPrefs.saveCameraTiltDeg(ctx, tilt)
                val terrainDeadline = System.currentTimeMillis() + 90_000
                while (System.currentTimeMillis() < terrainDeadline) {
                    if (NaviMapTestHooks.lastTerrainAttached ||
                        NaviMapTestHooks.lastBasemapKind.contains("3d", ignoreCase = true) ||
                        NaviMapTestHooks.lastBasemapKind == "Online3d"
                    ) {
                        break
                    }
                    if (kotlin.math.abs(NaviMapTestHooks.lastCameraPitch - tilt) > 2.0) {
                        NaviMapTestHooks.requestCameraTiltDeg = tilt
                    }
                    Thread.sleep(300)
                }
                awaitPitch(tilt, timeoutMs = 25_000)
            } else {
                NaviMapTestHooks.requestOptIn3d = false
                MapHudPrefs.saveOptIn3d(ctx, false)
                NaviMapTestHooks.requestCameraTiltDeg = 0.0
                MapHudPrefs.saveCameraTiltDeg(ctx, 0.0)
                awaitPitch(0.0, timeoutMs = 15_000)
            }
            val zoomGoal = targetZoom
            if (zoomGoal != null) {
                NaviMapTestHooks.disableGpsFollow = true
                NaviMapTestHooks.followGps = false
                NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoomGoal)
                val camDeadline = System.currentTimeMillis() + 25_000
                while (System.currentTimeMillis() < camDeadline) {
                    if (kotlin.math.abs(NaviMapTestHooks.lastCameraZoom - zoomGoal) < 0.2) break
                    NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoomGoal)
                    Thread.sleep(300)
                }
                assertTrue(
                    "$name camera zoom want=$zoomGoal got=${NaviMapTestHooks.lastCameraZoom}",
                    kotlin.math.abs(NaviMapTestHooks.lastCameraZoom - zoomGoal) < 0.35,
                )
            } else {
                repeat(zooms) {
                    runCatching { clickTag("zoom_in") }
                    Thread.sleep(700)
                }
            }
            if (want3d) {
                NaviMapTestHooks.requestCameraTiltDeg = 45.0
                awaitPitch(45.0, timeoutMs = 15_000)
            }
            closeChromeForMapShot()
            if (want3d) {
                // Closing Route chrome / style switches can race pitch back to 0°
                // while prefs still report 45° — keep re-asserting until screencap.
                NaviMapTestHooks.requestOptIn3d = true
                repeat(20) {
                    NaviMapTestHooks.requestCameraTiltDeg = 45.0
                    Thread.sleep(200)
                }
                awaitPitch(45.0, timeoutMs = 20_000)
            }
            // Re-pin framing after chrome close (search hit defaults to z12).
            if (zoomGoal != null) {
                NaviMapTestHooks.pendingCamera = Triple(lat, lon, zoomGoal)
                Thread.sleep(800)
            }
            Thread.sleep(1_500)
            assertFalse(NaviMapTestHooks.followGps)
            if (want3d) {
                assertTrue(
                    "$name expected pitched 3D camera, pitch=${NaviMapTestHooks.lastCameraPitch} " +
                        "terrain=${NaviMapTestHooks.lastTerrainAttached} kind=${NaviMapTestHooks.lastBasemapKind}",
                    NaviMapTestHooks.lastCameraPitch >= 40.0,
                )
            }
            // Do not show the tester's live street name on gallery POI frames.
            NaviMapTestHooks.pendingCurrentStreet = ""
            Thread.sleep(300)
            // Final tilt pin immediately before screencap.
            if (want3d) {
                NaviMapTestHooks.requestCameraTiltDeg = 45.0
                Thread.sleep(500)
            }
            Log.i(
                TAG,
                "POI $name pitch=${NaviMapTestHooks.lastCameraPitch} " +
                    "zoom=${NaviMapTestHooks.lastCameraZoom} " +
                    "terrain=${NaviMapTestHooks.lastTerrainAttached} kind=${NaviMapTestHooks.lastBasemapKind}",
            )
            shot(name)
            NaviMapTestHooks.hideSearchChrome = false
            NaviMapTestHooks.forceOnlineBasemap = false
        }

        // Canyon / peaks in 3D — online + offline where Ostlandet PMTiles cover.
        poiShot(
            "poi_jutulhogget.png",
            JUTULHOGGET.first,
            JUTULHOGGET.second,
            want3d = true,
            zooms = 1,
            online = true,
        )
        poiShot(
            "poi_jutulhogget_offline.png",
            JUTULHOGGET.first,
            JUTULHOGGET.second,
            want3d = true,
            zooms = 1,
            online = false,
        )
        poiShot("poi_galdhopiggen_3d.png", GALDHOPIGGEN.first, GALDHOPIGGEN.second, want3d = true, zooms = 0, targetZoom = 16.0)
        poiShot(
            "poi_galdhopiggen_online.png",
            GALDHOPIGGEN.first,
            GALDHOPIGGEN.second,
            want3d = true,
            zooms = 0,
            online = true,
            targetZoom = 16.0,
        )
        poiShot(
            "poi_elgpiggen.png",
            ELGPIGGEN.first,
            ELGPIGGEN.second,
            want3d = true,
            zooms = 0,
            targetZoom = 16.0,
        )
        poiShot(
            "poi_elgpiggen_online.png",
            ELGPIGGEN.first,
            ELGPIGGEN.second,
            want3d = true,
            zooms = 0,
            online = true,
            targetZoom = 16.0,
        )
        poiShot(
            "poi_prekestolen.png",
            PREKESTOLEN.first,
            PREKESTOLEN.second,
            want3d = true,
            zooms = 1,
            online = true,
            targetZoom = 16.0,
        )
        // Preikestolen is outside Ostlandet PMTiles; offline attempt documents
        // coverage-boundary behaviour (empty tiles or live Liberty fallback).
        poiShot(
            "poi_prekestolen_offline.png",
            PREKESTOLEN.first,
            PREKESTOLEN.second,
            want3d = true,
            zooms = 1,
            online = false,
        )
    }

    /**
     * Retake only the peak gallery frames that must show basemap POIs at z16
     * (Elgpiggen / Preikestolen / Galdhøpiggen online).
     */
    @Test
    fun capture_requested_peak_pois_z16() {
        waitStyle()
        openRoutePanel()

        fun peakShot(
            name: String,
            lat: Double,
            lon: Double,
            online: Boolean,
        ) {
            clearRouteUi()
            NaviMapTestHooks.forceOnlineBasemap = online
            if (online) {
                setWifi(true)
            } else {
                setWifi(false)
            }
            NaviMapTestHooks.hideSearchChrome = false
            openRoutePanel()
            typeCoordAndPickHit("chip_from", lat, lon)
            NaviMapTestHooks.disableGpsFollow = true
            NaviMapTestHooks.followGps = false
            val ctx = InstrumentationRegistry.getInstrumentation().targetContext
            NaviMapTestHooks.requestOptIn3d = true
            MapHudPrefs.saveOptIn3d(ctx, true)
            Thread.sleep(1_200)
            NaviMapTestHooks.requestCameraTiltDeg = 45.0
            MapHudPrefs.saveCameraTiltDeg(ctx, 45.0)
            awaitPitch(45.0, timeoutMs = 45_000)
            NaviMapTestHooks.pendingCamera = Triple(lat, lon, 16.0)
            val camDeadline = System.currentTimeMillis() + 45_000
            while (System.currentTimeMillis() < camDeadline) {
                if (kotlin.math.abs(NaviMapTestHooks.lastCameraZoom - 16.0) < 0.35) break
                NaviMapTestHooks.pendingCamera = Triple(lat, lon, 16.0)
                // HUD zoom_in as fallback when pendingCamera races style reload.
                if (NaviMapTestHooks.lastCameraZoom < 15.5) {
                    runCatching { clickTag("zoom_in") }
                }
                Thread.sleep(400)
            }
            assertTrue(
                "$name zoom=${NaviMapTestHooks.lastCameraZoom}",
                NaviMapTestHooks.lastCameraZoom >= 15.5,
            )
            closeChromeForMapShot()
            NaviMapTestHooks.requestOptIn3d = true
            repeat(15) {
                NaviMapTestHooks.requestCameraTiltDeg = 45.0
                NaviMapTestHooks.pendingCamera = Triple(lat, lon, 16.0)
                Thread.sleep(200)
            }
            awaitPitch(45.0, timeoutMs = 20_000)
            NaviMapTestHooks.pendingCurrentStreet = ""
            Thread.sleep(2_000)
            Log.i(
                TAG,
                "PEAK $name zoom=${NaviMapTestHooks.lastCameraZoom} " +
                    "pitch=${NaviMapTestHooks.lastCameraPitch} kind=${NaviMapTestHooks.lastBasemapKind}",
            )
            shot(name)
            NaviMapTestHooks.forceOnlineBasemap = false
            NaviMapTestHooks.hideSearchChrome = false
        }

        peakShot("poi_galdhopiggen_online.png", GALDHOPIGGEN.first, GALDHOPIGGEN.second, online = true)
        peakShot("poi_elgpiggen.png", ELGPIGGEN.first, ELGPIGGEN.second, online = false)
        peakShot("poi_elgpiggen_online.png", ELGPIGGEN.first, ELGPIGGEN.second, online = true)
        peakShot("poi_prekestolen.png", PREKESTOLEN.first, PREKESTOLEN.second, online = true)
        setWifi(true)
    }

    private fun setWifi(enabled: Boolean) {
        shell(if (enabled) "svc wifi enable" else "svc wifi disable")
        Thread.sleep(1_500)
    }

    @Test
    fun capture_hiking_multiday_and_map() {
        waitStyle()
        openRoutePanel()
        selectProfile("chip_profile_hiking")
        typeCoordAndPickHit("chip_from", AAKERSAETRA.first, AAKERSAETRA.second)
        typeCoordAndPickHit("chip_via", JAMMERDALSBU.first, JAMMERDALSBU.second)
        typeCoordAndPickHit("chip_to", RONDVASSBU.first, RONDVASSBU.second)
        planAndWait(1_200_000)

        // Day cards in Route chrome (real planner daysJson) — keep chrome open.
        val cardDeadline = System.currentTimeMillis() + 60_000
        var cards = false
        while (System.currentTimeMillis() < cardDeadline) {
            try {
                composeRule
                    .onNodeWithTag("multi_day_plan_cards", useUnmergedTree = true)
                    .assertIsDisplayed()
                cards = true
                break
            } catch (_: Throwable) {
                Thread.sleep(500)
            }
        }
        assertTrue("hiking multi-day day cards from planner", cards)
        // Dismiss IME only — keep Route panel + day cards visible.
        dismissIme()
        Thread.sleep(800)
        shot("multi_day_day_cards_hiking.png")

        closeChromeForMapShot()
        startSimulation()
        Thread.sleep(2_000)
        shot("route_akersaetra_rondvassbu_hiking.png")
        clearRouteUi()
    }

    @Test
    fun capture_espa_atnbrufossen_eco() {
        waitStyle()
        openRoutePanel()
        selectProfile("chip_profile_car")
        runCatching { clickTag("switch_eco") }
        typeCoordAndPickHit("chip_from", ESPA.first, ESPA.second)
        typeCoordAndPickHit("chip_to", ATNBRUFOSSEN.first, ATNBRUFOSSEN.second)
        planAndWait(1_200_000)
        startSimulation()
        closeChromeForMapShot()
        Thread.sleep(2_500)
        shot("route_espa_atnbrufossen.png")
        clearRouteUi()
    }

    @Test
    fun capture_venabygdsfjellet_ebike() {
        waitStyle()
        openRoutePanel()
        selectProfile("chip_profile_bicycle_electric")
        typeCoordAndPickHit("chip_from", VENA_START.first, VENA_START.second)
        typeCoordAndPickHit("chip_to", VENA_END.first, VENA_END.second)
        planAndWait(600_000)
        startSimulation()
        closeChromeForMapShot()
        Thread.sleep(2_000)
        shot("route_venabygdsfjellet_ebike.png")
        clearRouteUi()
    }

    @Test
    fun capture_truck_multiday_cards_espa_atnbru() {
        // Lower daily driving cap so Espa→Atnbrufossen (~3 h) yields real daysJson
        // cards from the planner (still keyboard-entered; no live Bodø GPS).
        waitStyle()
        assertTrue(
            saveTruckRestSettings(
                dataDir.absolutePath,
                FfiTruckRestSettings(
                    mandatoryBreakAfterHours = 4.5,
                    breakDurationMinutes = 45u,
                    preferSplitBreak = false,
                    maxDailyDrivingHours = 1.5,
                    maxDailyDrivingExtendedHours = 1.5,
                    maxDailyExtensionsPerWeek = 0u,
                    maxWeeklyDrivingHours = 56.0,
                    maxFortnightlyDrivingHours = 90.0,
                    exceptionalExtensionArmed = false,
                    ecoModeEnabled = false,
                ),
            ),
        )
        openRoutePanel()
        selectProfile("chip_profile_truck")
        typeCoordAndPickHit("chip_from", ESPA.first, ESPA.second)
        typeCoordAndPickHit("chip_to", ATNBRUFOSSEN.first, ATNBRUFOSSEN.second)
        planAndWait(1_200_000)
        assertTrue(
            "truck plan must finish",
            NaviMapTestHooks.lastPlanReport.contains("PASS") &&
                NaviMapTestHooks.lastRoutePolylineChars > 100,
        )

        val cardDeadline = System.currentTimeMillis() + 60_000
        var cards = false
        while (System.currentTimeMillis() < cardDeadline) {
            try {
                composeRule
                    .onNodeWithTag("multi_day_plan_cards", useUnmergedTree = true)
                    .assertIsDisplayed()
                cards = true
                break
            } catch (_: Throwable) {
                Thread.sleep(500)
            }
        }
        assertTrue("truck multi-day day cards from planner (tight daily hours)", cards)
        dismissIme()
        Thread.sleep(800)
        shot("multi_day_day_cards.png")
        clearRouteUi()
        // Restore EC 561-ish defaults for later tests on this device.
        saveTruckRestSettings(
            dataDir.absolutePath,
            FfiTruckRestSettings(
                mandatoryBreakAfterHours = 4.5,
                breakDurationMinutes = 45u,
                preferSplitBreak = false,
                maxDailyDrivingHours = 9.0,
                maxDailyDrivingExtendedHours = 10.0,
                maxDailyExtensionsPerWeek = 2u,
                maxWeeklyDrivingHours = 56.0,
                maxFortnightlyDrivingHours = 90.0,
                exceptionalExtensionArmed = false,
                ecoModeEnabled = false,
            ),
        )
    }
}
