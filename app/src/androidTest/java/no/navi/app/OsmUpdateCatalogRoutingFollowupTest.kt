package no.navi.app

import android.util.Log
import androidx.compose.ui.semantics.SemanticsProperties
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
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.FixMethodOrder
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.runners.MethodSorters
import java.net.HttpURLConnection
import java.net.URL

/**
 * Real-device follow-up: OSM update copy, keyboard cross-region prompts,
 * and catalog granularity for Sweden / US / Russia / Germany.
 */
@RunWith(AndroidJUnit4::class)
@FixMethodOrder(MethodSorters.NAME_ASCENDING)
class OsmUpdateCatalogRoutingFollowupTest {
    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

    private lateinit var device: UiDevice

    companion object {
        private const val TAG = "OsmCatalogFollowup"
        private val TECHNICAL =
            listOf(
                "user_visible",
                "local_sequence",
                "remote_sequence",
                "method=",
                "reason=",
                "region_meta",
                "osc.gz",
                "full_redownload",
                "days_behind",
            )
    }

    @Before
    fun setUp() {
        device = UiDevice.getInstance(InstrumentationRegistry.getInstrumentation())
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        val auto = InstrumentationRegistry.getInstrumentation().uiAutomation
        auto.grantRuntimePermission(ctx.packageName, android.Manifest.permission.ACCESS_FINE_LOCATION)
        auto.grantRuntimePermission(ctx.packageName, android.Manifest.permission.ACCESS_COARSE_LOCATION)
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = false
        NaviMapTestHooks.missingCoveragePromptVisible = false
        NaviMapTestHooks.lastMissingCoveragePath = ""
        NaviMapTestHooks.lastMissingCoverageMessage = ""
        NaviMapTestHooks.lastRoutePolylineChars = 0
        NaviMapTestHooks.lastPlanReport = ""
        dismissPermission()
    }

    @Test
    fun c_osm_check_and_apply_show_plain_language() {
        openTools()
        clickTag("btn_check_osm_updates")
        val checkMsg = waitToolsStatus(15_000) { it.isNotBlank() && it != "Ready" }
        Log.i(TAG, "OSM_CHECK msg='$checkMsg'")
        assertClean(checkMsg)
        assertTrue(
            "check message should be a known plain string, got: $checkMsg",
            checkMsg == OsmUpdateUserCopy.UP_TO_DATE ||
                checkMsg == OsmUpdateUserCopy.AVAILABLE ||
                checkMsg == OsmUpdateUserCopy.NO_REGION ||
                checkMsg == OsmUpdateUserCopy.FAILED,
        )
        val sawUpToDate = checkMsg == OsmUpdateUserCopy.UP_TO_DATE
        val sawAvailable = checkMsg == OsmUpdateUserCopy.AVAILABLE
        if (sawAvailable) {
            clickTag("btn_apply_osm_update")
            val applying =
                waitToolsStatus(12_000) {
                    it == OsmUpdateUserCopy.APPLYING ||
                        it == OsmUpdateUserCopy.UPDATED ||
                        it == OsmUpdateUserCopy.UPDATED_INDEXING
                }
            Log.i(TAG, "OSM_APPLY msg='$applying'")
            assertClean(applying)
            assertTrue(
                applying == OsmUpdateUserCopy.APPLYING ||
                    applying == OsmUpdateUserCopy.UPDATED ||
                    applying == OsmUpdateUserCopy.UPDATED_INDEXING,
            )
            // Full Ostlandet redownload can take many minutes; the applying copy is
            // the user-visible requirement. Close Tools so the download continues
            // without blocking the later route tests.
        }
        runCatching { clickTag("btn_close_tools") }
        Log.i(TAG, "OSM_SUMMARY upToDate=$sawUpToDate available=$sawAvailable")
        assertTrue(
            "expected at least one of up-to-date or update-available via the toggle",
            sawUpToDate || sawAvailable || checkMsg == OsmUpdateUserCopy.NO_REGION,
        )
    }

