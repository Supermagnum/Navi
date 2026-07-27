package no.navi.app

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.BeforeClass
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.CorridorRouteResult
import uniffi.navi.FfiIconTheme
import uniffi.navi.rasterizeIconPng
import uniffi.navi.runCarCorridorPipeline
import java.io.File

/**
 * Real Espa → Atnbrufossen corridor on-device, with drive HUD / menus visible.
 * No synthetic polylines — DATA_SOURCE=real_pbf only.
 */
@RunWith(AndroidJUnit4::class)
class RouteHudScreenshotTest {
    companion object {
        @JvmStatic
        lateinit var planned: CorridorRouteResult

        @JvmStatic
        lateinit var iconPng: ByteArray

        @JvmStatic
        @BeforeClass
        fun provisionRealCorridor() {
            val context = InstrumentationRegistry.getInstrumentation().targetContext
            val dataDir = NaviAppData.resolve(context)
            val iconsDir =
                File(context.filesDir, "icons").also { dest ->
                    dest.mkdirs()

                    fun copyAssetTree(
                        assetPath: String,
                        outDir: File,
                    ) {
                        outDir.mkdirs()
                        val am = context.assets
                        val children = am.list(assetPath) ?: return
                        for (name in children) {
                            val childAsset = if (assetPath.isEmpty()) name else "$assetPath/$name"
                            val childOut = File(outDir, name)
                            val sub = am.list(childAsset)
                            if (sub != null && sub.isNotEmpty()) {
                                copyAssetTree(childAsset, childOut)
                            } else if (!childOut.exists()) {
                                am.open(childAsset).use { input ->
                                    childOut.outputStream().use { output -> input.copyTo(output) }
                                }
                            }
                        }
                    }
                    copyAssetTree("icons", dest)
                }
            val url =
                System.getProperty("navi.fixture.pbf.url")
                    ?: InstrumentationRegistry.getArguments().getString("navi.fixture.pbf.url")
                    ?: "http://10.0.2.2:8765/espa-atnbrufossen-corridor.osm.pbf"
            val elevTar =
                System.getProperty("navi.fixture.elev.url")
                    ?: InstrumentationRegistry.getArguments().getString("navi.fixture.elev.url")
                    ?: "http://10.0.2.2:8765/elevation-corridor.tar"

            val pbf = File(dataDir, "espa-atnbrufossen-corridor.osm.pbf")
            val stagedPbf = File("/data/local/tmp/navi_fixtures/espa-atnbrufossen-corridor.osm.pbf")
            val stagedTar = File("/data/local/tmp/navi_fixtures/elevation-corridor.tar")
            check(stagedPbf.isFile) { "missing staged PBF at ${stagedPbf.absolutePath}" }
            check(stagedTar.isFile) { "missing staged elev tar at ${stagedTar.absolutePath}" }
            stagedPbf.copyTo(pbf, overwrite = true)

            // Unpack DEM with app-uid `tar` into app files (shell cannot write user-10
            // app-specific storage; UiAutomation shell extract alone is not enough).
            File(dataDir, "elevation").deleteRecursively()
            val tarProc =
                ProcessBuilder(
                    "tar",
                    "-xf",
                    stagedTar.absolutePath,
                    "-C",
                    dataDir.absolutePath,
                ).redirectErrorStream(true).start()
            val tarOut = tarProc.inputStream.bufferedReader().readText()
            val tarCode = tarProc.waitFor()
            check(tarCode == 0) { "app-uid tar failed ($tarCode): $tarOut" }
            val demSample =
                File(
                    dataDir,
                    "elevation/copernicus/N60E010/Copernicus_DSM_COG_10_N60_00_E010_00_DEM.tif",
                )
            check(demSample.isFile) {
                "DEM fixture missing after app-uid tar: ${demSample.absolutePath}"
            }

            planned =
                runCarCorridorPipeline(
                    pbfPath = pbf.absolutePath,
                    elevDir = File(dataDir, "elevation").absolutePath,
                    cacheDir = File(dataDir, "graph-cache").absolutePath,
                    breakIntervalHours = 1.0,
                )
            check(planned.report.contains("DATA_SOURCE=real_pbf")) { planned.report }
            check(planned.report.contains("PASS")) { planned.report }
            check(planned.distanceKm > 5.0) { "distance ${planned.distanceKm}" }
            check(planned.routePolyline.contains(';')) { "empty polyline" }

            iconPng =
                rasterizeIconPng(
                    key = planned.poiIconKey.ifBlank { "fuel" },
                    theme = FfiIconTheme.DAY,
                    width = 64u,
                    height = 64u,
                    bundledDir = iconsDir.absolutePath,
                )

            NaviMapTestHooks.hideUiChrome = false
            NaviMapTestHooks.hideSearchChrome = true
            NaviMapTestHooks.gpsAltitudeM = 412.0
            runCatching {
                InstrumentationRegistry
                    .getInstrumentation()
                    .uiAutomation
                    .grantRuntimePermission(
                        context.packageName,
                        android.Manifest.permission.ACCESS_FINE_LOCATION,
                    )
            }
        }
    }

    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

