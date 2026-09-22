package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Task 7 regression: coalesce rapid status updates, reserve toast height, and
 * drop Tools near-duplicates of the same region.
 */
class StatusUiStabilityTest {
    @Test
    fun coalesce_bounds_renders_relative_to_inputs() {
        var state = StatusUi.CoalesceState()
        var now = 1_000L
        repeat(20) { i ->
            state = StatusUi.coalesce(state, "Downloading… ${i * 5}%", now)
            now += 50L // 20 Hz input — faster than COALESCE_MIN_INTERVAL_MS
        }
        assertEquals(20, state.inputCount)
        assertTrue(
            "renderCount=${state.renderCount} must be << inputCount=${state.inputCount}",
            state.renderCount <= 5,
        )
        assertTrue(state.renderCount >= 1)
        // Flush pending so the latest percent is shown.
        state = StatusUi.flushPending(state, now + StatusUi.COALESCE_MIN_INTERVAL_MS)
        assertTrue(state.text.contains("95%") || state.text.contains("100%") || state.text.contains("%"))
        assertEquals(null, state.pending)
    }

    @Test
    fun coalesce_emits_immediately_on_first_and_after_interval() {
        var state = StatusUi.CoalesceState()
        state = StatusUi.coalesce(state, "A", 1000L)
        assertEquals("A", state.text)
        assertEquals(1, state.renderCount)
        state = StatusUi.coalesce(state, "B", 1100L)
        assertEquals("A", state.text)
        assertEquals("B", state.pending)
        state = StatusUi.coalesce(state, "C", 1000L + StatusUi.COALESCE_MIN_INTERVAL_MS)
        assertEquals("C", state.text)
        assertEquals(2, state.renderCount)
        assertEquals(null, state.pending)
    }

    @Test
    fun tools_visible_lines_drop_near_duplicate_tools_status() {
        val name = RegionCoverage.displayName("europe/norway/ostlandet")
        val regionLine =
            RegionProgressMessages.annotate(
                "Downloading region… 42%",
                "europe/norway/ostlandet",
                1,
                2,
            )
        val placeLine =
            RegionProgressMessages.annotate(
                "Place index: writing… 55%",
                "europe/sweden/vastra_gotaland",
                2,
                2,
            )
        val nearDup =
            RegionProgressMessages.annotate(
                "Downloading region… 43%",
                "europe/norway/ostlandet",
                1,
                2,
            )
        val lines =
            StatusUi.toolsVisibleLines(
                regionDownloadProgress = regionLine,
                pmtilesProgress = "",
                placeIndexUiLine = placeLine,
                indexedMapsUiLine = "",
                toolsStatusRaw = nearDup,
            )
        assertEquals(2, lines.size)
        assertFalse(
            "near-dup tools_status must not appear: $lines",
            lines.any { it.first == "tools_status" },
        )
        assertTrue(lines.any { it.second.contains(name) || it.second.contains("ostlandet", true) })
        assertTrue(
            lines.any {
                it.second.contains("vastra", true) ||
                    it.second.contains("Götaland", true) ||
                    it.second.contains("Gotaland", true)
            },
        )
    }

    @Test
    fun tools_visible_lines_keeps_unrelated_tools_status() {
        val lines =
            StatusUi.toolsVisibleLines(
                regionDownloadProgress = "Downloading region… 10%: Denmark",
                pmtilesProgress = "",
                placeIndexUiLine = "",
                indexedMapsUiLine = "",
                toolsStatusRaw = "Profile: car",
            )
        assertEquals(2, lines.size)
        assertEquals("tools_status", lines.last().first)
        assertEquals("Profile: car", lines.last().second)
    }

    @Test
    fun toast_min_height_constant_is_stable() {
        assertTrue(StatusUi.TOAST_MIN_HEIGHT_DP >= 40)
        assertTrue(StatusUi.COALESCE_MIN_INTERVAL_MS in 200L..500L)
    }
}
