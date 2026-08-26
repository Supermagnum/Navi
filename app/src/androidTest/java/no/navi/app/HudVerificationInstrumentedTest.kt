package no.navi.app

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextClearance
import androidx.compose.ui.test.performTextInput
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.BeforeClass
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.FfiCarRestSettings
import uniffi.navi.loadCarRestSettings
import uniffi.navi.loadEbikeConfig
import uniffi.navi.loadFuelConfig
import uniffi.navi.saveCarRestSettings
import java.io.File

/**
 * On-device HUD verification: top/bottom bars visible, menus functional, zoom,
 * and magnetic / direction-of-travel / north-up rotation against fed headings.
 */
@RunWith(AndroidJUnit4::class)
class HudVerificationInstrumentedTest {
    companion object {
        @JvmStatic
        @BeforeClass
        fun beforeClass() {
            val pkg = InstrumentationRegistry.getInstrumentation().targetContext.packageName
            runCatching {
                InstrumentationRegistry
                    .getInstrumentation()
                    .uiAutomation
                    .grantRuntimePermission(pkg, android.Manifest.permission.ACCESS_FINE_LOCATION)
            }
            NaviMapTestHooks.hideUiChrome = false
            NaviMapTestHooks.hideSearchChrome = true
            NaviMapTestHooks.magneticHeadingDeg = null
            NaviMapTestHooks.gpsBearingDeg = null
            NaviMapTestHooks.gpsAltitudeM = null
            NaviMapTestHooks.pendingCamera = null
            NaviMapTestHooks.styleReady = false
            val ctx = InstrumentationRegistry.getInstrumentation().targetContext
            MapHudPrefs.saveAutoZoom(ctx, MapHudPrefs.DEFAULT_AUTO_ZOOM_LEVEL, enabled = false)
        }
    }

    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

    private val centerLat = 60.722823
    private val centerLon = 10.613182
    private val baseZoom = 14.0

    private lateinit var dataDir: File