    @Test
    fun a_keyboard_four_routes_missing_coverage() {
        openRoutePanel()
        selectProfile("chip_profile_car")
        val rows =
            listOf(
                RouteCase(
                    "Grotli",
                    61.9803,
                    8.2775,
                    "Hjelle",
                    61.9160,
                    7.1640,
                    expectPrompt = true,
                    expectedPath = "europe/norway/vestlandet",
                ),
                RouteCase(
                    "Os",
                    62.4964,
                    11.1436,
                    "Roros",
                    62.5747,
                    11.3840,
                    expectPrompt = true,
                    expectedPath = "europe/norway",
                ),
                RouteCase(
                    "Fagernes",
                    60.9858,
                    9.2322,
                    "Gol",
                    60.7011,
                    8.9564,
                    expectPrompt = false,
                ),
                RouteCase(
                    "Strandlykkja",
                    60.5175,
                    11.2670,
                    "Morskogen",
                    60.5080,
                    11.2200,
                    expectPrompt = false,
                ),
            )
        for (row in rows) {
            planRouteAndAssert(row)
        }
    }

    @Test
    fun e_fagernes_gol_crosses_fylke_stays_in_ostlandet() {
        openRoutePanel()
        selectProfile("chip_profile_car")
        planRouteAndAssert(
            RouteCase(
                "Fagernes",
                60.9858,
                9.2322,
                "Gol",
                60.7011,
                8.9564,
                expectPrompt = false,
            ),
        )
    }

    @Test
    fun f_strandlykkja_morskogen_near_border_stays_in_ostlandet() {
        openRoutePanel()
        selectProfile("chip_profile_car")
        planRouteAndAssert(
            RouteCase(
                "Strandlykkja",
                60.5175,
                11.2670,
                "Morskogen",
                60.5080,
                11.2200,
                expectPrompt = false,
            ),
        )
    }

