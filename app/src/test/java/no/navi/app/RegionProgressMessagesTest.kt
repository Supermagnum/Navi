package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Task E: every download/index progress string must name the region and, when
 * the caller knows corridor position, keep the N-of-M sequence indicator.
 */
class RegionProgressMessagesTest {
    @Test
    fun phase_for_region_keeps_n_of_m_and_adds_display_name() {
        val line =
            RegionProgressMessages.phaseForRegion(
                phase = "Writing database",
                regionId = "europe/sweden/vastra_gotaland",
                index = 2,
                total = 4,
            )
        assertTrue(line.contains("2 of 4"))
        assertTrue(
            "expected Västra Götaland (or catalog label) in: $line",
            line.contains("Västra", ignoreCase = true) ||
                line.contains("Götaland", ignoreCase = true) ||
                line.contains("vastra", ignoreCase = true),
        )
        assertTrue(line.startsWith("Writing database"))
    }

    @Test
    fun annotate_keeps_name_and_adds_missing_n_of_m() {
        val name = RegionCoverage.displayName("europe/norway/ostlandet")
        val raw = "Downloading region… ($name)"
        val annotated =
            RegionProgressMessages.annotate(raw, "europe/norway/ostlandet", 1, 3)
        assertTrue(annotated.contains(name))
        assertTrue(annotated.contains("1 of 3"))
        assertTrue(annotated.startsWith("Downloading region"))
    }

    @Test
    fun annotate_skips_when_name_and_n_of_m_already_present() {
        val name = RegionCoverage.displayName("europe/norway/ostlandet")
        val raw = "Downloading region… ($name) (region 1 of 3)"
        assertEquals(
            raw,
            RegionProgressMessages.annotate(raw, "europe/norway/ostlandet", 1, 3),
        )
    }

    @Test
    fun annotate_adds_n_of_m_and_name_to_bare_phase() {
        val line =
            RegionProgressMessages.annotate(
                "Writing map archive…",
                "europe/denmark",
                index = 3,
                total = 4,
            )
        assertTrue(line.contains("Writing map archive"))
        assertTrue(line.contains("3 of 4"))
        assertTrue(line.contains("Denmark", ignoreCase = true))
    }

    @Test
    fun long_trip_part_includes_index_total_and_name() {
        val part =
            RegionProgressMessages.longTripPart(
                regionId = "europe/norway/ostlandet",
                state = "Downloading",
                index = 1,
                total = 4,
            )
        assertTrue(part.contains("1 of 4"))
        assertTrue(part.contains("Ostlandet") || part.contains("ostlandet"))
        assertTrue(part.contains("Downloading"))
        assertFalse(
            "must not be a bare leaf=State with no N-of-M",
            part == "ostlandet=Downloading",
        )
    }
}
