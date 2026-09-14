package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Documents Download-region phase order: packs+extract → basemap → place index,
 * with local convert handed to IndexedMapsBackground afterward (not blocking
 * place index). Offline style URIs differ per PMTiles archive (2nd-region render).
 */
class RegionDownloadOrderTest {
    @Test
    fun usable_status_prefix_is_stable_for_ui() {
        assertTrue(
            RegionDownloadBackground.USABLE_STATUS_PREFIX.startsWith("Place index ready"),
        )
    }

    @Test
    fun offline_style_leaf_name_differs_per_pmtiles_archive() {
        val a =
            BasemapStyleResolver.offlineStyleLeafName(
                "/data/pmtiles/europe_norway_ostlandet.pmtiles",
                withDem = false,
            )
        val b =
            BasemapStyleResolver.offlineStyleLeafName(
                "/data/pmtiles/europe_norway_vestlandet.pmtiles",
                withDem = false,
            )
        assertNotEquals(a, b)
        assertTrue(a.contains("ostlandet"))
        assertTrue(b.contains("vestlandet"))
        assertEquals(
            "style.local.v3.europe_norway_ostlandet.dem.json",
            BasemapStyleResolver.offlineStyleLeafName(
                "/data/pmtiles/europe_norway_ostlandet.pmtiles",
                withDem = true,
            ),
        )
    }
}
