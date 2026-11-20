package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class MwmInstallPhaseTest {
    @Test
    fun leaf_id_progress_uses_ostlandet_label_not_denmark() {
        val phase =
            MwmMapDownloader.mwmInstallPhase(
                rawMessage = "Denmark_North Denmark Region",
                label = "Østlandet",
                geofabrikPath = "europe/norway/ostlandet",
            )
        assertTrue(phase.contains("Østlandet"))
        assertFalse(phase.contains("Denmark"))
    }

    @Test
    fun region_display_label_for_bake_id() {
        assertEquals(
            "Østlandet",
            MwmMapDownloader.regionDisplayLabel("europe_norway_ostlandet"),
        )
        assertEquals(
            "Sørlandet",
            MwmMapDownloader.regionDisplayLabel("europe_norway_sorlandet"),
        )
    }

    @Test
    fun leaf_ids_for_region_install_strip_foreign_catalog_leaves() {
        val catalog =
            listOf(
                "Norway_Hedmark",
                "Denmark_North Denmark Region",
                "Sweden_Bergslagen",
                "Norway_Oppland",
            )
        val leaves =
            MwmMapDownloader.leafIdsForRegionInstall("europe_norway_ostlandet", catalog)
        assertTrue(leaves.isNotEmpty())
        assertTrue(leaves.all { it.startsWith("Norway_") })
        assertFalse(leaves.any { it.startsWith("Denmark_") })
    }
}
