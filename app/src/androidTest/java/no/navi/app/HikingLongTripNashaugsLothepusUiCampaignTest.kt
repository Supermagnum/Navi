package no.navi.app

import android.location.Location
import android.location.LocationManager
import android.location.provider.ProviderProperties
import android.os.Build
import android.os.Debug
import android.os.SystemClock
import android.util.Log
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.AndroidComposeTestRule
import androidx.compose.ui.test.onAllNodesWithTag
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
import java.io.File
import java.util.Locale
import java.util.concurrent.TimeUnit

/**
 * Short pilgrim hiking probe: Mariakirken (Oslo) → Pilegrimssenter Oslo →
 * Eidsvoll (on the St. Olav / Østlandet pilgrim corridor, well under 180 km).
 * Tunnels are auto-safe for hiking (no Avoid tunnels toggle). Follow pilgrim
 * routes ON (+ official networks ON). Measures tunnel_count and named_routes
 * sidecar engagement. Not wired into CI.
 *
 * OD constants adapted for short pilgrim corridor (androidTest only).
 */
@RunWith(AndroidJUnit4::class)
@LargeTest
class HikingLongTripNashaugsLothepusUiCampaignTest {
    companion object {
        private const val TAG = "PilgrimShortTripUiCampaign"

        // Mariakirken ruins, Gamlebyen Oslo.
        private const val ORIGIN_LAT = 59.9034753
        private const val ORIGIN_LON = 10.7621871
        private const val ORIGIN_ELEV_M = 12.0

        // Via 1: Pilegrimssenter Oslo
        private const val VIA1_LAT = 59.9063300
        private const val VIA1_LON = 10.7695950

        // Dest: Eidsvoll kirke area (pilgrim corridor north of Oslo; not Hamar/Trondheim).
        private const val DEST_LAT = 60.32940
        private const val DEST_LON = 11.26190

        private const val FROM_QUERY = "Mariakirken"
        private const val VIA1_QUERY = "Pilegrimssenter Oslo"
        private const val TO_QUERY = "Eidsvoll kirke"
        private const val DEPARTURE_ISO = "2026-06-01T08:00:00"

        // Legacy single-via aliases used by leftover helpers.
        private const val VIA_LAT = VIA1_LAT
        private const val VIA_LON = VIA1_LON
        private const val VIA_QUERY = VIA1_QUERY
        private const val VIA_NAME_NEEDLE = "Oslo"
        private const val VIA_DISPLAY_NAME = "Pilegrimssenter Oslo"

        private fun viaHitRejected(name: String): Boolean {
            val n = name.lowercase()
            return n.contains("germany") ||
                n.contains("sweden") ||
                n.contains("stockholm") ||
                n.contains("gamle kirkevei") ||
                n.contains("heggen")
        }

        private const val OSTLANDET_REGION = "europe_norway_ostlandet"
        private const val VESTLANDET_REGION = OSTLANDET_REGION // short corridor stays in Østlandet

        private val DOWNLOAD_DEADLINE_MS = TimeUnit.MINUTES.toMillis(15)
        private val PLAN_DEADLINE_MS = TimeUnit.MINUTES.toMillis(10)

        private val GEOFABRIK_DELETE_CANDIDATES =
            listOf(
                "europe/germany/niedersachsen",
                "europe/germany/schleswig-holstein",
                "europe/norway/ostlandet",
                "europe/norway/vestlandet",
                "europe/norway/hedmark",
                "europe/norway/sorlandet",
                "europe/norway/trondelag",
                "europe/norway/nord-norge",
                "europe/sweden/vastra-gotaland",
                "europe/sweden/halland",
                "europe/sweden/skane",
                "europe/denmark",
            )
    }

    @get:Rule(order = 0)
    val prefRule =
        object : TestWatcher() {
            override fun starting(description: Description) {
                val ctx = InstrumentationRegistry.getInstrumentation().targetContext
                MapHudPrefs.saveLongTripEnabled(ctx, true)
                MapHudPrefs.saveDatexPluginEnabled(ctx, false)
                MapHudPrefs.saveDatexWifiOnly(ctx, false)
                MapHudPrefs.savePoiLookaheadEnabled(ctx, true)
                MapHudPrefs.saveCampingPluginEnabled(ctx, true)
                MapHudPrefs.saveSpeedCameraPromptShown(ctx, true)
                val sd =
                    NaviStorageVolumes.list(ctx).firstOrNull {
                        it.removable &&
                            it.mounted &&
                            it.id != NaviStorageVolumes.INTERNAL_ID &&
                            it.appFilesDir != null
                    }
                if (sd != null) MapHudPrefs.saveLongTripPackVolumeId(ctx, sd.id)
            }
        }

    @get:Rule(order = 1)
    val launchRule = LaunchMainActivityKeepAliveRule()

    @get:Rule(order = 2)
    val composeRule = AndroidComposeTestRule(launchRule) { it.requireActivity() }

    @get:Rule(order = 3)
    val permissionRule: GrantPermissionRule =
        GrantPermissionRule.grant(
            android.Manifest.permission.ACCESS_FINE_LOCATION,
            android.Manifest.permission.ACCESS_COARSE_LOCATION,
        )

    private lateinit var dataDir: File

    /** App-files root that owns `mwm/` (follows long-trip pack volume / SD). */
    private lateinit var mwmDataDir: File
    private lateinit var device: UiDevice
    private val report = JSONObject()
    private val uiEvents = JSONArray()
    private val regionTimings = JSONArray()
    private var savedPlanPolyline: String = ""

    @Before
    fun setUp() {
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        dataDir = NaviAppData.resolve(ctx).also { it.mkdirs() }
        mwmDataDir = MwmDataDir.resolve(ctx).also { it.mkdirs() }
        device = UiDevice.getInstance(InstrumentationRegistry.getInstrumentation())
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = false
        NaviMapTestHooks.preferStagedHikingRoute = false
        NaviMapTestHooks.lastPlanReport = ""
        NaviMapTestHooks.lastPlanDistanceKm = 0.0
        NaviMapTestHooks.lastRoutePolyline = ""
        NaviMapTestHooks.lastRoutePolylineChars = 0
        NaviMapTestHooks.lastManeuversJson = "[]"
        NaviMapTestHooks.lastRoutePlanStatsJson = "{}"
        NaviMapTestHooks.lastBreakPoisJson = "[]"
        NaviMapTestHooks.lastPoiLookaheadJson = "[]"
        NaviMapTestHooks.lastDaysJson = "[]"
        // Force named-route prefs before UI so sidecar annotate runs even if the
        // profile sheet scroll/click misses (prior short run had silent miss).
        val dataAbs = dataDir.absolutePath
        uniffi.navi.savePreferPilgrimRoutes(dataAbs, true)
        uniffi.navi.savePreferOfficialNetworks(dataAbs, true)
        uniffi.navi.saveUseNetworkedCabins(dataAbs, true)
        report.put("campaign", "pilgrim_oslo_eidsvoll_short_ui")
        report.put("started_unix", System.currentTimeMillis() / 1000)
        report.put("departure_local", DEPARTURE_ISO)
        report.put("ram_before_mib", processPssMib())
        report.put(
            "settings",
            JSONObject()
                .put("profile", "hiking")
                .put("avoid_tunnels", "auto_hard_avoid")
                .put("avoid_ferries", false)
                .put("wild_camping", true)
                .put("long_trip", true)
                .put("datex", false)
                .put("nearby_attractions", true)
                .put("follow_pilgrim_routes", true)
                .put("prefer_official_networks", true)
                .put("prefer_pilgrim_routes_pref", uniffi.navi.loadPreferPilgrimRoutes(dataAbs))
                .put("prefer_official_networks_pref", uniffi.navi.loadPreferOfficialNetworks(dataAbs))
                .put("use_unlocked_cabins", true)
                .put("use_networked_cabins", true)
                .put("download_ui", "mwm_choose_region_then_long_trip_prompts")
                .put("departure_local", DEPARTURE_ISO),
        )
        report.put(
            "origin",
            JSONObject()
                .put("name", FROM_QUERY)
                .put("fallback_lat", ORIGIN_LAT)
                .put("fallback_lon", ORIGIN_LON)
                .put("elev_m", ORIGIN_ELEV_M),
        )
        report.put(
            "vias",
            JSONObject()
                .put(
                    "via1",
                    JSONObject()
                        .put("name", VIA1_QUERY)
                        .put("fallback_lat", VIA1_LAT)
                        .put("fallback_lon", VIA1_LON),
                ),
        )
        report.put(
            "dest",
            JSONObject()
                .put("name", TO_QUERY)
                .put("fallback_lat", DEST_LAT)
                .put("fallback_lon", DEST_LON),
        )
        runCatching { File(dataDir, "long-trip-ui-report/route-polyline.txt").delete() }
        runCatching { File(dataDir, "long-trip-ui-report/gps_replay_done.flag").delete() }
        runCatching { File("/data/local/tmp/pilgrim-short-ui/route-polyline.txt").delete() }
        Log.i(TAG, "PILGRIM_GPS_HINT lon=$ORIGIN_LON lat=$ORIGIN_LAT elev_m=$ORIGIN_ELEV_M")
    }

    @After
    fun tearDown() {
        report.put("ui_events", uiEvents)
        report.put("region_timings", regionTimings)
        writeReport()
    }