    private lateinit var dataDir: File

    @Before
    fun setUp() {
        dataDir =
            (
                NaviAppData.resolve(InstrumentationRegistry.getInstrumentation().targetContext)
            ).also { it.mkdirs() }
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = true
        NaviMapTestHooks.gpsAltitudeM = 412.0
    }

    @Test
    fun realCorridor_hudBars_menus_and_navItems_visible() {
        assertFalse(planned.report.contains("STUB", ignoreCase = true))
        assertTrue(planned.report.contains("TEST_KIND=REAL_PIPELINE"))

        composeRule.waitForIdle()
        // Wait for MapLibre style before injecting the real corridor polyline.
        run {
            val styleDeadline = System.currentTimeMillis() + 45_000
            while (System.currentTimeMillis() < styleDeadline) {
                if (NaviMapTestHooks.styleReady || NaviMapTestHooks.lastReportedLayerCount >= 1) break
                Thread.sleep(400)
            }
        }

        NaviMapTestHooks.requestShowTripEta = true
        NaviMapTestHooks.requestBreakReminders = true
        NaviMapTestHooks.pendingIconPng = iconPng
        NaviMapTestHooks.routeStartLabel = "Espa"
        NaviMapTestHooks.routeEndLabel = "Atnbrufossen"
        NaviMapTestHooks.routeViaLabel = ""
        NaviMapTestHooks.pendingRoute = planned

        var layers = 0
        val deadline = System.currentTimeMillis() + 90_000
        while (System.currentTimeMillis() < deadline) {
            Thread.sleep(500)
            layers = NaviMapTestHooks.lastReportedLayerCount
            val altOk =
                NaviMapTestHooks.lastHudAltitudeM != null &&
                    kotlin.math.abs(NaviMapTestHooks.lastHudAltitudeM!! - 412.0) < 0.5
            if (altOk && NaviMapTestHooks.lastBreakHudVisible) break
            // Re-inject: the poll loop may consume pendingRoute before Compose is ready.
            NaviMapTestHooks.pendingRoute = planned
            NaviMapTestHooks.requestShowTripEta = true
            NaviMapTestHooks.requestBreakReminders = true
        }
        assertTrue(
            "break countdown missing after real route " +
                "(layers=$layers styleReady=${NaviMapTestHooks.styleReady} " +
                "alt=${NaviMapTestHooks.lastHudAltitudeM})",
            NaviMapTestHooks.lastBreakHudVisible,
        )
        assertNotNull(NaviMapTestHooks.lastHudAltitudeM)
        assertTrue(
            "altitude must not be 0 (was ${NaviMapTestHooks.lastHudAltitudeM})",
            kotlin.math.abs(NaviMapTestHooks.lastHudAltitudeM!!) > 0.5,
        )
        assertTrue("break countdown should show with planned route", NaviMapTestHooks.lastBreakHudVisible)
        assertTrue("trip ETA should be enabled", NaviMapTestHooks.lastShowTripEta)

        composeRule.onNodeWithTag("top_drive_hud", useUnmergedTree = true).assertIsDisplayed()
        composeRule.onNodeWithTag("bottom_drive_hud", useUnmergedTree = true).assertIsDisplayed()
        composeRule.onNodeWithText("Alt 412 m", useUnmergedTree = true).assertIsDisplayed()
        composeRule.onNodeWithTag("zoom_in", useUnmergedTree = true).assertIsDisplayed()
        composeRule.onNodeWithTag("zoom_out", useUnmergedTree = true).assertIsDisplayed()

        fun shell(cmd: String) {
            val pfd = InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand(cmd)
            java.io.FileInputStream(pfd.fileDescriptor).use { input ->
                val buf = ByteArray(4096)
                while (input.read(buf) >= 0) {
                }
            }
            pfd.close()
        }

        fun capture(name: String) {
            NaviMapTestHooks.hideUiChrome = false
            Thread.sleep(1_800)
            val shot = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
            assertTrue("null shot $name", shot != null)
            assertNotEquals(0, shot!!.width)
            val out = File(dataDir, name)
            out.outputStream().use { shot.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) }
            assertTrue("$name too small (${out.length()})", out.length() > 10_000)
            shell("screencap -p /data/local/tmp/$name")
            shell("chmod 644 /data/local/tmp/$name")
            android.util.Log.i(
                "RouteHudScreenshotTest",
                "shot=$name alt=${NaviMapTestHooks.lastHudAltitudeM} " +
                    "breakVisible=${NaviMapTestHooks.lastBreakHudVisible} " +
                    "tripEta=${NaviMapTestHooks.lastShowTripEta} " +
                    "mapSettings=${NaviMapTestHooks.mapSettingsOpen} " +
                    "driveSettings=${NaviMapTestHooks.driveSettingsOpen} " +
                    "bytes=${out.length()}",
            )
        }

