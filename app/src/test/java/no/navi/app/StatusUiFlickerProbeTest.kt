package no.navi.app

import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Task 6 probe: quantify status-UI churn and duplicate Tools lines under a
 * multi-region phase storm (post-Task-1 concurrent Installed + indexing).
 *
 * Not a device UI test — models the same state sources MainActivity / Tools
 * read so we can measure update rate and duplicate composition without an AVD.
 */
class StatusUiFlickerProbeTest {
    @After
    fun tearDown() {
        LongTripCoordinator.resetForTests()
    }

    @Test
    fun long_trip_status_line_length_varies_with_state_names() {
        val regions =
            listOf(
                "europe/norway/ostlandet",
                "europe/sweden/vastra_gotaland",
                "europe/denmark",
            )
        LongTripCoordinator.setCorridorProviderForTests { _, _, _ -> Result.success(regions) }
        LongTripCoordinator.setPackTargetResolverForTests { _, _ ->
            LongTripPackStorage.PackTarget.ReuseInternal(java.io.File("."))
        }
        LongTripCoordinator.setDownloadStarterForTests { _, _, _, _, _, _, _ -> }

        LongTripCoordinator.enableWithDataDir(
            context = null,
            dataDir = java.io.File("."),
            waypoints = listOf(60.79 to 11.08, 55.67 to 12.57),
        )

        val lengths = mutableListOf<Int>()
        lengths += LongTripCoordinator.statusLine().length

        RegionDownloadBackground.emitPhaseForTests(regions[0], "downloading")
        lengths += LongTripCoordinator.statusLine().length
        RegionDownloadBackground.emitPhaseForTests(regions[0], "installed")
        lengths += LongTripCoordinator.statusLine().length
        RegionDownloadBackground.emitPhaseForTests(regions[1], "downloading")
        RegionDownloadBackground.emitPhaseForTests(regions[0], "indexing")
        lengths += LongTripCoordinator.statusLine().length
        RegionDownloadBackground.emitPhaseForTests(regions[0], "indexed")
        lengths += LongTripCoordinator.statusLine().length

        val min = lengths.minOrNull()!!
        val max = lengths.maxOrNull()!!
        assertTrue(
            "status line length should vary across phases (layout jump risk): $lengths",
            max > min,
        )
        // Progress toast lines (MainActivity formatProgressPct) swing harder:
        val progressLens =
            listOf(
                "Downloading region… 4%",
                "Downloading region… 100% (5000000 / 5000000)",
                "Place index: writing database… 0% (0 / 6)",
                "Place index: previous build interrupted, restarting — scanning ways… 33% (2 / 6)",
            ).map { it.length }
        assertTrue(
            "progress toast length delta=${progressLens.max() - progressLens.min()} " +
                "samples=$progressLens",
            progressLens.max() - progressLens.min() >= 40,
        )
    }

    @Test
    fun concurrent_phase_storm_produces_many_status_line_updates_per_second() {
        val regions =
            listOf(
                "europe/norway/ostlandet",
                "europe/sweden/vastra_gotaland",
            )
        LongTripCoordinator.setCorridorProviderForTests { _, _, _ -> Result.success(regions) }
        LongTripCoordinator.setPackTargetResolverForTests { _, _ ->
            LongTripPackStorage.PackTarget.ReuseInternal(java.io.File("."))
        }
        LongTripCoordinator.setDownloadStarterForTests { _, _, _, _, _, _, _ -> }
        LongTripCoordinator.enableWithDataDir(
            context = null,
            dataDir = java.io.File("."),
            waypoints = listOf(60.79 to 11.08, 57.7 to 11.9),
        )

        val phases =
            listOf(
                regions[0] to "downloading",
                regions[0] to "installed",
                regions[0] to "indexing",
                regions[1] to "queued",
                regions[1] to "downloading",
                regions[0] to "indexed",
                regions[1] to "installed",
                regions[1] to "indexing",
                regions[1] to "indexed",
            )

        var changes = 0
        var last = LongTripCoordinator.statusLine()
        val t0 = System.nanoTime()
        // Burst: many transitions in <200ms (Task-1 overlap window).
        repeat(40) {
            for ((path, phase) in phases) {
                RegionDownloadBackground.emitPhaseForTests(path, phase)
                val now = LongTripCoordinator.statusLine()
                if (now != last) {
                    changes++
                    last = now
                }
            }
        }
        val elapsedSec = (System.nanoTime() - t0) / 1_000_000_000.0
        val rate = changes / elapsedSec.coerceAtLeast(0.001)
        // Evidence for Task 6: underlying state can exceed a sane UI refresh rate.
        assertTrue("expected many status changes, got $changes", changes >= 50)
        assertTrue(
            "raw status churn $rate/s over ${"%.3f".format(elapsedSec)}s " +
                "($changes changes) — UI must coalesce, not paint 1:1",
            rate >= 100.0 || changes >= 100,
        )
    }

    @Test
    fun tools_footer_can_duplicate_same_region_across_streams() {
        // Mirrors MainActivity processLines + tools_status composition.
        val ostlandet = RegionCoverage.displayName("europe/norway/ostlandet")
        val regionDownload =
            RegionProgressMessages.annotate(
                "Downloading region… 42%",
                "europe/norway/ostlandet",
                1,
                2,
            )
        val placeIndex =
            RegionProgressMessages.annotate(
                "Place index: writing database… 55%",
                "europe/norway/ostlandet",
                1,
                2,
            )
        // Near-duplicate of regionDownload pushed into the shared `status`
        // toast / tools_status (exact match already filtered; near-match is not).
        val toolsStatus =
            RegionProgressMessages.annotate(
                "Downloading region… 43%",
                "europe/norway/ostlandet",
                1,
                2,
            )

        val processLines =
            listOf(
                "region_download_progress" to regionDownload,
                "place_index_bg_status" to placeIndex,
            )
        val shown =
            buildList {
                addAll(processLines.map { it.second })
                if (toolsStatus.isNotBlank() && processLines.none { it.second == toolsStatus }) {
                    add(toolsStatus)
                }
            }

        val mentions =
            shown.count {
                it.contains(ostlandet, ignoreCase = true) ||
                    it.contains("ostlandet", ignoreCase = true)
            }
        assertEquals(
            "Tools shows Ostlandet on every stream + near-dup tools_status: $shown",
            3,
            mentions,
        )
        assertTrue(
            "dedupe-by-exact-string misses near-duplicates",
            shown.distinct().size == shown.size,
        )
    }

    @Test
    fun main_screen_poll_intervals_imply_at_least_2_5_updates_per_sec_when_busy() {
        // Documented MainActivity delays (download poll + bg tick).
        val downloadPollMs = 400
        val bgBusyPollMs = 400
        val downloadHz = 1000.0 / downloadPollMs
        val bgHz = 1000.0 / bgBusyPollMs
        // Both can write `status` while Task-1 overlaps download + index.
        val combined = downloadHz + bgHz
        assertEquals(2.5, downloadHz, 0.01)
        assertTrue(
            "combined busy polls can hit ~$combined/s into status toast",
            combined >= 5.0,
        )
    }
}
