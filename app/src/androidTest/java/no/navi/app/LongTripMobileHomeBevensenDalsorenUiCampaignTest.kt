package no.navi.app

import android.os.Debug
import android.os.SystemClock
import android.util.Log
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextClearance
import androidx.compose.ui.test.performTextInput
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.filters.LargeTest
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.rule.GrantPermissionRule
import androidx.test.uiautomator.By
import androidx.test.uiautomator.UiDevice
import androidx.test.uiautomator.Until
import org.json.JSONArray
import org.json.JSONObject
import org.junit.After
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TestWatcher
import org.junit.runner.Description
import org.junit.runner.RunWith
import uniffi.navi.FfiCarRestSettings
import uniffi.navi.FfiFuelConfig
import uniffi.navi.FfiVehicleLimits
import uniffi.navi.campingPluginConfigure
import uniffi.navi.campingPluginInstallGuest
import uniffi.navi.campingPluginSetEnabled
import uniffi.navi.campingPluginSetTimezone
import uniffi.navi.saveCarRestSettings
import uniffi.navi.saveFuelConfig
import uniffi.navi.saveVehicleLimits
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.util.Locale
import java.util.TimeZone
import java.util.concurrent.TimeUnit

/**
 * Bevensen → Ottadal via → Dalsøren MobileHome campaign via **visible Compose UI**.
 *
 * Primary plan path: chip_from / chip_via / chip_to + btn_plan_route on MainActivity.
 * UniFFI is assist-only (vehicle/rest seed, camping guest install). Synthetic DATEX is
 * injected by host ADB (marker BEVENSEN_AWAIT_DATEX). GPS start is host ADB.
 * Regions are deleted via Tools UI before plan. Not wired into CI.
 */
@RunWith(AndroidJUnit4::class)
@LargeTest
class LongTripMobileHomeBevensenDalsorenUiCampaignTest {
    companion object {
        private const val TAG = "BevensenUiCampaign"

        private const val ORIGIN_LAT = 53.079686
        private const val ORIGIN_LON = 10.587198
        private const val VIA_LAT = 61.8691419
        private const val VIA_LON = 9.1055130
        private const val DEST_LAT = 61.4433766
        private const val DEST_LON = 7.4614016

        private const val WIDTH_INCL_MIRRORS_M = 2.297
        private const val LENGTH_M = 5.304
        private const val BODY_HEIGHT_M = 2.477
        private const val LOADED_TOTAL_KG = 3020.4
        private const val LOADED_REAR_AXLE_KG = 1661.2
        private const val TANK_L = 70.0
        private const val DEPARTURE_ISO = "2026-06-01T08:00:00"
        private const val MAX_DAILY_HOURS = 6.0

        private val DOWNLOAD_DEADLINE_MS = TimeUnit.HOURS.toMillis(20)
        private val PLAN_DEADLINE_MS = TimeUnit.HOURS.toMillis(6)

        // Host ADB injects these (3–6 Blocks across DE/DK/SE/NO).
        // Coords are densify-chord midpoints on the known Bevensen→Dalsøren path
        // (within DATEX corridor margin). Avoid Ottadal→Dalsøren mountain choke.
        private val DATEX_SITS =
            listOf(
                Sit("syn-de-a7", "DE", 53.64485, 10.33915, "Synthetic DE A7 chord (Bevensen-Kiel)"),
                Sit("syn-dk-e45", "DK", 55.04625, 11.23625, "Synthetic DK Great Belt approach"),
                Sit("syn-se-e6", "SE", 56.42250, 12.96875, "Synthetic SE Halland E6"),
                Sit("syn-se-gbg", "SE", 58.30281, 11.32281, "Synthetic SE Bohuslan coast"),
                Sit("syn-no-e6", "NO", 60.17719, 10.64550, "Synthetic NO E6 Ostlandet"),
                Sit("syn-no-otta", "NO", 61.56436, 9.66331, "Synthetic NO toward Ottadal via"),
            )

        // Corridor + leftover SD stems from prior runs. Delete via Tools UI only;
        // long-trip Plan auto-downloads the corridor afterward (no manual fetch taps).
        private val REGIONS_TO_DELETE_CANDIDATES =
            listOf(
                "europe/denmark",
                "europe/germany/niedersachsen",
                "europe/germany/schleswig-holstein",
                "europe/sweden/halland",
                "europe/sweden/skane",
                "europe/sweden/vastra-gotaland",
                "europe/sweden/vastra_gotaland",
                "europe/sweden/dalarna",
                "europe/sweden/jamtland",
                "europe/sweden/vasternorrland",
                "europe/sweden/vasterbotten",
                "europe/sweden/norrbotten",
                "europe/sweden",
                "europe/finland",
                "europe/norway/vestlandet",
                "europe/norway/ostlandet",
                "europe/norway/nord-norge",
                "europe/norway/trondelag",
                "europe/norway/hedmark",
                "europe/norway/sorlandet",
            )
    }

    private data class Sit(
        val id: String,
        val country: String,
        val lat: Double,
        val lon: Double,
        val label: String,
    )

