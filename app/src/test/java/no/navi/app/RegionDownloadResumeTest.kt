package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

/** Phase-aware region-download.json rediscovery (Part 3 launch resume). */
class RegionDownloadResumeTest {
    @get:Rule
    val tmp = TemporaryFolder()

    @Test
    fun phase_parse_accepts_wire_and_aliases() {
        assertEquals(RegionDownloadBackground.Phase.PACKS, RegionDownloadBackground.Phase.parse(null))
        assertEquals(RegionDownloadBackground.Phase.PACKS, RegionDownloadBackground.Phase.parse("packs"))
        assertEquals(
            RegionDownloadBackground.Phase.PLACE_INDEX,
            RegionDownloadBackground.Phase.parse("place_index"),
        )
        assertEquals(
            RegionDownloadBackground.Phase.BASEMAP,
            RegionDownloadBackground.Phase.parse("basemap"),
        )
    }

    @Test
    fun write_load_roundtrip_preserves_phase() {
        val dir = tmp.newFolder("data")
        val job =
            RegionDownloadBackground.Job(
                url = "https://example.test/ostlandet-latest.osm.pbf",
                filename = "ostlandet-latest.osm.pbf",
                geofabrikPath = "europe/norway/ostlandet",
                phase = RegionDownloadBackground.Phase.PLACE_INDEX,
            )
        RegionDownloadBackground.writeJob(dir, job)
        val loaded = RegionDownloadBackground.loadJob(dir)
        assertNotNull(loaded)
        assertEquals(job.url, loaded!!.url)
        assertEquals(job.filename, loaded.filename)
        assertEquals(job.geofabrikPath, loaded.geofabrikPath)
        assertEquals(RegionDownloadBackground.Phase.PLACE_INDEX, loaded.phase)
    }

    @Test
    fun discoverPending_keeps_job_when_pbf_exists_but_phase_incomplete() {
        val dir = tmp.newFolder("data")
        File(dir, "ostlandet-latest.osm.pbf").writeText("x".repeat(2_000_000))
        RegionDownloadBackground.writeJob(
            dir,
            RegionDownloadBackground.Job(
                url = "https://example.test/ostlandet-latest.osm.pbf",
                filename = "ostlandet-latest.osm.pbf",
                geofabrikPath = "europe/norway/ostlandet",
                phase = RegionDownloadBackground.Phase.PLACE_INDEX,
            ),
        )
        val pending = RegionDownloadBackground.discoverPending(dir)
        assertNotNull("completed PBF must not clear an incomplete place_index job", pending)
        assertEquals(RegionDownloadBackground.Phase.PLACE_INDEX, pending!!.phase)
    }

    @Test
    fun discoverPending_advances_packs_to_place_index_when_manifest_present() {
        val dir = tmp.newFolder("data")
        File(dir, "ostlandet-latest.navi-manifest.json").writeText("{}")
        RegionDownloadBackground.writeJob(
            dir,
            RegionDownloadBackground.Job(
                url = "https://example.test/ostlandet-latest.osm.pbf",
                filename = "ostlandet-latest.osm.pbf",
                geofabrikPath = "europe/norway/ostlandet",
                phase = RegionDownloadBackground.Phase.PACKS,
            ),
        )
        val pending = RegionDownloadBackground.discoverPending(dir)
        assertNotNull(pending)
        assertEquals(RegionDownloadBackground.Phase.PLACE_INDEX, pending!!.phase)
        assertEquals(
            RegionDownloadBackground.Phase.PLACE_INDEX,
            RegionDownloadBackground.loadJob(dir)!!.phase,
        )
    }

    @Test
    fun discoverIncompleteForPath_starts_place_index_when_packs_ready() {
        val dir = tmp.newFolder("data")
        File(dir, "ostlandet-latest.navi-manifest.json").writeText("{}")
        val job =
            RegionDownloadBackground.discoverIncompleteForPath(
                dir,
                "europe/norway/ostlandet",
            )
        assertNotNull(job)
        assertEquals(RegionDownloadBackground.Phase.PLACE_INDEX, job!!.phase)
        assertEquals("europe/norway/ostlandet", job.geofabrikPath)
    }

    @Test
    fun discoverIncompleteForPath_null_when_nothing_installed() {
        val dir = tmp.newFolder("data")
        assertNull(
            RegionDownloadBackground.discoverIncompleteForPath(
                dir,
                "europe/norway/ostlandet",
            ),
        )
    }

    @Test
    fun placeIndexLooksReady_false_for_missing_or_tiny_db() {
        val dir = tmp.newFolder("data")
        assertFalse(
            RegionDownloadBackground.placeIndexLooksReady(dir, "europe/norway/ostlandet"),
        )
        File(dir, "place_index.db").writeBytes(ByteArray(100))
        assertFalse(
            RegionDownloadBackground.placeIndexLooksReady(dir, "europe/norway/ostlandet"),
        )
    }

    @Test
    fun usable_status_prefix_unchanged() {
        assertTrue(
            RegionDownloadBackground.USABLE_STATUS_PREFIX.startsWith("Place index ready"),
        )
    }
}