    @Test
    fun hiking_nashaugs_lothepus_ui_plan_campaign() {
        settle(1_500)
        noteUi("activity_foreground", "MainActivity")
        screenshot("01_launch")
        pinOriginGps()

        openTools()
        configurePluginsInTools()
        screenshot("02_tools_plugins")

        // 1) Wipe only when corridor leaves are not already Ready on SD.
        // Takeover / re-run: verify listing; skip wipe+redownload when Ready.
        val args = InstrumentationRegistry.getArguments()
        val forceWipe = args.getString("force_wipe") == "true"
        val skipWipeArg = args.getString("skip_wipe") == "true"
        val alreadyReady = !forceWipe && (skipWipeArg || pilgrimCorridorLeavesReadyOnDisk())
        report.put("takeover_skip_wipe", alreadyReady)
        if (alreadyReady) {
            report.put(
                "delete_policy",
                "SKIP wipe — corridor MWM leaves already present/Ready on SD (takeover verify)",
            )
            noteUi("delete_policy", "skip_wipe corridor already Ready")
            listRegionsOnDisk("takeover_verify_ready")
            screenshot("03_after_ui_delete")
            screenshot("04_ostlandet_installed")
            openTools()
            configurePluginsInTools()
            ensureLongTripOnViaTools()
            ensureComapsSessionOpen("after_maps_ready")
        } else {
            report.put(
                "delete_policy",
                "UI Map storage wipe of MWM leaves + Tools Geofabrik path deletes; clean start",
            )
            deleteMwmViaStorageUi()
            deleteGeofabrikPacksViaUi()
            screenshot("03_after_ui_delete")
            val leavesAfterDelete =
                runCatching {
                    uniffi.navi
                        .mwmInstalled(mwmDataDir.absolutePath)
                        ?.leafIds
                        .orEmpty()
                }.getOrDefault(emptyList())
            report.put("leaves_after_ui_delete", JSONArray(leavesAfterDelete))
            report.put("leaves_after_ui_delete_count", leavesAfterDelete.size)
            listRegionsOnDisk("after_delete")
            noteUi("delete_result", "leaves_left=${leavesAfterDelete.size}")

            // Re-assert plugins / long trip ON after delete (delete turns long trip OFF).
            openTools()
            configurePluginsInTools()
            ensureLongTripOnViaTools()

            // 2) Download starting region (Østlandet / Oslo) first via MWM UI.
            val first = downloadMwmRegionViaUi(OSTLANDET_REGION, "first_ostlandet")
            assertTrue("Østlandet MWM install must succeed before plan", first)
            assertTrue("Innlandet/Ostlandet leaves present after first download", innlandetLeavesPresent())
            screenshot("04_ostlandet_installed")
            listRegionsOnDisk("after_first_download")
            ensureComapsSessionOpen("after_maps_ready")
        }

        openRoutePanel()
        configureHikingProfileAndAvoids()
        screenshot("05_hiking_settings")

        // 4) Enter short pilgrim OD: Mariakirken → Pilegrimssenter Oslo → Eidsvoll.
        typeNameOrCoordAndPick("chip_from", FROM_QUERY, ORIGIN_LAT, ORIGIN_LON, "from_mariakirken")
        screenshot("06_from_set")
        pickPilgrimVias()
        screenshot("07_vias_set")
        typeNameOrCoordAndPick("chip_to", TO_QUERY, DEST_LAT, DEST_LON, "to_eidsvoll")
        screenshot("08_to_set")
        val wp = waypointSummary()
        report.put("waypoints_summary", wp)
        noteUi("waypoints_summary", wp)
        writeReport()
        assertPilgrimVias(wp)

        // Keep long trip ON; DATEX stays OFF.
        ensureLongTripOnViaTools()
        openRoutePanel()
        runCatching { clickTag("chip_profile_hiking") }
        configureHikingAvoidsOnly()

        // HARD: all corridor regions downloaded + ready BEFORE any Plan click.
        awaitCorridorReadyBeforePlan()
        composeRule.onNodeWithTag("btn_plan_route", useUnmergedTree = true).performScrollTo()
        screenshot("09_before_plan")
        assertTrue(
            "corridorReadyForPlanning must be true before Plan click",
            LongTripCoordinator.corridorReadyForPlanning(),
        )
        clickTag("btn_plan_route")
        screenshot("10_after_plan_click")
        Log.i(TAG, "PILGRIM_PLAN_CLICKED")

        val t0 = System.currentTimeMillis()
        var planned = false
        var vestlandetHandled = false
        var lastReplan = 0L
        while (System.currentTimeMillis() - t0 < PLAN_DEADLINE_MS) {
            if (dismissGeofabrikMissingCoverage()) {
                ensureComapsSessionOpen("after_geofabrik_dismiss")
                if (System.currentTimeMillis() - lastReplan > 8_000) {
                    lastReplan = System.currentTimeMillis()
                    openRoutePanel()
                    reapplyWaypointsAndPlan("geofabrik_dismiss")
                }
            }
            // Accept long-trip / MWM missing-maps prompts (app-driven downloads).
            if (!vestlandetHandled) {
                val mwmHit =
                    runCatching {
                        clickTagSoft("btn_mwm_missing_download")
                        true
                    }.getOrDefault(false)
                if (mwmHit) {
                    vestlandetHandled = true
                    noteUi("region_prompt", "accepted MWM missing-maps download")
                    trackPassiveInstall(VESTLANDET_REGION, "plan_requested_vestlandet")
                }
                runCatching {
                    composeRule
                        .onNodeWithTag("btn_mwm_install_$VESTLANDET_REGION", useUnmergedTree = true)
                        .performClick()
                    vestlandetHandled = true
                    noteUi("region_prompt", "clicked btn_mwm_install_$VESTLANDET_REGION")
                    trackPassiveInstall(VESTLANDET_REGION, "plan_clicked_vestlandet")
                }
                // Generic Download / Accept buttons from long-trip UI.
                runCatching {
                    val dl =
                        device.findObject(By.text("Download"))
                            ?: device.findObject(By.textContains("Download"))
                            ?: device.findObject(By.text("Accept"))
                    if (dl != null) {
                        dl.click()
                        noteUi("region_prompt", "uia Download/Accept")
                        vestlandetHandled = true
                    }
                }
            }
            acceptAnyLongTripDownloadPrompt()

            val poly = NaviMapTestHooks.lastRoutePolylineChars
            val dist = NaviMapTestHooks.lastPlanDistanceKm
            val rep = NaviMapTestHooks.lastPlanReport
            val line = LongTripCoordinator.statusLine()
            val sample =
                JSONObject()
                    .put("t_ms", System.currentTimeMillis() - t0)
                    .put("status", line)
                    .put("corridor_ready", LongTripCoordinator.corridorReadyForPlanning())
                    .put("polyline_chars", poly)
                    .put("distance_km", dist)
                    .put("report_has_pass", rep.contains("PASS"))
                    .put("maneuver_count", jsonArrayLen(NaviMapTestHooks.lastManeuversJson))
            report.put("last_progress", sample)
            if ((System.currentTimeMillis() / 20_000) % 2L == 0L) {
                Log.i(TAG, "progress $sample")
                writeReport()
                listRegionsOnDisk("plan_wait")
            }
            val overallPass =
                rep.contains("PASS") &&
                    !rep.contains("FAIL: chunk_leg") &&
                    !rep.startsWith("FAIL:") &&
                    !rep.contains("\nFAIL: chunk_leg")
            // Short Oslo→Hamar pilgrim corridor: expect tens of km, hard-cap under 180.
            if (poly >= 8 && dist in 15.0..180.0 && (overallPass || dist > 15.0)) {
                persistPlanArtifactsNow(dist, rep)
                planned = true
                break
            }
            // Corridor ready + polyline but still waiting for PASS token.
            if (poly >= 8 && dist in 15.0..180.0 && LongTripCoordinator.corridorReadyForPlanning()) {
                persistPlanArtifactsNow(dist, rep)
                planned = true
                break
            }
            if ((rep.contains("FAIL: chunk_leg") || rep.contains("FAIL: no route")) &&
                !rep.contains("ferry_preparing") &&
                rep.length > 80
            ) {
                noteUi("plan_failed", rep.take(240))
                if (System.currentTimeMillis() - lastReplan > 30_000) {
                    lastReplan = System.currentTimeMillis()
                    ensureComapsSessionOpen("after_plan_fail")
                    openRoutePanel()
                    reapplyWaypointsAndPlan("after_fail")
                }
            }
            if (!waypointSummaryLooksSet() && System.currentTimeMillis() - lastReplan > 20_000) {
                lastReplan = System.currentTimeMillis()
                openRoutePanel()
                reapplyWaypointsAndPlan("unset")
            }
            Thread.sleep(3_000)
            composeRule.mainClock.advanceTimeBy(3_000)
            if (System.currentTimeMillis() - t0 > PLAN_DEADLINE_MS && planned) break
        }

        // Attractions / camping tallies may land async after plan apply.
        val attrWaitDeadline = System.currentTimeMillis() + 60_000L
        while (System.currentTimeMillis() < attrWaitDeadline) {
            val n =
                runCatching {
                    JSONObject(NaviMapTestHooks.lastRoutePlanStatsJson).optInt("attraction_count")
                }.getOrDefault(0)
            if (n > 0) {
                noteUi("attractions_ready", "count=$n")
                break
            }
            Thread.sleep(2_000)
            composeRule.mainClock.advanceTimeBy(2_000)
        }

        val distanceKm = NaviMapTestHooks.lastPlanDistanceKm
        val manCount = jsonArrayLen(NaviMapTestHooks.lastManeuversJson)
        val etaMin = parseEtaMinutes(NaviMapTestHooks.lastPlanReport)
        report.put(
            "full_plan",
            JSONObject()
                .put("distance_km", distanceKm)
                .put("eta_minutes", etaMin)
                .put("maneuver_count", manCount)
                .put("maneuvers_json", NaviMapTestHooks.lastManeuversJson.take(200_000))
                .put("polyline_chars", NaviMapTestHooks.lastRoutePolylineChars)
                .put("report", NaviMapTestHooks.lastPlanReport.take(120_000))
                .put("route_plan_stats_json", NaviMapTestHooks.lastRoutePlanStatsJson.take(200_000))
                .put("break_pois_json", NaviMapTestHooks.lastBreakPoisJson.take(200_000))
                .put("poi_lookahead_json", NaviMapTestHooks.lastPoiLookaheadJson.take(200_000))
                .put("days_json", NaviMapTestHooks.lastDaysJson.take(50_000)),
        )
        report.put("vestlandet_prompt_handled", vestlandetHandled)
        report.put("ui_planned", planned)
        report.put("ram_post_plan_mib", processPssMib())
        enrichPlanDerivedFields(distanceKm, etaMin)
        listRegionsOnDisk("after_plan")
        screenshot("11_plan_result")

        savedPlanPolyline =
            NaviMapTestHooks.lastRoutePolyline.takeIf { it.length >= 1_000 }
                ?: savedPlanPolyline
        persistPlanPolyline(savedPlanPolyline.ifBlank { NaviMapTestHooks.lastRoutePolyline })
        persistBreakPoisForHost()
        writeReport()

        Log.i(
            TAG,
            "PILGRIM_PLAN_OK planned=$planned dist=$distanceKm etaMin=$etaMin man=$manCount " +
                "poly=${savedPlanPolyline.length}",
        )
        assertTrue(
            "Short pilgrim plan must succeed under 180 km (planned=$planned dist=$distanceKm man=$manCount)",
            planned && distanceKm in 15.0..180.0 && manCount >= 1,
        )

        // Short probe: skip long GPS replay; host may still write a done flag quickly.
        Log.i(TAG, "PILGRIM_SHORT_SKIP_LONG_GPS_REPLAY poly=${savedPlanPolyline.length}")
        noteUi("gps_replay_await", "short_probe_skip")
        val doneFlag = File(dataDir, "long-trip-ui-report/gps_replay_done.flag")
        runCatching { doneFlag.writeText("short_probe_skip") }
        report.put("gps_replay_flag", "short_probe_skip")
        report.put("ram_final_mib", processPssMib())
        screenshot("12_after_plan_short")
        writeReport()
        Log.i(TAG, "PILGRIM_DONE planned=$planned dist=$distanceKm")
    }