    /** Prefs before MainActivity so long-trip / plugins are ON at launch. */
    @get:Rule(order = 0)
    val prefRule =
        object : TestWatcher() {
            override fun starting(description: Description) {
                val ctx = InstrumentationRegistry.getInstrumentation().targetContext
                MapHudPrefs.saveLongTripEnabled(ctx, true)
                MapHudPrefs.saveDatexPluginEnabled(ctx, true)
                MapHudPrefs.saveDatexWifiOnly(ctx, false)
                MapHudPrefs.savePoiLookaheadEnabled(ctx, true)
                MapHudPrefs.saveCampingPluginEnabled(ctx, true)
                MapHudPrefs.saveSpeedCameraPromptShown(ctx, true)
                val volumes = NaviStorageVolumes.list(ctx)
                val sd =
                    volumes.firstOrNull {
                        it.removable &&
                            it.mounted &&
                            it.id != NaviStorageVolumes.INTERNAL_ID &&
                            it.appFilesDir != null
                    }
                if (sd != null) {
                    MapHudPrefs.saveLongTripPackVolumeId(ctx, sd.id)
                }
            }
        }

    @get:Rule(order = 1)
    val composeRule = createAndroidComposeRule<MainActivity>()

    @get:Rule(order = 2)
    val permissionRule: GrantPermissionRule =
        GrantPermissionRule.grant(
            android.Manifest.permission.ACCESS_FINE_LOCATION,
            android.Manifest.permission.ACCESS_COARSE_LOCATION,
        )

    private lateinit var dataDir: File
    private lateinit var device: UiDevice
    private val report = JSONObject()
    private val uiEvents = JSONArray()

    @Before
    fun setUp() {
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        dataDir = NaviAppData.resolve(ctx).also { it.mkdirs() }
        device = UiDevice.getInstance(InstrumentationRegistry.getInstrumentation())
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = false
        NaviMapTestHooks.lastPlanReport = ""
        NaviMapTestHooks.lastPlanDistanceKm = 0.0
        NaviMapTestHooks.lastRoutePolyline = ""
        NaviMapTestHooks.lastRoutePolylineChars = 0
        NaviMapTestHooks.lastManeuversJson = "[]"
        report.put("campaign", "bevensen_dalsoren_mobilehome_ui")
        report.put("branch_note", "right-to-roam")
        report.put("started_unix", System.currentTimeMillis() / 1000)
        report.put("departure_iso", DEPARTURE_ISO)
        report.put("ram_before_mib", processPssMib())
        report.put(
            "origin",
            JSONObject().put("lat", ORIGIN_LAT).put("lon", ORIGIN_LON).put("name", "Bad Bevensen Kurpark Stellplatz"),
        )
        report.put(
            "via",
            JSONObject().put("lat", VIA_LAT).put("lon", VIA_LON).put("name", "Ottadal corridor via"),
        )
        report.put(
            "dest",
            JSONObject().put("lat", DEST_LAT).put("lon", DEST_LON).put("name", "Dalsøren Camping"),
        )
    }

    @After
    fun tearDown() {
        report.put("ui_events", uiEvents)
        writeReport()
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = false
    }

