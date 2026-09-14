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
        // hedmark is on the pack server but covered by Østlandet — not a separate chip.
        assertFalse(
            GeofabrikDownloadCatalog.norwayRegions.any { it.first == "hedmark" },
        )

        assertTrue(GeofabrikDownloadCatalog.hasRegionChips("europe/sweden"))
        assertTrue(GeofabrikDownloadCatalog.hasRegionChips("europe/sweden/stockholm"))
        assertEquals(21, GeofabrikDownloadCatalog.swedenRegions.size)
        assertEquals(
            "europe/sweden/stockholm",
            GeofabrikDownloadCatalog.defaultRegionChipPath("europe/sweden"),
        )
        assertTrue(
            GeofabrikDownloadCatalog.swedenRegions.any { it.first == "vastra_gotaland" },
        )
        assertFalse(
            GeofabrikDownloadCatalog.swedenRegions.any { it.first == "vastra-gotaland" },
        )
    }

    @Test
    fun sweden_extract_path_uses_country_pbf() {
        assertEquals(
            "europe/sweden",
            GeofabrikDownloadCatalog.extractPathForPbf("europe/sweden/stockholm"),
        )
        assertEquals(
            "europe/sweden/stockholm",
            GeofabrikDownloadCatalog.canonicalizePath("europe/sweden/stockholm"),
        )
    }

    @Test
    fun countries_without_chips_get_granularity_note() {
        val note = GeofabrikDownloadCatalog.regionGranularityNote("europe/denmark")
        assertTrue(note.contains("France", ignoreCase = true) || note.contains("current.json"))
        assertTrue(note.contains("Norway", ignoreCase = true) || note.contains("current.json"))
    }

    @Test
    fun germany_region_chips_match_pack_catalog_leaves() {
        assertTrue(GeofabrikDownloadCatalog.hasRegionChips("europe/germany"))
        assertEquals(16, GeofabrikDownloadCatalog.germanyRegions.size)
        assertEquals(
            "europe/germany/bremen",
            GeofabrikDownloadCatalog.defaultRegionChipPath("europe/germany"),
        )
        assertEquals(
            "europe/germany",
            GeofabrikDownloadCatalog.regionChipBasePath("europe/germany/bremen"),
        )
        assertEquals(
            "europe/germany/bayern",
            GeofabrikDownloadCatalog.regionChipBasePath("europe/germany/bayern/oberbayern"),
        )
        assertEquals(29, GeofabrikDownloadCatalog.germanyPackLeafPaths().size)
    }

    @Test
    fun pack_server_subregion_chips_cover_published_country_trees() {
        assertEquals(53, GeofabrikDownloadCatalog.usStates.size)
        assertEquals(2, GeofabrikDownloadCatalog.usCaliforniaRegions.size)
        assertEquals(
            "north-america/us/california",
            GeofabrikDownloadCatalog.regionChipBasePath("north-america/us/california/socal"),
        )
        assertEquals(54, GeofabrikDownloadCatalog.packCatalogLeafPaths("north-america/us").size)

        assertEquals(27, GeofabrikDownloadCatalog.franceRegions.size)
        assertEquals(27, GeofabrikDownloadCatalog.packCatalogLeafPaths("europe/france").size)

        assertEquals(18, GeofabrikDownloadCatalog.spainRegions.size)
        assertEquals(16, GeofabrikDownloadCatalog.polandRegions.size)
        assertEquals(14, GeofabrikDownloadCatalog.czechRepublicRegions.size)
        assertEquals(12, GeofabrikDownloadCatalog.netherlandsRegions.size)
        assertEquals(12, GeofabrikDownloadCatalog.australiaRegions.size)
        assertEquals(10, GeofabrikDownloadCatalog.russiaRegions.size)
        assertEquals(8, GeofabrikDownloadCatalog.japanRegions.size)
        assertEquals(7, GeofabrikDownloadCatalog.indonesiaRegions.size)
        assertEquals(6, GeofabrikDownloadCatalog.indiaRegions.size)
        assertEquals(5, GeofabrikDownloadCatalog.brazilRegions.size)
        assertEquals(5, GeofabrikDownloadCatalog.italyRegions.size)
        assertEquals(33, GeofabrikDownloadCatalog.chinaRegions.size)

        assertEquals(13, GeofabrikDownloadCatalog.canadaRegions.size)
        assertEquals(6, GeofabrikDownloadCatalog.canadaBritishColumbiaRegions.size)
        assertEquals(3, GeofabrikDownloadCatalog.canadaNunavutRegions.size)
        assertEquals(20, GeofabrikDownloadCatalog.packCatalogLeafPaths("north-america/canada").size)

        assertEquals(
            "europe/france/ile-de-france",
            GeofabrikDownloadCatalog.defaultRegionChipPath("europe/france"),
        )
        assertEquals(
            "russia/central-fed-district",
            GeofabrikDownloadCatalog.defaultRegionChipPath("russia"),
        )
        assertTrue(GeofabrikDownloadCatalog.hasRegionChips("asia/china/beijing"))
        assertTrue(GeofabrikDownloadCatalog.hasRegionChips("europe/italy/sud"))
    }

    @Test
    fun great_britain_note_points_at_united_kingdom() {
        val note = GeofabrikDownloadCatalog.regionGranularityNote("europe/great-britain")
        assertTrue(note.contains("United Kingdom"))
        assertTrue(note.contains("borough", ignoreCase = true) || note.contains("London"))
    }

    @Test
    fun united_kingdom_and_england_chips_are_live_geofabrik_leaves() {
        assertTrue(GeofabrikDownloadCatalog.hasRegionChips("europe/united-kingdom"))
        assertEquals(
            "europe/united-kingdom/england",
            GeofabrikDownloadCatalog.defaultRegionChipPath("europe/united-kingdom"),
        )
        assertTrue(
            GeofabrikDownloadCatalog.unitedKingdomNations.any { it.first == "england" },
        )
        assertTrue(
            GeofabrikDownloadCatalog.unitedKingdomNations.any { it.first == "bermuda" },
        )
        assertTrue(
            GeofabrikDownloadCatalog.unitedKingdomNations.any { it.first == "falklands" },
        )
        assertEquals(5, GeofabrikDownloadCatalog.unitedKingdomNations.size)
        assertFalse(
            GeofabrikDownloadCatalog.englandCounties.any { it.first == "enfield" },
        )
        assertTrue(
            GeofabrikDownloadCatalog.englandCounties.any { it.first == "greater-london" },
        )
        assertEquals(47, GeofabrikDownloadCatalog.englandCounties.size)
        assertEquals(
            "europe/united-kingdom/england/greater-london",
            GeofabrikDownloadCatalog.defaultRegionChipPath("europe/united-kingdom/england"),
        )
        assertEquals(
            "europe/united-kingdom/england",
            GeofabrikDownloadCatalog.regionChipBasePath(
                "europe/united-kingdom/england/greater-london",
            ),
        )
    }

    @Test
    fun canonicalize_retired_london_borough_to_greater_london() {
        val want = "europe/united-kingdom/england/greater-london"
        assertEquals(
            want,
            GeofabrikDownloadCatalog.canonicalizePath(
                "europe/united-kingdom/england/london/enfield",
            ),
        )
        assertEquals(
            want,
            GeofabrikDownloadCatalog.canonicalizePath("europe/united-kingdom/england/london"),
        )
        assertEquals(want, GeofabrikDownloadCatalog.canonicalizePath("enfield"))
        assertEquals(want, GeofabrikDownloadCatalog.canonicalizePath(want))
        assertEquals(
            "europe/great-britain",
            GeofabrikDownloadCatalog.canonicalizePath("europe/great-britain"),
        )
        assertEquals(
            "europe/united-kingdom/england",
            GeofabrikDownloadCatalog.canonicalizePath("europe/great-britain/england"),
        )
    }

    @Test
    fun catalog_country_paths_include_united_kingdom() {
        assertTrue(
            GeofabrikDownloadCatalog.countries.any { it.path == "europe/united-kingdom" },
        )
        assertTrue(
            GeofabrikDownloadCatalog.countries.any { it.path == "europe/great-britain" },
        )
    }
}
