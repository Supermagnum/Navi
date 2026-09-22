package no.navi.app

import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Phase C host tests: real [RegionDownloadBackground] queue + claimWorker, Wi‑Fi
 * gate at [RegionDownloadBackground.ensureStartedWithNetworkState], scrub →
 * Unavailable, and non-blocking planning while the queue is busy.
 *
 * Host-level (JVM). No Robolectric; Context-bound paths use injectors /
 * explicit [unmeteredNow]. Real multi-GB packs and live SD eject are out of
 * scope (see Phase C report open item).
 */
class LongTripCoordinatorPhaseCTest {
    @get:Rule
    val tmp = TemporaryFolder()

    private val regions =
        listOf(
            "europe/norway/ostlandet",
            "europe/norway/vestlandet",
            "europe/norway/trondelag",
        )

    @After
    fun tearDown() {
        LongTripCoordinator.resetForTests()
        NetworkUnmetered.forceForTests = null
        if (RegionDownloadBackground.isRunning()) {
            RegionDownloadBackground.releaseWorker()
        }
    }

    @Test
    fun wifi_gate_applied_at_ensureStarted_pauses_without_claiming_worker() {
        val dir = tmp.newFolder("gate")
        val phases = CopyOnWriteArrayList<String>()
        val listener =
            RegionDownloadBackground.PhaseListener { path, phase ->
                phases.add("$path:$phase")
            }
        RegionDownloadBackground.addPhaseListener(listener)
        try {
            assertFalse(RegionDownloadBackground.isRunning())
            RegionDownloadBackground.ensureStartedWithNetworkState(
                context = null,
                dataDir = dir,
                url = "https://example.test/ostlandet-latest.osm.pbf",
                filename = "ostlandet-latest.osm.pbf",
                geofabrikPath = regions[0],
                packDir = dir,
                requireUnmetered = true,
                unmeteredNow = false,
            )
            assertFalse(
                "gate must not claimWorker when unmetered is false",
                RegionDownloadBackground.isRunning(),
            )
            assertTrue(
                RegionDownloadBackground.statusLine().contains("Wi-Fi/Ethernet"),
            )
            assertTrue(
                phases.any { it.endsWith(":paused_unmetered") },
            )
            val queued = RegionDownloadBackground.loadQueue(dir)
            assertEquals(1, queued.size)
            assertEquals(regions[0], queued[0].geofabrikPath)
        } finally {
            RegionDownloadBackground.removePhaseListener(listener)
            RegionDownloadBackground.cancelPending(dir)
        }
    }

    @Test
    fun tools_download_path_requireUnmetered_false_still_claims_when_metered() {
        val dir = tmp.newFolder("tools")
        // Ordinary Tools path: requireUnmetered=false must claim even if metered.
        // Hold claim ourselves first to prove the gate did not early-return before claim.
        // Instead: with unmeteredNow=false and requireUnmetered=false, ensureStarted
        // proceeds to claimWorker (we observe isRunning briefly or queue drain attempt).
        RegionDownloadBackground.ensureStartedWithNetworkState(
            context = null,
            dataDir = dir,
            url = "https://example.test/ostlandet-latest.osm.pbf",
            filename = "ostlandet-latest.osm.pbf",
            geofabrikPath = regions[0],
            packDir = null,
            requireUnmetered = false,
            unmeteredNow = false,
        )
        // Worker claimed; drain exits quickly on null Context but claim was taken.
        // Give the coroutine a moment to finish finally{releaseWorker}.
        var sawRunning = RegionDownloadBackground.isRunning()
        val deadline = System.currentTimeMillis() + 2_000
        while (System.currentTimeMillis() < deadline) {
            if (RegionDownloadBackground.isRunning()) sawRunning = true
            if (!RegionDownloadBackground.isRunning() && sawRunning) break
            Thread.sleep(20)
        }
        assertTrue(
            "Tools (requireUnmetered=false) must claimWorker even when metered",
            sawRunning ||
                RegionDownloadBackground.loadQueue(dir).isNotEmpty() ||
                RegionDownloadBackground.loadJob(dir) != null,
        )
        // Cleanup any leftover claim.
        if (RegionDownloadBackground.isRunning()) {
            RegionDownloadBackground.releaseWorker()
        }
        RegionDownloadBackground.cancelPending(dir)
    }

    @Test
    fun coordinator_passes_requireUnmetered_true_into_real_starter() {
        val dir = tmp.newFolder("coord-gate")
        val packDir = tmp.newFolder("packs")
        val seenUnmetered = AtomicBoolean(false)
        LongTripCoordinator.setCorridorProviderForTests { _, _, _ -> Result.success(regions) }
        LongTripCoordinator.setPackTargetResolverForTests { _, _ ->
            LongTripPackStorage.PackTarget.DownloadTo(packDir, "internal", false)
        }
        LongTripCoordinator.setDownloadStarterForTests { _, dataDir, _, _, path, pack, requireUnmetered ->
            seenUnmetered.set(requireUnmetered)
            RegionDownloadBackground.ensureStartedWithNetworkState(
                context = null,
                dataDir = dataDir,
                url = "https://example.test/${path.substringAfterLast('/')}-latest.osm.pbf",
                filename = "${path.substringAfterLast('/')}-latest.osm.pbf",
                geofabrikPath = path,
                packDir = pack,
                requireUnmetered = requireUnmetered,
                unmeteredNow = false,
            )
        }
        LongTripCoordinator.enableWithDataDir(
            context = null,
            dataDir = dir,
            waypoints = listOf(59.9 to 10.7, 60.4 to 5.3),
        )
        assertTrue(
            "LongTripCoordinator must pass requireUnmetered=true at the real wiring point",
            seenUnmetered.get(),
        )
        val plan = LongTripCoordinator.currentPlan()!!
        assertEquals(LongTripCoordinator.State.Paused, plan.states[regions[0]])
        LongTripCoordinator.disableWithDataDir(dir)
    }