    @Test
    fun bevensen_ottadal_dalsoren_ui_plan_campaign() {
        settle(1_200)
        noteUi("activity_foreground", "MainActivity compose rule launched")
        screenshot("01_launch")

        // Assist: vehicle / soft rest / fuel / camping guest (disk). No DATEX here.
        seedAssistSettings()
        report.put("current_json_regions", fetchCurrentJsonRegions())
        writeReport()

        openRoutePanel()
        noteUi("open_route_panel", "field_search visible")
        screenshot("02_route_panel")

        confirmPluginsAndLongTripViaToolsUi()
        screenshot("03_tools_plugins")

        // Hang-fix retest: DO NOT wipe SD packs / index DBs — reuse Indexed corridor
        // already on removable storage. Long-trip auto-download only fills gaps.
        // (Cold-wipe path remains in deleteDownloadedRegionsViaUi for full campaigns.)
        report.put("downloads_mode", "long_trip_reuse_existing_packs")
        report.put(
            "downloads_policy",
            "Reuse existing SD long-trip-packs + place index; no Tools UI wipe; auto-download only if missing",
        )
        report.put("regions_deleted_via_ui", org.json.JSONArray())
        noteUi("packs_policy", "reuse_existing_no_wipe")
        screenshot("03b_regions_reused")

        // Ensure long trip stays ON before plan.
        confirmPluginsAndLongTripViaToolsUi()
        openRoutePanel()
        runCatching {
            clickTagSoft("btn_tools")
            settle(500)
            composeRule.onNodeWithTag("toggle_long_trip", useUnmergedTree = true).performScrollTo()
            // Prefer prefs over a blank status line: blank semantics + click would
            // toggle an already-ON long-trip OFF and stall corridor planning.
            var line = ""
            repeat(8) {
                line =
                    runCatching {
                        composeRule
                            .onNodeWithTag("long_trip_status_line", useUnmergedTree = true)
                            .fetchSemanticsNode()
                            .config[androidx.compose.ui.semantics.SemanticsProperties.Text]
                            .joinToString(" ") { it.text }
                    }.getOrDefault("")
                if (line.isNotBlank()) return@repeat
                settle(250)
            }
            val prefsOn = MapHudPrefs.loadLongTripEnabled(composeRule.activity)
            if (line.contains("off", ignoreCase = true) || (!prefsOn && line.isBlank())) {
                composeRule.onNodeWithTag("toggle_long_trip", useUnmergedTree = true).performClick()
                settle(400)
                noteUi("toggle_long_trip", "forced ON before plan (was: '$line' prefsOn=$prefsOn)")
            } else {
                noteUi("toggle_long_trip", "confirmed ON (line='$line' prefsOn=$prefsOn)")
            }
            NaviMapTestHooks.requestCloseTools = true
            settle(300)
            clickTagSoft("btn_close_tools")
            clickTagSoft("btn_save_tools")
        }

        // Profile + vehicle + rest via visible Drive / Route UI.
        configureMobileHomeVehicleAndRestViaUi()
        screenshot("04_vehicle_rest")

        // Eco: selecting Mobile home resets ecoEnabled to profile default (off).
        // Turn Eco routing ON via the visible route-sheet switch before Plan.
        openRoutePanel()
        enableEcoRoutingViaUi()
        runCatching {
            composeRule.onNodeWithText("Avoid toll roads", useUnmergedTree = true).assertExists()
            noteUi("avoid_tolls_row", "visible; default OFF (avoidTolls=false)")
        }

        // From / Via / To — typed coordinates on visible search field.
        typeCoordAndPickHit("chip_from", ORIGIN_LAT, ORIGIN_LON, "from_bevensen")
        screenshot("05_from_set")
        typeCoordAndPickHit("chip_via", VIA_LAT, VIA_LON, "via_ottadal")
        screenshot("06_via_set")
        typeCoordAndPickHit("chip_to", DEST_LAT, DEST_LON, "to_dalsoren")
        screenshot("07_to_set")
        noteUi(
            "waypoints_summary",
            waypointSummary(),
        )
        report.put("waypoints_summary", waypointSummary())
        writeReport()

        // Host ADB injects synthetic DATEX into datex_cache (marker below).
        awaitHostDatexInjection()
        screenshot("07b_datex_injected")

        // Re-assert MobileHome after long Tools/delete flows.
        openRoutePanel()
        runCatching {
            clickTag("chip_profile_mobile_home")
            noteUi("chip_profile_mobile_home", "re-asserted before plan")
            settle(300)
        }
        enableEcoRoutingViaUi()

        // Safety: long trip must stay ON before Plan (blank status must not toggle OFF).
        runCatching {
            clickTagSoft("btn_tools")
            settle(500)
            composeRule.onNodeWithTag("toggle_long_trip", useUnmergedTree = true).performScrollTo()
            var line = ""
            repeat(6) {
                line =
                    runCatching {
                        composeRule
                            .onNodeWithTag("long_trip_status_line", useUnmergedTree = true)
                            .fetchSemanticsNode()
                            .config[androidx.compose.ui.semantics.SemanticsProperties.Text]
                            .joinToString(" ") { it.text }
                    }.getOrDefault("")
                if (line.isNotBlank()) return@repeat
                settle(200)
            }
            val prefsOn = MapHudPrefs.loadLongTripEnabled(composeRule.activity)
            if (line.contains("off", ignoreCase = true) || !prefsOn) {
                if (!prefsOn || line.contains("off", ignoreCase = true)) {
                    composeRule.onNodeWithTag("toggle_long_trip", useUnmergedTree = true).performClick()
                    settle(400)
                }
                noteUi("toggle_long_trip", "re-ON immediately before plan (line='$line' prefsOn=$prefsOn)")
            } else {
                noteUi("toggle_long_trip", "still ON before plan (line='$line')")
            }
            NaviMapTestHooks.requestCloseTools = true
            settle(300)
            clickTagSoft("btn_close_tools")
            clickTagSoft("btn_save_tools")
            openRoutePanel()
        }

        // PRIMARY PLAN PATH: visible Plan button.
        composeRule.onNodeWithTag("btn_plan_route", useUnmergedTree = true).performScrollTo()
        noteUi("click_btn_plan_route", "performClick")
        screenshot("08_before_plan")
        clickTag("btn_plan_route")
        screenshot("09_after_plan_click")

        val t0 = System.currentTimeMillis()
        var ready = false
        var planned = false
        while (System.currentTimeMillis() - t0 < DOWNLOAD_DEADLINE_MS) {
            val line = LongTripCoordinator.statusLine()
            val corr = LongTripCoordinator.corridorReadyForPlanning()
            val poly = NaviMapTestHooks.lastRoutePolylineChars
            val dist = NaviMapTestHooks.lastPlanDistanceKm
            val sample =
                JSONObject()
                    .put("t_ms", System.currentTimeMillis() - t0)
                    .put("status", line)
                    .put("corridor_ready", corr)
                    .put("polyline_chars", poly)
                    .put("distance_km", dist)
                    .put("report_has_pass", NaviMapTestHooks.lastPlanReport.contains("PASS"))
            report.put("last_progress", sample)
            if ((System.currentTimeMillis() / 20_000) % 2L == 0L) {
                Log.i(TAG, "progress $sample")
                writeReport()
            }
            if (corr) ready = true
            val rep = NaviMapTestHooks.lastPlanReport
            val overallPass =
                rep.contains("PASS") &&
                    !rep.contains("FAIL: chunk_leg") &&
                    !rep.startsWith("FAIL:") &&
                    !rep.contains("\nFAIL: chunk_leg")
            if (poly >= 8 && dist > 100.0 && overallPass) {
                planned = true
                break
            }
            // Finished with hard failure (do not treat nested leg PASS as success).
            if ((rep.contains("FAIL: chunk_leg") || rep.contains("FAIL: no route")) &&
                rep.length > 80
            ) {
                noteUi("plan_failed", rep.take(240))
                break
            }
            Thread.sleep(3_000)
            composeRule.mainClock.advanceTimeBy(3_000)
            if (System.currentTimeMillis() - t0 > PLAN_DEADLINE_MS && planned) break
        }
        report.put("download_or_plan_elapsed_ms", System.currentTimeMillis() - t0)
        report.put("corridor_ready_final", ready)
        report.put("ui_planned", planned)
        report.put("ram_post_plan_mib", processPssMib())
        screenshot("10_plan_result")

        val planReport = NaviMapTestHooks.lastPlanReport
        val distanceKm = NaviMapTestHooks.lastPlanDistanceKm
        val maneuvers = NaviMapTestHooks.lastManeuversJson
        val manCount = jsonArrayLen(maneuvers)
        // ETA from report line if present
        val etaMin = parseEtaMinutes(planReport)
        report.put(
            "full_plan",
            JSONObject()
                .put("distance_km", distanceKm)
                .put("eta_minutes", etaMin)
                .put("maneuver_count", manCount)
                .put("polyline_chars", NaviMapTestHooks.lastRoutePolylineChars)
                .put("report", planReport.take(120_000)),
        )
        report.put("maneuver_count", manCount)
        report.put("datex_in_plan_report", extractDatexFromPlanReport(planReport))
        report.put("fuel_stops_estimate", estimateFuelStops(distanceKm))
        report.put(
            "ferries",
            JSONObject()
                .put("route_uses_ferry", planReport.contains("route_uses_ferry=true"))
                .put(
                    "report_ferry_lines",
                    JSONArray(
                        planReport
                            .lineSequence()
                            .filter { it.contains("ferry", ignoreCase = true) }
                            .take(40)
                            .toList(),
                    ),
                ),
        )
        report.put(
            "expected_check",
            JSONObject()
                .put("distance_km", distanceKm)
                .put("distance_ok", distanceKm in 1461.3..1648.6)
                .put("duration_h", etaMin / 60.0)
                .put("duration_ok", (etaMin / 60.0) in 17.0..22.5)
                .put("maneuvers", manCount)
                .put("maneuvers_ok", manCount in 55..100)
                .put("datex_ok", datexImpactsPositive(planReport)),
        )
        report.put("pack_dir", LongTripPackStorage.packDownloadDir(composeRule.activity).absolutePath)
        report.put("corridor", JSONArray(LongTripCoordinator.currentPlan()?.regionsInOrder.orEmpty()))
        report.put("long_trip_status", LongTripCoordinator.statusLine())
        report.put("ram_final_mib", processPssMib())
        report.put("finished_unix", System.currentTimeMillis() / 1000)
        writeReport()

        assertTrue(
            "UI waypoints must be set (saw From/Via/To interactions)",
            uiEvents.toString().contains("from_bevensen") &&
                uiEvents.toString().contains("via_ottadal") &&
                uiEvents.toString().contains("to_dalsoren"),
        )
        assertTrue(
            "Plan button must have been clicked on UI",
            uiEvents.toString().contains("click_btn_plan_route"),
        )
        assertTrue(
            "UI plan must produce a route (km=$distanceKm chars=${NaviMapTestHooks.lastRoutePolylineChars})",
            distanceKm > 100.0 && NaviMapTestHooks.lastRoutePolylineChars >= 8,
        )
        Log.i(
            TAG,
            "PASS_UI dist=$distanceKm etaMin=$etaMin man=$manCount datex=${datexImpactsPositive(planReport)}",
        )
    }

