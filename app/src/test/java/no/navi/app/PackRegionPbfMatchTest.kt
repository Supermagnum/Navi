package no.navi.app

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

class PackRegionPbfMatchTest {
    @Test
    fun vestlandet_pbf_matches_vestlandet_not_ostlandet() {
        val vest = File("/tmp/vestlandet-latest.osm.pbf")
        val ost = File("/tmp/ostlandet-latest.osm.pbf")
        assertTrue(
            PackRegionAvailability.pbfMatchesRegion(vest, "europe/norway/vestlandet"),
        )
        assertFalse(
            PackRegionAvailability.pbfMatchesRegion(ost, "europe/norway/vestlandet"),
        )
        assertTrue(
            PackRegionAvailability.pbfMatchesRegion(ost, "europe/norway/ostlandet"),
        )
    }
}
