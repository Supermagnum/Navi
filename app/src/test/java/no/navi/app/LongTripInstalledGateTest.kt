package no.navi.app

import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

/**
 * Corridor route resolution must proceed once every region is Installed
 * (packs Ready), without waiting for place-index / Indexed. Place search for
 * Installed-but-not-Indexed regions stays gated by [PlaceIndexReady].
 */
class LongTripInstalledGateTest {
    @get:Rule
    val tmp = TemporaryFolder()

    private val regions =
        listOf(
            "europe/norway/ostlandet",
            "europe/sweden/vastra_gotaland",
            "europe/denmark",
        )

    @After
    fun tearDown() {
        LongTripCoordinator.resetForTests()
        if (RegionDownloadBackground.isRunning()) {
            RegionDownloadBackground.releaseWorker()
        }
    }

    @Test
    fun corridor_all_installed_not_indexed_is_ready_for_planning() {
        val dir = tmp.newFolder("installed-gate")
        val packDir = tmp.newFolder("packs-installed-gate")

        LongTripCoordinator.setCorridorProviderForTests { _, _, _ -> Result.success(regions) }
        LongTripCoordinator.setPackTargetResolverForTests { _, _ ->
            LongTripPackStorage.PackTarget.DownloadTo(packDir, "internal", false)
        }
        LongTripCoordinator.setDownloadStarterForTests { _, _, _, _, _, _, _ -> }

        LongTripCoordinator.enableWithDataDir(
            context = null,
            dataDir = dir,
            waypoints = listOf(60.79 to 11.08, 55.67 to 12.57),
        )

        assertFalse(LongTripCoordinator.corridorReadyForPlanning())

        for (id in regions) {
            RegionDownloadBackground.emitPhaseForTests(id, "installed")
        }

        val plan = LongTripCoordinator.currentPlan()!!
        for (id in regions) {
            assertEquals(
                "installed must map to Installed, not Indexed",
                LongTripCoordinator.State.Installed,
                plan.states[id],
            )
            assertTrue(LongTripCoordinator.regionReadyForPlanning(id))
        }
        assertTrue(
            "multi-region corridor with only Installed must be planning-ready",
            LongTripCoordinator.corridorReadyForPlanning(),
        )

        // Background place-index must not revoke planning readiness.
        RegionDownloadBackground.emitPhaseForTests(regions[1], "indexing")
        assertEquals(
            LongTripCoordinator.State.Installed,
            LongTripCoordinator.currentPlan()!!.states[regions[1]],
        )
        assertTrue(LongTripCoordinator.corridorReadyForPlanning())

        RegionDownloadBackground.emitPhaseForTests(regions[1], "indexed")
        assertEquals(
            LongTripCoordinator.State.Indexed,
            LongTripCoordinator.currentPlan()!!.states[regions[1]],
        )
        assertTrue(LongTripCoordinator.corridorReadyForPlanning())
    }

    @Test
    fun installed_region_without_place_index_is_not_searchable() {
        val dir = tmp.newFolder("search-gate")
        // Packs Ready stamp is irrelevant for PlaceIndexReady — only the stamp file.
        assertFalse(
            "Installed-but-not-Indexed must not pass PlaceIndexReady",
            PlaceIndexReady.isReady(dir, regions[1]),
        )
        assertTrue(
            PlaceIndexReady.filterHitsToReadyRegions(dir, emptyList()).isEmpty(),
        )
        PlaceIndexReady.markReady(dir, regions[0])
        assertTrue(PlaceIndexReady.isReady(dir, regions[0]))
        assertFalse(
            "sibling Installed region stays unsearchable until Indexed",
            PlaceIndexReady.isReady(dir, regions[1]),
        )
    }
}