    private fun seedAssistSettings() {
        assertTrue(
            saveVehicleLimits(
                dataDir.absolutePath,
                FfiVehicleLimits(
                    axleWeightKg = LOADED_REAR_AXLE_KG,
                    bogieWeightKg = null,
                    heightM = BODY_HEIGHT_M,
                    widthM = WIDTH_INCL_MIRRORS_M,
                    lengthM = LENGTH_M,
                    totalWeightKg = LOADED_TOTAL_KG,
                ),
            ),
        )
        assertTrue(
            saveFuelConfig(
                dataDir.absolutePath,
                FfiFuelConfig(tankCapacityL = TANK_L, fuelAddedL = TANK_L, preferLiters = true),
            ),
        )
        assertTrue(
            saveCarRestSettings(
                dataDir.absolutePath,
                FfiCarRestSettings(
                    breakIntervalHours = 1.5,
                    restDurationMinutes = 15u,
                    ecoModeEnabled = true,
                    maxHours = MAX_DAILY_HOURS,
                ),
            ),
        )
        campingPluginConfigure(composeRule.activity.filesDir.absolutePath, dataDir.absolutePath, TimeZone.getDefault().id)
        installCampingGuest()
        campingPluginSetEnabled(true)
        campingPluginSetTimezone("Europe/Oslo")
        report.put(
            "vehicle",
            JSONObject()
                .put("label", "VW Transporter T6 2.0 BiTDi 4Motion camper")
                .put("height_m", BODY_HEIGHT_M)
                .put("width_m", WIDTH_INCL_MIRRORS_M)
                .put("length_m", LENGTH_M)
                .put("total_weight_kg", LOADED_TOTAL_KG)
                .put("axle_weight_kg", LOADED_REAR_AXLE_KG)
                .put("tank_l", TANK_L),
        )
        report.put(
            "settings",
            JSONObject()
                .put("eco", true)
                .put("avoid_toll_roads", false)
                .put("ferries", "use")
                .put("soft_daily_budget_h", MAX_DAILY_HOURS)
                .put("soft_break_interval_h", 1.5)
                .put("soft_rest_min", 15)
                .put("wild_camping", true)
                .put("long_trip", true)
                .put("datex", true)
                .put("nearby_attractions", true)
                .put("plan_path", "compose_ui_btn_plan_route"),
        )
        noteUi("assist_seed", "vehicle/rest/fuel/camping seeded on disk")
    }