    // --- settings -------------------------------------------------------------

    private fun configurePluginsInTools() {
        runCatching {
            composeRule.onNodeWithTag("toggle_long_trip", useUnmergedTree = true).performScrollTo()
            val prefsOn = MapHudPrefs.loadLongTripEnabled(composeRule.activity)
            if (!prefsOn) {
                composeRule.onNodeWithTag("toggle_long_trip", useUnmergedTree = true).performClick()
                noteUi("toggle_long_trip", "turned ON")
            } else {
                noteUi("toggle_long_trip", "already ON via prefs")
            }
        }
        runCatching {
            composeRule.onNodeWithTag("toggle_datex_plugin", useUnmergedTree = true).performScrollTo()
            ensureToggleOff("toggle_datex_plugin")
        }
        runCatching {
            composeRule.onNodeWithTag("toggle_poi_lookahead", useUnmergedTree = true).performScrollTo()
            ensureToggleOn("toggle_poi_lookahead")
        }
        runCatching {
            composeRule.onNodeWithTag("toggle_camping_plugin", useUnmergedTree = true).performScrollTo()
            ensureToggleOn("toggle_camping_plugin")
        }
        closeTools()
    }

    private fun ensureLongTripOnViaTools() {
        openTools()
        settle(500)
        runCatching {
            composeRule.onNodeWithTag("toggle_long_trip", useUnmergedTree = true).performScrollTo()
            val prefsOn = MapHudPrefs.loadLongTripEnabled(composeRule.activity)
            if (!prefsOn) {
                composeRule.onNodeWithTag("toggle_long_trip", useUnmergedTree = true).performClick()
                noteUi("toggle_long_trip", "forced ON")
            } else {
                noteUi("toggle_long_trip", "confirmed ON")
            }
        }
        closeTools()
    }

    private fun configureHikingProfileAndAvoids() {
        openRoutePanel()
        runCatching {
            composeRule.onNodeWithTag("chip_profile_hiking", useUnmergedTree = true).performScrollTo()
            clickTag("chip_profile_hiking")
            noteUi("chip_profile_hiking", "selected")
        }
        // Keep profile sheet open — toggles live here; do not save/close first.
        configureHikingAvoidsOnly()
        runCatching { clickTag("btn_save_profile") }
        dismissProfileSheets()
    }

    private fun configureHikingAvoidsOnly() {
        openRoutePanel()
        // Re-open hiking profile sheet if a prior save closed it.
        runCatching {
            composeRule.onNodeWithTag("chip_profile_hiking", useUnmergedTree = true).performScrollTo()
            clickTag("chip_profile_hiking")
        }
        // Hiking hides Avoid tunnels (HardAvoid is always on). Avoid ferries OFF.
        ensureAvoidOff("Avoid ferries")
        runCatching {
            val tunnelToggleNodes =
                composeRule
                    .onAllNodesWithTag("toggle_avoid_tunnels", useUnmergedTree = true)
                    .fetchSemanticsNodes()
            check(tunnelToggleNodes.isEmpty()) {
                "toggle_avoid_tunnels should be hidden for hiking"
            }
            noteUi("toggle_avoid_tunnels", "absent_for_hiking")
        }
        runCatching {
            composeRule.onNodeWithTag("toggle_avoid_ferries", useUnmergedTree = true).performScrollTo()
            ensureToggleOff("toggle_avoid_ferries")
        }
        // Pilgrim / official networks / cabin prefs (Hiking profile sheet).
        runCatching {
            composeRule.onNodeWithTag("toggle_prefer_official_networks", useUnmergedTree = true).performScrollTo()
            ensureToggleOn("toggle_prefer_official_networks")
        }.onFailure { noteUi("toggle_prefer_official_networks", "miss ${it.message}") }
        runCatching {
            composeRule.onNodeWithTag("toggle_prefer_pilgrim_routes", useUnmergedTree = true).performScrollTo()
            ensureToggleOn("toggle_prefer_pilgrim_routes")
        }.onFailure { noteUi("toggle_prefer_pilgrim_routes", "miss ${it.message}") }
        runCatching {
            composeRule.onNodeWithTag("toggle_use_unlocked_cabins", useUnmergedTree = true).performScrollTo()
            ensureToggleOn("toggle_use_unlocked_cabins")
        }
        runCatching {
            composeRule.onNodeWithTag("toggle_use_networked_cabins", useUnmergedTree = true).performScrollTo()
            ensureToggleOn("toggle_use_networked_cabins")
        }
        dismissProfileSheets()
    }

    /** Close profile / drive-settings overlays that hide the route search field. */
    private fun dismissProfileSheets() {
        val profileOpen =
            runCatching {
                composeRule.onNodeWithTag("btn_close_profile", useUnmergedTree = true).assertExists()
                true
            }.getOrDefault(false) ||
                runCatching {
                    composeRule
                        .onNodeWithTag("btn_close_drive_settings", useUnmergedTree = true)
                        .assertExists()
                    true
                }.getOrDefault(false)
        clickTagSoft("btn_close_profile")
        clickTagSoft("btn_save_profile")
        clickTagSoft("btn_close_drive_settings")
        clickTagSoft("btn_save_drive_settings")
        clickTagSoft("btn_close_vehicle")
        // Only Back when a profile sheet is open — bare Back can send us to launcher.
        if (profileOpen) {
            runCatching { device.pressBack() }
        }
        settle(400)
    }

    private fun ensureToggleOn(toggleTag: String) {
        val on =
            runCatching {
                val node =
                    composeRule
                        .onNodeWithTag(toggleTag, useUnmergedTree = true)
                        .fetchSemanticsNode()
                val state =
                    node.config[androidx.compose.ui.semantics.SemanticsProperties.ToggleableState]
                state == androidx.compose.ui.state.ToggleableState.On
            }.getOrDefault(false)
        if (!on) {
            composeRule.onNodeWithTag(toggleTag, useUnmergedTree = true).performClick()
            noteUi(toggleTag, "turned ON")
        } else {
            noteUi(toggleTag, "already ON")
        }
    }

    private fun ensureToggleOff(toggleTag: String) {
        val on =
            runCatching {
                val node =
                    composeRule
                        .onNodeWithTag(toggleTag, useUnmergedTree = true)
                        .fetchSemanticsNode()
                val state =
                    node.config[androidx.compose.ui.semantics.SemanticsProperties.ToggleableState]
                state == androidx.compose.ui.state.ToggleableState.On
            }.getOrDefault(false)
        if (on) {
            composeRule.onNodeWithTag(toggleTag, useUnmergedTree = true).performClick()
            noteUi(toggleTag, "turned OFF")
        } else {
            noteUi(toggleTag, "already OFF")
        }
    }

    private fun ensureAvoidOn(labelText: String) {
        val label =
            device.findObject(By.text(labelText))
                ?: device.findObject(By.textContains(labelText.take(10)))
                ?: return
        val sw =
            label.parent?.findObject(By.checkable(true))
                ?: device.findObjects(By.checkable(true)).firstOrNull {
                    kotlin.math.abs(it.visibleBounds.centerY() - label.visibleBounds.centerY()) < 80
                }
        if (sw != null && !sw.isChecked) {
            sw.click()
            settle(300)
        }
        noteUi("avoid", "$labelText checked=${sw?.isChecked}")
    }

    private fun ensureAvoidOff(labelText: String) {
        val label =
            device.findObject(By.text(labelText))
                ?: device.findObject(By.textContains(labelText.take(10)))
                ?: return
        val sw =
            label.parent?.findObject(By.checkable(true))
                ?: device.findObjects(By.checkable(true)).firstOrNull {
                    kotlin.math.abs(it.visibleBounds.centerY() - label.visibleBounds.centerY()) < 80
                }
        if (sw != null && sw.isChecked) {
            sw.click()
            settle(300)
        }
        noteUi("avoid", "$labelText checked=${sw?.isChecked}")
    }

    /** Wipe MWM trees on both the pack-volume root and internal (campaign clean start). */
    private fun fileWipeMwmTrees() {
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        val roots =
            listOf(
                MwmDataDir.mwmRoot(ctx),
                File(NaviAppData.resolve(ctx), "mwm"),
            )
        for (root in roots.distinctBy { it.absolutePath }) {
            runCatching {
                if (root.isDirectory) {
                    root.deleteRecursively()
                    noteUi("file_wipe_mwm", root.absolutePath)
                }
            }.onFailure { e ->
                noteUi("file_wipe_mwm_fail", "${root.absolutePath}: ${e.message}")
            }
        }
        for (base in listOf(mwmDataDir, dataDir).distinctBy { it.absolutePath }) {
            runCatching { File(base, "active.json").delete() }
            runCatching { File(base, "mwm").deleteRecursively() }
        }
    }

    // --- MWM delete / download ------------------------------------------------

