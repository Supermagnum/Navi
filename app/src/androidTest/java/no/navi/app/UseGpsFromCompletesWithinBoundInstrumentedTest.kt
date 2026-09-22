package no.navi.app

import android.os.SystemClock
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
import uniffi.navi.PlaceHit

/**
 * Regression for SM-P613 Use-GPS hang: tapping [btn_use_gps] must set From
 * immediately (coords) and must not ANR the UI. Address upgrade is allowed to
 * finish asynchronously within a looser bound when reverse geocode is stubbed.
 *
 * Bound: From set within [FROM_SET_BOUND_MS] of the click (immediate applyHit).
 * Address upgrade within [ADDRESS_UPGRADE_BOUND_MS] with a stubbed reverse hit.
 */
@RunWith(AndroidJUnit4::class)
class UseGpsFromCompletesWithinBoundInstrumentedTest {
    companion object {
        private const val TAG = "UseGpsFromBound"
        const val FROM_SET_BOUND_MS = 5_000L
        const val ADDRESS_UPGRADE_BOUND_MS = 15_000L
        private const val FIX_LAT = 60.79448
        private const val FIX_LON = 11.06799
    }

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
        NaviMapTestHooks.lastGpsLat = Double.NaN
        NaviMapTestHooks.lastGpsLon = Double.NaN
        NaviMapTestHooks.ignoreLiveGpsFixes = true
        NaviMapTestHooks.pendingInjectFixLatLon = FIX_LAT to FIX_LON
        OnlinePlaceSearch.reverseOverrideForTests = { lat, lon ->
            PlaceHit(
                osmId = 1L,
                name = "Welhavens gate 11A",
                kind = "online/place/house",
                lat = lat,
                lon = lon,
                subArea = "Sentrum",
                municipality = "Hamar",
                regionId = "europe/norway/ostlandet",
            )
        }
    }

    @After
    fun tearDown() {
        OnlinePlaceSearch.reverseOverrideForTests = null
        NaviMapTestHooks.ignoreLiveGpsFixes = false
        NaviMapTestHooks.pendingInjectFixLatLon = null
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = false
    }

    @Test
    fun useGpsAsFrom_setsFromWithinFiveSeconds_andUpgradesAddress() {
        composeRule.waitForIdle()
        val fixDeadline = SystemClock.elapsedRealtime() + 20_000
        var pendingSeenNull = false
        while (SystemClock.elapsedRealtime() < fixDeadline) {
            if (NaviMapTestHooks.pendingInjectFixLatLon == null) {
                pendingSeenNull = true
            }
            if (!NaviMapTestHooks.lastGpsLat.isNaN() &&
                kotlin.math.abs(NaviMapTestHooks.lastGpsLat - FIX_LAT) < 0.001
            ) {
                break
            }
            NaviMapTestHooks.pendingInjectFixLatLon = FIX_LAT to FIX_LON
            composeRule.mainClock.advanceTimeBy(250)
            Thread.sleep(250)
        }
        assertTrue(
            "injected GPS fix did not land (${NaviMapTestHooks.lastGpsLat},${NaviMapTestHooks.lastGpsLon}) " +
                "pendingConsumed=$pendingSeenNull pendingNow=${NaviMapTestHooks.pendingInjectFixLatLon}",
            !NaviMapTestHooks.lastGpsLat.isNaN(),
        )

        composeRule.waitForIdle()
        composeRule.onNodeWithTag("chip_from", useUnmergedTree = true).performClick()
        composeRule.waitForIdle()

        val t0 = SystemClock.elapsedRealtime()
        composeRule.onNodeWithTag("btn_use_gps", useUnmergedTree = true).performClick()

        var summary = ""
        val fromDeadline = t0 + FROM_SET_BOUND_MS
        while (SystemClock.elapsedRealtime() < fromDeadline) {
            summary = waypointSummaryText()
            if (summary.contains("From:") && !summary.contains("From: (unset)")) break
            Thread.sleep(100)
        }
        val fromElapsed = SystemClock.elapsedRealtime() - t0
        assertTrue(
            "From not set within ${FROM_SET_BOUND_MS}ms (took ${fromElapsed}ms): $summary",
            !summary.contains("From: (unset)") && summary.contains("From:"),
        )
        assertTrue(
            "Use GPS From-set exceeded bound: ${fromElapsed}ms > ${FROM_SET_BOUND_MS}ms",
            fromElapsed <= FROM_SET_BOUND_MS,
        )

        val upgradeDeadline = t0 + ADDRESS_UPGRADE_BOUND_MS
        var query = ""
        while (SystemClock.elapsedRealtime() < upgradeDeadline) {
            query = fieldSearchText()
            if (query.contains("Welhavens", ignoreCase = true)) break
            Thread.sleep(200)
        }
        val upgradeElapsed = SystemClock.elapsedRealtime() - t0
        assertTrue(
            "expected address upgrade to Welhavens within ${ADDRESS_UPGRADE_BOUND_MS}ms " +
                "(took ${upgradeElapsed}ms), query='$query' summary=$summary",
            query.contains("Welhavens", ignoreCase = true),
        )
        Log.i(
            TAG,
            "PASS fromElapsedMs=$fromElapsed upgradeElapsedMs=$upgradeElapsed " +
                "query='$query' summary=$summary",
        )
    }

    private fun waypointSummaryText(): String =
        runCatching {
            composeRule
                .onNodeWithTag("search_waypoints_summary", useUnmergedTree = true)
                .fetchSemanticsNode()
                .config[SemanticsProperties.Text]
                .joinToString(" ") { it.text }
        }.getOrDefault("")

    private fun fieldSearchText(): String =
        runCatching {
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
                return@runCatching editable.text
            }
            node.config
                .getOrElse(SemanticsProperties.Text) { emptyList() }
                .joinToString(" ") { it.text }
        }.getOrDefault("")
}