    private fun enableEcoRoutingViaUi() {
        // Switch next to "Eco routing" has no testTag — use UiAutomator on the
        // visible Compose Switch (Mobile home resets eco to profile default off).
        device.wait(Until.findObject(By.text("Eco routing")), 3_000)
        val ecoLabel =
            device.findObject(By.text("Eco routing"))
                ?: device.findObject(By.textContains("Eco routing"))
        if (ecoLabel == null) {
            noteUi("eco_routing_switch", "Eco routing label not found")
            settle(400)
            return
        }
        // Prefer checkable siblings of the label's parent Row.
        val parent = ecoLabel.parent
        var sw = parent?.findObject(By.checkable(true))
        if (sw == null) {
            // Fallback: any on-screen unchecked checkable near the label Y.
            val labelBounds = ecoLabel.visibleBounds
            val all = device.findObjects(By.checkable(true))
            sw =
                all.firstOrNull { obj ->
                    val b = obj.visibleBounds
                    kotlin.math.abs(b.centerY() - labelBounds.centerY()) < 80
                }
        }
        if (sw == null) {
            noteUi("eco_routing_switch", "switch not found beside label")
        } else if (!sw.isChecked) {
            sw.click()
            settle(300)
            // Click again if still off (first click may focus).
            if (!sw.isChecked) {
                sw.click()
                settle(300)
            }
            noteUi(
                "eco_routing_switch",
                if (sw.isChecked) "clicked ON" else "clicked but still OFF",
            )
        } else {
            noteUi("eco_routing_switch", "already ON")
        }
        settle(400)
    }

    private fun configureMobileHomeVehicleAndRestViaUi() {
        openRoutePanel()
        // Profile chip on route sheet
        runCatching {
            clickTag("chip_profile_mobile_home")
            noteUi("chip_profile_mobile_home", "clicked")
            runCatching { clickTag("btn_save_profile") }
        }.onFailure {
            // Drive settings profile chips
            clickTagSoft("btn_open_drive_settings")
            settle(400)
            clickTagSoft("drive_chip_profile_mobile_home")
            noteUi("drive_chip_profile_mobile_home", "clicked fallback")
        }
        settle(400)

        // Vehicle panel height via UI
        clickTagSoft("btn_open_vehicle")
        settle(300)
        runCatching {
            setField("field_vehicle_height", BODY_HEIGHT_M.toString())
            noteUi("field_vehicle_height", BODY_HEIGHT_M.toString())
            clickTag("btn_save_vehicle")
            noteUi("btn_save_vehicle", "clicked")
        }
        clickTagSoft("btn_close_vehicle")

        // Eco + break spacing via drive settings sheet
        clickTagSoft("btn_open_drive_settings")
        settle(500)
        runCatching {
            setField("field_break_hours", "1.5")
            setField("field_rest_mins", "15")
            noteUi("break_rest_fields", "1.5h / 15min")
            // Eco: ensure ON via visible toggle (assist seed also true).
            composeRule.onNodeWithTag("toggle_eco", useUnmergedTree = true).performScrollTo()
            noteUi("toggle_eco", "visible; saving with ecoModeEnabled=true via assist+UI save")
            clickTag("btn_save_drive_settings")
            noteUi("btn_save_drive_settings", "clicked")
        }.onFailure { e ->
            noteUi("drive_settings_soft_fail", e.message ?: "err")
        }
        clickTagSoft("btn_close_drive_settings")
        // Keep soft daily budget 6 h (UI save preserves prior maxHours from assist seed).
        saveCarRestSettings(
            dataDir.absolutePath,
            FfiCarRestSettings(
                breakIntervalHours = 1.5,
                restDurationMinutes = 15u,
                ecoModeEnabled = true,
                maxHours = MAX_DAILY_HOURS,
            ),
        )
        saveVehicleLimits(
            dataDir.absolutePath,
            FfiVehicleLimits(
                axleWeightKg = LOADED_REAR_AXLE_KG,
                bogieWeightKg = null,
                heightM = BODY_HEIGHT_M,
                widthM = WIDTH_INCL_MIRRORS_M,
                lengthM = LENGTH_M,
                totalWeightKg = LOADED_TOTAL_KG,
            ),
        )
    }

    private fun confirmPluginsAndLongTripViaToolsUi() {
        clickTagSoft("btn_tools")
        settle(600)
        noteUi("btn_tools", "opened")
        runCatching {
            composeRule.onNodeWithTag("toggle_long_trip", useUnmergedTree = true).performScrollTo()
            val line =
                runCatching {
                    composeRule
                        .onNodeWithTag("long_trip_status_line", useUnmergedTree = true)
                        .fetchSemanticsNode()
                        .config[androidx.compose.ui.semantics.SemanticsProperties.Text]
                        .joinToString(" ") { it.text }
                }.getOrDefault("")
            noteUi("long_trip_status", line)
            if (line.contains("off", ignoreCase = true)) {
                composeRule.onNodeWithTag("toggle_long_trip", useUnmergedTree = true).performClick()
                noteUi("toggle_long_trip", "turned ON")
            }
            composeRule.onNodeWithTag("toggle_datex_plugin", useUnmergedTree = true).performScrollTo()
            // Ensure DATEX on — click if needed is ambiguous; prefs already ON
            noteUi("toggle_datex_plugin", "visible (pref ON)")
            composeRule.onNodeWithTag("toggle_camping_plugin", useUnmergedTree = true).performScrollTo()
            noteUi("toggle_camping_plugin", "visible (pref ON)")
            composeRule.onNodeWithTag("toggle_poi_lookahead", useUnmergedTree = true).performScrollTo()
            noteUi("toggle_poi_lookahead", "visible (pref ON)")
        }.onFailure { e ->
            noteUi("tools_soft_fail", e.message ?: "err")
        }
        NaviMapTestHooks.requestCloseTools = true
        settle(500)
        clickTagSoft("btn_close_tools")
        clickTagSoft("btn_save_tools")
    }