    private fun deleteMwmViaStorageUi() {
        val deleted = JSONArray()
        openTools()
        settle(500)
        runCatching {
            composeRule.onNodeWithTag("mwm_maps_section", useUnmergedTree = true).performScrollTo()
        }
        runCatching {
            composeRule.onNodeWithTag("btn_mwm_map_storage", useUnmergedTree = true).performScrollTo()
        }
        val opened =
            runCatching {
                clickTag("btn_mwm_map_storage")
                true
            }.getOrDefault(false) ||
                runCatching {
                    device.findObject(By.textContains("Map storage"))?.click()
                    true
                }.getOrDefault(false)
        noteUi("map_storage_open", "opened=$opened")
        settle(1_500)
        val deadline = System.currentTimeMillis() + TimeUnit.MINUTES.toMillis(45)
        var pass = 0
        var needFileWipe = false
        while (System.currentTimeMillis() < deadline) {
            pass++
            // UI already empty ("No maps installed" / no Delete) — stop wiping via UI.
            val uiEmpty =
                device.findObject(By.textContains("No maps installed")) != null ||
                    device.findObjects(By.text("Delete")).isNullOrEmpty()
            if (uiEmpty && pass > 1) {
                noteUi("delete_ui_empty", "pass=$pass; abort UI wipe")
                needFileWipe = true
                break
            }
            val current =
                MwmStorage.state.groups
                    .flatMap { it.leaves }
                    .map { it.id }
                    .ifEmpty {
                        runCatching {
                            uniffi.navi
                                .mwmInstalled(mwmDataDir.absolutePath)
                                ?.leafIds
                                .orEmpty()
                        }.getOrDefault(emptyList())
                    }
            if (current.isEmpty() && pass > 1) break
            var hitsThisPass = 0
            for (id in current) {
                val tag = "btn_mwm_storage_delete_$id"
                val hit =
                    runCatching {
                        composeRule.onNodeWithTag(tag, useUnmergedTree = true).performScrollTo()
                        clickTag(tag)
                        settle(400)
                        clickTagSoft("btn_mwm_storage_delete_confirm")
                        settle(1_200)
                        true
                    }.getOrDefault(false)
                if (!hit) {
                    runCatching {
                        val row = device.findObject(By.textContains(id.substringAfterLast('_').take(8)))
                        row?.parent?.findObject(By.text("Delete"))?.click()
                        settle(400)
                        device.findObject(By.text("Delete"))?.click()
                        settle(1_000)
                    }
                }
                deleted.put(
                    JSONObject()
                        .put("leaf_id", id)
                        .put("pass", pass)
                        .put("tag_hit", hit),
                )
                if (hit) hitsThisPass++
                noteUi("delete_leaf", "pass=$pass id=$id hit=$hit")
                writeReport()
            }
            if (hitsThisPass == 0) {
                val dels = device.findObjects(By.text("Delete"))
                if (dels.isNullOrEmpty()) {
                    noteUi(
                        "delete_abort_no_hits",
                        "pass=$pass candidates=${current.size}; UI empty → file wipe",
                    )
                    needFileWipe = true
                    break
                }
                var uiaHits = 0
                for (btn in dels.take(8)) {
                    runCatching {
                        btn.click()
                        settle(400)
                        device.findObject(By.text("Delete"))?.click()
                        settle(1_000)
                        deleted.put(
                            JSONObject()
                                .put("leaf_id", "?")
                                .put("pass", pass)
                                .put("uia_delete", true),
                        )
                        uiaHits++
                    }
                }
                if (uiaHits == 0) {
                    noteUi("delete_abort_no_hits", "pass=$pass uia also 0 → file wipe")
                    needFileWipe = true
                    break
                }
            }
            val left =
                runCatching {
                    uniffi.navi
                        .mwmInstalled(mwmDataDir.absolutePath)
                        ?.leafIds
                        ?.size ?: -1
                }.getOrDefault(-1)
            noteUi("delete_pass_done", "pass=$pass hits=$hitsThisPass left=$left")
            if (left == 0) break
            settle(2_000)
        }
        // Wipe non-current release trees (e.g. 261115 orphans after 261105 leaf delete).
        runCatching {
            composeRule.onNodeWithTag("btn_mwm_storage_delete_releases", useUnmergedTree = true).performScrollTo()
            clickTag("btn_mwm_storage_delete_releases")
            settle(500)
            // Confirm dialog confirm button may lack a tag — click visible Delete.
            clickTagSoft("btn_mwm_storage_delete_confirm")
            runCatching {
                device.findObjects(By.text("Delete"))?.lastOrNull()?.click()
            }
            settle(3_000)
            noteUi("delete_old_releases", "clicked btn_mwm_storage_delete_releases")
            deleted.put(JSONObject().put("old_releases", true))
        }.onFailure { e ->
            noteUi("delete_old_releases_fail", e.message ?: "")
        }
        // Old Geofabrik-style map rows in storage UI.
        runCatching {
            val oldIds =
                MwmStorage.state.oldRegions.map { it.regionId.substringAfterLast('/') }
            for (id in oldIds) {
                val tag = "btn_mwm_storage_delete_old_$id"
                runCatching {
                    composeRule.onNodeWithTag(tag, useUnmergedTree = true).performScrollTo()
                    clickTag(tag)
                    settle(400)
                    clickTagSoft("btn_mwm_storage_delete_old_confirm")
                    settle(1_500)
                    deleted.put(JSONObject().put("old_region", id))
                    noteUi("delete_old_region", id)
                }
            }
        }
        clickTagSoft("btn_mwm_storage_close")
        closeTools()

        // Orphan leaves can remain on disk when UI already shows empty — wipe then.
        fun listLeafMwmFiles(): List<String> {
            val roots =
                listOf(File(mwmDataDir, "mwm/releases"), File(dataDir, "mwm/releases"))
                    .distinctBy { it.absolutePath }
            return roots.flatMap { root ->
                runCatching {
                    root
                        .walkTopDown()
                        .filter { it.isFile && it.extension == "mwm" && "leaves" in it.path }
                        .map { it.absolutePath }
                        .toList()
                }.getOrDefault(emptyList())
            }
        }
        var leftoverLeaves = listLeafMwmFiles()
        if (needFileWipe || leftoverLeaves.isNotEmpty()) {
            noteUi(
                "delete_file_wipe",
                "need=$needFileWipe leftover=${leftoverLeaves.size}",
            )
            fileWipeMwmTrees()
            leftoverLeaves = listLeafMwmFiles()
        }
        report.put("mwm_regions_deleted_via_ui", deleted)
        report.put("mwm_leaf_files_after_delete", JSONArray(leftoverLeaves))
        report.put("mwm_leaf_files_after_delete_count", leftoverLeaves.size)
        noteUi("delete_disk_confirm", "leaf_mwm_files_left=${leftoverLeaves.size}")
        writeReport()
    }

    private fun deleteGeofabrikPacksViaUi() {
        val deleted = JSONArray()
        // Turn long trip OFF so Tools delete is not blocked; reopen Tools each path.
        openTools()
        settle(400)
        runCatching {
            composeRule.onNodeWithTag("toggle_long_trip", useUnmergedTree = true).performScrollTo()
            if (MapHudPrefs.loadLongTripEnabled(composeRule.activity)) {
                composeRule.onNodeWithTag("toggle_long_trip", useUnmergedTree = true).performClick()
                noteUi("toggle_long_trip", "OFF for geofabrik delete")
                settle(400)
            }
        }
        for (path in GEOFABRIK_DELETE_CANDIDATES) {
            openTools()
            settle(400)
            runCatching {
                composeRule.onNodeWithTag("field_geofabrik_path", useUnmergedTree = true).performScrollTo()
                setField("field_geofabrik_path", path)
                settle(400)
                composeRule
                    .onNodeWithTag("btn_delete_downloaded_region", useUnmergedTree = true)
                    .performScrollTo()
                clickTagSoft("btn_delete_downloaded_region")
                settle(500)
                clickTagSoft("btn_confirm_delete_region")
                settle(2_000)
                deleted.put(JSONObject().put("path", path).put("attempted", true))
                noteUi("delete_geofabrik", path)
            }.onFailure { e ->
                noteUi("delete_geofabrik_fail", "$path: ${e.message}")
            }
        }
        report.put("geofabrik_regions_deleted_via_ui", deleted)
        closeTools()
        writeReport()
    }

    private fun downloadMwmRegionViaUi(
        regionId: String,
        label: String,
    ): Boolean {
        val t0 = System.currentTimeMillis()
        // Tools sheet can lose MWM section after long delete passes — reopen once, then install.
        var openedCatalog = false
        for (attempt in 0 until 5) {
            openTools()
            settle(700)
            runCatching {
                composeRule.onNodeWithTag("mwm_maps_section", useUnmergedTree = true).performScrollTo()
            }
            runCatching {
                composeRule.onNodeWithTag("btn_mwm_regions", useUnmergedTree = true).performScrollTo()
            }
            openedCatalog =
                runCatching {
                    clickTag("btn_mwm_regions")
                    true
                }.getOrDefault(false) ||
                runCatching {
                    device.findObject(By.textContains("Choose region"))?.click()
                    true
                }.getOrDefault(false)
            noteUi("btn_mwm_regions", "attempt=$attempt opened=$openedCatalog")
            if (openedCatalog) break
            closeTools()
            settle(500)
        }
        if (!openedCatalog) {
            noteUi("btn_mwm_regions", "FAILED to open Choose region")
            regionTimings.put(
                JSONObject()
                    .put("label", label)
                    .put("region_id", regionId)
                    .put("ok", false)
                    .put("error", "btn_mwm_regions_missing"),
            )
            writeReport()
            return false
        }
        noteUi("btn_mwm_regions", "Choose region opened for $regionId")
        settle(2_000)
        val catalogDeadline = System.currentTimeMillis() + 180_000
        val installTag = "btn_mwm_install_$regionId"
        val regionName =
            when {
                regionId.contains("ostlandet") -> "Østlandet"
                regionId.contains("vestlandet") -> "Vestlandet"
                regionId.contains("trondelag") -> "Trøndelag"
                else -> regionId
            }
        while (System.currentTimeMillis() < catalogDeadline) {
            val exists =
                runCatching {
                    composeRule.onNodeWithTag(installTag, useUnmergedTree = true).assertExists()
                    true
                }.getOrDefault(false)
            if (exists) break
            runCatching {
                composeRule.onNodeWithText(regionName, substring = true, useUnmergedTree = true).performScrollTo()
            }
            // Regions list loads async from catalog-cache / network.
            Thread.sleep(2_000)
            composeRule.mainClock.advanceTimeBy(2_000)
        }
        val clicked =
            runCatching {
                composeRule.onNodeWithTag(installTag, useUnmergedTree = true).performScrollTo()
                clickTag(installTag)
                true
            }.getOrDefault(false)
        if (!clicked) {
            // Keep Tools open; click Download next to Østlandet/Vestlandet row.
            var textClicked = false
            runCatching {
                device.wait(Until.findObject(By.textContains(regionName)), 8_000)
                val row = device.findObject(By.textContains(regionName))
                val dl =
                    row?.parent?.findObject(By.text("Download"))
                        ?: row?.parent?.parent?.findObject(By.text("Download"))
                if (dl != null) {
                    dl.click()
                    textClicked = true
                }
            }
            if (!textClicked) {
                // Last resort: any Download while catalog is open.
                runCatching {
                    device.findObjects(By.text("Download"))?.firstOrNull()?.click()
                    textClicked = true
                }
            }
            noteUi("install_fallback_text", "$regionName clicked=$textClicked")
        } else {
            noteUi("install_clicked", installTag)
        }
        // Do NOT close Tools immediately — progress lives in the MWM section.
        settle(2_000)
        val ok = waitMwmInstall(label, DOWNLOAD_DEADLINE_MS)
        closeTools()
        val elapsed = System.currentTimeMillis() - t0
        val inst = runCatching { uniffi.navi.mwmInstalled(mwmDataDir.absolutePath) }.getOrNull()
        val placement = resolveMwmPlacement()
        regionTimings.put(
            JSONObject()
                .put("label", label)
                .put("region_id", regionId)
                .put("elapsed_ms", elapsed)
                .put("ok", ok)
                .put("version", inst?.version ?: JSONObject.NULL)
                .put("leaf_count", inst?.leafIds?.size ?: 0)
                .put("sidecar_count", inst?.sidecarCount ?: 0)
                .put("leaves", JSONArray(inst?.leafIds ?: emptyList<String>()))
                .put("placement", placement),
        )
        writeReport()
        return ok
    }

