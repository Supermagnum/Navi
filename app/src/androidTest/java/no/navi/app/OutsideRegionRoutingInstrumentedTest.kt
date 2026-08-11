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
import org.junit.BeforeClass
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/**
 * Outside downloaded Ostlandet: keyboard Plan must show the missing-coverage
 * download prompt (not a silent/partial route). SM-P613.
 */
@RunWith(AndroidJUnit4::class)
class OutsideRegionRoutingInstrumentedTest {
    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

    private lateinit var device: UiDevice
    private lateinit var dataDir: File

    companion object {
        private const val TAG = "OutsideRegionRoute"

        private val ESPA = 60.5621914 to 11.2561239
        private val JUST_WEST = 60.4000000 to 7.4000000
        private val PREIKESTOLEN = 58.9870777 to 6.1887732
        private val TROMSO = 69.6492000 to 18.9553000

        @JvmStatic
        @BeforeClass
        fun beforeClass() {
            val ctx = InstrumentationRegistry.getInstrumentation().targetContext
            val auto = InstrumentationRegistry.getInstrumentation().uiAutomation
            auto.grantRuntimePermission(ctx.packageName, android.Manifest.permission.ACCESS_FINE_LOCATION)
            auto.grantRuntimePermission(ctx.packageName, android.Manifest.permission.ACCESS_COARSE_LOCATION)

            val dataDir = NaviAppData.resolve(ctx).also { it.mkdirs() }
            runCatching { OstlandetOfflineFixtures.ensureInstalled(dataDir) }
            val staged = File("/data/local/tmp/navi_fixtures/ostlandet-latest.osm.pbf")
            require(staged.isFile && staged.length() > 1_000_000L) {
                "push ostlandet-latest.osm.pbf to /data/local/tmp/navi_fixtures"
            }
            val dest = File(dataDir, "ostlandet-latest.osm.pbf")
            if (!dest.isFile || dest.length() < staged.length() / 2) {
                staged.copyTo(dest, overwrite = true)
            }
            Log.i(TAG, "setup pbf=${dest.length()}")

            val onlyOst = listOf("europe/norway/ostlandet")
            assertTrue(RegionCoverage.pointCovered(ESPA.first, ESPA.second, onlyOst))
            assertFalse(RegionCoverage.pointCovered(JUST_WEST.first, JUST_WEST.second, onlyOst))
            assertFalse(RegionCoverage.pointCovered(TROMSO.first, TROMSO.second, onlyOst))
            assertEquals(
                "europe/norway/vestlandet",
                RegionCoverage.suggestGeofabrikPath(JUST_WEST.first, JUST_WEST.second),
            )
            assertEquals(
                "europe/norway/nord-norge",
                RegionCoverage.suggestGeofabrikPath(TROMSO.first, TROMSO.second),
            )
            val missing =
                RegionCoverage.missingCoverage(
                    listOf(
                        RegionCoverage.Waypoint("From", "Espa", ESPA.first, ESPA.second),
                        RegionCoverage.Waypoint("To", "Tromso", TROMSO.first, TROMSO.second),
                    ),
                    dataDir,
                )
            requireNotNull(missing)
            assertTrue(
                "cross-region should suggest country or nord-norge: ${missing.suggestedGeofabrikPath}",
                missing.suggestedGeofabrikPath == "europe/norway" ||
                    missing.suggestedGeofabrikPath == "europe/norway/nord-norge",
            )
            Log.i(TAG, "coverage_ok suggest=${missing.suggestedGeofabrikPath}")
        }
    }