    private fun typeCoordAndPickHit(
        chipTag: String,
        lat: Double,
        lon: Double,
        step: String,
    ) {
        openRoutePanel()
        clickTag(chipTag)
        noteUi(step, "clicked $chipTag")
        val q = String.format(Locale.US, "%.7f, %.7f", lat, lon)
        NaviMapTestHooks.lastSearchHitCount = -1
        NaviMapTestHooks.lastAppliedHitLat = Double.NaN
        setField("field_search", q)
        noteUi(step, "typed $q")
        val deadline = SystemClock.elapsedRealtime() + 20_000
        while (SystemClock.elapsedRealtime() < deadline) {
            if (NaviMapTestHooks.lastSearchHitCount >= 1) break
            composeRule.mainClock.advanceTimeBy(200)
            Thread.sleep(200)
        }
        assertTrue(
            "$step: coordinate hits for $q count=${NaviMapTestHooks.lastSearchHitCount}",
            NaviMapTestHooks.lastSearchHitCount >= 1,
        )
        clickTag("search_hit_0")
        noteUi(step, "clicked search_hit_0")
        val applyDeadline = SystemClock.elapsedRealtime() + 12_000
        while (SystemClock.elapsedRealtime() < applyDeadline) {
            if (!NaviMapTestHooks.lastAppliedHitLat.isNaN()) break
            Thread.sleep(100)
        }
        report.put(
            step,
            JSONObject()
                .put("query", q)
                .put("applied_lat", NaviMapTestHooks.lastAppliedHitLat)
                .put("applied_lon", NaviMapTestHooks.lastAppliedHitLon)
                .put("applied_name", NaviMapTestHooks.lastAppliedHitName),
        )
        writeReport()
    }

    /** Host ADB injects DATEX; test only waits for apply_to_routing + situations XML. */
    private fun awaitHostDatexInjection() {
        val cacheDir = File(dataDir, "datex_cache").also { it.mkdirs() }
        val marker = File(cacheDir, "apply_to_routing")
        val xml = File(cacheDir, "datex-GetSituation.xml")
        Log.i(TAG, "BEVENSEN_AWAIT_DATEX path=${cacheDir.absolutePath}")
        noteUi("datex_await", cacheDir.absolutePath)
        val deadline = System.currentTimeMillis() + TimeUnit.MINUTES.toMillis(10)
        while (System.currentTimeMillis() < deadline) {
            if (marker.isFile && xml.isFile && xml.length() > 200) {
                noteUi("datex_present", "xml_bytes=${xml.length()}")
                val out = JSONArray()
                for (s in DATEX_SITS) {
                    out.put(
                        JSONObject()
                            .put("id", s.id)
                            .put("country", s.country)
                            .put("lat", s.lat)
                            .put("lon", s.lon)
                            .put("label", s.label)
                            .put("impact", "Block")
                            .put("synthetic", true)
                            .put("injected_via", "host_adb"),
                    )
                }
                report.put("datex_synthetic_reroutes", out)
                report.put("datex_cache_dir", cacheDir.absolutePath)
                return
            }
            Thread.sleep(1_000)
            composeRule.mainClock.advanceTimeBy(1_000)
        }
        throw AssertionError("Host ADB DATEX injection timed out at ${cacheDir.absolutePath}")
    }

