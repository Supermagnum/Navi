package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test

class LongTripMwmSupportTest {
    @Test
    fun geofabrikPathToBakeId_normalizes_slashes_and_case() {
        assertEquals(
            "europe_germany_niedersachsen",
            LongTripMwmSupport.geofabrikPathToBakeId("Europe/Germany/Niedersachsen/"),
        )
    }

    @Test
    fun bakeIdCandidates_includes_hyphen_and_underscore_variants() {
        val c = LongTripMwmSupport.bakeIdCandidates("europe/germany/schleswig-holstein")
        assertTrue(c.contains("europe_germany_schleswig-holstein"))
        assertTrue(c.contains("europe_germany_schleswig_holstein"))
    }

    @Test
    fun alias_table_maps_niedersachsen_and_schleswig_holstein() {
        val niedersachsen =
            LongTripMwmGeofabrikAliases.leafIdsForGeofabrikPath("europe/germany/niedersachsen")
        assertNotNull(niedersachsen)
        assertEquals(5, niedersachsen!!.size)
        assertTrue(
            niedersachsen.all { it.startsWith("Germany_Lower Saxony_") },
        )

        val sh =
            LongTripMwmGeofabrikAliases.leafIdsForBakeId("europe_germany_schleswig-holstein")
        assertNotNull(sh)
        assertEquals(
            listOf(
                "Germany_Schleswig-Holstein_Flensburg",
                "Germany_Schleswig-Holstein_Kiel",
            ),
            sh,
        )
    }

    @Test
    fun alias_table_covers_all_germany_pack_catalog_leaves() {
        val paths = GeofabrikDownloadCatalog.germanyPackLeafPaths()
        assertTrue(paths.isNotEmpty())
        for (path in paths) {
            val leaves = LongTripMwmGeofabrikAliases.leafIdsForGeofabrikPath(path)
            assertNotNull("missing alias for $path", leaves)
            assertTrue("empty alias for $path", leaves!!.isNotEmpty())
        }
    }
}