package no.navi.app

import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

/**
 * Corridor route planning waits until every region is Indexed (place-index
 * ready). Packs Installed alone must not open the planning gate — concurrent
 * place-index + graph-build contends on PBF/Rayon. Place search for
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
    fun corridor_all_installed_not_indexed_is_not_ready_for_planning() {
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
        assertFalse(LongTripCoordinator.corridorPacksReady())

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
            assertTrue(LongTripCoordinator.regionPacksReady(id))
            assertFalse(
                "Installed alone must not open planning for $id",
                LongTripCoordinator.regionReadyForPlanning(id),
            )
        }
        assertTrue(
            "packs Ready for every corridor region",
            LongTripCoordinator.corridorPacksReady(),
        )
        assertFalse(
            "Installed-only corridor must wait for place-index before planning",
            LongTripCoordinator.corridorReadyForPlanning(),
        )

        // Place-index in progress: show Indexing; still not planning-ready.
        RegionDownloadBackground.emitPhaseForTests(regions[1], "indexing")
        assertEquals(
            LongTripCoordinator.State.Indexing,
            LongTripCoordinator.currentPlan()!!.states[regions[1]],
        )
        assertFalse(LongTripCoordinator.corridorReadyForPlanning())
        assertTrue(
            LongTripCoordinator.statusLine().contains("Indexing", ignoreCase = true),
        )

        // One region Indexed is not enough.
        RegionDownloadBackground.emitPhaseForTests(regions[1], "indexed")
        assertEquals(
            LongTripCoordinator.State.Indexed,
            LongTripCoordinator.currentPlan()!!.states[regions[1]],
        )
        assertFalse(LongTripCoordinator.corridorReadyForPlanning())

        for (id in regions) {
            if (id == regions[1]) continue
            RegionDownloadBackground.emitPhaseForTests(id, "indexed")
        }
        assertTrue(
            "all Indexed opens the planning gate",
            LongTripCoordinator.corridorReadyForPlanning(),
        )
    }

    @Test
    fun packs_on_disk_without_place_index_skip_redownload_as_installed() {
        val dir = tmp.newFolder("reuse-installed")
        val packDir = tmp.newFolder("packs-reuse-installed")
        val started = mutableListOf<String>()

        for (id in regions) {
            val stem = PackRegionAvailability.localStem(id)
            // Manifest alone (pack-server Ready) — stub/missing PBF must not re-download.
            File(packDir, "$stem.navi-manifest.json").writeText("{}")
        }

        LongTripCoordinator.setCorridorProviderForTests { _, _, _ -> Result.success(regions) }
        LongTripCoordinator.setPackTargetResolverForTests { _, _ ->
            LongTripPackStorage.PackTarget.DownloadTo(packDir, "internal", false)
        }
        LongTripCoordinator.setDownloadStarterForTests { _, _, _, _, path, _, _ ->
            started.add(path)
        }

        LongTripCoordinator.enableWithDataDir(
            context = null,
            dataDir = dir,
            waypoints = listOf(60.79 to 11.08, 55.67 to 12.57),
        )

        assertTrue(
            "no HTTP when Ready manifests already under packDir",
            started.isEmpty(),
        )
        val plan = LongTripCoordinator.currentPlan()!!
        for (id in regions) {
            assertEquals(
                "packs without place-index must be Installed, not Downloading",
                LongTripCoordinator.State.Installed,
                plan.states[id],
            )
        }
        assertTrue(LongTripCoordinator.corridorPacksReady())
        assertFalse(
            "reuse Installed packs must still wait for Indexed before planning",
            LongTripCoordinator.corridorReadyForPlanning(),
        )
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
