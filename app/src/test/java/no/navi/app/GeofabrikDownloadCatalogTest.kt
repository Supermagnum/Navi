package no.navi.app

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class GeofabrikDownloadCatalogTest {
    @Test
    fun sweden_region_note_says_country_only() {
        val note = GeofabrikDownloadCatalog.regionGranularityNote("europe/sweden")
        assertTrue(note.contains("country extract", ignoreCase = true))
        assertTrue(note.contains("län", ignoreCase = true))
        assertFalse(note.contains("kronoberg", ignoreCase = true))
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
