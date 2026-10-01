package no.navi.app

import android.os.Debug
import android.os.SystemClock
import android.util.Log
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
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
import uniffi.navi.CampingCallKind
import uniffi.navi.FfiCarRestSettings
import uniffi.navi.FfiFuelConfig
import uniffi.navi.FfiVehicleLimits
import uniffi.navi.TravelProfile
import uniffi.navi.campingPluginConfigure
import uniffi.navi.campingPluginInstallGuest
import uniffi.navi.campingPluginSetEnabled
import uniffi.navi.campingPluginSetNavContext
import uniffi.navi.campingPluginSetTimezone
import uniffi.navi.campingPluginSuggestAlongRoute
import uniffi.navi.ensurePoiLookaheadLoaded
import uniffi.navi.poiLookaheadQueryJson
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
 * Elsa's caravan & galleri (Bugøynes) → Sjuvasslia Camping MobileHome campaign
 * via **visible Compose UI**.
 *
 * Primary plan path: chip_from / chip_to + btn_plan_route on MainActivity.
 * Settings / region delete / downloads / plan are UI-driven. Synthetic DATEX is
 * injected by host ADB (see marker ELSA_AWAIT_DATEX). GPS start is host ADB.
 * Not wired into CI. Does not modify app or plugin production code.
 */