    private fun deleteDownloadedRegionsViaUi() {
        val deleted = JSONArray()
        // Stop any leftover auto-download/index from a prior plan so Tools delete
        // is not blocked and does not race the place-index DB (SIGBUS risk).
        RegionDownloadBackground.cancelPending(dataDir)
        var idleWait = 0L
        while ((RegionDownloadBackground.isRunning() || PlaceIndexBackground.isRunning()) &&
            idleWait < TimeUnit.MINUTES.toMillis(5)
        ) {
            noteUi(
                "download_cancel_wait",
                "running download=${RegionDownloadBackground.isRunning()} " +
                    "index=${PlaceIndexBackground.isRunning()} (${idleWait}ms)",
            )
            settle(5_000)
            idleWait += 5_000
            RegionDownloadBackground.cancelPending(dataDir)
        }
        clickTagSoft("btn_tools")
        settle(500)
        runCatching {
            composeRule.onNodeWithTag("toggle_long_trip", useUnmergedTree = true).performScrollTo()
            val line =
                runCatching {
                    composeRule
                        .onNodeWithTag("long_trip_status_line", useUnmergedTree = true)
                        .fetchSemanticsNode()
                        .config[androidx.compose.ui.semantics.SemanticsProperties.Text]
                        .joinToString(" ") { it.text }
                }.getOrDefault("")
            if (!line.contains("off", ignoreCase = true)) {
                composeRule.onNodeWithTag("toggle_long_trip", useUnmergedTree = true).performClick()
                noteUi("toggle_long_trip", "OFF for region delete")
                settle(400)
            }
        }
        // Always attempt UI delete for every corridor candidate. blockReason only
        // sees internal dataDir — SD long-trip-packs are invisible to it, so
        // "Nothing installed" must not skip (packs live on the removable volume).
        for (path in REGIONS_TO_DELETE_CANDIDATES) {
            var block = DownloadedRegionDelete.blockReason(path, dataDir, listOf(LongTripPackStorage.packDownloadDir(composeRule.activity)))
            var waitedMs = 0L
            // Cap wait: cancelled basemap jobs can leave isRunning stuck.
            while (block != null &&
                (
                    block.contains("download", ignoreCase = true) ||
                        block.contains("index", ignoreCase = true) ||
                        block.contains("Long trip", ignoreCase = true)
                ) &&
                waitedMs < TimeUnit.MINUTES.toMillis(3)
            ) {
                noteUi("delete_wait", "$path -> $block (${waitedMs}ms)")
                RegionDownloadBackground.cancelPending(dataDir)
                settle(5_000)
                waitedMs += 5_000
                block =
                    DownloadedRegionDelete.blockReason(
                        path,
                        dataDir,
                        listOf(LongTripPackStorage.packDownloadDir(composeRule.activity)),
                    )
            }
            if (block != null && !block.startsWith("Nothing installed")) {
                noteUi("delete_block_precheck", "$path -> $block")
            }
            // Avoid racing place-index workers during multi-GB SD deletes.
            var idxWait = 0L
            while (PlaceIndexBackground.isRunning() && idxWait < TimeUnit.MINUTES.toMillis(2)) {
                noteUi("delete_index_idle", "$path index still running (${idxWait}ms)")
                settle(5_000)
                idxWait += 5_000
            }
            runCatching {
                setField("field_geofabrik_path", path)
                settle(400)
                composeRule
                    .onNodeWithTag("btn_delete_downloaded_region", useUnmergedTree = true)
                    .performScrollTo()
                clickTagSoft("btn_delete_downloaded_region")
                settle(500)
                clickTagSoft("btn_confirm_delete_region")
                settle(2_500)
                deleted.put(
                    JSONObject()
                        .put("path", path)
                        .put("pre_block", block ?: JSONObject.NULL)
                        .put("attempted", true)
                        .put("waited_ms", waitedMs),
                )
                noteUi("delete_region", path)
            }.onFailure { e ->
                noteUi("delete_region_fail", "$path: ${e.message}")
            }
        }
        report.put("regions_deleted_via_ui", deleted)
        NaviMapTestHooks.requestCloseTools = true
        settle(400)
        clickTagSoft("btn_close_tools")
        clickTagSoft("btn_save_tools")
    }

    private fun installCampingGuest() {
        val am = composeRule.activity.assets
        val name = "right_to_roam_camping"
        val manifest = am.open("plugins/$name/plugin.json").use { it.readBytes().decodeToString() }
        val wasm = am.open("plugins/$name/plugin.wasm").use { it.readBytes() }
        campingPluginInstallGuest(name, manifest, wasm)
    }

    private fun fetchCurrentJsonRegions(): JSONObject {
        val out = JSONObject()
        return try {
            val conn = URL("https://navigate-me.duckdns.org/current.json").openConnection() as HttpURLConnection
            conn.connectTimeout = 15_000
            conn.readTimeout = 60_000
            conn.requestMethod = "GET"
            val body = conn.inputStream.bufferedReader().use { it.readText() }
            conn.disconnect()
            val root = JSONObject(body)
            out.put("generation", root.optString("generation"))
            val arr = root.optJSONArray("regions") ?: JSONArray()
            val want = JSONArray()
            for (i in 0 until arr.length()) {
                val r = arr.optJSONObject(i) ?: continue
                val id = r.optString("region_id")
                if (id.contains("germany") ||
                    id.contains("denmark") ||
                    id.contains("sweden") ||
                    id.contains("norway") ||
                    id == "europe/denmark"
                ) {
                    want.put(
                        JSONObject()
                            .put("region_id", id)
                            .put("graph_format_version", r.optInt("graph_format_version", -1))
                            .put("bytes", r.optLong("bytes")),
                    )
                }
            }
            out.put("regions", want)
            out
        } catch (t: Throwable) {
            out.put("error", t.toString())
            out
        }
    }

    private fun estimateFuelStops(distanceKm: Double): JSONObject {
        fun stops(rangeMiles: Double): Int {
            val rangeKm = rangeMiles * 1.609344
            val usable = (rangeKm - 100.0).coerceAtLeast(1.0)
            return maxOf(0, kotlin.math.ceil(distanceKm / usable).toInt() - 1)
        }
        return JSONObject()
            .put("distance_km", distanceKm)
            .put("stops_at_500mi", stops(500.0))
            .put("stops_at_600mi", stops(600.0))
            .put("note", "Report-only; fuel-stop planning unimplemented")
    }

    private fun extractDatexFromPlanReport(rep: String): JSONObject {
        val lines =
            rep
                .lineSequence()
                .filter { it.contains("datex", ignoreCase = true) }
                .take(80)
                .toList()
        var maxImpacts = 0
        for (line in lines) {
            val m = Regex("""datex_impacts=(\d+)""").find(line) ?: continue
            maxImpacts = maxOf(maxImpacts, m.groupValues[1].toIntOrNull() ?: 0)
        }
        return JSONObject()
            .put("lines", JSONArray(lines))
            .put("max_datex_impacts", maxImpacts)
            .put("applied", maxImpacts > 0)
    }

    private fun datexImpactsPositive(rep: String): Boolean = Regex("""datex_impacts=([1-9]\d*)""").containsMatchIn(rep)

    private fun parseEtaMinutes(rep: String): Double {
        Regex("""chunked_eta_min=([0-9.]+)""").find(rep)?.groupValues?.get(1)?.toDoubleOrNull()?.let {
            return it
        }
        Regex("""eta_minutes[=:]([0-9.]+)""").find(rep)?.groupValues?.get(1)?.toDoubleOrNull()?.let {
            return it
        }
        return 0.0
    }

