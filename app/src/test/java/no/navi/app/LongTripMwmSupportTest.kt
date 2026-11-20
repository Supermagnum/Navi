package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class LongTripMwmSupportTest {
    /**
     * Bake-id prefixes covered by navi-mwm-catalog v2 `region_to_leaves`
     * (Nordics only on device catalog 261115). Aliases are intentionally absent
     * for these so catalog remains the source of truth.
     */
    private val catalogCoveredBakePrefixes =
        listOf(
            "europe_norway",
            "europe_sweden",
            "europe_denmark",
            "europe_finland",
        )

    /**
     * Pack-catalog leaves with no CoMaps MWM leaf in catalog 261115.
     * Long-trip still falls through to PBF for these until CoMaps publishes them.
     */
    private val noComapsLeafPackPaths =
        setOf(
            "europe/france/mayotte",
            "europe/france/reunion",
        )

    private fun isCatalogCovered(geofabrikPath: String): Boolean {
        val bake =
            LongTripMwmSupport.geofabrikPathToBakeId(geofabrikPath).replace('-', '_')
        return catalogCoveredBakePrefixes.any { prefix ->
            bake == prefix || bake.startsWith(prefix + "_")
        }
    }

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

    @Test
    fun alias_or_catalog_covers_every_europe_pack_leaf_with_comaps() {
        val paths = GeofabrikDownloadCatalog.europePackLeafPaths()
        assertTrue("europe pack paths empty", paths.isNotEmpty())
        val missing = mutableListOf<String>()
        for (path in paths) {
            if (path in noComapsLeafPackPaths) {
                assertNull(
                    "unexpected alias for known CoMaps gap $path",
                    LongTripMwmGeofabrikAliases.leafIdsForGeofabrikPath(path),
                )
                continue
            }
            if (isCatalogCovered(path)) continue
            val leaves = LongTripMwmGeofabrikAliases.leafIdsForGeofabrikPath(path)
            if (leaves.isNullOrEmpty()) {
                missing += path
            }
        }
        assertTrue(
            "missing MWM aliases for europe pack paths (have CoMaps leaves):\n" +
                missing.joinToString("\n"),
            missing.isEmpty(),
        )
    }

    @Test
    fun alias_table_maps_long_trip_priority_countries() {
        val samples =
            listOf(
                "europe/france" to "France_",
                "europe/france/ile-de-france" to "France_Ile-de-France_",
                "europe/poland" to "Poland_",
                "europe/poland/mazowieckie" to "Poland_Masovian",
                "europe/netherlands" to "Netherlands_",
                "europe/netherlands/noord-holland" to "Netherlands_North Holland_",
                "europe/belgium" to "Belgium_",
                "europe/austria" to "Austria_",
                "europe/switzerland" to "Switzerland_",
                "europe/czech-republic" to "Czech_",
                "europe/czech-republic/praha" to "Czech_Praha",
                "europe/italy" to "Italy_",
                "europe/italy/nord-ovest" to "Italy_",
                "europe/spain" to "Spain_",
                "europe/spain/madrid" to "Spain_Community of Madrid",
                "europe/great-britain" to "UK_England_",
                "europe/united-kingdom" to "UK_",
                "europe/united-kingdom/england/greater-london" to "UK_England_Greater London",
            )
        for ((path, prefix) in samples) {
            val leaves = LongTripMwmGeofabrikAliases.leafIdsForGeofabrikPath(path)
            assertNotNull("missing alias for $path", leaves)
            assertTrue("empty alias for $path", leaves!!.isNotEmpty())
            assertTrue(
                "alias for $path should relate to $prefix; got $leaves",
                leaves.any { it == prefix || it.startsWith(prefix) },
            )
        }
    }

    @Test
    fun nordic_paths_have_no_alias_rows_catalog_is_authoritative() {
        val nordicSamples =
            listOf(
                "europe/norway",
                "europe/norway/ostlandet",
                "europe/sweden",
                "europe/sweden/stockholm",
                "europe/denmark",
                "europe/finland",
            )
        for (path in nordicSamples) {
            assertTrue("expected catalog coverage for $path", isCatalogCovered(path))
            assertNull(
                "Nordic $path must not use alias table (catalog region_to_leaves)",
                LongTripMwmGeofabrikAliases.leafIdsForGeofabrikPath(path),
            )
        }
    }

    @Test
    fun france_overseas_without_comaps_leaves_stay_unmapped() {
        for (path in noComapsLeafPackPaths) {
            assertNull(LongTripMwmGeofabrikAliases.leafIdsForGeofabrikPath(path))
        }
    }
}