    @Test
    fun d_sweden_border_keyboard_prompt() {
        openRoutePanel()
        selectProfile("chip_profile_car")
        NaviMapTestHooks.missingCoveragePromptVisible = false
        NaviMapTestHooks.lastMissingCoveragePath = ""
        NaviMapTestHooks.lastMissingCoverageMessage = ""
        NaviMapTestHooks.lastRoutePolylineChars = 0
        NaviMapTestHooks.lastPlanReport = ""
        typeCoordAndPickHit("chip_from", 61.8956, 12.2208)
        typeCoordAndPickHit("chip_to", 61.8975, 12.2685)
        clickTag("btn_plan_route")
        val deadline = System.currentTimeMillis() + 15_000
        var prompted = false
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.missingCoveragePromptVisible) {
                prompted = true
                break
            }
            if (NaviMapTestHooks.lastRoutePolylineChars > 0 ||
                NaviMapTestHooks.lastPlanReport.contains("PASS")
            ) {
                break
            }
            Thread.sleep(200)
        }
        val path = NaviMapTestHooks.lastMissingCoveragePath
        val msg = NaviMapTestHooks.lastMissingCoverageMessage
        Log.i(TAG, "SWEDEN prompted=$prompted path=$path msg='$msg' poly=${NaviMapTestHooks.lastRoutePolylineChars}")
        assertTrue("expected Sweden missing-coverage prompt, path=$path msg=$msg", prompted)
        assertEquals("europe/sweden", path)
        assertTrue(msg.contains("Sweden"))
        composeRule
            .onNodeWithTag("btn_missing_coverage_download", useUnmergedTree = true)
            .assertIsDisplayed()
        clickTag("btn_missing_coverage_dismiss")
        assertEquals(0, NaviMapTestHooks.lastRoutePolylineChars)
    }

    @Test
    fun b_catalog_granularity_sweden_us_russia_germany() {
        openTools()
        clickTag("chip_download_country")
        clickTag("chip_continent_europe")
        clickTag("chip_country_europe_sweden")
        clickTag("chip_download_region")
        val swedenNote = taggedText("region_chips_norway_only_note")
        Log.i(TAG, "CATALOG sweden_note='$swedenNote'")
        assertTrue(swedenNote.contains("län") || swedenNote.contains("lan"))
        assertTrue(swedenNote.contains("country extract", ignoreCase = true))
        val kronoberg = head("europe/sweden/kronobergs-lan")
        val swedenCountry = head("europe/sweden")
        Log.i(TAG, "HEAD sweden/kronobergs-lan=$kronoberg sweden=$swedenCountry")
        assertTrue("sweden country should exist: $swedenCountry", swedenCountry.first in 200..399)
        assertTrue(
            "Kronobergs län must not be a Geofabrik extract: $kronoberg",
            kronoberg.first >= 400 || kronoberg.second < 1_000_000L,
        )

        clickTag("chip_download_country")
        clickTag("chip_continent_north_america")
        clickTag("chip_country_north_america_us")
        clickTag("chip_download_region")
        val usNote = taggedText("region_chips_norway_only_note")
        Log.i(TAG, "CATALOG us_note='$usNote'")
        assertTrue(usNote.contains("west-virginia"))
        val wv = head("north-america/us/west-virginia")
        Log.i(TAG, "HEAD west-virginia=$wv")
        assertTrue("West Virginia extract missing: $wv", wv.first in 200..399 && wv.second > 1_000_000L)

        clickTag("chip_download_country")
        clickTag("chip_continent_europe")
        clickTag("chip_country_russia")
        clickTag("chip_download_region")
        val ruNote = taggedText("region_chips_norway_only_note")
        Log.i(TAG, "CATALOG russia_note='$ruNote'")
        assertTrue(ruNote.contains("kaliningrad"))
        val russia = head("russia")
        val russiaDistrict = head("russia/kaliningrad")
        val russiaWrongSlug = head("russia/central-federal-district")
        Log.i(TAG, "HEAD russia=$russia kaliningrad=$russiaDistrict wrong=$russiaWrongSlug")
        assertTrue("russia country extract missing: $russia", russia.first in 200..399 && russia.second > 1_000_000L)
        assertTrue(
            "kaliningrad district extract missing: $russiaDistrict",
            russiaDistrict.first in 200..399 && russiaDistrict.second > 1_000_000L,
        )
        assertTrue(
            "bogus federal-district slug must not be a real PBF: $russiaWrongSlug",
            russiaWrongSlug.second < 1_000_000L,
        )

        clickTag("chip_download_country")
        clickTag("chip_country_europe_germany")
        clickTag("chip_download_region")
        val deNote = taggedText("region_chips_norway_only_note")
        Log.i(TAG, "CATALOG germany_note='$deNote'")
        assertTrue(deNote.contains("bremen"))
        val bremen = head("europe/germany/bremen")
        Log.i(TAG, "HEAD bremen=$bremen")
        assertTrue("Bremen extract missing: $bremen", bremen.first in 200..399 && bremen.second > 1_000_000L)

        startTypedDownload("europe/germany/bremen", "BREMEN_DL")
        startTypedDownload("russia/kaliningrad", "KALININGRAD_DL")
        startTypedDownload("north-america/us/west-virginia", "WV_DL")
    }

    private data class RouteCase(
        val fromName: String,
        val fromLat: Double,
        val fromLon: Double,
        val toName: String,
        val toLat: Double,
        val toLon: Double,
        val expectPrompt: Boolean,
        val expectedPath: String? = null,
    )

    private fun planRouteAndAssert(row: RouteCase) {
        NaviMapTestHooks.missingCoveragePromptVisible = false
        NaviMapTestHooks.lastMissingCoveragePath = ""
        NaviMapTestHooks.lastMissingCoverageMessage = ""
        NaviMapTestHooks.lastRoutePolylineChars = 0
        NaviMapTestHooks.lastPlanReport = ""
        typeCoordAndPickHit("chip_from", row.fromLat, row.fromLon)
        typeCoordAndPickHit("chip_to", row.toLat, row.toLon)
        clickTag("btn_plan_route")
        val timeoutMs = if (row.expectPrompt) 25_000L else 12_000L
        val deadline = System.currentTimeMillis() + timeoutMs
        var prompted = false
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.missingCoveragePromptVisible) {
                prompted = true
                break
            }
            if (!row.expectPrompt &&
                (
                    NaviMapTestHooks.lastRoutePolylineChars > 0 ||
                        NaviMapTestHooks.lastPlanReport.contains("PASS") ||
                        toastText().contains("indexing", ignoreCase = true) ||
                        toastText().contains("Planning", ignoreCase = true)
                )
            ) {
                break
            }
            if (row.expectPrompt &&
                (
                    NaviMapTestHooks.lastRoutePolylineChars > 0 ||
                        NaviMapTestHooks.lastPlanReport.contains("PASS")
                )
            ) {
                break
            }
            Thread.sleep(200)
        }
        val path = NaviMapTestHooks.lastMissingCoveragePath
        val msg = NaviMapTestHooks.lastMissingCoverageMessage
        val toast = toastText()
        Log.i(
            TAG,
            "ROUTE ${row.fromName} -> ${row.toName} expectPrompt=${row.expectPrompt} " +
                "prompted=$prompted path=$path msg='$msg' toast='$toast' " +
                "poly=${NaviMapTestHooks.lastRoutePolylineChars}",
        )
        assertEquals(
            "${row.fromName} -> ${row.toName} prompt expectation",
            row.expectPrompt,
            prompted,
        )
        if (row.expectPrompt) {
            assertClean(msg)
            assertClean(toast)
            composeRule
                .onNodeWithTag("btn_missing_coverage_download", useUnmergedTree = true)
                .assertIsDisplayed()
            row.expectedPath?.let { assertEquals(it, path) }
            clickTag("btn_missing_coverage_dismiss")
            Thread.sleep(400)
            assertFalse(NaviMapTestHooks.missingCoveragePromptVisible)
            assertEquals(0, NaviMapTestHooks.lastRoutePolylineChars)
        } else {
            assertFalse(
                "${row.fromName} -> ${row.toName} should not block on missing coverage",
                NaviMapTestHooks.missingCoveragePromptVisible,
            )
            assertTrue(
                "${row.fromName} -> ${row.toName} should start planning without a prompt",
                toast.contains("Planning", ignoreCase = true) ||
                    toast.contains("indexing", ignoreCase = true) ||
                    NaviMapTestHooks.lastPlanReport.isNotBlank() ||
                    NaviMapTestHooks.lastRoutePolylineChars > 0,
            )
            runCatching { clickTag("btn_delete_planned_route") }
        }
    }

    private fun assertClean(msg: String) {
        val lower = msg.lowercase()
        for (token in TECHNICAL) {
            assertFalse("leaked '$token' in: $msg", lower.contains(token))
        }
    }

    private fun startTypedDownload(
        path: String,
        logTag: String,
    ) {
        setField("field_geofabrik_path", path)
        composeRule
            .onNodeWithTag("btn_download_region", useUnmergedTree = true)
            .performScrollTo()
            .performClick()
        val progressDeadline = System.currentTimeMillis() + 25_000
        var started = false
        var last = ""
        while (System.currentTimeMillis() < progressDeadline) {
            val toast = toastText()
            val tools = taggedText("tools_status")
            last = "toast='$toast' tools='$tools'"
            Log.i(TAG, "$logTag $last")
            if (toast.contains("Download", ignoreCase = true) ||
                tools.contains("Download", ignoreCase = true) ||
                tools.contains("PASS") ||
                toast.contains("Downloading") ||
                tools.contains("progress", ignoreCase = true)
            ) {
                started = true
                break
            }
            Thread.sleep(500)
        }
        Log.i(TAG, "$logTag started=$started last=$last")
        assertTrue("$path download should start ($last)", started)
        // Let this extract finish or at least leave it running before the next tap.
        Thread.sleep(2_000)
    }

    private fun head(path: String): Pair<Int, Long> {
        val url = "https://download.geofabrik.de/$path-latest.osm.pbf"
        val conn = URL(url).openConnection() as HttpURLConnection
        conn.requestMethod = "HEAD"
        conn.instanceFollowRedirects = true
        conn.connectTimeout = 20_000
        conn.readTimeout = 20_000
        return try {
            val code = conn.responseCode
            val len = conn.getHeaderField("Content-Length")?.toLongOrNull() ?: 0L
            code to len
        } finally {
            conn.disconnect()
        }
    }

    private fun openTools() {
        runCatching { clickTag("btn_tools") }
            .onFailure { runCatching { clickTag("btn_tools_collapsed") } }
        composeRule.onNodeWithTag("tools_menu", useUnmergedTree = true).assertIsDisplayed()
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
        Thread.sleep(500)
        NaviMapTestHooks.disableGpsFollow = true
        NaviMapTestHooks.followGps = false
    }

    private fun selectProfile(chip: String) {
        runCatching { clickTag("btn_open_profile") }
        clickTag(chip)
        runCatching { clickTag("btn_save_profile") }
        Thread.sleep(300)
    }

    private fun taggedText(tag: String): String =
        runCatching {
            composeRule
                .onNodeWithTag(tag, useUnmergedTree = true)
                .fetchSemanticsNode()
                .config[SemanticsProperties.Text]
                .joinToString("") { it.text }
        }.getOrDefault("")

    private fun toastText(): String = taggedText("status_toast")

    private fun waitToolsStatus(
        timeoutMs: Long,
        predicate: (String) -> Boolean,
    ): String {
        val deadline = System.currentTimeMillis() + timeoutMs
        var last = ""
        while (System.currentTimeMillis() < deadline) {
            last = taggedText("tools_status")
            if (last.isNotBlank() && predicate(last)) return last
            Thread.sleep(250)
        }
        return last
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
}
