package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * Stable Tools process-footer ordering. Place-index / indexed-maps lines are
 * intentionally omitted from the footer (MWM search/routing is built-in).
 */
class ToolsProcessFooterOrderTest {
    @Test
    fun order_is_region_then_pmtiles_only() {
        val lines =
            StatusUi.toolsVisibleLines(
                regionDownloadProgress = "region A",
                pmtilesProgress = "Writing map archive…",
                placeIndexUiLine = "Preparing search: …",
                indexedMapsUiLine = "Preparing maps: …",
                toolsStatusRaw = "",
            )
        assertEquals(
            listOf(
                "region_download_progress" to "region A",
                "pmtiles_progress" to "Writing map archive…",
            ),
            lines,
        )
    }

    @Test
    fun omitting_pmtiles_keeps_region() {
        val lines =
            StatusUi.toolsVisibleLines(
                regionDownloadProgress = "Fetching packs…",
                pmtilesProgress = "",
                placeIndexUiLine = "",
                indexedMapsUiLine = "Preparing maps: ready",
                toolsStatusRaw = "",
            )
        assertEquals(
            listOf("region_download_progress" to "Fetching packs…"),
            lines,
        )
    }

    @Test
    fun empty_means_ready_candidate() {
        assertEquals(
            emptyList<Pair<String, String>>(),
            StatusUi.toolsVisibleLines("", "", "", "", ""),
        )
    }
}
