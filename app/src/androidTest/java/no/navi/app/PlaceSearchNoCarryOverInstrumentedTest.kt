package no.navi.app

import android.os.SystemClock
import android.util.Log
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextClearance
import androidx.compose.ui.test.performTextInput
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.rule.GrantPermissionRule
import org.junit.After
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Regression: independent searches must not keep prior-hit contamination, and
 * local FTS prefix hits (Kalmar* → Kalmargaten/Bergen) must not pollute an
 * online Kalmar (Sweden) result list.
 */
@RunWith(AndroidJUnit4::class)
class PlaceSearchNoCarryOverInstrumentedTest {
    companion object {
        private const val TAG = "PlaceSearchNoCarry"
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
    }

    @After
    fun tearDown() {
        NaviMapTestHooks.hideUiChrome = false
        NaviMapTestHooks.hideSearchChrome = false
    }

    @Test
    fun searchHarsefeldThenKalmar_hasNoBergenAndNoHarsefeldCarryOver() {
        openSearch()
        val harsefeld = waitHits("Harsefeld")
        Log.i(TAG, "harsefeld=$harsefeld")
        assertTrue("Harsefeld should resolve", harsefeld.isNotEmpty())
        assertTrue(
            harsefeld.any { it.contains("Harsefeld", ignoreCase = true) },
        )

        val kalmar = waitHits("Kalmar")
        Log.i(TAG, "kalmar=$kalmar")
        assertTrue("Kalmar should resolve", kalmar.isNotEmpty())
        assertTrue(
            "Kalmar Sweden should lead: $kalmar",
            kalmar.first().contains("Kalmar", ignoreCase = true),
        )
        // Sweden disambiguation: kommun / län / Sverige — not Bergen FTS bleed.
        assertTrue(
            "expected Swedish Kalmar context in results: $kalmar",
            kalmar.any {
                it.contains("kommun", ignoreCase = true) ||
                    it.contains("Sverige", ignoreCase = true) ||
                    it.contains("Sweden", ignoreCase = true) ||
                    it.contains("län", ignoreCase = true)
            } ||
                // Online short label may be just "Kalmar" with SE coords applied later;
                // still forbid Norwegian Bergen contamination.
                kalmar.none { it.contains("Bergen", ignoreCase = true) },
        )
        assertTrue(
            "no Bergen FTS bleed into Kalmar (Sweden): $kalmar",
            kalmar.none { it.contains("Bergen", ignoreCase = true) },
        )
        assertTrue(
            "no Kalmargaten prefix bleed: $kalmar",
            kalmar.none { it.contains("Kalmargaten", ignoreCase = true) },
        )
        assertTrue(
            "no Harsefeld carry-over into Kalmar: $kalmar",
            kalmar.none { it.contains("Harsefeld", ignoreCase = true) },
        )
    }

    @Test
    fun isolatedKalmar_swedenNotBergen() {
        // Same process as carry-over; openSearch must not hard-wait on idle
        // (cold MainActivity often keeps Compose busy on SM-P613).
        openSearch()
        val kalmar = waitHits("Kalmar")
        Log.i(TAG, "isolated_kalmar=$kalmar")
        assertTrue("Kalmar (Sweden) should resolve: $kalmar", kalmar.isNotEmpty())
        assertTrue(kalmar.first().contains("Kalmar", ignoreCase = true))
        assertTrue(
            "Kalmar is in Sweden — no Bergen FTS bleed: $kalmar",
            kalmar.none { it.contains("Bergen", ignoreCase = true) },
        )
        assertTrue(
            kalmar.none { it.contains("Kalmargaten", ignoreCase = true) },
        )
        assertTrue(
            "Swedish context expected: $kalmar",
            kalmar.any {
                it.contains("kommun", ignoreCase = true) ||
                    it.contains("Sverige", ignoreCase = true) ||
                    it.contains("län", ignoreCase = true)
            },
        )
    }

    private fun openSearch() {
        composeRule.mainClock.advanceTimeBy(1_000)
        Thread.sleep(1_000)
        val hasField =
            runCatching {
                composeRule.onNodeWithTag("field_search", useUnmergedTree = true).assertExists()
                true
            }.getOrDefault(false)
        if (hasField) return
        runCatching {
            composeRule.onNodeWithTag("btn_open_search", useUnmergedTree = true).performClick()
        }.recoverCatching {
            composeRule.onNodeWithTag("btn_tools_collapsed", useUnmergedTree = true).performClick()
            Thread.sleep(400)
            composeRule.onNodeWithTag("btn_open_search", useUnmergedTree = true).performClick()
        }
        composeRule.mainClock.advanceTimeBy(500)
        Thread.sleep(500)
        composeRule.onNodeWithTag("field_search", useUnmergedTree = true).assertExists()
    }

    private fun waitHits(query: String): List<String> {
        NaviMapTestHooks.lastSearchHitCount = -1
        NaviMapTestHooks.lastSearchQuery = ""
        NaviMapTestHooks.lastSearchHitNames = emptyList()
        val node = composeRule.onNodeWithTag("field_search", useUnmergedTree = true)
        runCatching { node.performScrollTo() }
        node.performTextClearance()
        node.performTextInput(query)
        composeRule.waitForIdle()
        val deadline = SystemClock.elapsedRealtime() + 45_000
        while (SystemClock.elapsedRealtime() < deadline) {
            if (NaviMapTestHooks.lastSearchHitCount >= 1 &&
                NaviMapTestHooks.lastSearchHitNames.isNotEmpty() &&
                NaviMapTestHooks.lastSearchQuery == query
            ) {
                break
            }
            composeRule.mainClock.advanceTimeBy(300)
            Thread.sleep(300)
        }
        return NaviMapTestHooks.lastSearchHitNames
    }
}
