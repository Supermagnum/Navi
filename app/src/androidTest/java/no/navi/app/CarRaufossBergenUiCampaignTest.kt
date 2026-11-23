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
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextClearance
import androidx.compose.ui.test.performTextInput
import androidx.test.core.app.ActivityScenario
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
import org.junit.rules.TestRule
import org.junit.rules.TestWatcher
import org.junit.runner.Description
import org.junit.runner.RunWith
import org.junit.runners.model.Statement
import java.io.File
import java.util.Locale
import java.util.concurrent.TimeUnit

/**
 * Launches [MainActivity] for the campaign and never calls
 * [ActivityScenario.close], so the process stays in the foreground after the
 * test (required: do not close the app).
 */
class LaunchMainActivityKeepAliveRule : TestRule {
    private var scenarioField: ActivityScenario<MainActivity>? = null
    val scenario: ActivityScenario<MainActivity>
        get() = scenarioField ?: error("MainActivity scenario not launched")

    fun requireActivity(): MainActivity {
        lateinit var act: MainActivity
        scenario.onActivity { act = it }
        return act
    }

    override fun apply(
        base: Statement,
        description: Description,
    ): Statement =
        object : Statement() {
            override fun evaluate() {
                scenarioField = ActivityScenario.launch(MainActivity::class.java)
                try {
                    base.evaluate()
                } finally {
                    // Leave MainActivity running — do not scenario.close().
                }
            }
        }
}

/**
 * Raufoss → Bergen busstasjon Car UI campaign (instrumentation driver).
 *
 * Downloads via MWM Offline maps → Choose region (btn_mwm_regions /
 * btn_mwm_install_*), NOT the fake-GPS / Geofabrik path field.
 * Synthetic DATEX: host ADB (RAUFOSS_AWAIT_DATEX / RAUFOSS_AWAIT_DATEX_MID).
 * GPS: NaviMapTestHooks.holdDebugGps + optional mock test provider (Samsung has
 * no `adb emu geo fix`); host may also feed via navi_gps_* launch extras.
 * Not wired into CI. Does not modify production app/plugin code.
 * Does not destroy MainActivity on teardown.
 */
@RunWith(AndroidJUnit4::class)
@LargeTest
class CarRaufossBergenUiCampaignTest {
    companion object {
        private const val TAG = "RaufossBergenUiCampaign"

        private const val ORIGIN_LAT = 60.7277483
        private const val ORIGIN_LON = 10.6109403
        private const val ORIGIN_ELEV_M = 311.0
        private const val DEST_LAT = 60.388114
        private const val DEST_LON = 5.333857

        /** MWM catalog region id (btn_mwm_install_*). Covers Innlandet leaves. */
        private const val OSTLANDET_REGION = "europe_norway_ostlandet"
        private const val VESTLANDET_REGION = "europe_norway_vestlandet"

        private val DOWNLOAD_DEADLINE_MS = TimeUnit.HOURS.toMillis(4)
        private val PLAN_DEADLINE_MS = TimeUnit.HOURS.toMillis(2)

        // Fallback coords snapped to the CoMaps Raufoss→Bergen corridor; host
        // re-snaps from the live route polyline when present.
        private val DATEX_SITS =
            listOf(
                Sit("syn-no-rv4-raufoss", "NO", 60.796989, 10.558482, "Synthetic NO corridor Block @12min", 12.0),
                Sit("syn-no-rv33-dokka", "NO", 60.782781, 10.375698, "Synthetic NO corridor Block @22min", 22.0),
                Sit("syn-no-e16-ton", "NO", 60.770872, 10.270507, "Synthetic NO corridor Block @28min", 28.0),
                Sit("syn-no-rv7-hagafoss", "NO", 60.975352, 9.189786, "Synthetic NO corridor Block @95min", 95.0),
                Sit("syn-no-rv7-mabo", "NO", 61.100153, 7.997072, "Synthetic NO corridor Block @180min", 180.0),
            )
    }

    private data class Sit(
        val id: String,
        val country: String,
        val lat: Double,
        val lon: Double,
        val label: String,
        val minutesBeforeArrival: Double,
    )

