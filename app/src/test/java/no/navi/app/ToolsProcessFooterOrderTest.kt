package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * Stable Tools process-footer ordering (Part 4). Mirrors the buildList order in
 * MainActivity Tools HUD.
 */
class ToolsProcessFooterOrderTest {
    private fun orderedLines(
        region: String,
        pmtiles: String,
        placeIndex: String,
        indexedMaps: String,
    ): List<String> =
        buildList {
            if (region.isNotBlank()) add(region)
            if (pmtiles.isNotBlank()) add(pmtiles)
            if (placeIndex.isNotBlank()) add(placeIndex)
            if (indexedMaps.isNotBlank()) add(indexedMaps)
        }

    @Test
    fun order_is_region_then_pmtiles_then_place_then_indexed() {
        assertEquals(
            listOf("region A", "Writing map archive…", "Place index: …", "Indexed maps: …"),
            orderedLines(
                region = "region A",
                pmtiles = "Writing map archive…",
                placeIndex = "Place index: …",
                indexedMaps = "Indexed maps: …",
            ),
        )
    }

    @Test
    fun omitting_middle_keeps_relative_order() {
        assertEquals(
            listOf("Fetching packs…", "Indexed maps: ready"),
            orderedLines(
                region = "Fetching packs…",
                pmtiles = "",
                placeIndex = "",
                indexedMaps = "Indexed maps: ready",
            ),
        )
    }

    @Test
    fun empty_means_ready_candidate() {
        assertEquals(
            emptyList<String>(),
            orderedLines("", "", "", ""),
        )
    }
}