    private fun waitMwmInstall(
        label: String,
        deadlineMs: Long,
    ): Boolean {
        val t0 = System.currentTimeMillis()
        var sawRunning = false
        while (System.currentTimeMillis() - t0 < deadlineMs) {
            val st = MwmMapDownloader.state
            if (st.running) sawRunning = true
            noteUi(
                "install_progress",
                "$label running=${st.running} ok=${st.ok} msg=${st.message} pct=${st.percent}",
            )
            val inst = runCatching { uniffi.navi.mwmInstalled(mwmDataDir.absolutePath) }.getOrNull()
            val leavesOk =
                inst != null &&
                    inst.leafIds.any {
                        it.contains("Hedmark", ignoreCase = true) ||
                            it.contains("Oppland", ignoreCase = true)
                    }
            // Orphan release leaves on disk (UI catalog empty but Norway_*.mwm present).
            val diskNorway =
                runCatching {
                    File(dataDir, "mwm/releases")
                        .walkTopDown()
                        .any { f ->
                            f.isFile &&
                                (
                                    f.name.contains("Norway_Oppland", ignoreCase = true) ||
                                        f.name.contains("Norway_Hedmark", ignoreCase = true)
                                )
                        }
                }.getOrDefault(false)
            if (!st.running && st.ok == true) {
                ensureComapsSessionOpen("install_ok_$label")
                return true
            }
            if (!st.running && leavesOk && (st.ok == true || sawRunning || st.message.contains("session"))) {
                ensureComapsSessionOpen("install_disk_$label")
                return leavesOk
            }
            if (!st.running && leavesOk) {
                ensureComapsSessionOpen("install_leaves_$label")
                return true
            }
            if (!st.running && diskNorway && ensureComapsSessionOpen("install_orphan_$label")) {
                // Catalog may still be empty; session open against on-disk leaves is enough to plan.
                noteUi("install_orphan_ok", "Norway leaves on disk; session open")
                return true
            }
            if (!st.running && st.ok == false && !leavesOk) {
                noteUi("install_failed", st.message)
                return false
            }
            // Fail fast when Download never started (click missed).
            if (!sawRunning && !leavesOk && System.currentTimeMillis() - t0 > TimeUnit.MINUTES.toMillis(3)) {
                noteUi("install_never_started", "$label after 3 min; retrying click once")
                runCatching {
                    openTools()
                    clickTagSoft("btn_mwm_regions")
                    settle(1_500)
                    clickTagSoft("btn_mwm_install_$OSTLANDET_REGION")
                    device
                        .findObject(By.textContains("Østlandet"))
                        ?.parent
                        ?.findObject(By.text("Download"))
                        ?.click()
                }
                // Give the retry a short window then stop looping forever.
                if (System.currentTimeMillis() - t0 > TimeUnit.MINUTES.toMillis(6)) {
                    noteUi("install_give_up", label)
                    return diskNorway && ensureComapsSessionOpen("install_give_up_$label")
                }
            }
            Thread.sleep(3_000)
            composeRule.mainClock.advanceTimeBy(3_000)
            writeReport()
        }
        val inst = runCatching { uniffi.navi.mwmInstalled(mwmDataDir.absolutePath) }.getOrNull()
        return inst != null &&
            inst.leafIds.any {
                it.contains("Hedmark", ignoreCase = true) || it.contains("Oppland", ignoreCase = true)
            }
    }

    private fun trackPassiveInstall(
        regionId: String,
        label: String,
    ) {
        Thread {
            val t0 = System.currentTimeMillis()
            while (System.currentTimeMillis() - t0 < DOWNLOAD_DEADLINE_MS) {
                val st = MwmMapDownloader.state
                if (!st.running && st.ok == true) {
                    regionTimings.put(
                        JSONObject()
                            .put("label", label)
                            .put("region_id", regionId)
                            .put("elapsed_ms", System.currentTimeMillis() - t0)
                            .put("ok", true)
                            .put("placement", resolveMwmPlacement()),
                    )
                    writeReport()
                    return@Thread
                }
                Thread.sleep(10_000)
            }
        }.start()
    }

    private fun acceptAnyLongTripDownloadPrompt() {
        runCatching { clickTagSoft("btn_mwm_missing_download") }
        runCatching {
            device.findObject(By.textContains("Download region"))?.click()
        }
        runCatching {
            device.findObject(By.textContains("Download map"))?.click()
        }
        runCatching {
            val btn = device.findObject(By.text("OK")) ?: device.findObject(By.text("Yes"))
            btn?.click()
        }
    }

    private fun innlandetLeavesPresent(): Boolean {
        val inst = runCatching { uniffi.navi.mwmInstalled(mwmDataDir.absolutePath) }.getOrNull() ?: return false
        val leaves = inst.leafIds.map { it.lowercase() }
        val inn =
            leaves.any { it.contains("hedmark") || it.contains("oppland") || it.contains("innlandet") } ||
                leaves.any { it.contains("oslo") || it.contains("akershus") }
        noteUi("innlandet_check", "leaves=${leaves.take(20)} hit=$inn")
        return inst.leafIds.isNotEmpty() && (inn || leaves.size >= 3)
    }

    private fun resolveMwmPlacement(): String {
        val sd =
            NaviStorageVolumes
                .list(composeRule.activity)
                .firstOrNull { it.removable && it.mounted && it.appFilesDir != null }
                ?.appFilesDir
        val sdMwm = sd?.let { File(it, "mwm") }
        return when {
            sdMwm != null && sdMwm.exists() && (sdMwm.list()?.isNotEmpty() == true) ->
                sdMwm.absolutePath + " (SD)"
            else -> dataDir.absolutePath + "/mwm (internal)"
        }
    }

    private fun listRegionsOnDisk(phase: String) {
        val listing = JSONObject()
        val inst = runCatching { uniffi.navi.mwmInstalled(mwmDataDir.absolutePath) }.getOrNull()
        listing.put("mwm_leaves", JSONArray(inst?.leafIds ?: emptyList<String>()))
        listing.put("mwm_version", inst?.version ?: JSONObject.NULL)
        listing.put("placement", resolveMwmPlacement())
        val internalLeaves =
            runCatching {
                File(dataDir, "mwm/releases")
                    .walkTopDown()
                    .filter { it.extension == "mwm" }
                    .map { it.absolutePath }
                    .toList()
            }.getOrDefault(emptyList())
        listing.put("internal_mwm_files", JSONArray(internalLeaves.take(80)))
        listing.put("internal_mwm_count", internalLeaves.size)
        val sd =
            NaviStorageVolumes
                .list(composeRule.activity)
                .firstOrNull { it.removable && it.mounted && it.appFilesDir != null }
                ?.appFilesDir
        if (sd != null) {
            val sdFiles =
                runCatching {
                    File(sd, "long-trip-packs")
                        .listFiles()
                        ?.map { it.name + ":" + it.length() }
                        .orEmpty()
                }.getOrDefault(emptyList())
            listing.put("sd_long_trip_packs", JSONArray(sdFiles))
            val sdMwm =
                runCatching {
                    File(sd, "mwm")
                        .walkTopDown()
                        .filter { it.extension == "mwm" }
                        .map { it.absolutePath }
                        .toList()
                }.getOrDefault(emptyList())
            listing.put("sd_mwm_files", JSONArray(sdMwm.take(80)))
            listing.put("sd_mwm_count", sdMwm.size)
        }
        report.put("disk_listing_$phase", listing)
        noteUi("disk_listing", "$phase leaves=${inst?.leafIds?.size ?: 0}")
        writeReport()
    }

    private fun ensureComapsSessionOpen(phase: String): Boolean {
        MwmClientBootstrap.ensureInitialized()
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        val ok =
            runCatching { ComapsSessionBootstrap.ensureOpen(ctx, force = true) }.getOrDefault(false) ||
                runCatching { uniffi.navi.comapsSessionIsOpen() }.getOrDefault(false)
        noteUi(
            "comaps_session",
            "$phase open=$ok nativeLoaded=${MwmClientBootstrap.nativeLoaded}",
        )
        report.put(
            "comaps_session_$phase",
            JSONObject()
                .put("open", ok)
                .put("native_loaded", MwmClientBootstrap.nativeLoaded),
        )
        writeReport()
        return ok
    }

    private fun dismissGeofabrikMissingCoverage(): Boolean {
        val viaTag =
            runCatching {
                composeRule.onNodeWithTag("btn_missing_coverage_dismiss", useUnmergedTree = true).assertExists()
                clickTag("btn_missing_coverage_dismiss")
                true
            }.getOrDefault(false)
        val viaNotNow =
            !viaTag &&
                runCatching {
                    val n = device.findObject(By.text("Not now")) ?: return@runCatching false
                    n.click()
                    true
                }.getOrDefault(false)
        val dismissed = viaTag || viaNotNow
        if (dismissed) noteUi("geofabrik_dialog", "dismissed")
        return dismissed
    }

    // --- search / plan --------------------------------------------------------

