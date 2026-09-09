package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class GeofabrikDownloadCatalogTest {
    @Test
    fun norway_and_sweden_region_chips_match_pack_catalog_slugs() {
        assertTrue(GeofabrikDownloadCatalog.hasRegionChips("europe/norway"))
        assertEquals(6, GeofabrikDownloadCatalog.norwayRegions.size)
        assertTrue(
            GeofabrikDownloadCatalog.norwayRegions.any { it.first == "svalbard-janmayen" },
        )
        assertTrue(
            GeofabrikDownloadCatalog.norwayRegions.any { it.first == "ostlandet" },
        )

        assertTrue(GeofabrikDownloadCatalog.hasRegionChips("europe/sweden"))
        assertTrue(GeofabrikDownloadCatalog.hasRegionChips("europe/sweden/stockholm"))
        assertEquals(21, GeofabrikDownloadCatalog.swedenRegions.size)
        assertEquals(
            "europe/sweden/stockholm",
            GeofabrikDownloadCatalog.defaultRegionChipPath("europe/sweden"),
        )
        // Chip slug matches published current.json id (underscore).
        assertTrue(
            GeofabrikDownloadCatalog.swedenRegions.any { it.first == "vastra_gotaland" },
        )
        assertFalse(
            GeofabrikDownloadCatalog.swedenRegions.any { it.first == "vastra-gotaland" },
        )
    }

    @Test
    fun sweden_no_longer_uses_country_only_granularity_note() {
        // Sweden has chips; the note path is for countries without chips.
        val note = GeofabrikDownloadCatalog.regionGranularityNote("europe/denmark")
        assertTrue(note.contains("Sweden", ignoreCase = true) || note.contains("län"))
        assertTrue(note.contains("Norway", ignoreCase = true))
    }

    @Test
    fun us_region_note_mentions_west_virginia_path() {
        val note = GeofabrikDownloadCatalog.regionGranularityNote("north-america/us")
        assertTrue(note.contains("west-virginia"))
        assertTrue(note.contains("states", ignoreCase = true))
    }

    @Test
    fun germany_region_note_mentions_typed_state_path() {
        val note = GeofabrikDownloadCatalog.regionGranularityNote("europe/germany")
        assertTrue(note.contains("bremen"))
    }

    @Test
    fun russia_region_note_points_at_typed_district() {
        val note = GeofabrikDownloadCatalog.regionGranularityNote("russia")
        assertTrue(note.contains("kaliningrad"))
        assertTrue(note.contains("federal-district", ignoreCase = true) || note.contains("district"))
    }
}