    @Before
    fun setUp() {
        device = UiDevice.getInstance(InstrumentationRegistry.getInstrumentation())
        dataDir = NaviAppData.resolve(InstrumentationRegistry.getInstrumentation().targetContext)
        NaviMapTestHooks.disableGpsFollow = true
        NaviMapTestHooks.followGps = false
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = false
        NaviMapTestHooks.missingCoveragePromptVisible = false
        NaviMapTestHooks.lastMissingCoveragePath = ""
        NaviMapTestHooks.lastRoutePolylineChars = 0
        NaviMapTestHooks.lastPlanReport = ""
        dismissPermission()
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

    private fun openRoutePanel() {
        runCatching {
            composeRule.onNodeWithTag("field_search", useUnmergedTree = true).assertIsDisplayed()
        }.onFailure { clickTag("btn_open_search") }
        composeRule.onNodeWithTag("field_search", useUnmergedTree = true).assertIsDisplayed()
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
        Thread.sleep(600)
        NaviMapTestHooks.disableGpsFollow = true
        NaviMapTestHooks.followGps = false
    }

    private fun selectProfile(chip: String) {
        runCatching { clickTag("btn_open_profile") }
        clickTag(chip)
        runCatching { clickTag("btn_save_profile") }
        Thread.sleep(400)
    }

    private fun planExpectMissingPrompt(timeoutMs: Long = 30_000): String {
        composeRule.onNodeWithTag("btn_plan_route", useUnmergedTree = true).performScrollTo()
        clickTag("btn_plan_route")
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.missingCoveragePromptVisible) {
                return NaviMapTestHooks.lastMissingCoveragePath
            }
            val hasBtn =
                runCatching {
                    composeRule
                        .onNodeWithTag("btn_missing_coverage_download", useUnmergedTree = true)
                        .assertExists()
                    true
                }.getOrDefault(false)
            if (hasBtn) {
                return NaviMapTestHooks.lastMissingCoveragePath.ifBlank { "dialog_visible" }
            }
            Thread.sleep(200)
        }
        error(
            "missing-coverage prompt did not appear; " +
                "poly=${NaviMapTestHooks.lastRoutePolylineChars} " +
                "report=${NaviMapTestHooks.lastPlanReport.take(200)}",
        )
    }

    private fun toastText(): String =
        runCatching {
            composeRule
                .onNodeWithTag("status_toast", useUnmergedTree = true)
                .fetchSemanticsNode()
                .config[SemanticsProperties.Text]
                .joinToString("") { it.text }
        }.getOrDefault("")

    private fun appendOut(line: String) {
        Log.i(TAG, line)
    }

    @Test
    fun keyboard_car_just_west_shows_prompt_and_dismiss_is_clean() {
        assertTrue(File(dataDir, "ostlandet-latest.osm.pbf").isFile)
        openRoutePanel()
        selectProfile("chip_profile_car")
        typeCoordAndPickHit("chip_from", ESPA.first, ESPA.second)
        typeCoordAndPickHit("chip_to", JUST_WEST.first, JUST_WEST.second)
        val path = planExpectMissingPrompt()
        appendOut("UI_CAR just_west prompt_path=$path toast=${toastText()}")
        assertTrue(
            "expected vestlandet or norway, got $path",
            path.contains("vestlandet") || path.contains("norway") || path == "dialog_visible",
        )
        assertEquals(0, NaviMapTestHooks.lastRoutePolylineChars)

        clickTag("btn_missing_coverage_dismiss")
        Thread.sleep(500)
        assertFalse(NaviMapTestHooks.missingCoveragePromptVisible)
        assertEquals(0, NaviMapTestHooks.lastRoutePolylineChars)
        assertFalse(NaviMapTestHooks.lastPlanReport.contains("PASS"))
        appendOut("UI_CAR just_west dismiss_ok toast=${toastText()}")
    }

    @Test
    fun keyboard_car_tromso_shows_prompt_with_download_action() {
        openRoutePanel()
        selectProfile("chip_profile_car")
        typeCoordAndPickHit("chip_from", ESPA.first, ESPA.second)
        typeCoordAndPickHit("chip_to", TROMSO.first, TROMSO.second)
        val path = planExpectMissingPrompt()
        appendOut("UI_CAR tromso prompt_path=$path")
        assertTrue(path.contains("norway") || path == "dialog_visible")
        composeRule
            .onNodeWithTag("btn_missing_coverage_download", useUnmergedTree = true)
            .assertIsDisplayed()
        // Do not start a full country download on CI hardware — dismiss cleanly.
        clickTag("btn_missing_coverage_dismiss")
        assertEquals(0, NaviMapTestHooks.lastRoutePolylineChars)
    }

    @Test
    fun keyboard_hiking_preikestolen_shows_prompt() {
        openRoutePanel()
        selectProfile("chip_profile_hiking")
        typeCoordAndPickHit("chip_from", ESPA.first, ESPA.second)
        typeCoordAndPickHit("chip_to", PREIKESTOLEN.first, PREIKESTOLEN.second)
        val path = planExpectMissingPrompt()
        appendOut("UI_HIKE preikestolen prompt_path=$path")
        assertTrue(path.contains("norway") || path.contains("vestlandet") || path == "dialog_visible")
        clickTag("btn_missing_coverage_dismiss")
        assertEquals(0, NaviMapTestHooks.lastRoutePolylineChars)
    }
}