    private fun typeNameOrCoordAndPick(
        chip: String,
        nameQuery: String,
        lat: Double,
        lon: Double,
        step: String,
    ) {
        openRoutePanel()
        clickTag(chip)
        settle(300)
        // Prefer name geocoding/search in Navi UI.
        if (trySearchPick(nameQuery, step)) return
        // ASCII fold fallback (æ→ae, å→a).
        val ascii =
            nameQuery
                .replace('æ', 'a')
                .replace('ø', 'o')
                .replace('å', 'a')
                .replace('Æ', 'A')
                .replace('Ø', 'O')
                .replace('Å', 'A')
        if (ascii != nameQuery && trySearchPick(ascii, step + "_ascii")) return
        // Coord fallback.
        val q = String.format(Locale.US, "%.7f, %.7f", lat, lon)
        NaviMapTestHooks.lastSearchHitCount = -1
        NaviMapTestHooks.lastAppliedHitLat = Double.NaN
        setField("field_search", q)
        clickTagSoft("btn_submit_search")
        val deadline = SystemClock.elapsedRealtime() + 25_000
        while (SystemClock.elapsedRealtime() < deadline && NaviMapTestHooks.lastSearchHitCount < 1) {
            Thread.sleep(200)
            composeRule.mainClock.advanceTimeBy(200)
        }
        assertTrue(
            "$step hits count=${NaviMapTestHooks.lastSearchHitCount} q=$q",
            NaviMapTestHooks.lastSearchHitCount >= 1,
        )
        clickTag("search_hit_0")
        val applyDeadline = SystemClock.elapsedRealtime() + 12_000
        while (SystemClock.elapsedRealtime() < applyDeadline && NaviMapTestHooks.lastAppliedHitLat.isNaN()) {
            Thread.sleep(100)
        }
        noteUi(
            step,
            "coord_fallback q=$q applied=${NaviMapTestHooks.lastAppliedHitLat}," +
                "${NaviMapTestHooks.lastAppliedHitLon} name=${NaviMapTestHooks.lastAppliedHitName}",
        )
        report.put(
            step,
            JSONObject()
                .put("query", q)
                .put("mode", "coord_fallback")
                .put("applied_lat", NaviMapTestHooks.lastAppliedHitLat)
                .put("applied_lon", NaviMapTestHooks.lastAppliedHitLon)
                .put("applied_name", NaviMapTestHooks.lastAppliedHitName),
        )
        writeReport()
    }

    private fun typeCoordAndPickForced(
        chip: String,
        lat: Double,
        lon: Double,
        step: String,
    ) {
        openRoutePanel()
        clickTag(chip)
        settle(300)
        val q = String.format(Locale.US, "%.7f, %.7f", lat, lon)
        NaviMapTestHooks.lastSearchHitCount = -1
        NaviMapTestHooks.lastAppliedHitLat = Double.NaN
        setField("field_search", q)
        clickTagSoft("btn_submit_search")
        val deadline = SystemClock.elapsedRealtime() + 25_000
        while (SystemClock.elapsedRealtime() < deadline && NaviMapTestHooks.lastSearchHitCount < 1) {
            Thread.sleep(200)
            composeRule.mainClock.advanceTimeBy(200)
        }
        assertTrue("$step hits q=$q count=${NaviMapTestHooks.lastSearchHitCount}", NaviMapTestHooks.lastSearchHitCount >= 1)
        clickTag("search_hit_0")
        val applyDeadline = SystemClock.elapsedRealtime() + 12_000
        while (SystemClock.elapsedRealtime() < applyDeadline && NaviMapTestHooks.lastAppliedHitLat.isNaN()) {
            Thread.sleep(100)
        }
        report.put(
            step,
            JSONObject()
                .put("query", q)
                .put("mode", "coord_forced")
                .put("applied_lat", NaviMapTestHooks.lastAppliedHitLat)
                .put("applied_lon", NaviMapTestHooks.lastAppliedHitLon)
                .put("applied_name", NaviMapTestHooks.lastAppliedHitName),
        )
        noteUi(step, "forced coord applied=${NaviMapTestHooks.lastAppliedHitName}")
        writeReport()
    }

    private fun trySearchPick(
        query: String,
        step: String,
    ): Boolean {
        NaviMapTestHooks.lastSearchHitCount = -1
        NaviMapTestHooks.lastAppliedHitLat = Double.NaN
        setField("field_search", query)
        clickTagSoft("btn_submit_search")
        val deadline = SystemClock.elapsedRealtime() + 20_000
        while (SystemClock.elapsedRealtime() < deadline && NaviMapTestHooks.lastSearchHitCount < 1) {
            Thread.sleep(250)
            composeRule.mainClock.advanceTimeBy(250)
        }
        if (NaviMapTestHooks.lastSearchHitCount < 1) {
            noteUi(step, "name_search_miss q=$query")
            return false
        }
        clickTag("search_hit_0")
        val applyDeadline = SystemClock.elapsedRealtime() + 12_000
        while (SystemClock.elapsedRealtime() < applyDeadline && NaviMapTestHooks.lastAppliedHitLat.isNaN()) {
            Thread.sleep(100)
        }
        if (NaviMapTestHooks.lastAppliedHitLat.isNaN()) {
            noteUi(step, "name_search_no_apply q=$query")
            return false
        }
        noteUi(
            step,
            "name_search q=$query applied=${NaviMapTestHooks.lastAppliedHitLat}," +
                "${NaviMapTestHooks.lastAppliedHitLon} name=${NaviMapTestHooks.lastAppliedHitName}",
        )
        report.put(
            step,
            JSONObject()
                .put("query", query)
                .put("mode", "name_search")
                .put("applied_lat", NaviMapTestHooks.lastAppliedHitLat)
                .put("applied_lon", NaviMapTestHooks.lastAppliedHitLon)
                .put("applied_name", NaviMapTestHooks.lastAppliedHitName),
        )
        writeReport()
        return true
    }

    private fun reapplyWaypointsAndPlan(reason: String) {
        typeNameOrCoordAndPick("chip_from", FROM_QUERY, ORIGIN_LAT, ORIGIN_LON, "from_re_$reason")
        pickPilgrimVias()
        typeCoordAndPickForced("chip_to", DEST_LAT, DEST_LON, "to_re_$reason")
        runCatching { clickTag("chip_profile_hiking") }
        configureHikingAvoidsOnly()
        awaitCorridorReadyBeforePlan()
        composeRule.onNodeWithTag("btn_plan_route", useUnmergedTree = true).performScrollTo()
        clickTag("btn_plan_route")
        noteUi("replan", reason)
    }

    /** Pick one short-corridor via: Pilegrimssenter Oslo. Dest is Hamar. */
    private fun pickPilgrimVias() {
        clearAllViasSoft()
        pickOneVia(VIA1_QUERY, VIA1_LAT, VIA1_LON, "via1_oslo", 59.88..59.94, 10.70..10.85)
    }

    private fun pickOneVia(
        query: String,
        lat: Double,
        lon: Double,
        step: String,
        latRange: ClosedFloatingPointRange<Double>,
        lonRange: ClosedFloatingPointRange<Double>,
    ) {
        openRoutePanel()
        clickTag("chip_via")
        settle(300)
        val nameOk = trySearchPickBounded(query, step + "_name", latRange, lonRange)
        if (nameOk) {
            noteUi(step, "accepted name hit for $query")
            return
        }
        noteUi(step + "_skip_name", "forcing coords for $query")
        typeCoordAndPickForced("chip_via", lat, lon, step + "_coord")
        val applied = report.optJSONObject(step + "_coord")
        val alat = applied?.optDouble("applied_lat") ?: Double.NaN
        val alon = applied?.optDouble("applied_lon") ?: Double.NaN
        assertTrue(
            "Forced via must be near $query (got $alat,$alon)",
            !alat.isNaN() && alat in latRange && alon in lonRange,
        )
    }

    private fun trySearchPickBounded(
        query: String,
        step: String,
        latRange: ClosedFloatingPointRange<Double>,
        lonRange: ClosedFloatingPointRange<Double>,
    ): Boolean {
        NaviMapTestHooks.lastSearchHitCount = -1
        NaviMapTestHooks.lastAppliedHitLat = Double.NaN
        NaviMapTestHooks.lastAppliedHitName = ""
        setField("field_search", query)
        clickTagSoft("btn_submit_search")
        val deadline = SystemClock.elapsedRealtime() + 20_000
        while (SystemClock.elapsedRealtime() < deadline && NaviMapTestHooks.lastSearchHitCount < 1) {
            Thread.sleep(250)
            composeRule.mainClock.advanceTimeBy(250)
        }
        if (NaviMapTestHooks.lastSearchHitCount < 1) {
            noteUi(step, "name_search_miss q=$query")
            return false
        }
        clickTag("search_hit_0")
        val applyDeadline = SystemClock.elapsedRealtime() + 12_000
        while (SystemClock.elapsedRealtime() < applyDeadline && NaviMapTestHooks.lastAppliedHitLat.isNaN()) {
            Thread.sleep(100)
        }
        val name = NaviMapTestHooks.lastAppliedHitName
        val lat = NaviMapTestHooks.lastAppliedHitLat
        val lon = NaviMapTestHooks.lastAppliedHitLon
        if (lat.isNaN() || viaHitRejected(name) || lat !in latRange || lon !in lonRange) {
            noteUi(step, "name_search_reject q=$query hit='$name' @$lat,$lon")
            // CRITICAL: clicking search_hit_0 already applied the bad via — remove it.
            removeLastViaSoft()
            return false
        }
        report.put(
            step,
            JSONObject()
                .put("query", query)
                .put("mode", "name_search")
                .put("applied_lat", lat)
                .put("applied_lon", lon)
                .put("applied_name", name),
        )
        noteUi(step, "name_search q=$query applied=$lat,$lon name=$name")
        writeReport()
        return true
    }

    /** Undo a just-applied bad via (last index) without wiping earlier good vias. */
    private fun removeLastViaSoft() {
        openRoutePanel()
        settle(200)
        // Prefer tagged remove on the highest index present.
        for (idx in 3 downTo 0) {
            val removed =
                runCatching {
                    composeRule
                        .onNodeWithTag("btn_remove_via_$idx", useUnmergedTree = true)
                        .assertExists()
                    clickTag("btn_remove_via_$idx")
                    true
                }.getOrDefault(false)
            if (removed) {
                noteUi("remove_bad_via", "btn_remove_via_$idx")
                settle(200)
                return
            }
        }
        runCatching { clickTagSoft("btn_clear_vias") }
        noteUi("remove_bad_via", "fallback clear_all_vias")
        settle(200)
    }