@RunWith(AndroidJUnit4::class)
@LargeTest
class LongTripMobileHomeElsaSjuvassliaUiCampaignTest {
    companion object {
        private const val TAG = "ElsaSjuvassliaUiCampaign"

        // Elsa's caravan & galleri, Bugøynes (elevation ~4 m).
        private const val ORIGIN_LAT = 69.9741435
        private const val ORIGIN_LON = 29.6337571
        private const val ORIGIN_ELEV_M = 4.0
        private const val DEST_LAT = 59.803175
        private const val DEST_LON = 9.397871

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

        // Synthetic DATEX Blocks on the Bugøynes→Sjuvasslia fair land corridor
        // (Pajala / Umeå class; mid-segment, off hop joints). Host ADB injects
        // these (not UniFFI). Coords are corridor candidates for timing.
        private val DATEX_SITS =
            listOf(
                Sit("syn-se-inari", "FI", 69.44041, 28.41524, "Synthetic FI chord mid Inari–Kautokeino land"),
                Sit("syn-se-pajala", "SE", 67.79923, 24.93191, "Synthetic SE/FI chord mid near Pajala"),
                Sit("syn-se-skelleftea", "SE", 66.00533, 22.56261, "Synthetic SE chord mid Skellefteå class"),
                Sit("syn-se-ornskoldsvik", "SE", 63.57672, 19.64146, "Synthetic SE chord mid Örnsköldsvik class"),
                Sit("syn-se-sveg", "SE", 62.29125, 15.13323, "Synthetic SE chord mid Sveg/Jämtland class"),
                Sit("syn-no-elverum", "NO", 60.62109, 11.26339, "Synthetic NO chord mid Elverum/Østlandet"),
            )
        private val REGIONS_TO_DELETE_CANDIDATES =
            listOf(
                "europe/denmark",
                "europe/germany/niedersachsen",
                "europe/germany/schleswig-holstein",
                "europe/sweden/halland",
                "europe/sweden/skane",
                "europe/sweden/vastra-gotaland",
                "europe/sweden/norrbotten",
                "europe/sweden/vasterbotten",
                "europe/sweden/vasternorrland",
                "europe/sweden/jamtland",
                "europe/sweden/dalarna",
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
        report.put("campaign", "elsa_sjuvasslia_mobilehome_ui")
        report.put("branch_note", "right-to-roam")
        report.put("started_unix", System.currentTimeMillis() / 1000)
        report.put("departure_iso", DEPARTURE_ISO)
        report.put("ram_before_mib", processPssMib())
        report.put(
            "origin",
            JSONObject()
                .put("lat", ORIGIN_LAT)
                .put("lon", ORIGIN_LON)
                .put("elev_m", ORIGIN_ELEV_M)
                .put("name", "Elsa's caravan & galleri, Bugøynes"),
        )
        report.put(
            "dest",
            JSONObject()
                .put("lat", DEST_LAT)
                .put("lon", DEST_LON)
                .put("name", "Sjuvasslia Camping"),
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
    fun elsa_sjuvasslia_ui_plan_campaign() {
        settle(1_200)
        noteUi("activity_foreground", "MainActivity compose rule launched")
        screenshot("01_launch")
        Log.i(TAG, "ELSA_GPS_HINT lon=$ORIGIN_LON lat=$ORIGIN_LAT elev_m=$ORIGIN_ELEV_M")

        // Assist: vehicle/rest/fuel/camping guest on disk (UI also configures). No DATEX here.
        seedAssistSettings()
        report.put("current_json_regions", fetchCurrentJsonRegions())
        writeReport()

        openRoutePanel()
        noteUi("open_route_panel", "field_search visible")
        screenshot("02_route_panel")

        confirmPluginsAndLongTripViaToolsUi()
        screenshot("03_tools_plugins")

        // Delete previously downloaded regions via Tools UI before planning.
        deleteDownloadedRegionsViaUi()
        screenshot("03b_regions_deleted")

        // Re-enable long trip after deletes (delete path turns it off when a plan was active).
        confirmPluginsAndLongTripViaToolsUi()
        openRoutePanel()
        // Hard-check long trip is ON before waypoints/plan (prior run stuck "Long trip off").
        runCatching {
            clickTagSoft("btn_tools")
            settle(500)
            composeRule.onNodeWithTag("toggle_long_trip", useUnmergedTree = true).performScrollTo()
            val line =
                runCatching {
                    composeRule
                        .onNodeWithTag("long_trip_status_line", useUnmergedTree = true)
                        .fetchSemanticsNode()
                        .config[androidx.compose.ui.semantics.SemanticsProperties.Text]
                        .joinToString(" ") { it.text }
                }.getOrDefault("")
            if (line.contains("off", ignoreCase = true) || line.isBlank()) {
                composeRule.onNodeWithTag("toggle_long_trip", useUnmergedTree = true).performClick()
                settle(400)
                noteUi("toggle_long_trip", "forced ON before plan (was: $line)")
            } else {
                noteUi("toggle_long_trip", "confirmed ON ($line)")
            }
            NaviMapTestHooks.requestCloseTools = true
            settle(300)
            clickTagSoft("btn_close_tools")
            clickTagSoft("btn_save_tools")
        }

        // Profile + vehicle + rest via visible Drive / Route UI.
        configureMobileHomeVehicleAndRestViaUi()
        screenshot("04_vehicle_rest")

        openRoutePanel()
        enableEcoRoutingViaUi()
        ensureAvoidTollsOffViaUi()

        // From / To — typed coordinates on visible search field (no forced vias).
        typeCoordAndPickHit("chip_from", ORIGIN_LAT, ORIGIN_LON, "from_elsa")
        screenshot("05_from_set")
        typeCoordAndPickHit("chip_to", DEST_LAT, DEST_LON, "to_sjuvasslia")
        screenshot("07_to_set")
        noteUi("waypoints_summary", waypointSummary())
        report.put("waypoints_summary", waypointSummary())
        writeReport()

        // Host ADB injects synthetic DATEX into datex_cache (marker below).
        awaitHostDatexInjection()
        screenshot("07b_datex_injected")

        // Re-assert MobileHome after long downloads: Compose remember can reset
        // to Car if the activity is recreated while packs fetch.
        openRoutePanel()
        runCatching {
            clickTag("chip_profile_mobile_home")
            noteUi("chip_profile_mobile_home", "re-asserted before plan")
            settle(300)
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
                .put("distance_ok", distanceKm in 1800.0..2300.0)
                .put("duration_h", etaMin / 60.0)
                .put("duration_ok", (etaMin / 60.0) in 20.0..31.0)
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
            "UI waypoints must be set (saw From/To interactions)",
            uiEvents.toString().contains("from_elsa") &&
                uiEvents.toString().contains("to_sjuvasslia"),
        )
        assertTrue(
            "Plan button must have been clicked on UI",
            uiEvents.toString().contains("click_btn_plan_route"),
        )
        assertTrue(
            "UI plan must produce a route (km=$distanceKm chars=${NaviMapTestHooks.lastRoutePolylineChars})",
            distanceKm > 100.0 && NaviMapTestHooks.lastRoutePolylineChars >= 8,
        )
        // Soft breaks / overnights come from post-chunk finalize (breakPoisJson /
        // daysJson), not per-leg poi_skipped=chunk_leg lines in the plan report.
        report.put(
            "rest_places",
            parseRestPlaces(
                NaviMapTestHooks.lastBreakPoisJson,
                NaviMapTestHooks.lastDaysJson,
            ),
        )
        report.put("break_poi_count", NaviMapTestHooks.lastBreakPoiCount)
        report.put(
            "chunked_soft_report_lines",
            JSONArray(
                planReport
                    .lineSequence()
                    .filter {
                        it.startsWith("chunked_") ||
                            it.startsWith("motor_") ||
                            it.startsWith("chunked_break_poi:")
                    }.take(80)
                    .toList(),
            ),
        )
        report.put(
            "attractions",
            sampleAttractionsAlongPolyline(NaviMapTestHooks.lastRoutePolyline),
        )
        report.put(
            "wild_camping",
            sampleWildCamping(NaviMapTestHooks.lastRoutePolyline),
        )
        report.put(
            "maneuver_kinds",
            summarizeManeuverKinds(maneuvers),
        )
        writeReport()
        Log.i(
            TAG,
            "PASS_UI dist=$distanceKm etaMin=$etaMin man=$manCount datex=${datexImpactsPositive(planReport)} " +
                "breaks=${NaviMapTestHooks.lastBreakPoiCount}",
        )
        Log.i(TAG, "ELSA_CAMPAIGN_DONE")
    }

    private fun summarizeManeuverKinds(raw: String): JSONObject {
        val counts = JSONObject()
        try {
            val arr = JSONArray(raw)
            for (i in 0 until arr.length()) {
                val k = arr.optJSONObject(i)?.optString("kind") ?: "unknown"
                counts.put(k, counts.optInt(k) + 1)
            }
        } catch (_: Throwable) {
        }
        return counts
    }

    private fun parseRestPlaces(
        breakPoisJson: String,
        daysJson: String,
    ): JSONArray {
        val out = JSONArray()
        try {
            val breaks = JSONArray(breakPoisJson.ifBlank { "[]" })
            for (i in 0 until breaks.length()) {
                val o = breaks.optJSONObject(i) ?: continue
                out.put(
                    JSONObject()
                        .put("name", o.optString("name"))
                        .put("lat", o.optDouble("lat"))
                        .put("lon", o.optDouble("lon"))
                        .put("kind", o.optString("kind"))
                        .put("along_km", o.optDouble("along_km", Double.NaN)),
                )
            }
            val days = JSONArray(daysJson.ifBlank { "[]" })
            for (i in 0 until days.length()) {
                val d = days.optJSONObject(i) ?: continue
                val name = d.optString("overnight_name")
                if (name.isNotBlank()) {
                    out.put(
                        JSONObject()
                            .put("name", name)
                            .put("lat", d.optDouble("overnight_lat", Double.NaN))
                            .put("lon", d.optDouble("overnight_lon", Double.NaN))
                            .put("kind", "overnight"),
                    )
                }
            }
        } catch (_: Throwable) {
        }
        return out
    }

    private fun sampleAttractionsAlongPolyline(polyline: String): JSONObject {
        val o = JSONObject()
        return try {
            val pbfCandidates =
                listOf(
                    File(dataDir, "region.osm.pbf"),
                    File(dataDir, "ostlandet-latest.osm.pbf"),
                    File(dataDir, "nord-norge-latest.osm.pbf"),
                )
            val pbf = pbfCandidates.firstOrNull { it.isFile }
            if (pbf != null) {
                val stats = ensurePoiLookaheadLoaded(dataDir.absolutePath, pbf.absolutePath)
                o.put("load_ok", true)
                o.put("records", stats.records.toLong())
                o.put("pbf", pbf.name)
            } else {
                o.put("load_ok", false)
                o.put("load_note", "no local PBF; using already-loaded look-ahead if any")
            }
            val samples = samplePolylinePoints(polyline, 8)
            var total = 0
            val hits = JSONArray()
            val byType = JSONObject()
            for ((lat, lon) in samples) {
                val raw = poiLookaheadQueryJson(lat, lon, 0.0, true, false)
                val arr =
                    try {
                        JSONArray(raw)
                    } catch (_: Throwable) {
                        JSONObject(raw).optJSONArray("hits") ?: JSONArray()
                    }
                for (i in 0 until arr.length()) {
                    val h = arr.optJSONObject(i) ?: continue
                    total++
                    val kind = h.optString("kind", h.optString("category", "unknown"))
                    byType.put(kind, byType.optInt(kind) + 1)
                    if (hits.length() < 40) {
                        hits.put(
                            JSONObject()
                                .put("name", h.optString("name"))
                                .put("kind", kind)
                                .put("lat", h.optDouble("lat"))
                                .put("lon", h.optDouble("lon")),
                        )
                    }
                }
            }
            o.put("sample_points", samples.size)
            o.put("hit_count", total)
            o.put("by_type", byType)
            o.put("hits_snip", hits)
            o
        } catch (t: Throwable) {
            o.put("error", t.toString())
            o
        }
    }

    private fun sampleWildCamping(polyline: String): JSONObject {
        val o = JSONObject()
        return try {
            val samples = samplePolylinePoints(polyline, 12)
            if (samples.size < 2) {
                o.put("error", "polyline too short")
                return o
            }
            val wpJson =
                campingWaypointsJson(
                    samples.map { (lat, lon) -> doubleArrayOf(lat, lon) },
                )
            val dest = samples.last()
            campingPluginSetNavContext(
                waypointsJson = wpJson,
                destLat = dest.first,
                destLon = dest.second,
                profile = TravelProfile.MOBILE_HOME,
                professionalDriver = false,
            )
            val call = campingPluginSuggestAlongRoute(16u)
            o.put("kind", call.kind.name)
            o.put("message", call.message)
            if (call.kind == CampingCallKind.OK && !call.resultJson.isNullOrBlank()) {
                val parsed = JSONObject(call.resultJson!!)
                val cards = JSONArray()
                val list = parsed.optJSONObject("list")?.optJSONArray("cards") ?: JSONArray()
                val foot = parsed.optJSONObject("onFootFromHere")?.optJSONArray("cards") ?: JSONArray()
                for (src in listOf(list, foot)) {
                    for (i in 0 until src.length()) {
                        val c = src.optJSONObject(i) ?: continue
                        cards.put(
                            JSONObject()
                                .put("title", c.optString("title", c.optString("name")))
                                .put("lat", c.optDouble("lat"))
                                .put("lon", c.optDouble("lon"))
                                .put("country", c.optString("countryIso"))
                                .put("legal", c.optString("legal")),
                        )
                    }
                }
                o.put("wild_camping_site_count", cards.length())
                o.put("sites", cards)
            } else {
                o.put("wild_camping_site_count", 0)
            }
            o
        } catch (t: Throwable) {
            o.put("error", t.toString())
            o
        }
    }

    private fun campingWaypointsJson(points: List<DoubleArray>): String {
        val arr = JSONArray()
        for (p in points) {
            arr.put(JSONArray().put(p[0]).put(p[1]))
        }
        return arr.toString()
    }

    private fun samplePolylinePoints(
        polyline: String,
        n: Int,
    ): List<Pair<Double, Double>> {
        try {
            val arr = JSONArray(polyline)
            if (arr.length() >= 2) {
                val step = maxOf(1, arr.length() / n)
                val out = ArrayList<Pair<Double, Double>>()
                var i = 0
                while (i < arr.length() && out.size < n) {
                    val pt = arr.optJSONArray(i)
                    if (pt != null && pt.length() >= 2) {
                        out.add(pt.getDouble(0) to pt.getDouble(1))
                    }
                    i += step
                }
                return out
            }
        } catch (_: Throwable) {
        }
        // Prefer sim samples from the last UI plan (stable lat/lon pairs).
        try {
            val sims = JSONArray(NaviMapTestHooks.lastSimSamplesJson.ifBlank { "[]" })
            if (sims.length() >= 2) {
                val step = maxOf(1, sims.length() / n)
                val out = ArrayList<Pair<Double, Double>>()
                var i = 0
                while (i < sims.length() && out.size < n) {
                    val o = sims.optJSONObject(i)
                    if (o != null) {
                        out.add(o.optDouble("lat") to o.optDouble("lon"))
                    }
                    i += step
                }
                if (out.size >= 2) return out
            }
        } catch (_: Throwable) {
        }
        return emptyList()
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
        Log.i(TAG, "ELSA_AWAIT_DATEX path=${cacheDir.absolutePath}")
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

    private fun ensureAvoidTollsOffViaUi() {
        // Campaign spec: Avoid toll roads = off.
        device.wait(Until.findObject(By.text("Avoid toll roads")), 3_000)
        val label =
            device.findObject(By.text("Avoid toll roads"))
                ?: device.findObject(By.textContains("Avoid toll"))
        if (label == null) {
            noteUi("avoid_tolls_switch", "label not found (default OFF)")
            return
        }
        val parent = label.parent
        var sw = parent?.findObject(By.checkable(true))
        if (sw == null) {
            val labelBounds = label.visibleBounds
            sw =
                device.findObjects(By.checkable(true)).firstOrNull { obj ->
                    kotlin.math.abs(obj.visibleBounds.centerY() - labelBounds.centerY()) < 80
                }
        }
        if (sw == null) {
            noteUi("avoid_tolls_switch", "switch not found (assume OFF)")
            return
        }
        if (sw.isChecked) {
            sw.click()
            settle(300)
            if (sw.isChecked) {
                sw.click()
                settle(300)
            }
        }
        noteUi("avoid_tolls_switch", if (!sw.isChecked) "OFF" else "still ON")
    }

    private fun deleteDownloadedRegionsViaUi() {
        val deleted = JSONArray()
        // Long-trip plan blocks delete — turn long trip off first via Tools.
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
        for (path in REGIONS_TO_DELETE_CANDIDATES) {
            val block = DownloadedRegionDelete.blockReason(path, dataDir)
            // Probe SD packs: treat "Nothing installed" as skip.
            if (block != null && block.startsWith("Nothing installed")) {
                continue
            }
            if (block != null) {
                noteUi("delete_block_precheck", "$path -> $block")
            }
            runCatching {
                setField("field_geofabrik_path", path)
                settle(300)
                composeRule
                    .onNodeWithTag("btn_delete_downloaded_region", useUnmergedTree = true)
                    .performScrollTo()
                clickTagSoft("btn_delete_downloaded_region")
                settle(400)
                clickTagSoft("btn_confirm_delete_region")
                // Large packs (Østlandet / Sweden län) can stall the UI thread briefly.
                settle(1_500)
                Thread.sleep(2_000)
                deleted.put(
                    JSONObject()
                        .put("path", path)
                        .put("pre_block", block ?: JSONObject.NULL)
                        .put("attempted", true),
                )
                noteUi("delete_region", path)
                writeReport()
            }.onFailure { e ->
                noteUi("delete_region_soft_fail", "$path ${e.message}")
                writeReport()
            }
        }
        NaviMapTestHooks.requestCloseTools = true
        settle(400)
        clickTagSoft("btn_close_tools")
        clickTagSoft("btn_save_tools")
        report.put("regions_deleted_via_ui", deleted)
        writeReport()
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
                if (id.contains("norway") || id.contains("sweden") || id.contains("finland")) {
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
            val dir = File("/sdcard/Pictures/elsa-sjuvasslia-ui").also { it.mkdirs() }
            val f = File(dir, "$name.png")
            device.takeScreenshot(f)
            noteUi("screenshot", f.absolutePath)
            // Also pull-friendly path
            runCatching {
                File("/data/local/tmp/elsa-sjuvasslia-ui").also { it.mkdirs() }
                device.takeScreenshot(File("/data/local/tmp/elsa-sjuvasslia-ui/$name.png"))
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
            File(dataDir, "long-trip-elsa-sjuvasslia").also { it.mkdirs() }.let {
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
                val dir = File(sd, "long-trip-elsa-sjuvasslia").also { it.mkdirs() }
                File(dir, "report.json").writeText(text)
            }
        }
        runCatching { File("/data/local/tmp/long-trip-elsa-sjuvasslia-ui.json").writeText(text) }
        runCatching {
            composeRule.activity.getExternalFilesDir(null)?.let {
                File(it, "long-trip-elsa-sjuvasslia-ui.json").writeText(text)
            }
        }
        Log.i(TAG, "report written (${text.length} chars)")
    }
}