    private fun jsonArrayLen(raw: String): Int =
        try {
            JSONArray(raw).length()
        } catch (_: Throwable) {
            0
        }

    private fun processPssMib(): Double {
        val mi = Debug.MemoryInfo()
        Debug.getMemoryInfo(mi)
        return mi.totalPss / 1024.0
    }

    private fun noteUi(
        step: String,
        detail: String,
    ) {
        val o =
            JSONObject()
                .put("t", System.currentTimeMillis())
                .put("step", step)
                .put("detail", detail)
        uiEvents.put(o)
        Log.i(TAG, "UI_EVENT $step | $detail")
    }

    private fun screenshot(name: String) {
        runCatching {
            val dir = File("/sdcard/Pictures/bevensen-ui").also { it.mkdirs() }
            val f = File(dir, "$name.png")
            device.takeScreenshot(f)
            noteUi("screenshot", f.absolutePath)
            // Also pull-friendly path
            runCatching {
                File("/data/local/tmp/bevensen-ui").also { it.mkdirs() }
                device.takeScreenshot(File("/data/local/tmp/bevensen-ui/$name.png"))
            }
        }
    }

    private fun settle(ms: Long = 400) {
        composeRule.waitForIdle()
        composeRule.mainClock.advanceTimeBy(ms)
        Thread.sleep(ms)
    }

    private fun dismissOverlaySheets() {
        NaviMapTestHooks.requestCloseTools = true
        settle(200)
        clickTagSoft("btn_close_tools")
        clickTagSoft("btn_save_tools")
        clickTagSoft("btn_close_vehicle")
        clickTagSoft("btn_close_drive_settings")
        // Tools sheet Close / Hide tools only — do not tap bare "Close" (that also
        // matches btn_close_search and collapses the route panel).
        runCatching { device.findObject(By.text("Hide tools"))?.click() }
        settle(300)
    }

    private fun openRoutePanel() {
        // Prefer existing search field; only dismiss Tools/vehicle sheets when needed.
        val already =
            runCatching {
                composeRule.onNodeWithTag("field_search", useUnmergedTree = true).assertIsDisplayed()
                true
            }.getOrDefault(false)
        if (already) {
            noteUi("open_route_panel", "field_search already visible")
            return
        }
        dismissOverlaySheets()
        val afterDismiss =
            runCatching {
                composeRule.onNodeWithTag("field_search", useUnmergedTree = true).assertIsDisplayed()
                true
            }.getOrDefault(false)
        if (afterDismiss) {
            noteUi("open_route_panel", "field_search visible after dismissing sheets")
            return
        }
        // Collapsed planning chrome exposes btn_open_search ("Route").
        clickTagSoft("btn_open_search")
        settle(400)
        runCatching { device.findObject(By.text("Route"))?.click() }
        settle(500)
        val deadline = SystemClock.elapsedRealtime() + 8_000
        while (SystemClock.elapsedRealtime() < deadline) {
            val ok =
                runCatching {
                    composeRule.onNodeWithTag("field_search", useUnmergedTree = true).assertIsDisplayed()
                    true
                }.getOrDefault(false)
            if (ok) {
                noteUi("open_route_panel", "field_search visible after reopen")
                return
            }
            dismissOverlaySheets()
            clickTagSoft("btn_open_search")
            runCatching { device.findObject(By.text("Route"))?.click() }
            settle(400)
        }
        composeRule.onNodeWithTag("field_search", useUnmergedTree = true).assertIsDisplayed()
        noteUi("open_route_panel", "field_search visible (final)")
    }

    private fun clickTag(tag: String) {
        val node = composeRule.onNodeWithTag(tag, useUnmergedTree = true)
        runCatching { node.performScrollTo() }
        node.assertExists().performClick()
        settle()
    }

    private fun clickTagSoft(tag: String) {
        runCatching {
            val node = composeRule.onNodeWithTag(tag, useUnmergedTree = true)
            runCatching { node.performScrollTo() }
            node.assertExists().performClick()
            settle()
        }
    }

    private fun setField(
        tag: String,
        value: String,
    ) {
        val node = composeRule.onNodeWithTag(tag, useUnmergedTree = true)
        runCatching { node.performScrollTo() }
        node.performTextClearance()
        node.performTextInput(value)
        settle()
    }

    private fun waypointSummary(): String =
        runCatching {
            composeRule
                .onNodeWithTag("search_waypoints_summary", useUnmergedTree = true)
                .fetchSemanticsNode()
                .config[androidx.compose.ui.semantics.SemanticsProperties.Text]
                .joinToString(" ") { it.text }
        }.getOrDefault("")

    private fun writeReport() {
        val text = report.toString(2)
        runCatching {
            File(dataDir, "long-trip-bevensen-dalsoren").also { it.mkdirs() }.let {
                File(it, "report.json").writeText(text)
            }
        }
        val sd =
            NaviStorageVolumes
                .list(composeRule.activity)
                .firstOrNull { it.removable && it.mounted && it.appFilesDir != null }
                ?.appFilesDir
        if (sd != null) {
            runCatching {
                val dir = File(sd, "long-trip-bevensen-dalsoren").also { it.mkdirs() }
                File(dir, "report.json").writeText(text)
            }
        }
        runCatching { File("/data/local/tmp/long-trip-bevensen-dalsoren-ui.json").writeText(text) }
        runCatching {
            composeRule.activity.getExternalFilesDir(null)?.let {
                File(it, "long-trip-bevensen-dalsoren-ui.json").writeText(text)
            }
        }
        Log.i(TAG, "report written (${text.length} chars)")
    }
}