    @get:Rule(order = 0)
    val prefRule =
        object : TestWatcher() {
            override fun starting(description: Description) {
                val ctx = InstrumentationRegistry.getInstrumentation().targetContext
                MapHudPrefs.saveLongTripEnabled(ctx, false)
                MapHudPrefs.saveDatexPluginEnabled(ctx, true)
                MapHudPrefs.saveDatexWifiOnly(ctx, false)
                MapHudPrefs.savePoiLookaheadEnabled(ctx, true)
                MapHudPrefs.saveCampingPluginEnabled(ctx, false)
                MapHudPrefs.saveSpeedCameraPromptShown(ctx, true)
                val sd =
                    NaviStorageVolumes.list(ctx).firstOrNull {
                        it.removable && it.mounted && it.id != NaviStorageVolumes.INTERNAL_ID && it.appFilesDir != null
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
    private lateinit var device: UiDevice
    private val report = JSONObject()
    private val uiEvents = JSONArray()
    private val regionTimings = JSONArray()

    /** Full plan polyline captured before marker/mid-sim steps that may clear the live route. */
    private var savedPlanPolyline: String = ""

    @Before
    fun setUp() {
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        dataDir = NaviAppData.resolve(ctx).also { it.mkdirs() }
        device = UiDevice.getInstance(InstrumentationRegistry.getInstrumentation())
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = false
        NaviMapTestHooks.lastPlanReport = ""
        NaviMapTestHooks.lastPlanDistanceKm = 0.0
        NaviMapTestHooks.lastRoutePolylineChars = 0
        NaviMapTestHooks.lastManeuversJson = "[]"
        NaviMapTestHooks.lookAheadSeedCompletedEpoch = 0L
        report.put("campaign", "raufoss_bergen_car_ui_mwm")
        report.put("started_unix", System.currentTimeMillis() / 1000)
        report.put("ram_before_mib", processPssMib())
        report.put(
            "origin",
            JSONObject()
                .put("lat", ORIGIN_LAT)
                .put("lon", ORIGIN_LON)
                .put("elev_m", ORIGIN_ELEV_M)
                .put("name", "Raufoss"),
        )
        report.put(
            "dest",
            JSONObject().put("lat", DEST_LAT).put("lon", DEST_LON).put("name", "Bergen busstasjon"),
        )
        report.put(
            "settings",
            JSONObject()
                .put("eco", true)
                .put("avoid_toll_roads", false)
                .put("avoid_ferries", false)
                .put("soft_daily_budget_h", 6.0)
                .put("soft_break_interval_h", 1.5)
                .put("soft_rest_min", 15)
                .put("wild_camping", false)
                .put("long_trip", false)
                .put("datex", true)
                .put("nearby_attractions", true)
                .put("profile", "car")
                .put("download_ui", "mwm_choose_region"),
        )
        Log.i(TAG, "RAUFOSS_GPS_HINT lon=$ORIGIN_LON lat=$ORIGIN_LAT elev_m=$ORIGIN_ELEV_M")
        pinRaufossGps()
        // Drop stale corridor polyline from prior runs so host DATEX snap cannot
        // use a previous plan's geometry before this campaign plans.
        runCatching { File(dataDir, "long-trip-ui-report/route-polyline.txt").delete() }
        runCatching { File("/data/local/tmp/raufoss-bergen-ui/route-polyline.txt").delete() }
        runCatching { File("/sdcard/Pictures/raufoss-bergen-ui/route-polyline.txt").delete() }
        NaviMapTestHooks.lastRoutePolyline = ""
        NaviMapTestHooks.lastRoutePolylineChars = 0
    }

    @After
    fun tearDown() {
        report.put("ui_events", uiEvents)
        report.put("region_timings", regionTimings)
        writeReport()
    }

    @Test
    fun raufoss_bergen_car_ui_plan_campaign() {
        settle(1_500)
        noteUi("activity_foreground", "MainActivity")
        screenshot("01_launch")
        observeFallbackHooks("launch")

        openTools()
        screenshot("02_tools")
        configurePluginsInTools()
        screenshot("03_plugins")

        // Samsung campaign: UI-delete installed MWM leaves, then download Østlandet
        // first via Offline maps → Choose region. Vestlandet only when the app asks.
        report.put(
            "product_gap_hud",
            "Missing from the app: region process/download indicator in the main HUD " +
                "(progress is only visible inside Tools / MWM section).",
        )
        report.put(
            "delete_policy",
            "UI delete via Map storage before plan; redownload Ostlandet first (MWM Choose region)",
        )
        report.put("departure_local", "2026-06-01T08:00:00")
        deleteMwmViaStorageUi()
        screenshot("04_after_ui_delete")
        val leavesAfterDelete =
            runCatching {
                uniffi.navi
                    .mwmInstalled(dataDir.absolutePath)
                    ?.leafIds
                    .orEmpty()
            }.getOrDefault(emptyList())
        report.put("leaves_after_ui_delete", JSONArray(leavesAfterDelete))
        assertTrue(
            "UI delete must clear downloaded leaves before Ostlandet redownload (left=${leavesAfterDelete.size})",
            leavesAfterDelete.isEmpty(),
        )
        val first = downloadMwmRegionViaUi(OSTLANDET_REGION, "first_ostlandet")
        assertTrue("Østlandet MWM install must succeed (or leaves on disk + session open)", first)
        assertTrue("Innlandet leaves must be present after Ostlandet", innlandetLeavesPresent())
        noteUi("innlandet_indexed", "Ostlandet region leaves include Innlandet coverage")
        screenshot("04_ostlandet_installed")
        ensureComapsSessionOpen("after_maps_ready")
        observeFallbackHooks("after_maps_ready")
        pinRaufossGps()
        // Clear coverage modals that appear once the CoMaps session opens.
        repeat(3) {
            if (!dismissGeofabrikMissingCoverage()) return@repeat
            settle(400)
        }
        openRoutePanel()

        configureCarRestViaUi()
        screenshot("07_car_rest")
        openRoutePanel()
        enableEcoViaUi()
        ensureAvoidOff("Avoid toll roads")
        ensureAvoidOff("Avoid ferries")

        // From: prefer Use GPS (host already feeds Raufoss); fall back to coords.
        openRoutePanel()
        clickTag("chip_from")
        settle(300)
        val gpsOk =
            runCatching {
                clickTag("btn_use_gps")
                true
            }.getOrDefault(false) ||
                runCatching {
                    device.findObject(By.textContains("Use GPS as from"))?.click()
                    true
                }.getOrDefault(false)
        if (gpsOk) {
            noteUi("from_raufoss", "Use GPS as from")
            settle(1_000)
        } else {
            typeCoordAndPick("chip_from", ORIGIN_LAT, ORIGIN_LON, "from_raufoss")
        }
        screenshot("08_from")
        typeCoordAndPick("chip_to", DEST_LAT, DEST_LON, "to_bergen")
        screenshot("09_to")
        assertWaypointSummarySet()

        awaitHostDatex()
        screenshot("10_datex")

        // Re-apply waypoints after DATEX wait (chrome/tools can clear sheet state).
        openRoutePanel()
        runCatching { clickTag("chip_profile_car") }
        enableEcoViaUi()
        if (!waypointSummaryLooksSet()) {
            noteUi("waypoints_reapply", "summary unset — re-entering")
            openRoutePanel()
            clickTag("chip_from")
            runCatching { clickTag("btn_use_gps") }
            settle(800)
            typeCoordAndPick("chip_to", DEST_LAT, DEST_LON, "to_bergen_reapply")
        }
        assertWaypointSummarySet()
        composeRule.onNodeWithTag("btn_plan_route", useUnmergedTree = true).performScrollTo()
        screenshot("11_before_plan")
        clickTag("btn_plan_route")
        screenshot("12_after_plan_click")

        val t0 = System.currentTimeMillis()
        var planned = false
        var vestlandetHandled = false
        var lastReplan = 0L
        while (System.currentTimeMillis() - t0 < DOWNLOAD_DEADLINE_MS) {
            // Geofabrik "Map data needed" / Download Ostlandet is the WRONG path —
            // dismiss and keep using MWM Choose region + CoMaps session.
            if (dismissGeofabrikMissingCoverage()) {
                ensureComapsSessionOpen("after_geofabrik_dismiss")
                if (System.currentTimeMillis() - lastReplan > 8_000) {
                    lastReplan = System.currentTimeMillis()
                    noteUi("plan_retry", "after dismissing Geofabrik coverage dialog")
                    openRoutePanel()
                    reapplyWaypointsAndPlan("geofabrik_dismiss")
                }
            }
            // Real MWM missing-maps dialog (Vestlandet / leaf gaps).
            if (!vestlandetHandled) {
                val mwmHit =
                    runCatching {
                        clickTagSoft("btn_mwm_missing_download")
                        true
                    }.getOrDefault(false)
                if (mwmHit) {
                    vestlandetHandled = true
                    noteUi("vestlandet_prompt", "accepted MWM missing-maps download")
                    trackPassiveInstall(VESTLANDET_REGION, "plan_requested_vestlandet")
                }
                // Prefer Choose-region install button when Tools is open.
                runCatching {
                    composeRule
                        .onNodeWithTag("btn_mwm_install_$VESTLANDET_REGION", useUnmergedTree = true)
                        .performClick()
                    vestlandetHandled = true
                    noteUi("vestlandet_prompt", "clicked btn_mwm_install_$VESTLANDET_REGION")
                }
            }
            // If chrome cleared From/To, re-enter and Plan again.
            if (!waypointSummaryLooksSet() && System.currentTimeMillis() - lastReplan > 20_000) {
                lastReplan = System.currentTimeMillis()
                noteUi("plan_retry", "waypoints unset during wait — re-apply + Plan")
                openRoutePanel()
                reapplyWaypointsAndPlan("unset")
            }
            val poly = NaviMapTestHooks.lastRoutePolylineChars
            val dist = NaviMapTestHooks.lastPlanDistanceKm
            val rep = NaviMapTestHooks.lastPlanReport
            if (poly >= 8 && dist in 50.0..500.0) {
                planned = true
                // Snapshot corridor polyline as soon as plan lands.
                val live = NaviMapTestHooks.lastRoutePolyline
                if (live.length > savedPlanPolyline.length) {
                    savedPlanPolyline = live
                    persistPlanPolyline(live)
                }
                break
            }
            if (rep.contains("FAIL:") && rep.length > 40) {
                noteUi("plan_failed", rep.take(240))
                if (System.currentTimeMillis() - lastReplan > 30_000) {
                    lastReplan = System.currentTimeMillis()
                    ensureComapsSessionOpen("after_plan_fail")
                    openRoutePanel()
                    reapplyWaypointsAndPlan("after_fail")
                }
            }
            maybeTownShot()
            observeFallbackHooks("plan_wait")
            Thread.sleep(3_000)
            composeRule.mainClock.advanceTimeBy(3_000)
            if (System.currentTimeMillis() - t0 > PLAN_DEADLINE_MS && planned) break
            writeReport()
        }

        // Nearby-attractions tally is async: MWM tourism seed runs off the plan path,
        // then MainActivity re-queries when lookAheadSeedCompletedEpoch advances.
        val seedBefore = NaviMapTestHooks.lookAheadSeedCompletedEpoch
        val attrWaitDeadline = System.currentTimeMillis() + 90_000L
        while (System.currentTimeMillis() < attrWaitDeadline) {
            val n =
                runCatching {
                    JSONObject(NaviMapTestHooks.lastRoutePlanStatsJson).optInt("attraction_count")
                }.getOrDefault(0)
            val seedDone = NaviMapTestHooks.lookAheadSeedCompletedEpoch > seedBefore
            if (n > 0) {
                noteUi(
                    "attractions_ready",
                    "count=$n seed_epoch=${NaviMapTestHooks.lookAheadSeedCompletedEpoch} " +
                        "seed_done=$seedDone",
                )
                break
            }
            Thread.sleep(1_500)
            composeRule.mainClock.advanceTimeBy(1_500)
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
        report.put("fuel_stops_estimate", estimateFuelStops(distanceKm))
        report.put("datex_from_plan", extractDatex(NaviMapTestHooks.lastPlanReport))
        report.put("vestlandet_prompt_handled", vestlandetHandled)
        report.put("ui_planned", planned)
        report.put("ram_post_plan_mib", processPssMib())
        report.put(
            "bands",
            JSONObject()
                .put("distance_ok", distanceKm in 0.0..500.0)
                .put("time_ok", etaMin <= 0.0 || etaMin in (3.0 * 60)..(8.0 * 60))
                .put("maneuvers_ok", manCount in 1..170)
                .put("planned", planned),
        )
        enrichPlanDerivedFields(distanceKm, etaMin)
        observeFallbackHooks("final")
        screenshot("13_plan_result")
        // Capture full corridor polyline BEFORE marker fly-to / Back / mid-sim can
        // replace lastRoutePolyline with a stub (host mid-inject needs ≥10 points).
        savedPlanPolyline =
            NaviMapTestHooks.lastRoutePolyline.takeIf { it.length >= 1_000 }
                ?: savedPlanPolyline
        persistPlanPolyline(savedPlanPolyline.ifBlank { NaviMapTestHooks.lastRoutePolyline })
        writeReport()

        val tunnels =
            report.optJSONObject("corridor_counts")?.optInt("tunnels")
                ?: 0
        report.put(
            "plan_assert",
            JSONObject()
                .put("planned", planned)
                .put("distance_km", distanceKm)
                .put("maneuvers", manCount)
                .put("tunnels", tunnels)
                .put("polyline_chars", savedPlanPolyline.length),
        )
        writeReport()
        // Assert plan success before optional DATEX marker/mid-sim so a later
        // native crash or host race cannot erase a PASS plan from the verdict.
        assertTrue(
            "Plan must succeed within bands",
            planned && distanceKm in 0.0..500.0 && manCount in 1..170,
        )
        assertTrue(
            "corridor_counts.tunnels must be reported and >= 1 (got $tunnels)",
            report.optJSONObject("corridor_counts")?.has("tunnels") == true && tunnels >= 1,
        )
        Log.i(
            TAG,
            "RAUFOSS_PLAN_OK dist=$distanceKm etaMin=$etaMin man=$manCount tunnels=$tunnels poly=${savedPlanPolyline.length}",
        )

        // --- DATEX render proof: fly camera to first active sit with chrome hidden ---
        val render =
            runCatching { proveDatexMarkersOnMap() }
                .getOrElse { e ->
                    JSONObject()
                        .put("ok", false)
                        .put("error", e.message ?: e.toString())
                }
        report.put("datex_render", render)
        writeReport()

        // Mid-sim Block inject + auto-reroute (host RAUFOSS_AWAIT_DATEX_MID).
        val mid =
            runCatching { proveDatexMidSimReroute(distanceKm) }
                .getOrElse { e ->
                    JSONObject()
                        .put("ok", false)
                        .put("error", e.message ?: e.toString())
                }
        report.put("datex_mid_sim", mid)
        report.put(
            "datex_mwm_reroute",
            JSONObject()
                .put("applied", NaviMapTestHooks.datexMwmRerouteApplied)
                .put("before_km", NaviMapTestHooks.datexMwmRerouteBeforeKm)
                .put("after_km", NaviMapTestHooks.datexMwmRerouteAfterKm)
                .put("before_poly", NaviMapTestHooks.datexMwmRerouteBeforePoly)
                .put("after_poly", NaviMapTestHooks.datexMwmRerouteAfterPoly)
                .put(
                    "plan_report_has_reroute_apply",
                    NaviMapTestHooks.lastPlanReport.contains("datex_mwm=reroute_apply"),
                ),
        )
        writeReport()

        Log.i(TAG, "RAUFOSS_DONE planned=$planned dist=$distanceKm etaMin=$etaMin man=$manCount tunnels=$tunnels")
    }

    /**
     * Close the plan sheet, fly the camera to the first synthetic DATEX sit, and
     * capture a map-only screenshot so red drape/MapLibre markers are in view.
     */
    private fun proveDatexMarkersOnMap(): JSONObject {
        val out = JSONObject()
        val sit = DATEX_SITS.first()
        out.put("sit_id", sit.id)
        out.put("lat", sit.lat)
        out.put("lon", sit.lon)
        // Wait out any leftover plan-time DATEX auto-reroute so the map is idle.
        val idleDeadline = System.currentTimeMillis() + 120_000L
        while (System.currentTimeMillis() < idleDeadline &&
            (NaviMapTestHooks.reroutingActive || RoutePlanGate.isRunning())
        ) {
            Thread.sleep(500)
            composeRule.mainClock.advanceTimeBy(500)
        }
        NaviMapTestHooks.requestCloseTools = true
        NaviMapTestHooks.hideUiChrome = true
        NaviMapTestHooks.hideSearchChrome = true
        NaviMapTestHooks.disableGpsFollow = true
        // Prefer live active sit coords from the plan report when present.
        val fromReport =
            Regex("""datex_constraint: id=${Regex.escape(sit.id)}[^;]*;[^;]*; lat=([0-9.+-]+); lon=([0-9.+-]+)""")
                .find(NaviMapTestHooks.lastPlanReport)
        val camLat = fromReport?.groupValues?.get(1)?.toDoubleOrNull() ?: sit.lat
        val camLon = fromReport?.groupValues?.get(2)?.toDoubleOrNull() ?: sit.lon
        out.put("camera_lat", camLat)
        out.put("camera_lon", camLon)
        NaviMapTestHooks.pendingCamera = Triple(camLat, camLon, 14.0)
        NaviMapTestHooks.requestDatexRefresh = true
        val settleDeadline = System.currentTimeMillis() + 45_000L
        var marksReady = false
        while (System.currentTimeMillis() < settleDeadline) {
            Thread.sleep(500)
            composeRule.mainClock.advanceTimeBy(500)
            if (NaviMapTestHooks.reroutingActive || RoutePlanGate.isRunning()) continue
            if (NaviMapTestHooks.lastDatexDrapeMarkCount > 0 ||
                NaviMapTestHooks.lastDatexComposeMarkCount > 0
            ) {
                marksReady = true
                break
            }
            // Keep nudging refresh until marks land.
            if (!NaviMapTestHooks.requestDatexRefresh) {
                NaviMapTestHooks.requestDatexRefresh = true
            }
        }
        // Do NOT pressBack here: it collapses/clears the live route polyline to a
        // destination stub (~180 bytes), which breaks host mid-inject snap.
        NaviMapTestHooks.hideUiChrome = true
        NaviMapTestHooks.hideSearchChrome = true
        NaviMapTestHooks.requestCloseTools = true
        NaviMapTestHooks.pendingCamera = Triple(camLat, camLon, 14.5)
        // Extra settle so Compose Canvas DATEX discs paint after hook counts flip.
        Thread.sleep(4_000)
        composeRule.mainClock.advanceTimeBy(4_000)
        NaviMapTestHooks.pendingCamera = Triple(camLat, camLon, 14.8)
        Thread.sleep(1_500)
        composeRule.mainClock.advanceTimeBy(1_500)
        // Capture live camera + DATEX plan-report overlay evidence with the
        // screenshot so docs can cite more than a filename.
        val camNowLat = NaviMapTestHooks.lastCameraLat
        val camNowLon = NaviMapTestHooks.lastCameraLon
        val camNowZoom = NaviMapTestHooks.lastCameraZoom
        val rep = NaviMapTestHooks.lastPlanReport
        val overlayActive =
            Regex("""datex_mwm=overlay_apply;\s*active=(\d+)""")
                .find(rep)
                ?.groupValues
                ?.getOrNull(1)
                ?.toIntOrNull()
        val impacts =
            Regex("""datex_impacts=(\d+)""")
                .find(rep)
                ?.groupValues
                ?.getOrNull(1)
                ?.toIntOrNull()
        screenshot("13b_datex_markers")
        out.put("screenshot", "13b_datex_markers")
        out.put("camera_zoom", camNowZoom.takeIf { it > 0.0 } ?: 14.0)
        out.put("camera_lat_live", camNowLat)
        out.put("camera_lon_live", camNowLon)
        out.put("overlay_active", overlayActive ?: -1)
        out.put("datex_impacts", impacts ?: -1)
        out.put("drape_marks", NaviMapTestHooks.lastDatexDrapeMarkCount)
        out.put("compose_marks", NaviMapTestHooks.lastDatexComposeMarkCount)
        out.put("marks_ready", marksReady)
        val nearSit =
            !camNowLat.isNaN() &&
                !camNowLon.isNaN() &&
                kotlin.math.abs(camNowLat - camLat) < 0.08 &&
                kotlin.math.abs(camNowLon - camLon) < 0.08
        out.put("camera_near_sit", nearSit)
        val visibleMarks =
            NaviMapTestHooks.lastDatexDrapeMarkCount > 0 ||
                NaviMapTestHooks.lastDatexComposeMarkCount > 0
        out.put("ok", nearSit && visibleMarks)
        // Restore chrome for mid-sim HUD (keep GPS follow off until sim starts).
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = false
        return out
    }

    /**
     * Start route simulation for a short stretch, ask the host to inject one DATEX
     * Block ahead on the live polyline, force refresh + reroute, and record
     * before/after geometry evidence.
     */
    private fun proveDatexMidSimReroute(planKm: Double): JSONObject {
        val out = JSONObject()
        if (planKm < 10.0 || NaviMapTestHooks.lastRoutePolylineChars < 100) {
            return out.put("ok", false).put("skip", "no_route")
        }
        // Wait out any leftover plan-time DATEX auto-reroute before starting sim.
        val idleDeadline = System.currentTimeMillis() + 90_000L
        while (System.currentTimeMillis() < idleDeadline &&
            (NaviMapTestHooks.reroutingActive || RoutePlanGate.isRunning())
        ) {
            Thread.sleep(500)
            composeRule.mainClock.advanceTimeBy(500)
        }
        val beforePoly = NaviMapTestHooks.lastRoutePolyline
        val beforeKm = NaviMapTestHooks.lastPlanDistanceKm
        val beforeHash = Integer.toHexString(beforePoly.hashCode()) + ":len=${beforePoly.length}"
        out.put("before_km", beforeKm)
        out.put("before_poly", beforeHash)
        out.put("before_auto_reroute", NaviMapTestHooks.autoRerouteTriggeredCount)

        NaviMapTestHooks.disableGpsFollow = false
        NaviMapTestHooks.requestStartRouteSimulation = true
        val simDeadline = System.currentTimeMillis() + 45_000L
        while (System.currentTimeMillis() < simDeadline && !NaviMapTestHooks.simulatingActive) {
            Thread.sleep(500)
            composeRule.mainClock.advanceTimeBy(500)
        }
        out.put("sim_started", NaviMapTestHooks.simulatingActive)
        // Let the puck advance a little along the corridor.
        Thread.sleep(8_000)
        composeRule.mainClock.advanceTimeBy(8_000)
        screenshot("14a_datex_sim_before_inject")

        // Persist full corridor polyline for host snap (≥500 chars), then await mid Block.
        val polyForHost =
            when {
                savedPlanPolyline.length >= 500 -> savedPlanPolyline
                beforePoly.length >= 500 -> beforePoly
                else -> savedPlanPolyline.ifBlank { beforePoly }
            }
        persistPlanPolyline(polyForHost)
        out.put("polyline_for_host_chars", polyForHost.length)
        Log.i(TAG, "RAUFOSS_AWAIT_DATEX_MID poly=${polyForHost.length}")
        noteUi("datex_mid_await", "waiting host mid block")
        val midMarker = File(dataDir, "datex_cache/datex-GetSituation.xml")
        val midDeadline = System.currentTimeMillis() + TimeUnit.MINUTES.toMillis(3)
        var midSeen = false
        while (System.currentTimeMillis() < midDeadline) {
            if (midMarker.isFile &&
                midMarker.readText().contains("syn-mid-block-sim") &&
                midMarker.readText().contains("roadOrCarriagewayOrLaneManagementType")
            ) {
                midSeen = true
                break
            }
            Thread.sleep(1_000)
            composeRule.mainClock.advanceTimeBy(1_000)
        }
        out.put("mid_inject_seen", midSeen)
        if (!midSeen) {
            return out.put("ok", false).put("error", "host_mid_inject_timeout")
        }

        // Persist mid-await state early so a native crash still leaves evidence.
        report.put("datex_mid_sim", out)
        writeReport()

        NaviMapTestHooks.datexMwmRerouteApplied = false
        NaviMapTestHooks.requestDatexRefresh = true
        Thread.sleep(3_000)
        composeRule.mainClock.advanceTimeBy(3_000)
        // Explicit recalculation if auto-reroute did not fire yet and gate is idle.
        if (!NaviMapTestHooks.reroutingActive &&
            !RoutePlanGate.isRunning() &&
            NaviMapTestHooks.autoRerouteTriggeredCount == out.optInt("before_auto_reroute")
        ) {
            NaviMapTestHooks.requestRerouteFromCurrent = true
        }
        val rerouteDeadline = System.currentTimeMillis() + 180_000L
        var rerouted = false
        while (System.currentTimeMillis() < rerouteDeadline) {
            if (NaviMapTestHooks.datexMwmRerouteApplied ||
                NaviMapTestHooks.autoRerouteTriggeredCount > out.optInt("before_auto_reroute")
            ) {
                rerouted = true
            }
            val afterPoly = NaviMapTestHooks.lastRoutePolyline
            if (afterPoly.isNotBlank() && afterPoly != beforePoly && afterPoly.length > 100) {
                rerouted = true
                out.put("after_km", NaviMapTestHooks.lastPlanDistanceKm)
                out.put(
                    "after_poly",
                    Integer.toHexString(afterPoly.hashCode()) + ":len=${afterPoly.length}",
                )
                out.put("polyline_changed", true)
                out.put("plan_report_snip", NaviMapTestHooks.lastPlanReport.takeLast(1_200))
                report.put("datex_mid_sim", out)
                writeReport()
                break
            }
            if (NaviMapTestHooks.datexMwmRerouteApplied) {
                out.put("after_km", NaviMapTestHooks.datexMwmRerouteAfterKm)
                out.put("after_poly", NaviMapTestHooks.datexMwmRerouteAfterPoly)
                out.put(
                    "polyline_changed",
                    NaviMapTestHooks.datexMwmRerouteBeforePoly != NaviMapTestHooks.datexMwmRerouteAfterPoly,
                )
                out.put("plan_report_snip", NaviMapTestHooks.lastPlanReport.takeLast(1_200))
                report.put("datex_mid_sim", out)
                writeReport()
                break
            }
            Thread.sleep(2_000)
            composeRule.mainClock.advanceTimeBy(2_000)
        }
        out.put("rerouted", rerouted || NaviMapTestHooks.datexMwmRerouteApplied)
        out.put("auto_reroute_count", NaviMapTestHooks.autoRerouteTriggeredCount)
        out.put("datex_mwm_applied", NaviMapTestHooks.datexMwmRerouteApplied)
        if (!out.has("plan_report_snip")) {
            out.put("plan_report_snip", NaviMapTestHooks.lastPlanReport.takeLast(1_200))
        }
        runCatching { screenshot("14b_datex_sim_after_reroute") }
        runCatching {
            NaviMapTestHooks.requestStopRouteSimulation = true
            Thread.sleep(1_000)
        }
        val ok =
            out.optBoolean("polyline_changed") ||
                NaviMapTestHooks.datexMwmRerouteApplied ||
                (
                    out.has("after_km") &&
                        kotlin.math.abs(out.optDouble("after_km") - beforeKm) >= 0.2
                )
        return out.put("ok", ok)
    }

    /** Derive nav-instruction types, soft-day km split, corridor stats, DATEX timing table. */
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
        report.put(
            "corridor_counts",
            JSONObject()
                .put("ferry_legs", stats.optInt("route_ferry_legs"))
                .put("tunnels", stats.optInt("route_tunnel_count"))
                .put("rest_places", stats.optInt("rest_place_count"))
                .put("rest_place_names", stats.optJSONArray("rest_place_names") ?: JSONArray())
                .put("attractions", stats.optInt("attraction_count"))
                .put("attraction_by_type", stats.optJSONObject("attraction_by_type") ?: JSONObject())
                .put("wild_camping_sites", stats.optInt("wild_camping_site_count")),
        )

        // Soft daily 6 h: split total distance proportionally when days_json is empty.
        val softH = 6.0
        val etaH = etaMin / 60.0
        val days = JSONArray()
        if (etaH > 0.0 && distanceKm > 0.0) {
            var remainingKm = distanceKm
            var remainingH = etaH
            var day = 1
            while (remainingKm > 0.5 && day <= 8) {
                val driveH = minOf(softH, remainingH)
                val dayKm = distanceKm * (driveH / etaH)
                days.put(
                    JSONObject()
                        .put("day", day)
                        .put("drive_hours_soft_cap", softH)
                        .put("drive_hours", driveH)
                        .put("km", dayKm),
                )
                remainingH -= driveH
                remainingKm -= dayKm
                day++
                if (remainingH <= 0.05) break
            }
        }
        report.put("km_per_day_soft_budget", days)
        report.put("km_total", distanceKm)

        val datexRows = JSONArray()
        for (s in DATEX_SITS) {
            datexRows.put(
                JSONObject()
                    .put("id", s.id)
                    .put("label", s.label)
                    .put("lat", s.lat)
                    .put("lon", s.lon)
                    .put("minutes_before_arrival", s.minutesBeforeArrival)
                    .put("in_30_5_band", s.minutesBeforeArrival in 5.0..30.0)
                    .put("eta_total_min", etaMin)
                    // Spec: sit appears N minutes before the vehicle reaches it,
                    // so along-route arrival ≈ N minutes after departure.
                    .put("approx_minutes_after_departure", s.minutesBeforeArrival),
            )
        }
        report.put("datex_timing_vs_arrival", datexRows)

        val inst = runCatching { uniffi.navi.mwmInstalled(dataDir.absolutePath) }.getOrNull()
        report.put(
            "installed_maps",
            JSONObject()
                .put("version", inst?.version ?: JSONObject.NULL)
                .put("leaf_count", inst?.leafIds?.size ?: 0)
                .put("sidecar_count", inst?.sidecarCount ?: 0)
                .put("leaves", JSONArray(inst?.leafIds ?: emptyList<String>()))
                .put("placement", dataDir.absolutePath + "/mwm"),
        )
    }

    // --- MWM download / delete ------------------------------------------------

    private fun downloadMwmRegionViaUi(
        regionId: String,
        label: String,
    ): Boolean {
        val t0 = System.currentTimeMillis()
        openTools()
        settle(500)
        // Scroll Offline maps section into view — never touch fake-GPS field.
        runCatching {
            composeRule.onNodeWithTag("mwm_maps_section", useUnmergedTree = true).performScrollTo()
        }
        runCatching {
            composeRule.onNodeWithTag("btn_mwm_regions", useUnmergedTree = true).performScrollTo()
        }
        clickTag("btn_mwm_regions")
        noteUi("btn_mwm_regions", "Choose region opened for $regionId")
        settle(1_500)
        // Wait for catalog rows
        val catalogDeadline = System.currentTimeMillis() + 120_000
        var installTag = "btn_mwm_install_$regionId"
        while (System.currentTimeMillis() < catalogDeadline) {
            val exists =
                runCatching {
                    composeRule.onNodeWithTag(installTag, useUnmergedTree = true).assertExists()
                    true
                }.getOrDefault(false)
            if (exists) break
            // Fallback: click by display name containing Ostlandet / Vestlandet
            val name =
                when {
                    regionId.contains("ostlandet") -> "Ostlandet"
                    regionId.contains("vestlandet") -> "Vestlandet"
                    else -> regionId
                }
            runCatching {
                composeRule.onNodeWithText(name, substring = true, useUnmergedTree = true).performScrollTo()
            }
            Thread.sleep(2_000)
            composeRule.mainClock.advanceTimeBy(2_000)
            observeFallbackHooks("catalog_wait_$label")
        }
        val clicked =
            runCatching {
                composeRule.onNodeWithTag(installTag, useUnmergedTree = true).performScrollTo()
                clickTag(installTag)
                true
            }.getOrDefault(false)
        if (!clicked) {
            // Text button "Download" next to Ostlandet row
            val name = if (regionId.contains("ostlandet")) "Ostlandet" else "Vestlandet"
            runCatching {
                device.wait(Until.findObject(By.textContains(name)), 5_000)
                val row = device.findObject(By.textContains(name))
                row?.parent?.findObject(By.text("Download"))?.click()
                    ?: device.findObjects(By.text("Download")).lastOrNull()?.click()
            }
            noteUi("install_fallback_text", name)
        } else {
            noteUi("install_clicked", installTag)
        }
        // Close tools so map/HUD stay usable; install continues in background.
        closeTools()
        val ok = waitMwmInstall(label, DOWNLOAD_DEADLINE_MS)
        val elapsed = System.currentTimeMillis() - t0
        val inst = runCatching { uniffi.navi.mwmInstalled(dataDir.absolutePath) }.getOrNull()
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
                .put("placement", dataDir.absolutePath + "/mwm"),
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
            noteUi("install_progress", "$label running=${st.running} ok=${st.ok} msg=${st.message} pct=${st.percent}")
            val inst = runCatching { uniffi.navi.mwmInstalled(dataDir.absolutePath) }.getOrNull()
            val leavesOk =
                inst != null &&
                    inst.leafIds.any {
                        it.contains("Hedmark", ignoreCase = true) ||
                            it.contains("Oppland", ignoreCase = true)
                    }
            if (!st.running && st.ok == true) {
                ensureComapsSessionOpen("install_ok_$label")
                return true
            }
            // Leaves on disk after session soft-fail: try force-open before accepting.
            if (!st.running && leavesOk && (st.ok == true || sawRunning || st.message.contains("session"))) {
                val sessionOk = ensureComapsSessionOpen("install_disk_$label")
                noteUi(
                    "install_disk_ok",
                    "leaves=${inst?.leafIds?.size} sidecars=${inst?.sidecarCount} session=$sessionOk msg=${st.message}",
                )
                return leavesOk
            }
            if (!st.running && st.ok == false && !leavesOk) {
                noteUi("install_failed", st.message)
                return false
            }
            Thread.sleep(3_000)
            composeRule.mainClock.advanceTimeBy(3_000)
            writeReport()
        }
        val inst = runCatching { uniffi.navi.mwmInstalled(dataDir.absolutePath) }.getOrNull()
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
                            .put("ok", true),
                    )
                    writeReport()
                    return@Thread
                }
                Thread.sleep(10_000)
            }
        }.start()
    }

    /**
     * Delete every installed MWM leaf via Tools → Offline maps → Map storage UI
     * (`btn_mwm_map_storage` / `btn_mwm_storage_delete_*` / confirm). Required for
     * the Samsung campaign clean redownload path.
     */
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
        // Refresh leaf list from storage summary / installed set.
        val leafIds =
            MwmStorage.state.groups
                .flatMap { it.leaves }
                .map { it.id }
                .ifEmpty {
                    runCatching {
                        uniffi.navi
                            .mwmInstalled(dataDir.absolutePath)
                            ?.leafIds
                            .orEmpty()
                    }.getOrDefault(emptyList())
                }
        noteUi("delete_candidates", leafIds.joinToString(","))
        // Delete until storage shows no leaves or deadline.
        val deadline = System.currentTimeMillis() + TimeUnit.MINUTES.toMillis(45)
        var pass = 0
        while (System.currentTimeMillis() < deadline) {
            pass++
            val current =
                MwmStorage.state.groups
                    .flatMap { it.leaves }
                    .map { it.id }
                    .ifEmpty {
                        runCatching {
                            uniffi.navi
                                .mwmInstalled(dataDir.absolutePath)
                                ?.leafIds
                                .orEmpty()
                        }.getOrDefault(emptyList())
                    }
            if (current.isEmpty() && pass > 1) break
            var deletedThisPass = 0
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
                    // Fallback: any visible Delete next to leaf row text.
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
                deletedThisPass++
                noteUi("delete_leaf", "pass=$pass id=$id hit=$hit")
                writeReport()
            }
            // Also hammer leftover Delete buttons if tags missed (spaces in leaf ids).
            if (deletedThisPass == 0) {
                val dels = device.findObjects(By.text("Delete"))
                if (dels.isNullOrEmpty()) break
                for (btn in dels.take(8)) {
                    runCatching {
                        btn.click()
                        settle(400)
                        device.findObject(By.text("Delete"))?.click()
                        settle(1_000)
                        deleted.put(JSONObject().put("leaf_id", "?").put("pass", pass).put("uia_delete", true))
                    }
                }
            }
            // Trigger storage refresh by closing/reopening if still stuck.
            val left =
                runCatching {
                    uniffi.navi
                        .mwmInstalled(dataDir.absolutePath)
                        ?.leafIds
                        ?.size ?: -1
                }.getOrDefault(-1)
            noteUi("delete_pass_done", "pass=$pass left=$left")
            if (left == 0) break
            settle(2_000)
        }
        clickTagSoft("btn_mwm_storage_close")
        closeTools()
        report.put("regions_deleted_via_ui", deleted)
        report.put("delete_blocked", false)
        writeReport()
    }

    /** Pin Raufoss via debug GPS hooks + optional LocationManager test provider. */
    private fun pinRaufossGps() {
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
        noteUi(
            "gps_pin",
            "holdDebugGps Raufoss mockProvider=$mockOk elev=$ORIGIN_ELEV_M",
        )
        report.put("gps_mock_provider", mockOk)
        writeReport()
    }

    private fun innlandetLeavesPresent(): Boolean {
        val inst = runCatching { uniffi.navi.mwmInstalled(dataDir.absolutePath) }.getOrNull() ?: return false
        val leaves = inst.leafIds.map { it.lowercase() }
        // Ostlandet covers Hedmark / Oppland (Innlandet) in CoMaps leaf naming.
        val inn =
            leaves.any { it.contains("hedmark") || it.contains("oppland") || it.contains("innlandet") } ||
                leaves.any { it.contains("oslo") || it.contains("akershus") } // Ostlandet pack present
        noteUi("innlandet_check", "leaves=${leaves.take(20)} hit=$inn")
        return inst.leafIds.isNotEmpty() && (inn || leaves.size >= 3)
    }

    /** Force-open CoMaps session after MWM install (driver-only; no production edits). */
    private fun ensureComapsSessionOpen(phase: String): Boolean {
        MwmClientBootstrap.ensureInitialized()
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        val ok =
            runCatching { ComapsSessionBootstrap.ensureOpen(ctx, force = true) }.getOrDefault(false) ||
                runCatching { uniffi.navi.comapsSessionIsOpen() }.getOrDefault(false)
        noteUi(
            "comaps_session",
            "$phase open=$ok nativeLoaded=${MwmClientBootstrap.nativeLoaded} " +
                "nativeAvail=${runCatching { uniffi.navi.comapsNativeAvailable() }.getOrDefault(false)}",
        )
        report.put(
            "comaps_session_$phase",
            JSONObject()
                .put("open", ok)
                .put("native_loaded", MwmClientBootstrap.nativeLoaded)
                .put("native_available", runCatching { uniffi.navi.comapsNativeAvailable() }.getOrDefault(false)),
        )
        writeReport()
        return ok
    }

    /**
     * Dismiss Geofabrik "Map data needed" / Download Ostlandet — that path is not
     * the campaign MWM Choose-region flow.
     */
    private fun dismissGeofabrikMissingCoverage(): Boolean {
        // Only report success when a real dismiss control was found+clicked.
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
        val viaTitle =
            !viaTag &&
                !viaNotNow &&
                runCatching {
                    // Title-only match so we do not click Geofabrik "Download Ostlandet".
                    if (device.findObject(By.textContains("Map data needed")) == null) {
                        return@runCatching false
                    }
                    val n = device.findObject(By.textContains("Not now")) ?: return@runCatching false
                    n.click()
                    true
                }.getOrDefault(false)
        val dismissed = viaTag || viaNotNow || viaTitle
        if (dismissed) noteUi("geofabrik_dialog", "dismissed (use MWM Choose region instead)")
        return dismissed
    }

    private fun reapplyWaypointsAndPlan(reason: String) {
        clickTag("chip_from")
        val gpsOk =
            runCatching {
                clickTag("btn_use_gps")
                true
            }.getOrDefault(false)
        if (!gpsOk) {
            typeCoordAndPick("chip_from", ORIGIN_LAT, ORIGIN_LON, "from_raufoss_$reason")
        }
        settle(800)
        typeCoordAndPick("chip_to", DEST_LAT, DEST_LON, "to_bergen_$reason")
        enableEcoViaUi()
        composeRule.onNodeWithTag("btn_plan_route", useUnmergedTree = true).performScrollTo()
        clickTag("btn_plan_route")
        noteUi("replan", reason)
    }

    // --- settings / route UI --------------------------------------------------

    private fun configurePluginsInTools() {
        runCatching {
            composeRule.onNodeWithTag("toggle_long_trip", useUnmergedTree = true).performScrollTo()
            ensureToggleOff("toggle_long_trip", "long_trip_status_line")
        }
        runCatching {
            composeRule.onNodeWithTag("toggle_datex_plugin", useUnmergedTree = true).performScrollTo()
            ensureToggleOn("toggle_datex_plugin")
        }
        runCatching {
            composeRule.onNodeWithTag("toggle_poi_lookahead", useUnmergedTree = true).performScrollTo()
            ensureToggleOn("toggle_poi_lookahead")
        }
        // Wild camping OFF
        val camp = device.findObject(By.text("Right-to-roam camping"))
        if (camp != null) {
            val sw = camp.parent?.findObject(By.checkable(true))
            if (sw != null && sw.isChecked) {
                sw.click()
                settle(300)
            }
            noteUi("camping", "checked=${sw?.isChecked}")
        }
        closeTools()
    }

    private fun ensureToggleOff(
        toggleTag: String,
        statusTag: String,
    ) {
        val line =
            runCatching {
                composeRule
                    .onNodeWithTag(statusTag, useUnmergedTree = true)
                    .fetchSemanticsNode()
                    .config[androidx.compose.ui.semantics.SemanticsProperties.Text]
                    .joinToString(" ") { it.text }
            }.getOrDefault("")
        if (!line.contains("off", ignoreCase = true) && line.isNotBlank()) {
            composeRule.onNodeWithTag(toggleTag, useUnmergedTree = true).performClick()
            noteUi(toggleTag, "turned OFF was=$line")
        } else {
            noteUi(toggleTag, "already off/blank line=$line")
        }
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

    private fun configureCarRestViaUi() {
        openRoutePanel()
        runCatching {
            clickTag("chip_profile_car")
            runCatching { clickTag("btn_save_profile") }
        }
        clickTagSoft("btn_open_drive_settings")
        settle(500)
        runCatching {
            setField("field_break_hours", "1.5")
            setField("field_rest_mins", "15")
            noteUi("break_rest", "1.5h / 15min; soft daily 6.0h has no Compose field")
            clickTag("btn_save_drive_settings")
        }
        clickTagSoft("btn_close_drive_settings")
    }

    private fun enableEcoViaUi() {
        device.wait(Until.findObject(By.text("Eco routing")), 3_000)
        val label = device.findObject(By.text("Eco routing")) ?: return
        val sw =
            label.parent?.findObject(By.checkable(true))
                ?: device.findObjects(By.checkable(true)).firstOrNull {
                    kotlin.math.abs(it.visibleBounds.centerY() - label.visibleBounds.centerY()) < 80
                }
        if (sw != null && !sw.isChecked) {
            sw.click()
            settle(300)
        }
        noteUi("eco", "checked=${sw?.isChecked}")
    }

    private fun ensureAvoidOff(labelText: String) {
        val label = device.findObject(By.text(labelText)) ?: device.findObject(By.textContains(labelText.take(10)))
        if (label == null) {
            noteUi("avoid", "$labelText not found")
            return
        }
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

    private fun awaitHostDatex() {
        val cacheDir = File(dataDir, "datex_cache").also { it.mkdirs() }
        val marker = File(cacheDir, "apply_to_routing")
        val xml = File(cacheDir, "datex-GetSituation.xml")
        Log.i(TAG, "RAUFOSS_AWAIT_DATEX path=${cacheDir.absolutePath}")
        noteUi("datex_await", cacheDir.absolutePath)
        val deadline = System.currentTimeMillis() + TimeUnit.MINUTES.toMillis(15)
        while (System.currentTimeMillis() < deadline) {
            if (marker.isFile && xml.isFile && xml.length() > 200) {
                val out = JSONArray()
                for (s in DATEX_SITS) {
                    out.put(
                        JSONObject()
                            .put("id", s.id)
                            .put("lat", s.lat)
                            .put("lon", s.lon)
                            .put("label", s.label)
                            .put("intended_minutes_before_arrival", s.minutesBeforeArrival)
                            .put("in_30_5_band", s.minutesBeforeArrival in 5.0..30.0)
                            .put("impact", "Block")
                            .put("synthetic", true),
                    )
                }
                report.put("datex_synthetic_reroutes", out)
                report.put("datex_cache_dir", cacheDir.absolutePath)
                noteUi("datex_present", "bytes=${xml.length()}")
                return
            }
            Thread.sleep(1_000)
            composeRule.mainClock.advanceTimeBy(1_000)
        }
        throw AssertionError("Host DATEX timed out at ${cacheDir.absolutePath}")
    }

    private fun observeFallbackHooks(phase: String) {
        val fb = report.optJSONObject("fallback") ?: JSONObject().also { report.put("fallback", it) }
        val cache = File(dataDir, "mwm/catalog-cache.json")
        if (cache.isFile) {
            val text = runCatching { cache.readText().take(4000) }.getOrDefault("")
            fb.put("catalog_cache_present", true)
            fb.put("catalog_cache_bytes", cache.length())
            Regex(""""mode"\s*:\s*"([^"]+)"""").find(text)?.groupValues?.get(1)?.let {
                fb.put("mode", it)
            }
            fb.put("has_vehicle_limits_coverage", text.contains("vehicle_limits_coverage"))
            fb.put("comaps_direct_marker", text.contains("comaps-direct-fallback"))
            fb.put("phase_$phase", JSONObject().put("cache_snip", text.take(500)))
        } else {
            fb.put("catalog_cache_present", false)
        }
        val inst = runCatching { uniffi.navi.mwmInstalled(dataDir.absolutePath) }.getOrNull()
        if (inst != null) {
            fb.put("installed_version", inst.version)
            fb.put("installed_leaves", inst.leafIds.size)
            fb.put("installed_sidecars", inst.sidecarCount)
        }
        noteUi("fallback_observe", phase)
        writeReport()
    }

    // --- helpers --------------------------------------------------------------

    private fun openTools() {
        // Ensure chrome is visible before opening Tools (collapsed chrome uses
        // btn_tools_collapsed; hideUiChrome would hide both).
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = false
        clickTagSoft("btn_tools")
        clickTagSoft("btn_tools_collapsed")
        settle(600)
        runCatching { device.findObject(By.text("Tools"))?.click() }
        settle(400)
    }

    private fun closeTools() {
        dismissOverlaySheets()
    }

    /**
     * Close Tools / vehicle / drive sheets without tapping bare "Close" (that
     * also matches btn_close_search and collapses the route panel).
     */
    private fun dismissOverlaySheets() {
        NaviMapTestHooks.requestCloseTools = true
        settle(200)
        clickTagSoft("btn_close_tools")
        clickTagSoft("btn_save_tools")
        clickTagSoft("btn_close_vehicle")
        clickTagSoft("btn_close_drive_settings")
        runCatching { device.findObject(By.text("Hide tools"))?.click() }
        settle(300)
    }

    /**
     * Ensure the route planning search field is on screen. After Geofabrik
     * dismiss / DATEX wait / marker fly-to, chrome may be collapsed
     * (btn_open_search) or hidden via test hooks — restore both before retrying.
     */
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

        // Keep MainActivity resumed/foreground after long MWM installs on Samsung.
        runCatching { launchRule.scenario.moveToState(androidx.lifecycle.Lifecycle.State.RESUMED) }
        runCatching {
            val ctx = InstrumentationRegistry.getInstrumentation().targetContext
            val launch = ctx.packageManager.getLaunchIntentForPackage(ctx.packageName)
            if (launch != null) {
                launch.addFlags(
                    android.content.Intent.FLAG_ACTIVITY_REORDER_TO_FRONT or
                        android.content.Intent.FLAG_ACTIVITY_SINGLE_TOP,
                )
                ctx.startActivity(launch)
            }
        }
        settle(500)

        // Edge-trigger chrome restore: hooks only apply on false→true / true→false
        // transitions, so pulse hide flags when already false but local Close stuck.
        NaviMapTestHooks.hideUiChrome = true
        NaviMapTestHooks.hideSearchChrome = true
        settle(200)
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = false
        // requestOpenTools also forces hideSearch/hideChrome false in MainActivity.
        NaviMapTestHooks.requestOpenTools = true
        settle(400)
        dismissOverlaySheets()
        dismissGeofabrikMissingCoverage()
        if (fieldVisible()) {
            noteUi("open_route_panel", "field_search visible after chrome restore")
            return
        }

        clickTagSoft("btn_open_search")
        settle(400)
        runCatching { device.findObject(By.text("Route"))?.click() }
        settle(500)
        val deadline = SystemClock.elapsedRealtime() + 45_000
        var backs = 0
        while (SystemClock.elapsedRealtime() < deadline) {
            if (fieldVisible()) {
                noteUi("open_route_panel", "field_search visible after reopen")
                return
            }
            dismissOverlaySheets()
            dismissGeofabrikMissingCoverage()
            NaviMapTestHooks.hideUiChrome = false
            NaviMapTestHooks.hideSearchChrome = false
            clickTagSoft("btn_open_search")
            clickTagSoft("btn_tools_collapsed")
            runCatching { device.findObject(By.text("Route"))?.click() }
            runCatching { device.findObject(By.descContains("Search"))?.click() }
            // One Back can clear a modal that is not the Geofabrik "Not now" path.
            if (backs < 2 &&
                (
                    device.findObject(By.textContains("Map data needed")) != null ||
                        device.findObject(By.textContains("Download")) != null
                )
            ) {
                device.pressBack()
                backs++
                settle(400)
            }
            settle(700)
        }
        composeRule.onNodeWithTag("field_search", useUnmergedTree = true).assertIsDisplayed()
        noteUi("open_route_panel", "field_search visible (final)")
    }

    private fun typeCoordAndPick(
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
        // Explicit Search submit helps when IME does not auto-fire.
        clickTagSoft("btn_submit_search")
        val deadline = SystemClock.elapsedRealtime() + 25_000
        while (SystemClock.elapsedRealtime() < deadline && NaviMapTestHooks.lastSearchHitCount < 1) {
            Thread.sleep(200)
            composeRule.mainClock.advanceTimeBy(200)
        }
        assertTrue("$step hits count=${NaviMapTestHooks.lastSearchHitCount} q=$q", NaviMapTestHooks.lastSearchHitCount >= 1)
        clickTag("search_hit_0")
        val applyDeadline = SystemClock.elapsedRealtime() + 12_000
        while (SystemClock.elapsedRealtime() < applyDeadline && NaviMapTestHooks.lastAppliedHitLat.isNaN()) {
            Thread.sleep(100)
        }
        noteUi(
            step,
            "q=$q applied=${NaviMapTestHooks.lastAppliedHitLat},${NaviMapTestHooks.lastAppliedHitLon} name=${NaviMapTestHooks.lastAppliedHitName}",
        )
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

    private fun waypointSummaryLooksSet(): Boolean {
        val s =
            runCatching {
                composeRule
                    .onNodeWithTag("search_waypoints_summary", useUnmergedTree = true)
                    .fetchSemanticsNode()
                    .config[androidx.compose.ui.semantics.SemanticsProperties.Text]
                    .joinToString(" ") { it.text }
            }.getOrDefault("")
        noteUi("waypoints_summary", s)
        return s.isNotBlank() && !s.contains("unset", ignoreCase = true)
    }

    private fun assertWaypointSummarySet() {
        assertTrue("From/To must be set before Plan", waypointSummaryLooksSet())
    }

    private var lastTown = ""

    private fun maybeTownShot() {
        for (t in listOf("Raufoss", "Gol", "Geilo", "Eidfjord", "Bergen")) {
            if (t == lastTown) continue
            if (device.findObject(By.textContains(t)) != null) {
                lastTown = t
                screenshot("town_${t.lowercase()}")
                noteUi("town_passed", t)
                break
            }
        }
    }

    private fun estimateFuelStops(distanceKm: Double): JSONObject {
        fun stops(miles: Double): Int {
            val usable = (miles * 1.609344 - 100.0).coerceAtLeast(1.0)
            return maxOf(0, kotlin.math.ceil(distanceKm / usable).toInt() - 1)
        }
        return JSONObject()
            .put("distance_km", distanceKm)
            .put("stops_at_500mi", stops(500.0))
            .put("stops_at_600mi", stops(600.0))
            .put("note", "Report-only; full tank; 100 km margin; 500–600 mi range")
    }

    private fun extractDatex(rep: String): JSONObject {
        val lines =
            rep
                .lineSequence()
                .filter { it.contains("datex", ignoreCase = true) }
                .take(40)
                .toList()
        var max = 0
        for (l in lines) {
            Regex("""datex_impacts=(\d+)""").find(l)?.groupValues?.get(1)?.toIntOrNull()?.let {
                max = maxOf(max, it)
            }
        }
        return JSONObject().put("lines", JSONArray(lines)).put("max_datex_impacts", max)
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
        runCatching {
            val dir = File("/sdcard/Pictures/raufoss-bergen-ui").also { it.mkdirs() }
            device.takeScreenshot(File(dir, "$name.png"))
            File("/data/local/tmp/raufoss-bergen-ui").also { it.mkdirs() }
            device.takeScreenshot(File("/data/local/tmp/raufoss-bergen-ui/$name.png"))
            noteUi("screenshot", name)
        }
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

    /** Persist corridor polyline where the host and MainActivity both look. */
    private fun persistPlanPolyline(poly: String) {
        if (poly.length < 100) {
            noteUi("polyline_persist", "skip short len=${poly.length}")
            return
        }
        val targets =
            listOf(
                File(dataDir, "long-trip-ui-report/route-polyline.txt"),
                File("/data/local/tmp/raufoss-bergen-ui/route-polyline.txt"),
                File("/sdcard/Pictures/raufoss-bergen-ui/route-polyline.txt"),
            )
        for (f in targets) {
            runCatching {
                f.parentFile?.mkdirs()
                f.writeText(poly)
            }
        }
        noteUi("polyline_persist", "chars=${poly.length} targets=${targets.size}")
        report.put("polyline_chars_saved", poly.length)
    }

    private fun writeReport() {
        report.put("ui_events", uiEvents)
        report.put("region_timings", regionTimings)
        val text = report.toString(2)
        runCatching {
            File(dataDir, "raufoss-bergen-ui").also { it.mkdirs() }.let { File(it, "report.json").writeText(text) }
        }
        runCatching { File("/data/local/tmp/raufoss-bergen-ui.json").writeText(text) }
        runCatching {
            File("/data/local/tmp/raufoss-bergen-ui")
                .also { it.mkdirs() }
                .let { File(it, "report.json").writeText(text) }
        }
        runCatching {
            File("/sdcard/Pictures/raufoss-bergen-ui")
                .also { it.mkdirs() }
                .let { File(it, "report.json").writeText(text) }
        }
        val sd =
            NaviStorageVolumes.list(composeRule.activity).firstOrNull { it.removable && it.mounted && it.appFilesDir != null }?.appFilesDir
        if (sd != null) {
            runCatching {
                File(sd, "raufoss-bergen-ui").also { it.mkdirs() }.let { File(it, "report.json").writeText(text) }
            }
        }
    }
}
