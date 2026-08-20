package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class RegionCoverageTest {
    @Test
    fun langflon_is_east_of_border_rundfloen_is_not() {
        assertTrue(RegionCoverage.eastOfNorwaySwedenBorder(61.8975, 12.2685))
        assertFalse(RegionCoverage.eastOfNorwaySwedenBorder(61.8956, 12.2208))
        assertFalse(RegionCoverage.eastOfNorwaySwedenBorder(59.91, 10.75))
    }

    @Test
    fun ostlandet_download_does_not_cover_sweden_identity() {
        assertFalse(
            RegionCoverage.downloadedCoversIdentity(
                "europe/norway/ostlandet",
                "europe/sweden",
            ),
        )
        assertTrue(
            RegionCoverage.downloadedCoversIdentity(
                "europe/sweden",
                "europe/sweden",
            ),
        )
        assertTrue(
            RegionCoverage.downloadedCoversIdentity(
                "europe/norway",
                "europe/norway/ostlandet",
            ),
        )
        assertFalse(
            RegionCoverage.downloadedCoversIdentity(
                "europe/norway/ostlandet",
                "europe/norway/trondelag",
            ),
        )
    }

    @Test
    fun sweden_prompt_copy_names_sweden() {
        assertEquals("Sweden", RegionCoverage.displayName("europe/sweden"))
        assertFalse(
            RegionCoverage.displayName("europe/sweden").contains("norway", ignoreCase = true),
        )
        assertEquals("West Virginia", RegionCoverage.displayName("north-america/us/west-virginia"))
        assertEquals("Nevada", RegionCoverage.displayName("north-america/us/nevada"))
    }

    @Test
    fun fylke_crossing_endpoints_stay_norwegian_not_sweden() {
        val fagernes = 60.9858 to 9.2322
        val gol = 60.7011 to 8.9564
        val strand = 60.5175 to 11.2670
        val morskogen = 60.5080 to 11.2200
        for ((lat, lon) in listOf(fagernes, gol, strand, morskogen)) {
            assertFalse(RegionCoverage.eastOfNorwaySwedenBorder(lat, lon))
        }
        assertTrue(
            RegionCoverage.downloadedCoversIdentity(
                "europe/norway/ostlandet",
                "europe/norway/ostlandet",
            ),
        )
    }
}