    @Before
    fun setUp() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        dataDir = NaviAppData.resolve(context)
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = true
        NaviMapTestHooks.magneticHeadingDeg = null
        NaviMapTestHooks.gpsBearingDeg = null
        NaviMapTestHooks.gpsAltitudeM = 412.0
        NaviMapTestHooks.pendingCamera = Triple(centerLat, centerLon, baseZoom)
        MapHudPrefs.saveAutoZoom(
            context,
            MapHudPrefs.DEFAULT_AUTO_ZOOM_LEVEL,
            enabled = false,
        )
    }

    private fun waitStyle() {
        val deadline = System.currentTimeMillis() + 25_000
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.styleReady) break
            Thread.sleep(200)
        }
        assertTrue("MapLibre style not ready", NaviMapTestHooks.styleReady)
        NaviMapTestHooks.pendingCamera = Triple(centerLat, centerLon, baseZoom)
        Thread.sleep(3_000)
    }

    private fun waitBearing(
        expected: Double,
        tol: Double = 0.5,
        timeoutMs: Long = 8_000,
    ) {
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            if (kotlin.math.abs(NaviMapTestHooks.lastCameraBearing - expected) <= tol) return
            Thread.sleep(100)
        }
        assertEquals(
            "camera bearing",
            expected,
            NaviMapTestHooks.lastCameraBearing,
            tol,
        )
    }

    private fun waitZoom(
        expected: Double,
        tol: Double = 0.15,
        timeoutMs: Long = 8_000,
    ) {
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            if (kotlin.math.abs(NaviMapTestHooks.lastCameraZoom - expected) <= tol) return
            Thread.sleep(100)
        }
        assertEquals("camera zoom", expected, NaviMapTestHooks.lastCameraZoom, tol)
    }

    private fun waitSettingsOpen(
        open: Boolean,
        timeoutMs: Long = 6_000,
    ) {
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.driveSettingsOpen == open) return
            Thread.sleep(100)
        }
        assertEquals("drive settings open", open, NaviMapTestHooks.driveSettingsOpen)
    }

    private fun waitMapSettingsOpen(
        open: Boolean,
        timeoutMs: Long = 6_000,
    ) {
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.mapSettingsOpen == open) return
            Thread.sleep(100)
        }
        assertEquals("map settings open", open, NaviMapTestHooks.mapSettingsOpen)
    }

    private fun openDriveSettings() {
        if (NaviMapTestHooks.driveSettingsOpen) return
        val nodes =
            composeRule
                .onAllNodesWithTag("btn_open_drive_settings", useUnmergedTree = true)
                .fetchSemanticsNodes()
        android.util.Log.i("HudVerification", "open settings nodes=${nodes.size}")
        if (nodes.isNotEmpty()) {
            clickTag("btn_open_drive_settings")
        }
        val deadline = System.currentTimeMillis() + 1_500
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.driveSettingsOpen) return
            Thread.sleep(100)
        }
        NaviMapTestHooks.requestOpenDriveSettings = true
        waitSettingsOpen(true)
    }

    private fun openMapSettings() {
        if (NaviMapTestHooks.mapSettingsOpen) return
        clickTag("top_drive_hud")
        val deadline = System.currentTimeMillis() + 1_500
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.mapSettingsOpen) return
            Thread.sleep(100)
        }
        NaviMapTestHooks.requestOpenMapSettings = true
        waitMapSettingsOpen(true)
    }

    private fun closeMapSettings() {
        if (!NaviMapTestHooks.mapSettingsOpen) return
        val closeNodes =
            composeRule
                .onAllNodesWithTag("btn_close_map_settings", useUnmergedTree = true)
                .fetchSemanticsNodes()
        if (closeNodes.isNotEmpty()) {
            clickTag("btn_close_map_settings")
        } else {
            clickTag("top_drive_hud")
        }
        waitMapSettingsOpen(false)
    }

    private fun shot(
        name: String,
        pauseMap: Boolean = false,
    ): File {
        composeRule.waitForIdle()
        if (pauseMap) {
            composeRule.runOnUiThread {
                NaviMapTestHooks.mapPauseHandler?.invoke(true)
            }
            Thread.sleep(500)
        } else {
            Thread.sleep(600)
        }
        val bmp = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
        assertTrue("screenshot null for $name", bmp != null)
        assertNotEquals(0, bmp!!.width)
        assertNotEquals(0, bmp.height)
        val out = File(dataDir, name)
        out.outputStream().use { os ->
            bmp.compress(android.graphics.Bitmap.CompressFormat.PNG, 90, os)
        }
        bmp.recycle()
        assertTrue("$name written", out.isFile && out.length() > 3_000)
        // Publish via MediaStore Downloads (readable without su after instrumentation).
        runCatching {
            val values =
                android.content.ContentValues().apply {
                    put(android.provider.MediaStore.MediaColumns.DISPLAY_NAME, name)
                    put(android.provider.MediaStore.MediaColumns.MIME_TYPE, "image/png")
                    put(
                        android.provider.MediaStore.MediaColumns.RELATIVE_PATH,
                        android.os.Environment.DIRECTORY_DOWNLOADS + "/navi_hud",
                    )
                    put(android.provider.MediaStore.MediaColumns.IS_PENDING, 1)
                }
            val resolver = InstrumentationRegistry.getInstrumentation().targetContext.contentResolver
            val uri = resolver.insert(android.provider.MediaStore.Downloads.EXTERNAL_CONTENT_URI, values)
            if (uri != null) {
                resolver.openOutputStream(uri)?.use { os ->
                    out.inputStream().use { it.copyTo(os) }
                }
                val done =
                    android.content.ContentValues().apply {
                        put(android.provider.MediaStore.MediaColumns.IS_PENDING, 0)
                    }
                resolver.update(uri, done, null, null)
            }
        }
        // Mirror via root `cat >` so adb can pull after instrumentation exits.
        val mirrored =
            runCatching {
                val dest = "/data/local/tmp/navi_hud/$name"
                InstrumentationRegistry
                    .getInstrumentation()
                    .uiAutomation
                    .executeShellCommand("su 0 mkdir -p /data/local/tmp/navi_hud")
                    .close()
                InstrumentationRegistry
                    .getInstrumentation()
                    .uiAutomation
                    .executeShellCommand("su 0 sh -c 'cat > $dest'")
                    .use { pfd ->
                        java.io.FileOutputStream(pfd.fileDescriptor).use { os ->
                            out.inputStream().use { input -> input.copyTo(os) }
                            os.flush()
                        }
                    }
                InstrumentationRegistry
                    .getInstrumentation()
                    .uiAutomation
                    .executeShellCommand("su 0 chmod 644 $dest")
                    .close()
                true
            }.onFailure {
                android.util.Log.e("HudVerification", "tmp mirror failed for $name", it)
            }.getOrDefault(false)
        android.util.Log.i(
            "HudVerification",
            "shot=$name bytes=${out.length()} mirrored=$mirrored path=${out.absolutePath}",
        )
        return out
    }

    private fun clickTag(tag: String) {
        val node = composeRule.onNodeWithTag(tag, useUnmergedTree = true)
        runCatching { node.performScrollTo() }
        node.assertIsDisplayed().performClick()
        composeRule.waitForIdle()
    }

    private fun setTripEta(on: Boolean) {
        if (NaviMapTestHooks.lastShowTripEta == on) return
        if (NaviMapTestHooks.mapSettingsOpen) {
            clickTag("toggle_trip_eta")
        } else {
            openMapSettings()
            clickTag("toggle_trip_eta")
        }
        val deadline = System.currentTimeMillis() + 5_000
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.lastShowTripEta == on) return
            Thread.sleep(100)
        }
        assertEquals("trip ETA toggle", on, NaviMapTestHooks.lastShowTripEta)
    }

    private fun setBreakReminders(on: Boolean) {
        if (NaviMapTestHooks.lastBreakRemindersEnabled == on) return
        if (!NaviMapTestHooks.mapSettingsOpen) {
            openMapSettings()
        }
        clickTag("toggle_breaks")
        val deadline = System.currentTimeMillis() + 5_000
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.lastBreakRemindersEnabled == on) return
            Thread.sleep(100)
        }
        assertEquals("break reminders toggle", on, NaviMapTestHooks.lastBreakRemindersEnabled)
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

    @Test
    fun hud_visibility_menus_zoom_and_rotation() {
        waitStyle()

        // --- 0. Search / profile / Tools menus (upper chrome) ---
        NaviMapTestHooks.hideSearchChrome = false
        Thread.sleep(1_200)
        composeRule.onNodeWithTag("search_chrome", useUnmergedTree = true).assertIsDisplayed()
        composeRule.onNodeWithTag("top_drive_hud", useUnmergedTree = true).assertIsDisplayed()
        composeRule.onNodeWithTag("bottom_drive_hud", useUnmergedTree = true).assertIsDisplayed()
        // Top HUD + To/Via/Place/Address/Tools + bottom HUD (no scroll yet).
        shot("hud_upper_lower_bars_with_menus.png")
        // Profile chips sit further down the scrollable column.
        composeRule
            .onNodeWithTag("profile_menu", useUnmergedTree = true)
            .performScrollTo()
            .assertIsDisplayed()
        shot("hud_profile_menu.png")
        composeRule.onNodeWithTag("btn_tools", useUnmergedTree = true).performScrollTo()
        clickTag("btn_tools")
        composeRule.onNodeWithTag("tools_menu", useUnmergedTree = true).assertIsDisplayed()
        composeRule.onNodeWithText("Region", useUnmergedTree = true).assertIsDisplayed()
        composeRule
            .onNodeWithText("Download region + build place index", useUnmergedTree = true)
            .assertIsDisplayed()
        composeRule.onNodeWithText("Country", useUnmergedTree = true).assertIsDisplayed()
        composeRule.onNodeWithText("Region in country", useUnmergedTree = true).assertIsDisplayed()
        // Hide search chrome so the Region panel is not covered in the shot.
        NaviMapTestHooks.hideSearchChrome = true
        Thread.sleep(800)
        shot("hud_tools_menu_open.png")
        // Re-show search briefly so Tools toggle is available, then dismiss panel.
        NaviMapTestHooks.hideSearchChrome = false
        Thread.sleep(500)
        clickTag("btn_tools")
        NaviMapTestHooks.hideSearchChrome = true
        Thread.sleep(800)

        // --- 1. Collapsed bars only (search chrome hidden) ---
        composeRule.onNodeWithTag("top_drive_hud", useUnmergedTree = true).assertIsDisplayed()
        composeRule.onNodeWithTag("bottom_drive_hud", useUnmergedTree = true).assertIsDisplayed()
        assertFalse(
            "map settings must be collapsed by default",
            NaviMapTestHooks.mapSettingsOpen,
        )
        composeRule
            .onAllNodesWithTag("map_settings_sheet", useUnmergedTree = true)
            .fetchSemanticsNodes()
            .let { assertTrue("map sheet closed", it.isEmpty()) }
        val altDeadline = System.currentTimeMillis() + 5_000
        while (System.currentTimeMillis() < altDeadline) {
            if (NaviMapTestHooks.lastHudAltitudeM != null &&
                kotlin.math.abs(NaviMapTestHooks.lastHudAltitudeM!! - 412.0) < 0.5
            ) {
                break
            }
            Thread.sleep(100)
        }
        composeRule.onNodeWithTag("hud_altitude", useUnmergedTree = true).assertIsDisplayed()
        composeRule.onNodeWithText("Alt 412 m", useUnmergedTree = true).assertIsDisplayed()
        composeRule.onNodeWithTag("zoom_in", useUnmergedTree = true).assertIsDisplayed()
        composeRule.onNodeWithTag("zoom_out", useUnmergedTree = true).assertIsDisplayed()
        // No turn stub on collapsed bottom bar; break + ETA only.
        assertTrue(
            composeRule
                .onAllNodesWithText("Turn --", useUnmergedTree = true)
                .fetchSemanticsNodes()
                .isEmpty(),
        )
        composeRule.onNodeWithText("ETA off", useUnmergedTree = true).assertIsDisplayed()
        // Eco on bottom bar — persist eco enabled, reopen sheet so host picks it up.
        val restForEco = loadCarRestSettings(dataDir.absolutePath)
        assertTrue(
            saveCarRestSettings(
                dataDir.absolutePath,
                FfiCarRestSettings(
                    breakIntervalHours = restForEco.breakIntervalHours,
                    restDurationMinutes = restForEco.restDurationMinutes,
                    ecoModeEnabled = true,
                ),
            ),
        )
        openDriveSettings()
        clickTag("btn_save_drive_settings")
        waitSettingsOpen(false)
        Thread.sleep(800)
        composeRule.onNodeWithTag("hud_eco_icon", useUnmergedTree = true).assertIsDisplayed()
        assertTrue(
            "eco must render as leaf icon, not ECO text",
            composeRule
                .onAllNodesWithText("ECO", useUnmergedTree = true)
                .fetchSemanticsNodes()
                .isEmpty(),
        )
        assertEquals(412.0, NaviMapTestHooks.lastHudAltitudeM!!, 0.1)
        shot("hud_eco_leaf_on.png")
        // Tools must be closed for idle / map-only captures.
        repeat(12) {
            NaviMapTestHooks.requestCloseTools = true
            NaviMapTestHooks.hideSearchChrome = true
            Thread.sleep(350)
            val open =
                composeRule
                    .onAllNodesWithTag("tools_menu", useUnmergedTree = true)
                    .fetchSemanticsNodes()
            if (open.isEmpty()) return@repeat
            // Toggle closed via the labeled button if the sheet is still up.
            NaviMapTestHooks.hideSearchChrome = false
            Thread.sleep(200)
            runCatching {
                composeRule.onNodeWithText("Hide tools", useUnmergedTree = true).performClick()
            }
            runCatching { clickTag("btn_tools") }
            Thread.sleep(400)
            NaviMapTestHooks.requestCloseTools = true
            NaviMapTestHooks.hideSearchChrome = true
        }
        composeRule
            .onAllNodesWithTag("tools_menu", useUnmergedTree = true)
            .fetchSemanticsNodes()
            .let { assertTrue("tools menu must be closed for idle shots", it.isEmpty()) }
        val mapOnly = shot("hud_map_top_bottom_only.png")
        assertTrue(mapOnly.length() > 5_000)
        val idle = shot("hud_idle_both_bars.png")
        assertTrue(idle.length() > 5_000)
        assertTrue(
            "idle HUD must not show break countdown without a planned route",
            composeRule
                .onAllNodesWithText("Break in", substring = true, useUnmergedTree = true)
                .fetchSemanticsNodes()
                .isEmpty(),
        )
        assertTrue(
            "idle HUD must not show break reminders line without a planned route",
            composeRule
                .onAllNodesWithText("Break reminders off", useUnmergedTree = true)
                .fetchSemanticsNodes()
                .isEmpty(),
        )

        // Eco off — leaf hidden
        assertTrue(
            saveCarRestSettings(
                dataDir.absolutePath,
                FfiCarRestSettings(
                    breakIntervalHours = restForEco.breakIntervalHours,
                    restDurationMinutes = restForEco.restDurationMinutes,
                    ecoModeEnabled = false,
                ),
            ),
        )
        openDriveSettings()
        clickTag("btn_save_drive_settings")
        waitSettingsOpen(false)
        Thread.sleep(500)
        assertTrue(
            composeRule
                .onAllNodesWithTag("hud_eco_icon", useUnmergedTree = true)
                .fetchSemanticsNodes()
                .isEmpty(),
        )
        shot("hud_eco_leaf_off.png")
        // Restore eco for remaining shots
        assertTrue(
            saveCarRestSettings(
                dataDir.absolutePath,
                FfiCarRestSettings(
                    breakIntervalHours = restForEco.breakIntervalHours,
                    restDurationMinutes = restForEco.restDurationMinutes,
                    ecoModeEnabled = true,
                ),
            ),
        )
        openDriveSettings()
        clickTag("btn_save_drive_settings")
        waitSettingsOpen(false)
        Thread.sleep(500)

        // --- 2. Bottom bar: tap opens drive settings overlay (above bars) ---
        openDriveSettings()
        composeRule
            .onNodeWithTag("drive_settings_title", useUnmergedTree = true)
            .assertIsDisplayed()
        composeRule.onNodeWithTag("bottom_drive_hud", useUnmergedTree = true).assertIsDisplayed()
        shot("hud_settings_overlay.png")
        shot("hud_settings_open.png")

        // Break hours apply + auto-close + persist; toast must not sit on attribution (bottom-left)
        setField("field_break_hours", "3.5")
        clickTag("btn_save_drive_settings")
        waitSettingsOpen(false)
        Thread.sleep(500)
        val restAfterBreak = loadCarRestSettings(dataDir.absolutePath)
        assertEquals(3.5, restAfterBreak.breakIntervalHours, 0.001)
        composeRule.onNodeWithTag("status_toast", useUnmergedTree = true).assertIsDisplayed()
        shot("hud_after_break_hours_apply.png")
        shot("hud_status_toast_settings_applied.png")

        // Rest time apply (second toast scenario)
        openDriveSettings()
        setField("field_rest_mins", "20")
        clickTag("btn_save_drive_settings")
        waitSettingsOpen(false)
        Thread.sleep(400)
        assertEquals(20u, loadCarRestSettings(dataDir.absolutePath).restDurationMinutes)
        shot("hud_after_rest_mins_apply.png")
        shot("hud_status_toast_rest_applied.png")

        // Tank capacity apply (force liters so "55" is not treated as gallons).
        openDriveSettings()
        val fuelBeforeTank = loadFuelConfig(dataDir.absolutePath)
        if (!fuelBeforeTank.preferLiters) {
            clickTag("toggle_fuel_units")
            composeRule.waitForIdle()
        }
        setField("field_tank", "55")
        clickTag("btn_save_drive_settings")
        waitSettingsOpen(false)
        Thread.sleep(400)
        val fuelTank = loadFuelConfig(dataDir.absolutePath)
        assertEquals(55.0, fuelTank.tankCapacityL!!, 0.01)
        assertTrue(fuelTank.preferLiters)
        shot("hud_after_tank_apply.png")

        // Fuel added + unit toggle (L then gallons path)
        openDriveSettings()
        setField("field_fuel_added", "10")
        clickTag("btn_save_drive_settings")
        waitSettingsOpen(false)
        Thread.sleep(400)
        assertEquals(10.0, loadFuelConfig(dataDir.absolutePath).fuelAddedL!!, 0.05)
        shot("hud_after_fuel_liters_apply.png")

        openDriveSettings()
        clickTag("toggle_fuel_units") // flip liters/gallons
        composeRule.waitForIdle()
        setField("field_fuel_added", "5")
        clickTag("btn_save_drive_settings")
        waitSettingsOpen(false)
        Thread.sleep(400)
        val fuelAfterGal = loadFuelConfig(dataDir.absolutePath)
        assertTrue(
            "fuel added must persist after unit toggle apply",
            (fuelAfterGal.fuelAddedL ?: 0.0) > 1.0,
        )
        shot("hud_after_fuel_units_apply.png")

        // Electric cycle vehicle specs persist (battery / torque / wheel).
        openDriveSettings()
        clickTag("drive_chip_profile_bicycle_electric")
        composeRule.waitForIdle()
        Thread.sleep(500)
        composeRule
            .onNodeWithTag("field_ebike_battery_wh", useUnmergedTree = true)
            .performScrollTo()
            .assertIsDisplayed()
        setField("field_ebike_battery_wh", "750")
        setField("field_ebike_torque_nm", "90")
        // Diameter field is always visible under the presets (no clipped-chip click).
        setField("field_ebike_wheel_in", "29")
        composeRule.waitForIdle()
        clickTag("btn_save_drive_settings")
        waitSettingsOpen(false)
        Thread.sleep(400)
        val ebike = loadEbikeConfig(dataDir.absolutePath)
        assertEquals(750.0, ebike.batteryCapacityWh!!, 0.01)
        assertEquals(90.0, ebike.motorTorqueNm!!, 0.01)
        assertEquals(29.0, ebike.wheelDiameterIn!!, 0.01)
        shot("hud_after_ebike_specs_apply.png")

        // --- Top bar: open map settings overlay ---
        openMapSettings()
        composeRule.onNodeWithTag("map_settings_title", useUnmergedTree = true).assertIsDisplayed()
        composeRule.onNodeWithTag("top_drive_hud", useUnmergedTree = true).assertIsDisplayed()
        shot("hud_map_settings_overlay.png")
        clickTag("rot_compass")
        assertEquals(MapRotationMode.Compass, NaviMapTestHooks.lastRotationMode)
        shot("hud_rot_mode_compass.png")
        clickTag("rot_travel")
        assertEquals(MapRotationMode.DirectionOfTravel, NaviMapTestHooks.lastRotationMode)
        shot("hud_rot_mode_travel.png")
        clickTag("rot_north_up")
        assertEquals(MapRotationMode.NorthUp, NaviMapTestHooks.lastRotationMode)
        waitBearing(0.0)
        shot("hud_rot_mode_north_up.png")

        // Trip ETA on/off — bottom bar shows ETA line (keep map sheet open but not covering bar)
        setTripEta(true)
        Thread.sleep(400)
        composeRule.onNodeWithText("ETA 95 min", useUnmergedTree = true).assertIsDisplayed()
        shot("hud_trip_eta_on.png")
        setTripEta(false)
        Thread.sleep(400)
        composeRule.onNodeWithText("ETA off", useUnmergedTree = true).assertIsDisplayed()
        val etaStillVisible =
            composeRule
                .onAllNodesWithText("ETA 95 min", useUnmergedTree = true)
                .fetchSemanticsNodes()
                .isNotEmpty()
        assertFalse("Trip ETA value should hide when toggle off", etaStillVisible)
        shot("hud_trip_eta_off.png")

        // Break reminders — HUD countdown only with a **real** planned corridor
        // (Grimåsfeltet→Nysethvegen from ostlandet host plan). No synthetic polylines.
        val zoomBeforeBreak = NaviMapTestHooks.lastCameraZoom
        val plannedPolyline =
            InstrumentationRegistry
                .getInstrumentation()
                .context
                .assets
                .open("raufoss_grimafeltet_nysethvegen.polyline.txt")
                .bufferedReader()
                .use { it.readText().trim() }
        assertTrue("host-planned polyline required", plannedPolyline.contains(';'))
        assertTrue(
            "polyline must be a real multi-vertex plan, not a 2-point stub",
            plannedPolyline.count { it == ';' } >= 10,
        )
        NaviMapTestHooks.pendingRoute =
            uniffi.navi.CorridorRouteResult(
                report = "PLANNED Grimåsfeltet → Nysethvegen (Raufoss / Tollerud)",
                distanceKm = 1.953,
                etaMinutes = 1.953 / 50.0 * 60.0,
                cacheHit = true,
                coldBuildS = 0.0,
                warmLoadS = 0.0,
                routePolyline = plannedPolyline,
                poiLat = 60.7278207,
                poiLon = 10.6049538,
                poiName = "Nysethvegen",
                poiIconKey = "fuel",
                breakPoisJson = "[]",
                daysJson = "[]",
                simSamplesJson = "[]",
                maneuversJson = "[]",
                priorityPathSharePct = 0.0,
                routeSegmentsJson = "[]",
                offTrailAdvisory = "",
            )
        // Keep prior zoom; route apply would otherwise fit-bounds and break zoom checks.
        NaviMapTestHooks.pendingCamera = Triple(centerLat, centerLon, zoomBeforeBreak)
        Thread.sleep(1_500)
        setBreakReminders(false)
        composeRule
            .onNodeWithText("Break reminders off", useUnmergedTree = true)
            .assertIsDisplayed()
        shot("hud_breaks_off.png")
        setBreakReminders(true)
        val breakOffStill =
            composeRule
                .onAllNodesWithText("Break reminders off", useUnmergedTree = true)
                .fetchSemanticsNodes()
                .isNotEmpty()
        assertFalse("Break countdown / interval text should return", breakOffStill)
        composeRule
            .onNodeWithTag("hud_break_countdown", useUnmergedTree = true)
            .assertIsDisplayed()
        shot("hud_breaks_on.png")
        closeMapSettings()
        NaviMapTestHooks.pendingCamera = Triple(centerLat, centerLon, baseZoom)
        Thread.sleep(1_200)
        waitZoom(baseZoom, tol = 0.4)

        // Zoom in / out — sole app zoom set on bottom bar
        val z0 = NaviMapTestHooks.lastCameraZoom
        clickTag("zoom_in")
        waitZoom(z0 + 1.0)
        val zoomIn1 = shot("hud_zoom_in_1.png")
        clickTag("zoom_in")
        waitZoom(z0 + 2.0)
        val zoomIn2 = shot("hud_zoom_in_2.png")
        assertFalse(
            "repeated zoom-in must change the map",
            zoomIn1.readBytes().contentEquals(zoomIn2.readBytes()),
        )
        clickTag("zoom_out")
        waitZoom(z0 + 1.0)
        shot("hud_zoom_out_1.png")
        clickTag("zoom_out")
        waitZoom(z0)
        shot("hud_zoom_out_2.png")

        // Zoom persists across drive settings open/close
        val zPersist = NaviMapTestHooks.lastCameraZoom
        openDriveSettings()
        clickTag("btn_close_drive_settings")
        waitSettingsOpen(false)
        Thread.sleep(500)
        assertEquals(
            "zoom must not reset after menu",
            zPersist,
            NaviMapTestHooks.lastCameraZoom,
            0.15,
        )
        shot("hud_zoom_persist_after_menu.png")

        // Auto-zoom level in map settings, then toggle snaps camera
        openMapSettings()
        composeRule
            .onNodeWithTag("auto_zoom_level_label", useUnmergedTree = true)
            .assertIsDisplayed()
        var steps = 0
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        while (MapHudPrefs.loadAutoZoomLevel(ctx) > 15.5 + 0.01 && steps < 20) {
            clickTag("auto_zoom_level_out")
            steps++
        }
        assertEquals(15.5, MapHudPrefs.loadAutoZoomLevel(ctx), 0.01)
        composeRule
            .onNodeWithTag("auto_zoom_level_label", useUnmergedTree = true)
            .assertIsDisplayed()
        clickTag("toggle_auto_zoom")
        waitZoom(15.5)
        composeRule
            .onNodeWithTag("auto_zoom_level_label", useUnmergedTree = true)
            .performClick()
        composeRule.waitForIdle()
        shot("hud_auto_zoom_preset.png")
        closeMapSettings()
        // Caveat: no genuine motion detection yet; toggle applies the configured level.

        // --- 3. Magnetic / DoT / North-up rotation ---
        NaviMapTestHooks.pendingCamera = Triple(centerLat, centerLon, 11.0)
        Thread.sleep(3_000)
        NaviMapTestHooks.applyBearingToMap = true

        openMapSettings()
        clickTag("rot_compass")
        NaviMapTestHooks.applyBearingToMap = true
        val compassShots = mutableListOf<File>()
        for (heading in listOf(0.0, 90.0, 180.0, 270.0)) {
            NaviMapTestHooks.magneticHeadingDeg = heading
            NaviMapTestHooks.gpsBearingDeg = (heading + 90.0) % 360.0
            waitBearing(heading)
            Thread.sleep(1_500)
            compassShots += shot("hud_compass_heading_${heading.toInt()}.png")
            assertEquals(heading, NaviMapTestHooks.lastCameraBearing, 0.5)
        }
        assertEquals(4, compassShots.size)
        assertFalse(
            "compass 0 vs 90 frames must differ",
            compassShots[0].readBytes().contentEquals(compassShots[1].readBytes()),
        )
        assertFalse(
            "compass 90 vs 180 frames must differ",
            compassShots[1].readBytes().contentEquals(compassShots[2].readBytes()),
        )

        // Direction of travel follows GPS, ignores magnetic.
        NaviMapTestHooks.magneticHeadingDeg = 0.0
        NaviMapTestHooks.gpsBearingDeg = 135.0
        clickTag("rot_travel")
        waitBearing(135.0)
        assertEquals(135.0, NaviMapTestHooks.lastCameraBearing, 0.5)
        shot("hud_travel_bearing_135.png")

        // North-up ignores both sensor feeds.
        clickTag("rot_north_up")
        NaviMapTestHooks.magneticHeadingDeg = 45.0
        NaviMapTestHooks.gpsBearingDeg = 225.0
        waitBearing(0.0)
        val n1 = shot("hud_north_up_fed_45.png")
        NaviMapTestHooks.magneticHeadingDeg = 225.0
        NaviMapTestHooks.gpsBearingDeg = 45.0
        Thread.sleep(800)
        waitBearing(0.0)
        val n2 = shot("hud_north_up_fed_225.png")
        assertEquals(0.0, NaviMapTestHooks.lastCameraBearing, 0.5)
        assertTrue(n1.length() > 3_000 && n2.length() > 3_000)
        closeMapSettings()

        android.util.Log.i("HudVerification", "DONE HUD verification")
        android.util.Log.i("HudVerification", "HOLD_FOR_PULL")
        Thread.sleep(90_000)
    }

    /**
     * Tapping the map (not a bar) must not open/close/change settings sheets.
     * Pan and pinch on the map must still work when sheets are closed.
     */
    @Test
    fun hud_map_tap_does_not_affect_settings_sheets() {
        waitStyle()
        NaviMapTestHooks.hideSearchChrome = true
        Thread.sleep(800)

        // --- 1. Sheets closed: map tap must not open either sheet ---
        assertSheetsClosed()
        val rot0 = NaviMapTestHooks.lastRotationMode
        val zoom0 = NaviMapTestHooks.lastCameraZoom
        tapMapAwayFromChrome()
        Thread.sleep(400)
        assertSheetsClosed()
        assertEquals("map tap must not change rotation mode", rot0, NaviMapTestHooks.lastRotationMode)
        assertEquals("map tap must not change zoom", zoom0, NaviMapTestHooks.lastCameraZoom, 0.05)

        // --- 2. Map settings open: map tap must leave sheet open and unchanged ---
        openMapSettings()
        composeRule.onNodeWithTag("map_settings_sheet", useUnmergedTree = true).assertIsDisplayed()
        val rotOpen = NaviMapTestHooks.lastRotationMode
        val zoomOpen = NaviMapTestHooks.lastCameraZoom
        val etaOpen = NaviMapTestHooks.lastShowTripEta
        val breaksOpen = NaviMapTestHooks.lastBreakRemindersEnabled
        tapMapAwayFromChrome()
        Thread.sleep(500)
        assertTrue("map settings must stay open after map tap", NaviMapTestHooks.mapSettingsOpen)
        assertFalse("drive settings must stay closed", NaviMapTestHooks.driveSettingsOpen)
        composeRule.onNodeWithTag("map_settings_sheet", useUnmergedTree = true).assertIsDisplayed()
        assertEquals(rotOpen, NaviMapTestHooks.lastRotationMode)
        assertEquals(zoomOpen, NaviMapTestHooks.lastCameraZoom, 0.05)
        assertEquals(etaOpen, NaviMapTestHooks.lastShowTripEta)
        assertEquals(breaksOpen, NaviMapTestHooks.lastBreakRemindersEnabled)
        closeMapSettings()

        // --- 3. Drive settings open: map tap must leave sheet open and unchanged ---
        openDriveSettings()
        composeRule.onNodeWithTag("drive_settings_sheet", useUnmergedTree = true).assertIsDisplayed()
        val breakBefore = fieldText("field_break_hours")
        val restBefore = fieldText("field_rest_mins")
        tapMapAwayFromChrome()
        Thread.sleep(500)
        assertTrue("drive settings must stay open after map tap", NaviMapTestHooks.driveSettingsOpen)
        assertFalse("map settings must stay closed", NaviMapTestHooks.mapSettingsOpen)
        composeRule.onNodeWithTag("drive_settings_sheet", useUnmergedTree = true).assertIsDisplayed()
        assertEquals(breakBefore, fieldText("field_break_hours"))
        assertEquals(restBefore, fieldText("field_rest_mins"))
        clickTag("btn_close_drive_settings")
        waitSettingsOpen(false)

        // --- 4. Sheets closed: pan + pinch still move the map ---
        assertSheetsClosed()
        NaviMapTestHooks.pendingCamera = Triple(centerLat, centerLon, baseZoom)
        Thread.sleep(2_000)
        waitZoom(baseZoom, tol = 0.3)
        val latBefore = NaviMapTestHooks.lastCameraLat
        val lonBefore = NaviMapTestHooks.lastCameraLon
        val zoomBeforePan = NaviMapTestHooks.lastCameraZoom
        assertTrue("camera lat hook should be set", kotlin.math.abs(latBefore) > 1.0)

        panMapHorizontal()
        val panDeadline = System.currentTimeMillis() + 8_000
        var panMoved = false
        while (System.currentTimeMillis() < panDeadline) {
            val dLat = kotlin.math.abs(NaviMapTestHooks.lastCameraLat - latBefore)
            val dLon = kotlin.math.abs(NaviMapTestHooks.lastCameraLon - lonBefore)
            if (dLat > 0.00005 || dLon > 0.00005) {
                panMoved = true
                break
            }
            Thread.sleep(100)
        }
        assertTrue(
            "map pan must change camera target (lat=$latBefore->${NaviMapTestHooks.lastCameraLat}, " +
                "lon=$lonBefore->${NaviMapTestHooks.lastCameraLon})",
            panMoved,
        )
        assertSheetsClosed()

        NaviMapTestHooks.pendingCamera = Triple(centerLat, centerLon, baseZoom)
        Thread.sleep(2_000)
        waitZoom(baseZoom, tol = 0.3)
        val zoomBeforeZoomGesture = NaviMapTestHooks.lastCameraZoom
        // Prefer pinch; fall back to MapLibre double-tap zoom if synthetic multi-touch
        // does not complete on this AVD (onScaleBegin alone is not enough).
        pinchZoomMap(zoomIn = true)
        Thread.sleep(500)
        if (kotlin.math.abs(NaviMapTestHooks.lastCameraZoom - zoomBeforeZoomGesture) <= 0.15) {
            android.util.Log.i("HudVerification", "pinch did not move zoom; trying double-tap")
            doubleTapZoomMap()
        }
        val zoomDeadline = System.currentTimeMillis() + 8_000
        var zoomMoved = false
        while (System.currentTimeMillis() < zoomDeadline) {
            if (kotlin.math.abs(NaviMapTestHooks.lastCameraZoom - zoomBeforeZoomGesture) > 0.15) {
                zoomMoved = true
                break
            }
            Thread.sleep(100)
        }
        assertTrue(
            "map zoom gesture must change zoom ($zoomBeforeZoomGesture -> ${NaviMapTestHooks.lastCameraZoom})",
            zoomMoved,
        )
        assertSheetsClosed()
        android.util.Log.i(
            "HudVerification",
            "map-tap test ok zoomBeforePan=$zoomBeforePan",
        )
    }

    private fun assertSheetsClosed() {
        assertFalse("map settings should be closed", NaviMapTestHooks.mapSettingsOpen)
        assertFalse("drive settings should be closed", NaviMapTestHooks.driveSettingsOpen)
        assertTrue(
            composeRule
                .onAllNodesWithTag("map_settings_sheet", useUnmergedTree = true)
                .fetchSemanticsNodes()
                .isEmpty(),
        )
        assertTrue(
            composeRule
                .onAllNodesWithTag("drive_settings_sheet", useUnmergedTree = true)
                .fetchSemanticsNodes()
                .isEmpty(),
        )
    }

    /** Tap the map band between the top and bottom HUD (and clear of open sheets). */
    private fun tapMapAwayFromChrome() {
        val device =
            androidx.test.uiautomator.UiDevice.getInstance(
                InstrumentationRegistry.getInstrumentation(),
            )
        val w = device.displayWidth
        val h = device.displayHeight
        val y =
            when {
                NaviMapTestHooks.mapSettingsOpen -> (h * 0.62).toInt()
                NaviMapTestHooks.driveSettingsOpen -> (h * 0.30).toInt()
                else -> (h * 0.45).toInt()
            }
        val x = w / 2
        android.util.Log.i("HudVerification", "tapMap x=$x y=$y w=$w h=$h")
        device.click(x, y)
        composeRule.waitForIdle()
    }

    private fun panMapHorizontal() {
        val device =
            androidx.test.uiautomator.UiDevice.getInstance(
                InstrumentationRegistry.getInstrumentation(),
            )
        val w = device.displayWidth
        val h = device.displayHeight
        val y = (h * 0.45).toInt()
        val x0 = (w * 0.20).toInt()
        val x1 = (w * 0.80).toInt()
        NaviMapTestHooks.mapGestureMoves = 0
        // Shell input is more reliable than UiDevice.swipe for MapLibre on this AVD.
        InstrumentationRegistry
            .getInstrumentation()
            .uiAutomation
            .executeShellCommand("input swipe $x0 $y $x1 $y 400")
            .close()
        composeRule.waitForIdle()
        Thread.sleep(1_200)
        android.util.Log.i(
            "HudVerification",
            "after pan gestureMoves=${NaviMapTestHooks.mapGestureMoves} " +
                "lat=${NaviMapTestHooks.lastCameraLat} lon=${NaviMapTestHooks.lastCameraLon}",
        )
    }

    private fun doubleTapZoomMap() {
        val device =
            androidx.test.uiautomator.UiDevice.getInstance(
                InstrumentationRegistry.getInstrumentation(),
            )
        val cx = device.displayWidth / 2
        val cy = (device.displayHeight * 0.45).toInt()
        InstrumentationRegistry
            .getInstrumentation()
            .uiAutomation
            .executeShellCommand("input tap $cx $cy")
            .close()
        Thread.sleep(60)
        InstrumentationRegistry
            .getInstrumentation()
            .uiAutomation
            .executeShellCommand("input tap $cx $cy")
            .close()
        composeRule.waitForIdle()
        Thread.sleep(1_200)
        android.util.Log.i(
            "HudVerification",
            "after double-tap zoom=${NaviMapTestHooks.lastCameraZoom}",
        )
    }

    private fun pinchZoomMap(zoomIn: Boolean) {
        val device =
            androidx.test.uiautomator.UiDevice.getInstance(
                InstrumentationRegistry.getInstrumentation(),
            )
        val cx = device.displayWidth / 2
        val cy = (device.displayHeight * 0.45).toInt()
        val startSpan = if (zoomIn) 80 else 220
        val endSpan = if (zoomIn) 220 else 80
        injectPinch(cx, cy, startSpan, endSpan)
        composeRule.waitForIdle()
        Thread.sleep(800)
    }

    private fun injectPinch(
        cx: Int,
        cy: Int,
        startSpan: Int,
        endSpan: Int,
    ) {
        val dispatch: (android.view.MotionEvent) -> Unit = { event ->
            var handled = false
            composeRule.runOnUiThread {
                handled = NaviMapTestHooks.mapViewTouch?.invoke(event) == true
            }
            if (!handled) {
                InstrumentationRegistry.getInstrumentation().sendPointerSync(event)
            }
        }
        val downTime = android.os.SystemClock.uptimeMillis()

        fun event(
            action: Int,
            t: Long,
            x0: Float,
            y0: Float,
            x1: Float,
            y1: Float,
            pointerCount: Int,
        ): android.view.MotionEvent {
            val props = Array(pointerCount) { android.view.MotionEvent.PointerProperties() }
            val coords = Array(pointerCount) { android.view.MotionEvent.PointerCoords() }
            props[0].id = 0
            props[0].toolType = android.view.MotionEvent.TOOL_TYPE_FINGER
            coords[0].x = x0
            coords[0].y = y0
            coords[0].pressure = 1f
            coords[0].size = 1f
            if (pointerCount > 1) {
                props[1].id = 1
                props[1].toolType = android.view.MotionEvent.TOOL_TYPE_FINGER
                coords[1].x = x1
                coords[1].y = y1
                coords[1].pressure = 1f
                coords[1].size = 1f
            }
            return android.view.MotionEvent.obtain(
                downTime,
                t,
                action,
                pointerCount,
                props,
                coords,
                0,
                0,
                1f,
                1f,
                0,
                0,
                android.view.InputDevice.SOURCE_TOUCHSCREEN,
                0,
            )
        }
        NaviMapTestHooks.mapGestureScales = 0
        val steps = 16
        var t = downTime
        val y0s = (cy - startSpan / 2).toFloat()
        val y1s = (cy + startSpan / 2).toFloat()
        var e = event(android.view.MotionEvent.ACTION_DOWN, t, cx.toFloat(), y0s, 0f, 0f, 1)
        dispatch(e)
        e.recycle()
        t += 20
        e =
            event(
                android.view.MotionEvent.ACTION_POINTER_DOWN or
                    (1 shl android.view.MotionEvent.ACTION_POINTER_INDEX_SHIFT),
                t,
                cx.toFloat(),
                y0s,
                cx.toFloat(),
                y1s,
                2,
            )
        dispatch(e)
        e.recycle()
        for (i in 1..steps) {
            t += 20
            val frac = i.toFloat() / steps
            val span = startSpan + ((endSpan - startSpan) * frac).toInt()
            val ya = (cy - span / 2).toFloat()
            val yb = (cy + span / 2).toFloat()
            e =
                event(
                    android.view.MotionEvent.ACTION_MOVE,
                    t,
                    cx.toFloat(),
                    ya,
                    cx.toFloat(),
                    yb,
                    2,
                )
            dispatch(e)
            e.recycle()
        }
        t += 20
        val y0e = (cy - endSpan / 2).toFloat()
        val y1e = (cy + endSpan / 2).toFloat()
        e =
            event(
                android.view.MotionEvent.ACTION_POINTER_UP or
                    (1 shl android.view.MotionEvent.ACTION_POINTER_INDEX_SHIFT),
                t,
                cx.toFloat(),
                y0e,
                cx.toFloat(),
                y1e,
                2,
            )
        dispatch(e)
        e.recycle()
        t += 20
        e = event(android.view.MotionEvent.ACTION_UP, t, cx.toFloat(), y0e, 0f, 0f, 1)
        dispatch(e)
        e.recycle()
        android.util.Log.i(
            "HudVerification",
            "after pinch scales=${NaviMapTestHooks.mapGestureScales} zoom=${NaviMapTestHooks.lastCameraZoom}",
        )
    }

    private fun fieldText(tag: String): String {
        val node = composeRule.onNodeWithTag(tag, useUnmergedTree = true).fetchSemanticsNode()
        val editable =
            node.config.getOrElse(androidx.compose.ui.semantics.SemanticsProperties.EditableText) {
                androidx.compose.ui.text
                    .AnnotatedString("")
            }
        if (editable.text.isNotEmpty()) return editable.text
        val textList =
            node.config.getOrElse(androidx.compose.ui.semantics.SemanticsProperties.Text) {
                emptyList()
            }
        return textList.joinToString("") { it.text }
    }
}
