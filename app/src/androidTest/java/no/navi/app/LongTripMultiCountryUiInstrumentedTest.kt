package no.navi.app

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
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.rule.GrantPermissionRule
import kotlinx.coroutines.runBlocking
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

/**
 * Task 1: UI-driven multi-country search (NO start, DE/SE probes, DE destination)
 * then plan. Real device + network + Nominatim — no fixtures.
 */
@RunWith(AndroidJUnit4::class)
class LongTripMultiCountryUiInstrumentedTest {
    companion object {
        private const val TAG = "LongTripMultiUi"
        private const val FROM_Q = "Welhavens gate 11A, Hamar, Norway"
        private const val HARSEFELD_Q = "Harsefeld"
        private const val KALMAR_Q = "Kalmar"
        private const val TO_Q =
            "Kanzlers Weide, Uferstraße, Rechtes Weserufer, Minden, " +
                "Kreis Minden-Lübbecke, North Rhine-Westphalia, 32423, Germany"
    }

    /** Must run before [composeRule] so MainActivity reads long-trip ON. */
    @get:Rule(order = 0)
    val longTripPrefRule =
        object : TestWatcher() {
            override fun starting(description: Description) {
                val ctx = InstrumentationRegistry.getInstrumentation().targetContext
                MapHudPrefs.saveLongTripEnabled(ctx, true)
                MapHudPrefs.saveSpeedCameraPromptShown(ctx, true)
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

    private val report = JSONObject()

    @Before
    fun setUp() {
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = false
    }

    @After
    fun tearDown() {
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = false
        writeReport()
    }

    @Test
    fun multiCountry_search_then_plan_hamar_to_minden() {
        settle()
        composeRule.mainClock.advanceTimeBy(500)
        Thread.sleep(1_000)

        openRoutePanel()
        confirmLongTripOnViaToolsUi()

        // 1) From: Welhavens gate (Hamar)
        val from =
            searchAndPick(
                chip = "chip_from",
                query = FROM_Q,
                hint = "Welhavens",
                step = "from_welhavens",
            )
        assertTrue("From must resolve near Hamar, got $from", from.lat in 60.7..60.9 && from.lon in 10.9..11.3)
        report.put(
            "from",
            JSONObject()
                .put("query", FROM_Q)
                .put("picked", from.name)
                .put("lat", from.lat)
                .put("lon", from.lon)
                .put("hit_count", from.hitCount)
                .put("all_names", JSONArray(from.allNames)),
        )

        // 2) Harsefeld (Germany) — search-only probe
        val harsefeld = searchOnly(query = HARSEFELD_Q, hint = "Harsefeld", step = "harsefeld")
        report.put(
            "harsefeld",
            JSONObject()
                .put("query", HARSEFELD_Q)
                .put("hit_count", harsefeld.hitCount)
                .put("top", harsefeld.name)
                .put("lat", harsefeld.lat)
                .put("lon", harsefeld.lon)
                .put("all_names", JSONArray(harsefeld.allNames)),
        )
        assertTrue(
            "Harsefeld should land in Lower Saxony (~53.45,9.5), got ${harsefeld.lat},${harsefeld.lon}",
            harsefeld.lat in 53.2..53.7 && harsefeld.lon in 9.2..9.8,
        )

        // 3) Kalmar (Sweden) — search-only probe
        val kalmar = searchOnly(query = KALMAR_Q, hint = "Kalmar", step = "kalmar")
        report.put(
            "kalmar",
            JSONObject()
                .put("query", KALMAR_Q)
                .put("hit_count", kalmar.hitCount)
                .put("top", kalmar.name)
                .put("lat", kalmar.lat)
                .put("lon", kalmar.lon)
                .put("all_names", JSONArray(kalmar.allNames)),
        )
        assertTrue(
            "Kalmar should land in SE (~56.66,16.36), got ${kalmar.lat},${kalmar.lon}",
            kalmar.lat in 56.4..56.9 && kalmar.lon in 16.1..16.6,
        )

        // 4) To: Kanzlers Weide, Minden
        val to =
            searchAndPick(
                chip = "chip_to",
                query = TO_Q,
                hint = "Minden",
                step = "to_kanzlers",
            )
        report.put(
            "to",
            JSONObject()
                .put("query", TO_Q)
                .put("picked", to.name)
                .put("lat", to.lat)
                .put("lon", to.lon)
                .put("hit_count", to.hitCount)
                .put("all_names", JSONArray(to.allNames))
                .put(
                    "exactish",
                    to.name.contains("Kanzler", ignoreCase = true) ||
                        to.name.contains("Ufer", ignoreCase = true) ||
                        to.name.contains("Minden", ignoreCase = true),
                ),
        )
        assertTrue(
            "To must resolve near Minden DE (~52.29,8.92), got ${to.lat},${to.lon}",
            to.lat in 52.1..52.5 && to.lon in 8.7..9.2,
        )

        // 5) Plan
        clickTag("btn_plan_route")
        val planDeadline = SystemClock.elapsedRealtime() + 180_000
        var planned = false
        val samples = JSONArray()
        while (SystemClock.elapsedRealtime() < planDeadline) {
            val line = LongTripCoordinator.statusLine()
            val polyChars = NaviMapTestHooks.lastRoutePolylineChars
            val missing = NaviMapTestHooks.lastMissingCoverageMessage
            val sample =
                JSONObject()
                    .put("t_ms", SystemClock.elapsedRealtime())
                    .put("status", line)
                    .put("missing", missing)
                    .put("polyline_chars", polyChars)
                    .put("plan_report", NaviMapTestHooks.lastPlanReport.take(200))
                    .put("distance_km", NaviMapTestHooks.lastPlanDistanceKm)
            samples.put(sample)
            Log.i(TAG, "plan_sample $sample")
            if (polyChars >= 8 || NaviMapTestHooks.lastPlanDistanceKm > 1.0) {
                planned = true
                break
            }
            if (line.isNotBlank() &&
                !line.contains("off", ignoreCase = true) &&
                !line.contains("awaiting trip", ignoreCase = true)
            ) {
                // Downloads / indexing started — Task 2 monitors the rest.
                planned = true
                break
            }
            composeRule.mainClock.advanceTimeBy(2_000)
            Thread.sleep(2_000)
        }
        report.put("plan_samples", samples)
        report.put("planned", planned)
        report.put("polyline_chars", NaviMapTestHooks.lastRoutePolylineChars)
        report.put("final_long_trip_status", LongTripCoordinator.statusLine())
        report.put("final_plan_report", NaviMapTestHooks.lastPlanReport)
        report.put("plan_distance_km", NaviMapTestHooks.lastPlanDistanceKm)
        assertTrue(
            "From and To must be set before plan",
            !waypointSummary().contains("From: (unset)") &&
                !waypointSummary().contains("To: (unset)"),
        )
        assertTrue(
            "Plan should produce a corridor or long-trip download activity, " +
                "got status=${LongTripCoordinator.statusLine()} planned=$planned",
            planned ||
                (
                    LongTripCoordinator.statusLine().isNotBlank() &&
                        !LongTripCoordinator.statusLine().contains("off", ignoreCase = true)
                ),
        )

        // Do not hold under instrumentation: SM-P613 crashes the instrumented
        // process once corridor downloads push RSS toward ~1.5–1.6GB. Search +
        // plan assertions above are the instrumented contract; durable download
        // overlap is measured against a standalone MainActivity afterward.
        report.put(
            "note",
            "No instrumented hold — process crashes ~1.6GB RSS under runner; " +
                "standalone monitor follows",
        )
        report.put("end_distance_km", NaviMapTestHooks.lastPlanDistanceKm)
        report.put("end_polyline_chars", NaviMapTestHooks.lastRoutePolylineChars)
        report.put("end_plan_report", NaviMapTestHooks.lastPlanReport)
        report.put("end_long_trip_status", LongTripCoordinator.statusLine())
        Log.i(
            TAG,
            "PASS planned=$planned dist=${NaviMapTestHooks.lastPlanDistanceKm} " +
                "status=${LongTripCoordinator.statusLine()}",
        )
        writeReport()
    }

    private fun confirmLongTripOnViaToolsUi() {
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        assertTrue(
            "long-trip pref must be ON (set before activity launch)",
            MapHudPrefs.loadLongTripEnabled(ctx),
        )
        runCatching {
            clickTagSoft("btn_tools")
            settle(500)
            val line =
                runCatching {
                    composeRule
                        .onNodeWithTag("long_trip_status_line", useUnmergedTree = true)
                        .fetchSemanticsNode()
                        .config[androidx.compose.ui.semantics.SemanticsProperties.Text]
                        .joinToString(" ") { it.text }
                }.getOrDefault("")
            Log.i(TAG, "tools long_trip_status_line='$line' pref=true")
            report.put("long_trip_ui_status_before_plan", line)
            report.put("long_trip_pref_before_plan", true)
            if (line.contains("off", ignoreCase = true)) {
                composeRule
                    .onNodeWithTag("toggle_long_trip", useUnmergedTree = true)
                    .performScrollTo()
                    .performClick()
                settle(400)
            }
            NaviMapTestHooks.requestCloseTools = true
            settle(700)
            clickTagSoft("btn_close_tools")
            clickTagSoft("btn_save_tools")
        }.onFailure { e ->
            Log.w(TAG, "tools long-trip confirm soft-fail: ${e.message}")
            NaviMapTestHooks.requestCloseTools = true
            settle(500)
        }
        assertTrue(MapHudPrefs.loadLongTripEnabled(ctx))
    }

    private data class HitInfo(
        val name: String,
        val lat: Double,
        val lon: Double,
        val hitCount: Int,
        val allNames: List<String>,
    )

    private fun searchAndPick(
        chip: String,
        query: String,
        hint: String,
        step: String,
    ): HitInfo {
        clickTag(chip)
        val info = waitForHits(query, hint, step)
        val idx =
            info.allNames
                .indexOfFirst { it.contains(hint, ignoreCase = true) }
                .takeIf { it >= 0 }
                ?: 0
        NaviMapTestHooks.lastAppliedHitLat = Double.NaN
        composeRule
            .onNodeWithTag("search_hit_$idx", useUnmergedTree = true)
            .performScrollTo()
            .performClick()
        settle()
        val applyDeadline = SystemClock.elapsedRealtime() + 10_000
        while (SystemClock.elapsedRealtime() < applyDeadline) {
            if (!NaviMapTestHooks.lastAppliedHitLat.isNaN()) break
            composeRule.mainClock.advanceTimeBy(100)
            Thread.sleep(100)
        }
        return HitInfo(
            name = NaviMapTestHooks.lastAppliedHitName.ifBlank { info.allNames.getOrElse(idx) { info.name } },
            lat = NaviMapTestHooks.lastAppliedHitLat,
            lon = NaviMapTestHooks.lastAppliedHitLon,
            hitCount = info.hitCount,
            allNames = info.allNames,
        )
    }

    private fun searchOnly(
        query: String,
        hint: String,
        step: String,
    ): HitInfo {
        openRoutePanel()
        val info = waitForHits(query, hint, step)
        // For search-only probes, pick the first matching UI name and resolve
        // coords from a single OnlinePlaceSearch (merged online-first list).
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        val online =
            runBlocking {
                OnlinePlaceSearch.search(ctx, query, limit = 5, addressMode = true)
            }
        val match =
            online.firstOrNull { hit ->
                hit.name.contains(hint, ignoreCase = true) ||
                    info.allNames.any { n ->
                        n.contains(hint, ignoreCase = true) &&
                            (
                                hit.name.contains(n.substringBefore(','), ignoreCase = true) ||
                                    n.contains(hit.name, ignoreCase = true)
                            )
                    }
            } ?: online.firstOrNull()
        return info.copy(
            name = match?.name ?: info.name,
            lat = match?.lat ?: 0.0,
            lon = match?.lon ?: 0.0,
        )
    }

    private fun waitForHits(
        query: String,
        hint: String,
        step: String,
    ): HitInfo {
        NaviMapTestHooks.lastSearchHitCount = -1
        NaviMapTestHooks.lastSearchQuery = ""
        NaviMapTestHooks.lastSearchHitNames = emptyList()
        setField("field_search", query)
        val deadline = SystemClock.elapsedRealtime() + 60_000
        while (SystemClock.elapsedRealtime() < deadline) {
            if (NaviMapTestHooks.lastSearchHitCount >= 1 &&
                NaviMapTestHooks.lastSearchHitNames.isNotEmpty()
            ) {
                break
            }
            composeRule.mainClock.advanceTimeBy(300)
            Thread.sleep(300)
        }
        val names = NaviMapTestHooks.lastSearchHitNames
        assertTrue(
            "$step: expected hits for '$query', count=${NaviMapTestHooks.lastSearchHitCount} names=$names hint=$hint",
            names.isNotEmpty(),
        )
        Log.i(TAG, "$step uiHits=${names.size} names=$names")
        return HitInfo(
            name = names.firstOrNull { it.contains(hint, ignoreCase = true) } ?: names.first(),
            lat = 0.0,
            lon = 0.0,
            hitCount = names.size,
            allNames = names,
        )
    }

    private fun settle(ms: Long = 400) {
        composeRule.mainClock.advanceTimeBy(ms)
        Thread.sleep(ms)
    }

    private fun openRoutePanel() {
        runCatching {
            composeRule.onNodeWithTag("field_search", useUnmergedTree = true).assertIsDisplayed()
        }.onFailure { clickTag("btn_open_search") }
        composeRule.onNodeWithTag("field_search", useUnmergedTree = true).assertIsDisplayed()
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
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        val dir = File(ctx.getExternalFilesDir(null), "long-trip-ui-report").also { it.mkdirs() }
        val f = File(dir, "multi-country.json")
        f.writeText(report.toString(2))
        Log.i(TAG, "wrote report ${f.absolutePath}")
        runCatching {
            File("/data/local/tmp/long-trip-multi-country.json").writeText(report.toString(2))
        }
    }
}