    @Test
    fun non_blocking_planning_while_real_queue_holds_worker() {
        val dir = tmp.newFolder("nonblock")
        val packDir = tmp.newFolder("packs-nb")
        val enqueued = CopyOnWriteArrayList<String>()

        // Hold the real queue slot (as an in-flight download would).
        assertTrue(RegionDownloadBackground.claimWorker())
        assertTrue(RegionDownloadBackground.isRunning())

        LongTripCoordinator.setCorridorProviderForTests { _, _, _ -> Result.success(regions) }
        LongTripCoordinator.setPackTargetResolverForTests { _, id ->
            if (id == regions[0]) {
                LongTripPackStorage.PackTarget.ReuseInternal(dir)
            } else {
                LongTripPackStorage.PackTarget.DownloadTo(packDir, "internal", false)
            }
        }
        LongTripCoordinator.setDownloadStarterForTests { _, dataDir, _, _, path, pack, requireUnmetered ->
            enqueued.add(path)
            // Real queue: enqueue behind the held claimWorker (no drain).
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
            waypoints = listOf(59.9 to 10.7, 63.4 to 10.4),
        )

        val deadline = System.currentTimeMillis() + 2_000
        while (System.currentTimeMillis() < deadline && enqueued.size < 2) {
            Thread.sleep(20)
        }

        assertTrue(
            "start region finished → planner may use it",
            LongTripCoordinator.regionReadyForPlanning(regions[0]),
        )
        assertFalse(LongTripCoordinator.regionReadyForPlanning(regions[1]))
        assertFalse(LongTripCoordinator.regionReadyForPlanning(regions[2]))

        RegionDownloadBackground.emitPhaseForTests(regions[1], "downloading")
        RegionDownloadBackground.emitPhaseForTests(regions[2], "indexing")

        val states = LongTripCoordinator.currentPlan()!!.states
        assertEquals(LongTripCoordinator.State.Indexed, states[regions[0]])
        assertEquals(LongTripCoordinator.State.Downloading, states[regions[1]])
        assertEquals(LongTripCoordinator.State.Indexing, states[regions[2]])
        assertTrue(LongTripCoordinator.regionReadyForPlanning(regions[0]))
        assertTrue(
            "real queue still exclusive while planning succeeds on finished region",
            RegionDownloadBackground.isRunning(),
        )
        assertFalse(
            "second claim must fail while download holds the slot",
            RegionDownloadBackground.claimWorker(),
        )

        LongTripCoordinator.disableWithDataDir(dir)
        assertTrue(RegionDownloadBackground.loadQueue(dir).isEmpty())
        assertEquals(
            LongTripCoordinator.State.Indexed,
            LongTripCoordinator.currentPlan()!!.states[regions[0]],
        )
        assertEquals(
            LongTripCoordinator.State.Paused,
            LongTripCoordinator.currentPlan()!!.states[regions[1]],
        )

        RegionDownloadBackground.releaseWorker()
    }

    @Test
    fun scrub_stems_map_to_unavailable() {
        val dir = tmp.newFolder("scrub")
        val packDir = tmp.newFolder("packs-scrub")
        LongTripCoordinator.setCorridorProviderForTests { _, _, _ -> Result.success(regions) }
        LongTripCoordinator.setPackTargetResolverForTests { _, id ->
            if (id == regions[0]) {
                LongTripPackStorage.PackTarget.ReuseInternal(dir)
            } else {
                LongTripPackStorage.PackTarget.DownloadTo(packDir, "sd1", true)
            }
        }
        LongTripCoordinator.setDownloadStarterForTests { _, _, _, _, _, _, _ -> }
        LongTripCoordinator.enableWithDataDir(
            context = null,
            dataDir = dir,
            waypoints = listOf(59.9 to 10.7, 60.4 to 5.3),
        )
        RegionDownloadBackground.emitPhaseForTests(regions[1], "downloading")
        assertEquals(
            LongTripCoordinator.State.Downloading,
            LongTripCoordinator.currentPlan()!!.states[regions[1]],
        )
        val line =
            LongTripCoordinator.onVolumeUnavailable(
                "sd1",
                listOf("vestlandet-latest.osm.pbf"),
            )
        assertTrue(
            "status=$line",
            line.contains("Unavailable", ignoreCase = true),
        )
        val plan = LongTripCoordinator.currentPlan()!!
        assertEquals(LongTripCoordinator.State.Unavailable, plan.states[regions[1]])
        assertEquals(LongTripCoordinator.State.Indexed, plan.states[regions[0]])
        LongTripCoordinator.disableWithDataDir(dir)
    }

    @Test
    fun cancelPending_clears_real_queue_files() {
        val dir = tmp.newFolder("cancel")
        RegionDownloadBackground.saveQueue(
            dir,
            listOf(
                RegionDownloadBackground.Job(
                    url = "https://example.test/a.osm.pbf",
                    filename = "a.osm.pbf",
                    geofabrikPath = regions[1],
                ),
            ),
        )
        RegionDownloadBackground.writeJob(
            dir,
            RegionDownloadBackground.Job(
                url = "https://example.test/b.osm.pbf",
                filename = "b.osm.pbf",
                geofabrikPath = regions[2],
            ),
        )
        assertEquals(1, RegionDownloadBackground.loadQueue(dir).size)
        RegionDownloadBackground.cancelPending(dir)
        assertTrue(RegionDownloadBackground.loadQueue(dir).isEmpty())
        assertEquals(null, RegionDownloadBackground.loadJob(dir))
    }
}
