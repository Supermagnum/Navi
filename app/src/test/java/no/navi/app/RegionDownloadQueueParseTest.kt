package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

/**
 * Regression: hand-rolled brace scanning duplicated a one-entry queue, so
 * drainQueue re-ran the same region forever after the first completion.
 */
class RegionDownloadQueueParseTest {
    @get:Rule
    val tmp = TemporaryFolder()

    private fun job(
        path: String,
        phase: RegionDownloadBackground.Phase = RegionDownloadBackground.Phase.PACKS,
    ) = RegionDownloadBackground.Job(
        url = "https://example.test/${path.substringAfterLast('/')}-latest.osm.pbf",
        filename = "${path.substringAfterLast('/')}-latest.osm.pbf",
        geofabrikPath = path,
        phase = phase,
    )

    @Test
    fun one_entry_queue_loads_once_and_pop_empties_without_redrain() {
        val dir = tmp.newFolder("data")
        // Exact on-disk shape that previously duplicated via indexOf('{').
        File(dir, RegionDownloadBackground.QUEUE_FILE).writeText(
            """[{"url":"https://example.test/ostlandet-latest.osm.pbf","filename":"ostlandet-latest.osm.pbf","geofabrikPath":"europe/norway/ostlandet","phase":"packs"}]""",
        )
        val loaded = RegionDownloadBackground.loadQueue(dir)
        assertEquals(1, loaded.size)
        assertEquals("europe/norway/ostlandet", loaded[0].geofabrikPath)

        val first = RegionDownloadBackground.popQueue(dir)
        assertEquals("europe/norway/ostlandet", first!!.geofabrikPath)
        assertTrue(RegionDownloadBackground.loadQueue(dir).isEmpty())
        assertNull(
            "must not drain the same region a second time",
            RegionDownloadBackground.popQueue(dir),
        )
    }

    @Test
    fun multi_entry_queue_preserves_order_and_pops_each_path_once() {
        val dir = tmp.newFolder("multi")
        RegionDownloadBackground.saveQueue(
            dir,
            listOf(
                job("europe/norway/ostlandet"),
                job("europe/norway/vestlandet"),
                job("europe/norway/nordland"),
            ),
        )
        val loaded = RegionDownloadBackground.loadQueue(dir)
        assertEquals(3, loaded.size)
        assertEquals(
            listOf(
                "europe/norway/ostlandet",
                "europe/norway/vestlandet",
                "europe/norway/nordland",
            ),
            loaded.map { it.geofabrikPath },
        )

        val drained = mutableListOf<String>()
        while (true) {
            val next = RegionDownloadBackground.popQueue(dir) ?: break
            drained.add(next.geofabrikPath)
        }
        assertEquals(
            listOf(
                "europe/norway/ostlandet",
                "europe/norway/vestlandet",
                "europe/norway/nordland",
            ),
            drained,
        )
        assertTrue(RegionDownloadBackground.loadQueue(dir).isEmpty())
    }

    @Test
    fun save_and_load_dedupe_same_geofabrik_path() {
        val dir = tmp.newFolder("dedupe")
        val dup =
            listOf(
                job("europe/norway/ostlandet"),
                job("europe/norway/ostlandet", RegionDownloadBackground.Phase.BASEMAP),
                job("europe/norway/vestlandet"),
            )
        RegionDownloadBackground.saveQueue(dir, dup)
        val loaded = RegionDownloadBackground.loadQueue(dir)
        assertEquals(2, loaded.size)
        assertEquals("europe/norway/ostlandet", loaded[0].geofabrikPath)
        assertEquals(RegionDownloadBackground.Phase.PACKS, loaded[0].phase)
        assertEquals("europe/norway/vestlandet", loaded[1].geofabrikPath)

        // Legacy duplicated array on disk must also collapse on load.
        File(dir, RegionDownloadBackground.QUEUE_FILE).writeText(
            """[
              {"url":"u1","filename":"f1","geofabrikPath":"europe/norway/ostlandet","phase":"packs"},
              {"url":"u2","filename":"f2","geofabrikPath":"europe/norway/ostlandet","phase":"basemap"}
            ]""",
        )
        assertEquals(1, RegionDownloadBackground.loadQueue(dir).size)
    }
}