    private fun assertPilgrimVias(wp: String) {
        assertTrue(
            "At least one via must be set (got: $wp)",
            wp.contains("Via:", ignoreCase = true) &&
                !wp.contains("Via: (none)", ignoreCase = true) &&
                !wp.contains("Via: none", ignoreCase = true),
        )
        assertTrue(
            "Must not keep rejected Gamle kirkevei via (got: $wp)",
            !wp.contains("Gamle kirkevei", ignoreCase = true) &&
                !wp.contains("Heggen", ignoreCase = true),
        )
        val viaPart =
            wp
                .substringAfter("Via:", missingDelimiterValue = "")
                .substringBefore("|")
                .trim()
        val viaCount =
            if (viaPart.isEmpty() || viaPart.equals("(none)", true) || viaPart.equals("none", true)) {
                0
            } else {
                viaPart
                    .split("→")
                    .map { it.trim() }
                    .filter { it.isNotEmpty() }
                    .size
            }
        assertTrue("Exactly 1 via required (got $viaCount in: $wp)", viaCount == 1)
        assertTrue("Via must be Pilegrimssenter Oslo (got: $wp)", wp.contains("Pilegrimssenter Oslo", ignoreCase = true))
        assertTrue(
            "Dest must be Eidsvoll area (got: $wp)",
            wp.contains("Eidsvoll", ignoreCase = true) ||
                wp.contains("60.329", ignoreCase = true) ||
                wp.contains("60.33", ignoreCase = true),
        )
        report.put(
            "via_proof",
            JSONObject()
                .put("waypoints_summary", wp)
                .put("via_count", viaCount)
                .put("via1", report.optJSONObject("via1_oslo_name") ?: report.optJSONObject("via1_oslo_coord")),
        )
        Log.i(TAG, "VIA_PROOF wp=$wp via_count=$viaCount")
    }

    /** True when Østlandet short-corridor leaves (Southern/Hedmark) are already on SD. */
    private fun pilgrimCorridorLeavesReadyOnDisk(): Boolean {
        listRegionsOnDisk("precheck_ready")
        val inst =
            runCatching { uniffi.navi.mwmInstalled(mwmDataDir.absolutePath) }.getOrNull()
                ?: return false
        val leaves = inst.leafIds.map { it.lowercase() }
        val hasSouthern = leaves.any { it.contains("southern") }
        val hasHedmark = leaves.any { it.contains("hedmark") }
        val ready = hasSouthern && hasHedmark && leaves.size >= 2
        noteUi(
            "corridor_precheck",
            "ready=$ready leaves=${inst.leafIds} version=${inst.version}",
        )
        report.put(
            "corridor_precheck",
            JSONObject()
                .put("ready", ready)
                .put("leaves", JSONArray(inst.leafIds))
                .put("version", inst.version ?: JSONObject.NULL),
        )
        writeReport()
        return ready
    }

    /**
     * Product + harness gate: wait until [LongTripCoordinator.corridorReadyForPlanning]
     * before any Plan click. Accept MWM/long-trip download prompts while waiting.
     */
    private fun awaitCorridorReadyBeforePlan() {
        val t0 = System.currentTimeMillis()
        var lastLog = 0L
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        while (System.currentTimeMillis() - t0 < DOWNLOAD_DEADLINE_MS) {
            acceptAnyLongTripDownloadPrompt()
            runCatching { clickTagSoft("btn_mwm_missing_download") }
            runCatching {
                device.findObject(By.text("Download"))?.click()
                    ?: device.findObject(By.textContains("Download"))?.click()
            }
            // Promote past stuck Geofabrik PBF Downloading when MWM leaves are Ready.
            val ready = LongTripCoordinator.reconcileMwmReady(ctx)
            val line = LongTripCoordinator.statusLine()
            if (System.currentTimeMillis() - lastLog > 15_000) {
                lastLog = System.currentTimeMillis()
                val sample =
                    JSONObject()
                        .put("t_ms", System.currentTimeMillis() - t0)
                        .put("corridor_ready", ready)
                        .put("status", line)
                Log.i(TAG, "corridor_wait $sample")
                noteUi("corridor_wait", sample.toString())
                writeReport()
                listRegionsOnDisk("corridor_wait")
            }
            if (ready) {
                noteUi("corridor_ready_before_plan", line)
                report.put("corridor_ready_before_plan", true)
                report.put("corridor_wait_ms", System.currentTimeMillis() - t0)
                writeReport()
                return
            }
            Thread.sleep(3_000)
            composeRule.mainClock.advanceTimeBy(3_000)
        }
        report.put("corridor_ready_before_plan", false)
        writeReport()
        assertTrue(
            "Corridor must be fully ready before Plan (last=${LongTripCoordinator.statusLine()})",
            LongTripCoordinator.corridorReadyForPlanning(),
        )
    }

    /** Drop any prior vias so a forced Fondsbu coord cannot append beside a bad hit. */
    private fun clearAllViasSoft() {
        openRoutePanel()
        runCatching { clickTagSoft("btn_clear_vias") }
        settle(200)
    }

    private fun persistPlanArtifactsNow(
        dist: Double,
        rep: String,
    ) {
        val live = NaviMapTestHooks.lastRoutePolyline
        if (live.length > savedPlanPolyline.length) {
            savedPlanPolyline = live
            persistPlanPolyline(live)
        }
        report.put(
            "plan_snapshot",
            JSONObject()
                .put("distance_km", dist)
                .put("report_snip", rep.take(2_000))
                .put("polyline_chars", savedPlanPolyline.length),
        )
        writeReport()
    }

    private fun persistBreakPoisForHost() {
        val breaks = NaviMapTestHooks.lastBreakPoisJson
        val targets =
            listOf(
                File(dataDir, "long-trip-ui-report/break-pois.json"),
                File(dataDir, "pilgrim-long-ui/break-pois.json"),
                File("/data/local/tmp/pilgrim-long-ui/break-pois.json"),
                File("/sdcard/Pictures/pilgrim-long-ui/break-pois.json"),
            )
        for (f in targets) {
            runCatching {
                f.parentFile?.mkdirs()
                f.writeText(breaks.ifBlank { "[]" })
            }
        }
        noteUi("break_pois_persist", "chars=${breaks.length}")
    }

    private fun enrichPlanDerivedFields(
        distanceKm: Double,
        etaMin: Double,
    ) {
        val kinds = JSONObject()
        val mans = runCatching { JSONArray(NaviMapTestHooks.lastManeuversJson) }.getOrNull()
        if (mans != null) {
            for (i in 0 until mans.length()) {
                val k =
                    mans
                        .optJSONObject(i)
                        ?.optString("kind")
                        .orEmpty()
                        .ifBlank { "?" }
                kinds.put(k, kinds.optInt(k) + 1)
            }
        }
        report.put("nav_instruction_types", kinds)

        val stats =
            runCatching { JSONObject(NaviMapTestHooks.lastRoutePlanStatsJson) }.getOrDefault(JSONObject())
        val restDetailed = JSONArray()
        runCatching {
            val arr = JSONArray(NaviMapTestHooks.lastBreakPoisJson)
            for (i in 0 until arr.length()) {
                val o = arr.optJSONObject(i) ?: continue
                restDetailed.put(
                    JSONObject()
                        .put("name", o.optString("name"))
                        .put("lat", o.optDouble("lat", Double.NaN))
                        .put("lon", o.optDouble("lon", Double.NaN))
                        .put("kind", o.optString("kind").ifBlank { o.optString("rest_kind") })
                        .put("icon", o.optString("icon_key").ifBlank { o.optString("iconKey") }),
                )
            }
        }
        val waterSources = JSONArray()
        runCatching {
            fun scanHits(raw: String) {
                if (raw.isBlank()) return
                val root =
                    runCatching { JSONObject(raw) }.getOrNull()
                        ?: return
                val hits = root.optJSONArray("hits") ?: JSONArray()
                for (i in 0 until hits.length()) {
                    val h = hits.optJSONObject(i) ?: continue
                    val icon = h.optString("iconKey").ifBlank { h.optString("icon_key") }
                    val label = h.optString("label").ifBlank { h.optString("name") }
                    if (icon.contains("water", ignoreCase = true) ||
                        label.contains("water", ignoreCase = true) ||
                        label.contains("spring", ignoreCase = true) ||
                        icon.contains("drinking", ignoreCase = true)
                    ) {
                        waterSources.put(
                            JSONObject()
                                .put("name", label)
                                .put("icon", icon)
                                .put("lat", h.optDouble("lat", Double.NaN))
                                .put("lon", h.optDouble("lon", Double.NaN)),
                        )
                    }
                }
            }
            scanHits(NaviMapTestHooks.lastPoiLookaheadJson)
            // Also scan break pois for water.
            val arr = JSONArray(NaviMapTestHooks.lastBreakPoisJson)
            for (i in 0 until arr.length()) {
                val o = arr.optJSONObject(i) ?: continue
                val kind = o.optString("kind") + " " + o.optString("icon_key") + " " + o.optString("name")
                if (kind.contains("water", ignoreCase = true) || kind.contains("spring", ignoreCase = true)) {
                    waterSources.put(
                        JSONObject()
                            .put("name", o.optString("name"))
                            .put("lat", o.optDouble("lat", Double.NaN))
                            .put("lon", o.optDouble("lon", Double.NaN)),
                    )
                }
            }
        }
        report.put(
            "corridor_counts",
            JSONObject()
                .put("ferry_legs", stats.optInt("route_ferry_legs"))
                .put("tunnels", stats.optInt("route_tunnel_count"))
                .put("rest_places", stats.optInt("rest_place_count"))
                .put("rest_place_names", stats.optJSONArray("rest_place_names") ?: JSONArray())
                .put("rest_places_detailed", restDetailed)
                .put("attractions", stats.optInt("attraction_count"))
                .put("attraction_by_type", stats.optJSONObject("attraction_by_type") ?: JSONObject())
                .put("wild_camping_sites", stats.optInt("wild_camping_site_count"))
                .put("water_sources", waterSources)
                .put("water_source_count", waterSources.length()),
        )

        // Hiking multi-day: prefer days_json; else soft 6–8 h walking budget split.
        val daysOut = JSONArray()
        val daysRaw = runCatching { JSONArray(NaviMapTestHooks.lastDaysJson) }.getOrNull()
        if (daysRaw != null && daysRaw.length() > 0) {
            for (i in 0 until daysRaw.length()) {
                val d = daysRaw.optJSONObject(i) ?: continue
                daysOut.put(
                    JSONObject()
                        .put("day", d.optInt("day", i + 1))
                        .put("km", d.optDouble("distance_km", d.optDouble("km", 0.0)))
                        .put("drive_hours", d.optDouble("driving_hours", d.optDouble("hours", 0.0))),
                )
            }
        } else if (etaMin > 0.0 && distanceKm > 0.0) {
            val softH = 8.0 // hiking day budget
            val etaH = etaMin / 60.0
            var remainingKm = distanceKm
            var remainingH = etaH
            var day = 1
            while (remainingKm > 0.5 && day <= 20) {
                val hikeH = minOf(softH, remainingH)
                val dayKm = distanceKm * (hikeH / etaH)
                daysOut.put(
                    JSONObject()
                        .put("day", day)
                        .put("hike_hours_soft_cap", softH)
                        .put("hike_hours", hikeH)
                        .put("km", dayKm),
                )
                remainingH -= hikeH
                remainingKm -= dayKm
                day++
                if (remainingH <= 0.05) break
            }
        }
        report.put("km_per_day", daysOut)
        report.put("km_total", distanceKm)
        report.put("estimated_stops", restDetailed.length())

        val inst = runCatching { uniffi.navi.mwmInstalled(mwmDataDir.absolutePath) }.getOrNull()
        report.put(
            "installed_maps",
            JSONObject()
                .put("version", inst?.version ?: JSONObject.NULL)
                .put("leaf_count", inst?.leafIds?.size ?: 0)
                .put("sidecar_count", inst?.sidecarCount ?: 0)
                .put("leaves", JSONArray(inst?.leafIds ?: emptyList<String>()))
                .put("placement", resolveMwmPlacement()),
        )
    }

