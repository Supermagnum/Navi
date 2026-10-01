package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

class DownloadedRegionDeleteTest {
    @get:Rule
    val tmp = TemporaryFolder()

    private val region = "europe/norway/ostlandet"
    private val stem = "ostlandet-latest"
    private val regionKey = "europe_norway_ostlandet"

    private fun seedInstalled(dir: File): Long {
        var bytes = 0L

        fun put(
            name: String,
            size: Int,
        ): File {
            val f = File(dir, name)
            f.parentFile?.mkdirs()
            f.writeBytes(ByteArray(size) { 1 })
            bytes += size.toLong()
            return f
        }
        put("$stem.navi-manifest.json", 128)
        put("$stem.osm.pbf", 2_000)
        put("$stem.navi-graph-car.rkyv", 4_000)
        put("$stem.navi-poi-barrier.rkyv", 1_500)
        put("pmtiles/$regionKey.pmtiles", 3_000)
        put("pmtiles/${regionKey}_dem.pmtiles", 2_500)
        File(dir, "graph-cache-$stem-car").mkdirs()
        File(dir, "graph-cache-$stem-car/entry.bin").writeBytes(ByteArray(800))
        bytes += 800
        PlaceIndexReady.readyFile(dir).writeText("""["$region"]""")
        RegionDownloadBackground.writeJob(
            dir,
            RegionDownloadBackground.Job(
                url = "https://example.test/$stem.osm.pbf",
                filename = "$stem.osm.pbf",
                geofabrikPath = region,
                phase = RegionDownloadBackground.Phase.PACKS,
            ),
        )
        return bytes
    }

    @Test
    fun delete_frees_pack_pmtiles_cache_and_clears_ready_stamp() {
        val dir = tmp.newFolder("data")
        val before = seedInstalled(dir)
        assertTrue(PackRegionAvailability.localBakeReady(dir, region))
        assertTrue(PackRegionAvailability.localPmtilesReady(dir, region))
        assertTrue(PlaceIndexReady.isReady(dir, region))

        val result = DownloadedRegionDelete.delete(context = null, dataDir = dir, geofabrikPath = region)
        assertTrue(result.message, result.ok)
        assertTrue("expected files removed", result.filesRemoved >= 5)
        assertTrue("expected bytes freed > 0", result.bytesFreed > 0L)
        assertTrue(result.bytesFreed <= before + 4096)

        assertFalse(PackRegionAvailability.localBakeReady(dir, region))
        assertFalse(PackRegionAvailability.localPmtilesReady(dir, region))
        assertFalse(PackRegionAvailability.localInstalledReady(dir, region))
        assertFalse(PlaceIndexReady.isReady(dir, region))
        assertFalse(File(dir, "$stem.navi-manifest.json").exists())
        assertFalse(File(dir, "$stem.osm.pbf").exists())
        assertFalse(File(dir, "pmtiles/$regionKey.pmtiles").exists())
        assertFalse(File(dir, "pmtiles/${regionKey}_dem.pmtiles").exists())
        assertFalse(File(dir, "graph-cache-$stem-car").exists())
        assertFalse(RegionDownloadBackground.jobFile(dir).exists())
    }

    @Test
    fun delete_also_cleans_redirected_long_trip_packs() {
        val internal = tmp.newFolder("internal")
        val redirect = File(internal, LongTripPackStorage.PACKS_SUBDIR).also { it.mkdirs() }
        File(redirect, "$stem.navi-manifest.json").writeText("{}")
        File(redirect, "$stem.navi-graph-car.rkyv").writeBytes(ByteArray(1024))
        PlaceIndexReady.readyFile(internal).writeText("""["$region"]""")

        val result =
            DownloadedRegionDelete.delete(
                context = null,
                dataDir = internal,
                geofabrikPath = region,
            )
        assertTrue(result.ok)
        assertFalse(File(redirect, "$stem.navi-manifest.json").exists())
        assertFalse(File(redirect, "$stem.navi-graph-car.rkyv").exists())
        assertFalse(PlaceIndexReady.isReady(internal, region))
    }

