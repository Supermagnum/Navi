package no.navi.app

import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

/**
 * Task E: for a known multi-region long-trip queue, each phase transition's
 * displayed status string contains the correct region name (not only N-of-M).
 */
class LongTripRegionAwareProgressTest {
    @get:Rule
    val tmp = TemporaryFolder()

    private val regions =
        listOf(
            "europe/norway/ostlandet",
            "europe/sweden/vastra_gotaland",
            "europe/denmark",
            "europe/germany/schleswig-holstein",
        )

    @After
    fun tearDown() {
        LongTripCoordinator.resetForTests()
        if (RegionDownloadBackground.isRunning()) {
            RegionDownloadBackground.releaseWorker()
        }
    }

    @Test
    fun phase_transitions_include_region_name_and_n_of_m() {
        val dir = tmp.newFolder("region-aware")
        val packDir = tmp.newFolder("packs-ra")

        LongTripCoordinator.setCorridorProviderForTests { _, _, _ -> Result.success(regions) }
        LongTripCoordinator.setPackTargetResolverForTests { _, _ ->
            LongTripPackStorage.PackTarget.DownloadTo(packDir, "internal", false)
        }
        LongTripCoordinator.setDownloadStarterForTests { _, dataDir, _, _, path, pack, requireUnmetered ->
            RegionDownloadBackground.ensureStartedWithNetworkState(
                context = null,
                dataDir = dataDir,
                url = "https://example.test/${path.substringAfterLast('/')}-latest.osm.pbf",
                filename = "${path.substringAfterLast('/')}-latest.osm.pbf",
                geofabrikPath = path,
                packDir = pack,
                requireUnmetered = requireUnmetered,
                unmeteredNow = true,
            )
        }

        LongTripCoordinator.enableWithDataDir(
            context = null,
            dataDir = dir,
            waypoints = listOf(60.79 to 11.08, 52.28 to 8.92),
        )

        val expectedNames =
            regions.map { id ->
                RegionCoverage.displayName(id)
            }

        fun assertLineNamesRegion(
            line: String,
            regionIndex: Int,
            phaseToken: String,
        ) {
            val name = expectedNames[regionIndex]
            val n = regionIndex + 1
            val total = regions.size
            assertTrue(
                "status must mention region $n of $total name=$name; got: $line",
                line.contains("$n of $total") &&
                    (
                        line.contains(name, ignoreCase = true) ||
                            line.contains(
                                regions[regionIndex].substringAfterLast('/'),
                                ignoreCase = true,
                            )
                    ),
            )
            assertTrue(
                "status must reflect phase $phaseToken; got: $line",
                line.contains(phaseToken, ignoreCase = true),
            )
        }

        // Needed / queued initial line: every region named with N-of-M.
        val initial = LongTripCoordinator.statusLine()
        for (i in regions.indices) {
            assertTrue(
                "initial status missing ${expectedNames[i]} ($i of ${regions.size}): $initial",
                initial.contains("${i + 1} of ${regions.size}") &&
                    (
                        initial.contains(expectedNames[i], ignoreCase = true) ||
                            initial.contains(
                                regions[i].substringAfterLast('/'),
                                ignoreCase = true,
                            )
                    ),
            )
        }

        RegionDownloadBackground.emitPhaseForTests(regions[0], "queued")
        assertLineNamesRegion(LongTripCoordinator.statusLine(), 0, "Queued")

        RegionDownloadBackground.emitPhaseForTests(regions[0], "downloading")
        assertLineNamesRegion(LongTripCoordinator.statusLine(), 0, "Downloading")

        RegionDownloadBackground.emitPhaseForTests(regions[0], "indexing")
        assertLineNamesRegion(LongTripCoordinator.statusLine(), 0, "Indexing")

        RegionDownloadBackground.emitPhaseForTests(regions[0], "indexed")
        assertLineNamesRegion(LongTripCoordinator.statusLine(), 0, "Indexed")

        RegionDownloadBackground.emitPhaseForTests(regions[1], "downloading")
        assertLineNamesRegion(LongTripCoordinator.statusLine(), 1, "Downloading")
        // Corridor position 2 of 4 must still name Västra Götaland (not only "2 of 4").
        val mid = LongTripCoordinator.statusLine()
        assertTrue(mid.contains("2 of 4"))
        assertTrue(
            mid.contains("Västra", ignoreCase = true) ||
                mid.contains("Götaland", ignoreCase = true) ||
                mid.contains("vastra", ignoreCase = true),
        )

        RegionDownloadBackground.emitPhaseForTests(regions[2], "paused_unmetered")
        assertLineNamesRegion(LongTripCoordinator.statusLine(), 2, "Paused")

        RegionDownloadBackground.emitPhaseForTests(regions[3], "failed")
        assertLineNamesRegion(LongTripCoordinator.statusLine(), 3, "Failed")

        val downloadLine =
            RegionProgressMessages.phaseForRegion(
                "Downloading",
                regions[1],
                index = 2,
                total = 4,
            )
        assertTrue(downloadLine.contains("2 of 4"))
        assertTrue(
            downloadLine.contains("Västra", ignoreCase = true) ||
                downloadLine.contains("vastra", ignoreCase = true),
        )

        assertEquals(4, regions.size)
        LongTripCoordinator.disableWithDataDir(dir)
    }
}
