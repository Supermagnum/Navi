package no.navi.app

import android.os.SystemClock
import android.util.Log
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextClearance
import androidx.compose.ui.test.performTextInput
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.rule.GrantPermissionRule
import org.json.JSONArray
import org.json.JSONObject
import org.junit.After
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/**
 * Task B probe: dump real UI hit labels for Bergen / country-qualified queries.
 * Writes JSON under external files; not a regression assertion suite.
 */
@RunWith(AndroidJUnit4::class)
class PlaceSearchDisambiguationProbeInstrumentedTest {
    companion object {
        private const val TAG = "PlaceDisambigProbe"
    }

    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

    @get:Rule
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
        writeReport()
    }

    @Test
    fun probe_bergen_and_country_qualified() {
        settle(800)
        openRoutePanel()

        val queries =
            listOf(
                "Bergen",
                "Bergen, Germany",
                "Bergen Germany",
                "Bergen, Norway",
            )
        for (q in queries) {
            report.put(q, probeQuery(q))
        }
        writeReport()
        Log.i(TAG, "probe done $report")
    }

    private fun probeQuery(query: String): JSONObject {
        NaviMapTestHooks.lastSearchHitCount = -1
        NaviMapTestHooks.lastSearchQuery = ""
        NaviMapTestHooks.lastSearchHitNames = emptyList()
        setField("field_search", query)
        val deadline = SystemClock.elapsedRealtime() + 45_000
        while (SystemClock.elapsedRealtime() < deadline) {
            if (NaviMapTestHooks.lastSearchHitCount >= 0 &&
                NaviMapTestHooks.lastSearchQuery.equals(query, ignoreCase = false)
            ) {
                break
            }
            composeRule.mainClock.advanceTimeBy(300)
            Thread.sleep(300)
        }
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        val online =
            runCatching {
                OnlinePlaceSearch.search(ctx, query, limit = 8, addressMode = false)
            }.getOrDefault(emptyList())
        val uiNames = NaviMapTestHooks.lastSearchHitNames
        val rows = JSONArray()
        for ((idx, name) in uiNames.withIndex()) {
            rows.put(
                JSONObject()
                    .put("idx", idx)
                    .put("ui_label", name)
                    .put(
                        "online_match",
                        online
                            .firstOrNull {
                                it.name.equals(name, true) ||
                                    placeHitDisplayLabel(it).equals(name, true) ||
                                    name.contains(it.name, true)
                            }?.let { hit ->
                                JSONObject()
                                    .put("name", hit.name)
                                    .put("label", placeHitDisplayLabel(hit))
                                    .put("lat", hit.lat)
                                    .put("lon", hit.lon)
                                    .put("municipality", hit.municipality)
                                    .put("subArea", hit.subArea)
                                    .put("regionId", hit.regionId)
                                    .put("kind", hit.kind)
                            },
                    ),
            )
        }
        val onlineArr = JSONArray()
        for (hit in online) {
            onlineArr.put(
                JSONObject()
                    .put("name", hit.name)
                    .put("label", placeHitDisplayLabel(hit))
                    .put("lat", hit.lat)
                    .put("lon", hit.lon)
                    .put("municipality", hit.municipality)
                    .put("subArea", hit.subArea)
                    .put("regionId", hit.regionId)
                    .put("kind", hit.kind),
            )
        }
        return JSONObject()
            .put("ui_hit_count", NaviMapTestHooks.lastSearchHitCount)
            .put("ui_labels", JSONArray(uiNames))
            .put("ui_rows", rows)
            .put("online_direct", onlineArr)
            .put("auto_selected", uiNames.size == 1)
    }

    private fun settle(ms: Long = 400) {
        composeRule.mainClock.advanceTimeBy(ms)
        Thread.sleep(ms)
    }

    private fun openRoutePanel() {
        runCatching {
            composeRule.onNodeWithTag("field_search", useUnmergedTree = true).assertIsDisplayed()
        }.onFailure {
            composeRule.onNodeWithTag("btn_open_search", useUnmergedTree = true).performClick()
            settle()
        }
        composeRule.onNodeWithTag("field_search", useUnmergedTree = true).assertIsDisplayed()
    }

    private fun setField(
        tag: String,
        value: String,
    ) {
        val node = composeRule.onNodeWithTag(tag, useUnmergedTree = true)
        node.performTextClearance()
        node.performTextInput(value)
        settle()
    }

    private fun writeReport() {
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        val dir = File(ctx.getExternalFilesDir(null), "place-disambig-probe").also { it.mkdirs() }
        File(dir, "bergen-probe.json").writeText(report.toString(2))
        Log.i(TAG, "wrote ${dir.absolutePath}/bergen-probe.json")
    }
}