    @Test
    fun after_delete_region_is_re_downloadable_clean_slate() {
        val dir = tmp.newFolder("redownload")
        seedInstalled(dir)
        val first = DownloadedRegionDelete.delete(null, dir, region)
        assertTrue(first.ok)

        // Simulate a fresh download landing again — no ghost stamp/job.
        File(dir, "$stem.navi-manifest.json").writeText("{}")
        File(dir, "pmtiles").mkdirs()
        File(dir, "pmtiles/$regionKey.pmtiles").writeText("stub")
        assertTrue(PackRegionAvailability.localInstalledReady(dir, region))
        assertFalse(
            "ready stamp must not resurrect without markReady",
            PlaceIndexReady.isReady(dir, region),
        )
        assertFalse(RegionDownloadBackground.jobFile(dir).exists())
    }

    @Test
    fun sd_eject_scrub_does_not_clear_place_index_ready() {
        // Regression vs user delete: Unavailable scrub must leave internal
        // place-index bookkeeping alone.
        val packs = tmp.newFolder("sd-packs")
        File(packs, "$stem.osm.pbf.partial").writeText("partial")
        File(packs, "$stem.osm.pbf").writeText("trunc")
        val internal = tmp.newFolder("internal-pi")
        PlaceIndexReady.readyFile(internal).writeText("""["$region"]""")
        File(internal, "$stem.navi-manifest.json").writeText("{}")

        val stems = LongTripPackStorage.scrubIncompletePacks(packs)
        assertTrue(stems.any { it.contains("ostlandet") })
        assertTrue(
            "eject scrub must not wipe place-index ready stamp",
            PlaceIndexReady.isReady(internal, region),
        )
        assertTrue(
            "eject scrub must not delete installed internal packs",
            File(internal, "$stem.navi-manifest.json").isFile,
        )
    }

    @Test
    fun block_when_nothing_installed() {
        val dir = tmp.newFolder("empty")
        val reason = DownloadedRegionDelete.blockReason(region, dir)
        assertTrue(reason!!.contains("Nothing installed"))
        val result = DownloadedRegionDelete.delete(null, dir, region)
        assertFalse(result.ok)
        assertEquals(0, result.filesRemoved)
    }

    @Test
    fun block_reason_sees_sd_pack_dir_and_delete_clears_it() {
        val internal = tmp.newFolder("internal-empty")
        val sd = tmp.newFolder("sd-long-trip-packs")
        File(sd, "$stem.navi-manifest.json").writeText("{}")
        File(sd, "$stem.navi-graph-car.rkyv").writeBytes(ByteArray(2048))
        File(sd, ".pack-fetch-$stem.partial").mkdirs()

        assertTrue(
            "SD-only packs must be visible to Tools delete",
            DownloadedRegionDelete.hasAnyInstall(internal, region, listOf(sd)),
        )
        assertEquals(null, DownloadedRegionDelete.blockReason(region, internal, listOf(sd)))

        val result =
            DownloadedRegionDelete.delete(
                context = null,
                dataDir = internal,
                geofabrikPath = region,
                extraPackDirs = listOf(sd),
            )
        assertTrue(result.message, result.ok)
        assertFalse(File(sd, "$stem.navi-manifest.json").exists())
        assertFalse(File(sd, "$stem.navi-graph-car.rkyv").exists())
        assertFalse(File(sd, ".pack-fetch-$stem.partial").exists())
    }

    @Test
    fun leaf_stems_include_catalog_aliases() {
        val stems = DownloadedRegionDelete.leafStems("europe/sweden/vastra-gotaland")
        assertTrue(stems.any { it.contains("vastra") })
        assertTrue(stems.size >= 1)
    }
}