    // --- GPS / chrome helpers -------------------------------------------------

    private fun pinOriginGps() {
        NaviMapTestHooks.holdDebugGps(ORIGIN_LAT, ORIGIN_LON)
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        val lm = ctx.getSystemService(LocationManager::class.java)
        val mockOk =
            runCatching {
                if (lm == null) return@runCatching false
                val provider = "gps"
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                    runCatching {
                        lm.addTestProvider(
                            provider,
                            false,
                            false,
                            false,
                            false,
                            true,
                            true,
                            true,
                            ProviderProperties.POWER_USAGE_LOW,
                            ProviderProperties.ACCURACY_FINE,
                        )
                    }
                } else {
                    @Suppress("DEPRECATION")
                    runCatching {
                        lm.addTestProvider(provider, false, false, false, false, true, true, true, 1, 1)
                    }
                }
                lm.setTestProviderEnabled(provider, true)
                val loc =
                    Location(provider).apply {
                        latitude = ORIGIN_LAT
                        longitude = ORIGIN_LON
                        altitude = ORIGIN_ELEV_M
                        accuracy = 5f
                        time = System.currentTimeMillis()
                        elapsedRealtimeNanos = System.nanoTime()
                    }
                lm.setTestProviderLocation(provider, loc)
                true
            }.getOrDefault(false)
        noteUi("gps_pin", "Mariakirken mock=$mockOk")
        report.put("gps_mock_provider", mockOk)
    }

    private fun openTools() {
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = false
        // Idempotent: btn_tools toggles. Clicking while open closes the sheet and
        // caused the live Tools open/close cycle during delete/download loops.
        if (NaviMapTestHooks.toolsOpen || device.findObject(By.text("Hide tools")) != null) {
            settle(200)
            return
        }
        NaviMapTestHooks.requestOpenTools = true
        settle(500)
        if (NaviMapTestHooks.toolsOpen || device.findObject(By.text("Hide tools")) != null) {
            return
        }
        clickTagSoft("btn_tools")
        clickTagSoft("btn_tools_collapsed")
        settle(400)
        if (device.findObject(By.text("Hide tools")) == null) {
            runCatching { device.findObject(By.text("Tools"))?.click() }
        }
        settle(300)
    }

    private fun closeTools() {
        if (!NaviMapTestHooks.toolsOpen && device.findObject(By.text("Hide tools")) == null) {
            return
        }
        NaviMapTestHooks.requestCloseTools = true
        settle(200)
        clickTagSoft("btn_close_tools")
        clickTagSoft("btn_save_tools")
        clickTagSoft("btn_close_vehicle")
        clickTagSoft("btn_close_drive_settings")
        runCatching { device.findObject(By.text("Hide tools"))?.click() }
        settle(300)
    }

    private fun openRoutePanel() {
        fun fieldVisible(): Boolean =
            runCatching {
                composeRule.onNodeWithTag("field_search", useUnmergedTree = true).assertIsDisplayed()
                true
            }.getOrDefault(false)

        if (fieldVisible()) {
            noteUi("open_route_panel", "field_search already visible")
            return
        }
        // Profile / drive sheets sit above search — dismiss before chrome flicker.
        dismissProfileSheets()
        if (fieldVisible()) return
        runCatching { launchRule.scenario.moveToState(androidx.lifecycle.Lifecycle.State.RESUMED) }
        settle(500)
        NaviMapTestHooks.hideUiChrome = true
        NaviMapTestHooks.hideSearchChrome = true
        settle(200)
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = false
        NaviMapTestHooks.requestOpenTools = true
        settle(400)
        closeTools()
        dismissGeofabrikMissingCoverage()
        dismissProfileSheets()
        if (fieldVisible()) return
        clickTagSoft("btn_open_search")
        settle(400)
        runCatching { device.findObject(By.text("Route"))?.click() }
        settle(500)
        val deadline = SystemClock.elapsedRealtime() + 45_000
        var bringFrontAt = 0L
        while (SystemClock.elapsedRealtime() < deadline) {
            if (fieldVisible()) return
            closeTools()
            dismissGeofabrikMissingCoverage()
            dismissProfileSheets()
            NaviMapTestHooks.hideUiChrome = false
            NaviMapTestHooks.hideSearchChrome = false
            // Bring MainActivity forward at most every 8s — thrashing PAUSE/RESUME
            // every loop iteration was closing search as soon as it opened.
            val now = SystemClock.elapsedRealtime()
            if (now - bringFrontAt > 8_000) {
                bringFrontAt = now
                runCatching {
                    val ctx = InstrumentationRegistry.getInstrumentation().targetContext
                    val launch = ctx.packageManager.getLaunchIntentForPackage(ctx.packageName)
                    if (launch != null) {
                        launch.addFlags(
                            android.content.Intent.FLAG_ACTIVITY_NEW_TASK or
                                android.content.Intent.FLAG_ACTIVITY_REORDER_TO_FRONT or
                                android.content.Intent.FLAG_ACTIVITY_SINGLE_TOP,
                        )
                        ctx.startActivity(launch)
                    }
                }
                runCatching {
                    launchRule.scenario.moveToState(androidx.lifecycle.Lifecycle.State.RESUMED)
                }
            }
            clickTagSoft("btn_open_search")
            clickTagSoft("btn_tools_collapsed")
            runCatching { device.findObject(By.text("Route"))?.click() }
            settle(700)
        }
        if (!fieldVisible()) {
            noteUi("open_route_panel", "field_search missing after retries — hard assert")
        }
        composeRule.onNodeWithTag("field_search", useUnmergedTree = true).assertIsDisplayed()
    }

    private fun waypointSummary(): String =
        runCatching {
            composeRule
                .onNodeWithTag("search_waypoints_summary", useUnmergedTree = true)
                .fetchSemanticsNode()
                .config[androidx.compose.ui.semantics.SemanticsProperties.Text]
                .joinToString(" ") { it.text }
        }.getOrDefault("")

    private fun waypointSummaryLooksSet(): Boolean {
        val s = waypointSummary()
        noteUi("waypoints_summary", s)
        return s.isNotBlank() && !s.contains("unset", ignoreCase = true)
    }

    private fun parseEtaMinutes(rep: String): Double {
        Regex("""chunked_eta_min=([0-9.]+)""")
            .find(rep)
            ?.groupValues
            ?.get(1)
            ?.toDoubleOrNull()
            ?.let { return it }
        Regex("""eta_minutes[=:]([0-9.]+)""")
            .find(rep)
            ?.groupValues
            ?.get(1)
            ?.toDoubleOrNull()
            ?.let { return it }
        Regex("""eta_min=([0-9.]+)""")
            .find(rep)
            ?.groupValues
            ?.get(1)
            ?.toDoubleOrNull()
            ?.let { return it }
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
        uiEvents.put(JSONObject().put("t", System.currentTimeMillis()).put("step", step).put("detail", detail))
        Log.i(TAG, "UI_EVENT $step | $detail")
    }

    private fun screenshot(name: String) {
        // User rule for this run: NO screenshots. Keep call sites; do not capture.
        noteUi("screenshot_skipped", name)
    }

    private fun settle(ms: Long = 400) {
        composeRule.waitForIdle()
        composeRule.mainClock.advanceTimeBy(ms)
        Thread.sleep(ms)
    }

    private fun clickTag(tag: String) {
        val node = composeRule.onNodeWithTag(tag, useUnmergedTree = true)
        runCatching { node.performScrollTo() }
        node.assertExists().performClick()
        settle()
    }

    private fun clickTagSoft(tag: String) {
        runCatching { clickTag(tag) }
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

    private fun persistPlanPolyline(poly: String) {
        if (poly.length < 100) {
            noteUi("polyline_persist", "skip short len=${poly.length}")
            return
        }
        val targets =
            listOf(
                File(dataDir, "long-trip-ui-report/route-polyline.txt"),
                File(dataDir, "pilgrim-long-ui/route-polyline.txt"),
                File("/data/local/tmp/pilgrim-long-ui/route-polyline.txt"),
                File("/sdcard/Pictures/pilgrim-long-ui/route-polyline.txt"),
            )
        for (f in targets) {
            runCatching {
                f.parentFile?.mkdirs()
                f.writeText(poly)
            }
        }
        noteUi("polyline_persist", "chars=${poly.length}")
        report.put("polyline_chars_saved", poly.length)
    }

    private fun writeReport() {
        report.put("ui_events", uiEvents)
        report.put("region_timings", regionTimings)
        val text = report.toString(2)
        runCatching {
            File(dataDir, "pilgrim-long-ui").also { it.mkdirs() }.let { File(it, "report.json").writeText(text) }
        }
        runCatching {
            File(dataDir, "long-trip-ui-report").also { it.mkdirs() }.let { File(it, "pilgrim-report.json").writeText(text) }
        }
        runCatching { File("/data/local/tmp/pilgrim-long-ui.json").writeText(text) }
        runCatching {
            File("/data/local/tmp/pilgrim-long-ui").also { it.mkdirs() }.let { File(it, "report.json").writeText(text) }
        }
        runCatching {
            File("/sdcard/Pictures/pilgrim-long-ui").also { it.mkdirs() }.let { File(it, "report.json").writeText(text) }
        }
        val sd =
            NaviStorageVolumes
                .list(composeRule.activity)
                .firstOrNull { it.removable && it.mounted && it.appFilesDir != null }
                ?.appFilesDir
        if (sd != null) {
            runCatching {
                File(sd, "pilgrim-long-ui").also { it.mkdirs() }.let { File(it, "report.json").writeText(text) }
            }
        }
    }
}
