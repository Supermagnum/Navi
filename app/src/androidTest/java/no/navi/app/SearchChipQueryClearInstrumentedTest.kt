package no.navi.app

import android.util.Log
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.rule.GrantPermissionRule
import org.junit.After
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Shared search `query` must clear when switching From/To/Via chips so a
 * resolved GPS label from one chip does not linger on another.
 */
@RunWith(AndroidJUnit4::class)
class SearchChipQueryClearInstrumentedTest {
    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

    @get:Rule
    val permissionRule: GrantPermissionRule =
        GrantPermissionRule.grant(
            android.Manifest.permission.ACCESS_FINE_LOCATION,
            android.Manifest.permission.ACCESS_COARSE_LOCATION,
        )

    @Before
    fun setUp() {
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = false
        NaviMapTestHooks.ignoreLiveGpsFixes = true
        NaviMapTestHooks.pendingInjectFixLatLon = 60.79448 to 11.06799
    }

    @After
    fun tearDown() {
        NaviMapTestHooks.ignoreLiveGpsFixes = false
        NaviMapTestHooks.pendingInjectFixLatLon = null
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = false
    }

    @Test
    fun useGpsAsFrom_thenSwitchToTo_clearsSearchBox() {
        // Wait for injected fix to land in mapState.
        val fixDeadline = System.currentTimeMillis() + 20_000
        while (System.currentTimeMillis() < fixDeadline) {
            if (!NaviMapTestHooks.lastGpsLat.isNaN() &&
                kotlin.math.abs(NaviMapTestHooks.lastGpsLat - 60.79448) < 0.001
            ) {
                break
            }
            NaviMapTestHooks.pendingInjectFixLatLon = 60.79448 to 11.06799
            Thread.sleep(300)
        }
        assertTrue(
            "injected GPS fix did not land (${NaviMapTestHooks.lastGpsLat},${NaviMapTestHooks.lastGpsLon})",
            !NaviMapTestHooks.lastGpsLat.isNaN(),
        )

        composeRule.waitForIdle()
        composeRule.onNodeWithTag("chip_from", useUnmergedTree = true).performClick()
        composeRule.waitForIdle()
        composeRule.onNodeWithTag("btn_use_gps", useUnmergedTree = true).performClick()

        val fromDeadline = System.currentTimeMillis() + 30_000
        var summary = ""
        while (System.currentTimeMillis() < fromDeadline) {
            summary = waypointSummaryText()
            if (summary.contains("From:") && !summary.contains("From: (unset)")) break
            Thread.sleep(300)
        }
        assertTrue("From not set after Use GPS: $summary", !summary.contains("From: (unset)"))
        // Query field held the resolved label before the chip switch.
        val queryBefore = fieldSearchText()
        assertTrue("expected non-empty query after Use GPS, got '$queryBefore'", queryBefore.isNotBlank())

        composeRule.onNodeWithTag("chip_to", useUnmergedTree = true).performClick()
        composeRule.waitForIdle()
        Thread.sleep(500)

        val queryAfter = fieldSearchText()
        assertTrue(
            "search box should be empty after chip switch, got '$queryAfter'",
            queryAfter.isBlank(),
        )
        summary = waypointSummaryText()
        assertTrue(
            "From should remain set after switching to To: $summary",
            !summary.contains("From: (unset)"),
        )
        Log.i(TAG, "PASS queryBefore='$queryBefore' queryAfter='$queryAfter' summary=$summary")
    }

    @Test
    fun useGpsAsTo_thenSwitchToFrom_clearsSearchBox() {
        val fixDeadline = System.currentTimeMillis() + 20_000
        while (System.currentTimeMillis() < fixDeadline) {
            if (!NaviMapTestHooks.lastGpsLat.isNaN()) break
            NaviMapTestHooks.pendingInjectFixLatLon = 60.79448 to 11.06799
            Thread.sleep(300)
        }

        composeRule.waitForIdle()
        composeRule.onNodeWithTag("chip_to", useUnmergedTree = true).performClick()
        composeRule.waitForIdle()
        composeRule.onNodeWithTag("btn_use_gps", useUnmergedTree = true).performClick()

        val toDeadline = System.currentTimeMillis() + 30_000
        var summary = ""
        while (System.currentTimeMillis() < toDeadline) {
            summary = waypointSummaryText()
            if (summary.contains("To:") && !summary.contains("To: (unset)")) break
            Thread.sleep(300)
        }
        assertTrue("To not set after Use GPS: $summary", !summary.contains("To: (unset)"))

        composeRule.onNodeWithTag("chip_from", useUnmergedTree = true).performClick()
        composeRule.waitForIdle()
        Thread.sleep(500)

        val queryAfter = fieldSearchText()
        assertTrue(
            "search box should be empty after chip switch, got '$queryAfter'",
            queryAfter.isBlank(),
        )
        summary = waypointSummaryText()
        assertTrue(
            "To should remain set after switching to From: $summary",
            !summary.contains("To: (unset)"),
        )
        Log.i(TAG, "PASS reverse queryAfter='$queryAfter' summary=$summary")
    }

    private fun waypointSummaryText(): String {
        val node =
            composeRule
                .onNodeWithTag("search_waypoints_summary", useUnmergedTree = true)
                .fetchSemanticsNode()
        return node.config[SemanticsProperties.Text].joinToString(" ") { it.text }
    }

    private fun fieldSearchText(): String {
        val node =
            composeRule
                .onNodeWithTag("field_search", useUnmergedTree = true)
                .fetchSemanticsNode()
        val editable =
            node.config.getOrElse(SemanticsProperties.EditableText) {
                androidx.compose.ui.text
                    .AnnotatedString("")
            }
        if (editable.text.isNotEmpty()) {
            return editable.text
        }
        val texts =
            node.config.getOrElse(SemanticsProperties.Text) {
                emptyList()
            }
        return texts.joinToString(" ") { it.text }
    }

    companion object {
        private const val TAG = "SearchChipQueryClear"
    }
}
