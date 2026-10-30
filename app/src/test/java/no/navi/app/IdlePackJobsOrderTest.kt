package no.navi.app

import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

class IdlePackJobsOrderTest {
    @get:Rule
    val tmp = TemporaryFolder()

    @After
    fun tearDown() {
        IdlePackJobs.executeJobs = false
        IdlePackJobs.testRunJob = { false }
        val deadline = System.currentTimeMillis() + 5_000
        while (IdlePackJobs.isRunning() && System.currentTimeMillis() < deadline) {
            Thread.sleep(20)
        }
        IdlePackJobs.resetForTests()
    }

    @Test
    fun place_index_runs_before_skeleton_when_both_queued() {
        IdlePackJobs.resetForTests()
        // Queue both before the drain starts so pollNext preference is tested.
        IdlePackJobs.executeJobs = false
        val order = CopyOnWriteArrayList<IdlePackJobs.Kind>()
        val done = CountDownLatch(2)
        IdlePackJobs.testRunJob = { job ->
            order.add(job.kind)
            done.countDown()
            false
        }
        val packDir = tmp.newFolder("packs")
        val pbf =
            File(packDir, "ostlandet-latest.osm.pbf").also { f ->
                java.io.RandomAccessFile(f, "rw").use { it.setLength(RegionDownloadBackground.MIN_PBF_BYTES) }
            }
        File(packDir, "ostlandet-latest.navi-manifest.json").writeText("{}")
        File(packDir, "ostlandet-latest.navi-server-install.json").writeText(
            """{"schema":1,"region_id":"europe/norway/ostlandet","generation":"g1"}""",
        )
        // Scratch DB path — must not match the product place_index.db or
        // mayIndexRegion refuses without an InstalledMaps snapshot.
        val db = File(tmp.newFolder("scratch-index"), "place_index.db")
        // Offer skeleton first; poll order must still prefer place index.
        IdlePackJobs.offerSkeleton(packDir, "ostlandet-latest")
        IdlePackJobs.offerPlaceIndex(pbf, db, "europe/norway/ostlandet")
        IdlePackJobs.executeJobs = true
        IdlePackJobs.onAppIdle()
        assertTrue(
            "both jobs must run; got order=$order scheduled=${IdlePackJobs.scheduledForTest()}",
            done.await(4, TimeUnit.SECONDS),
        )
        assertEquals(
            listOf(IdlePackJobs.Kind.PLACE_INDEX, IdlePackJobs.Kind.SKELETON),
            order.toList(),
        )
    }
}