        // 1) Planned route + both drive HUD bars.
        capture("route_hud_bars.png")
        shell("cp /data/local/tmp/route_hud_bars.png /data/local/tmp/navi_route_map.png")
        shell("cp /data/local/tmp/route_hud_bars.png /data/local/tmp/route_map.png")

        // 2) Map settings menu.
        composeRule.onNodeWithTag("top_drive_hud", useUnmergedTree = true).performClick()
        composeRule.waitForIdle()
        Thread.sleep(800)
        composeRule.onNodeWithTag("map_settings_sheet", useUnmergedTree = true).assertIsDisplayed()
        assertTrue(NaviMapTestHooks.mapSettingsOpen)
        capture("route_map_settings_menu.png")
        composeRule.onNodeWithTag("top_drive_hud", useUnmergedTree = true).performClick()
        composeRule.waitForIdle()
        Thread.sleep(500)

        // 3) Drive settings menu (status area, not zoom buttons).
        composeRule.onNodeWithTag("btn_open_drive_settings", useUnmergedTree = true).performClick()
        composeRule.waitForIdle()
        Thread.sleep(800)
        composeRule.onNodeWithTag("drive_settings_sheet", useUnmergedTree = true).assertIsDisplayed()
        assertTrue(NaviMapTestHooks.driveSettingsOpen)
        capture("route_drive_settings_menu.png")
        composeRule
            .onNodeWithTag("btn_close_drive_settings", useUnmergedTree = true)
            .performScrollTo()
            .performClick()
        composeRule.waitForIdle()
        Thread.sleep(800)
        assertFalse("drive settings should close", NaviMapTestHooks.driveSettingsOpen)

        // 4) Tools / region panel with route still planned (search chrome briefly on).
        NaviMapTestHooks.hideSearchChrome = false
        Thread.sleep(1_000)
        composeRule.onNodeWithTag("search_chrome", useUnmergedTree = true).assertIsDisplayed()
        composeRule
            .onNodeWithTag("btn_tools", useUnmergedTree = true)
            .performScrollTo()
            .performClick()
        composeRule.waitForIdle()
        Thread.sleep(800)
        composeRule.onNodeWithTag("tools_menu", useUnmergedTree = true).assertIsDisplayed()
        composeRule.onNodeWithText("Region", useUnmergedTree = true).assertIsDisplayed()
        // Hide search so Region panel is not covered (same pattern as HudVerification).
        NaviMapTestHooks.hideSearchChrome = true
        Thread.sleep(800)
        assertFalse(NaviMapTestHooks.driveSettingsOpen)
        assertFalse(NaviMapTestHooks.mapSettingsOpen)
        capture("route_tools_menu_open.png")
    }
}
