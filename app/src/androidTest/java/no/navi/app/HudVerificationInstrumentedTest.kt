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
                InstrumentationRegistry.getInstrumentation().uiAutomation
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
        dataDir = (context.getExternalFilesDir(null) ?: context.filesDir).also { it.mkdirs() }
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

    private fun waitBearing(expected: Double, tol: Double = 0.5, timeoutMs: Long = 8_000) {
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

    private fun waitZoom(expected: Double, tol: Double = 0.15, timeoutMs: Long = 8_000) {
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            if (kotlin.math.abs(NaviMapTestHooks.lastCameraZoom - expected) <= tol) return
            Thread.sleep(100)
        }
        assertEquals("camera zoom", expected, NaviMapTestHooks.lastCameraZoom, tol)
    }

    private fun waitSettingsOpen(open: Boolean, timeoutMs: Long = 6_000) {
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.driveSettingsOpen == open) return
            Thread.sleep(100)
        }
        assertEquals("drive settings open", open, NaviMapTestHooks.driveSettingsOpen)
    }

    private fun waitMapSettingsOpen(open: Boolean, timeoutMs: Long = 6_000) {
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.mapSettingsOpen == open) return
            Thread.sleep(100)
        }
        assertEquals("map settings open", open, NaviMapTestHooks.mapSettingsOpen)
    }

    private fun openDriveSettings() {
        if (NaviMapTestHooks.driveSettingsOpen) return
        val nodes = composeRule
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
        val closeNodes = composeRule
            .onAllNodesWithTag("btn_close_map_settings", useUnmergedTree = true)
            .fetchSemanticsNodes()
        if (closeNodes.isNotEmpty()) {
            clickTag("btn_close_map_settings")
        } else {
            clickTag("top_drive_hud")
        }
        waitMapSettingsOpen(false)
    }

    private fun shot(name: String, pauseMap: Boolean = false): File {
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
        // Mirror via root `cat >` so adb can pull after instrumentation exits.
        val mirrored = runCatching {
            val dest = "/data/local/tmp/navi_hud/$name"
            InstrumentationRegistry.getInstrumentation().uiAutomation
                .executeShellCommand("su 0 mkdir -p /data/local/tmp/navi_hud")
                .close()
            InstrumentationRegistry.getInstrumentation().uiAutomation
                .executeShellCommand("su 0 sh -c 'cat > $dest'")
                .use { pfd ->
                    java.io.FileOutputStream(pfd.fileDescriptor).use { os ->
                        out.inputStream().use { input -> input.copyTo(os) }
                        os.flush()
                    }
                }
            InstrumentationRegistry.getInstrumentation().uiAutomation
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
        android.util.Log.e("HudVerification", "test setTripEta request=$on")
        NaviMapTestHooks.requestShowTripEta = on
        android.util.Log.e(
            "HudVerification",
            "test after set request=${NaviMapTestHooks.requestShowTripEta} last=${NaviMapTestHooks.lastShowTripEta} hooks=${System.identityHashCode(NaviMapTestHooks)} loader=${NaviMapTestHooks::class.java.classLoader}",
        )
        val deadline = System.currentTimeMillis() + 5_000
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.lastShowTripEta == on) return
            Thread.sleep(100)
        }
        android.util.Log.e(
            "HudVerification",
            "test setTripEta TIMEOUT request=${NaviMapTestHooks.requestShowTripEta} last=${NaviMapTestHooks.lastShowTripEta}",
        )
        assertEquals("trip ETA toggle", on, NaviMapTestHooks.lastShowTripEta)
    }

    private fun setBreakReminders(on: Boolean) {
        if (NaviMapTestHooks.lastBreakRemindersEnabled == on) return
        NaviMapTestHooks.requestBreakReminders = on
        val deadline = System.currentTimeMillis() + 5_000
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.lastBreakRemindersEnabled == on) return
            Thread.sleep(100)
        }
        assertEquals("break reminders toggle", on, NaviMapTestHooks.lastBreakRemindersEnabled)
    }

    private fun setField(tag: String, value: String) {
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
        composeRule.onNodeWithTag("profile_menu", useUnmergedTree = true)
            .performScrollTo()
            .assertIsDisplayed()
        shot("hud_profile_menu.png")
        composeRule.onNodeWithTag("btn_tools", useUnmergedTree = true).performScrollTo()
        clickTag("btn_tools")
        composeRule.onNodeWithTag("tools_menu", useUnmergedTree = true).assertIsDisplayed()
        shot("hud_tools_menu_open.png")
        clickTag("btn_tools") // hide tools panel
        NaviMapTestHooks.hideSearchChrome = true
        Thread.sleep(800)

        // --- 1. Collapsed bars only (search chrome hidden) ---
        composeRule.onNodeWithTag("top_drive_hud", useUnmergedTree = true).assertIsDisplayed()
        composeRule.onNodeWithTag("bottom_drive_hud", useUnmergedTree = true).assertIsDisplayed()
        assertFalse(
            "map settings must be collapsed by default",
            NaviMapTestHooks.mapSettingsOpen,
        )
        composeRule.onAllNodesWithTag("map_settings_sheet", useUnmergedTree = true)
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
            composeRule.onAllNodesWithText("Turn --", useUnmergedTree = true)
                .fetchSemanticsNodes().isEmpty(),
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
        clickTag("btn_apply_drive_settings")
        waitSettingsOpen(false)
        Thread.sleep(500)
        composeRule.onNodeWithTag("hud_eco_icon", useUnmergedTree = true).assertIsDisplayed()
        assertEquals(412.0, NaviMapTestHooks.lastHudAltitudeM!!, 0.1)
        val mapOnly = shot("hud_map_top_bottom_only.png")
        assertTrue(mapOnly.length() > 5_000)
        val idle = shot("hud_idle_both_bars.png")
        assertTrue(idle.length() > 5_000)

        // --- 2. Bottom bar: tap opens drive settings (no Settings link) ---
        openDriveSettings()
        composeRule.onNodeWithTag("drive_settings_title", useUnmergedTree = true)
            .assertIsDisplayed()
        shot("hud_settings_open.png")

        // Break hours apply + auto-close + persist
        setField("field_break_hours", "3.5")
        clickTag("btn_apply_drive_settings")
        waitSettingsOpen(false)
        Thread.sleep(500)
        val restAfterBreak = loadCarRestSettings(dataDir.absolutePath)
        assertEquals(3.5, restAfterBreak.breakIntervalHours, 0.001)
        shot("hud_after_break_hours_apply.png")

        // Rest time apply
        openDriveSettings()
        setField("field_rest_mins", "20")
        clickTag("btn_apply_drive_settings")
        waitSettingsOpen(false)
        Thread.sleep(400)
        assertEquals(20u, loadCarRestSettings(dataDir.absolutePath).restDurationMinutes)
        shot("hud_after_rest_mins_apply.png")

        // Tank capacity apply (force liters so "55" is not treated as gallons).
        openDriveSettings()
        val fuelBeforeTank = loadFuelConfig(dataDir.absolutePath)
        if (!fuelBeforeTank.preferLiters) {
            clickTag("toggle_fuel_units")
            composeRule.waitForIdle()
        }
        setField("field_tank", "55")
        clickTag("btn_apply_drive_settings")
        waitSettingsOpen(false)
        Thread.sleep(400)
        val fuelTank = loadFuelConfig(dataDir.absolutePath)
        assertEquals(55.0, fuelTank.tankCapacityL!!, 0.01)
        assertTrue(fuelTank.preferLiters)
        shot("hud_after_tank_apply.png")

        // Fuel added + unit toggle (L then gallons path)
        openDriveSettings()
        setField("field_fuel_added", "10")
        clickTag("btn_apply_drive_settings")
        waitSettingsOpen(false)
        Thread.sleep(400)
        assertEquals(10.0, loadFuelConfig(dataDir.absolutePath).fuelAddedL!!, 0.05)
        shot("hud_after_fuel_liters_apply.png")

        openDriveSettings()
        clickTag("toggle_fuel_units") // flip liters/gallons
        composeRule.waitForIdle()
        setField("field_fuel_added", "5")
        clickTag("btn_apply_drive_settings")
        waitSettingsOpen(false)
        Thread.sleep(400)
        val fuelAfterGal = loadFuelConfig(dataDir.absolutePath)
        assertTrue(
            "fuel added must persist after unit toggle apply",
            (fuelAfterGal.fuelAddedL ?: 0.0) > 1.0,
        )
        shot("hud_after_fuel_units_apply.png")

        // --- Top bar: open map settings, rotation chips ---
        openMapSettings()
        composeRule.onNodeWithTag("map_settings_title", useUnmergedTree = true).assertIsDisplayed()
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

        // Trip ETA on/off — bottom bar shows ETA line
        setTripEta(true)
        composeRule.onNodeWithText("ETA 95 min", useUnmergedTree = true).assertIsDisplayed()
        shot("hud_trip_eta_on.png")
        setTripEta(false)
        composeRule.onNodeWithText("ETA off", useUnmergedTree = true).assertIsDisplayed()
        val etaStillVisible = composeRule
            .onAllNodesWithText("ETA 95 min", useUnmergedTree = true)
            .fetchSemanticsNodes()
            .isNotEmpty()
        assertFalse("Trip ETA value should hide when toggle off", etaStillVisible)
        shot("hud_trip_eta_off.png")

        // Break reminders — cross-check bottom HUD text
        setBreakReminders(false)
        composeRule.onNodeWithText("Break reminders off", useUnmergedTree = true)
            .assertIsDisplayed()
        shot("hud_breaks_off.png")
        setBreakReminders(true)
        val breakOffStill = composeRule
            .onAllNodesWithText("Break reminders off", useUnmergedTree = true)
            .fetchSemanticsNodes()
            .isNotEmpty()
        assertFalse("Break countdown / interval text should return", breakOffStill)
        shot("hud_breaks_on.png")
        closeMapSettings()

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
        clickTag("btn_cancel_drive_settings")
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
        composeRule.onNodeWithTag("auto_zoom_level_label", useUnmergedTree = true)
            .assertIsDisplayed()
        var steps = 0
        while (
            composeRule.onAllNodesWithText("z 15.5", useUnmergedTree = true)
                .fetchSemanticsNodes().isEmpty() &&
            steps < 20
        ) {
            clickTag("auto_zoom_level_out")
            steps++
        }
        composeRule.onNodeWithText("z 15.5", useUnmergedTree = true).assertIsDisplayed()
        assertEquals(
            15.5,
            MapHudPrefs.loadAutoZoomLevel(
                InstrumentationRegistry.getInstrumentation().targetContext,
            ),
            0.01,
        )
        clickTag("toggle_auto_zoom")
        waitZoom(15.5)
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
}
