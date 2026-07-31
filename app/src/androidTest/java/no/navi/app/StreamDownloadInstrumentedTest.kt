package no.navi.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.pmtilesPlanetUrl
import uniffi.navi.pmtilesQueueRegion
import uniffi.navi.pmtilesRunJob
import uniffi.navi.provisionRegionData
import java.io.File

/**
 * Device verification that shared stream-to-disk downloaders work on hardware
 * (same SM-P613 class of checks used for the PMTiles DEM fix).
 */
@RunWith(AndroidJUnit4::class)
class StreamDownloadInstrumentedTest {
    private fun dataDir(): File {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        return NaviAppData.resolve(context)
    }

    @Test
    fun elevation_copernicus_tile_streams_and_completes() {
        val dataDir = dataDir()
        val elevRoot = File(dataDir, "elevation/copernicus")
        elevRoot.mkdirs()

        val target = File(elevRoot, "N60E010")
        if (target.exists()) {
            target.deleteRecursively()
        }
        assertTrue("precondition: tile dir removed", !target.exists())

        val pbfName =
            dataDir
                .listFiles()
                ?.firstOrNull { it.name.endsWith(".osm.pbf") && it.length() > 1_000_000 }
                ?.name
                ?: error("Need an existing OSM PBF under $dataDir")

        val report =
            provisionRegionData(
                dataDir.absolutePath,
                "https://download.geofabrik.de/europe/norway/ostlandet-latest.osm.pbf",
                pbfName,
                null,
            )
        assertTrue("provision should report, got: $report", report.contains("PASS") || report.isNotBlank())
        assertTrue(
            "Copernicus tile N60E010 should be restored by streaming download",
            target.isDirectory && (target.listFiles()?.isNotEmpty() == true),
        )
        val tif = target.listFiles()?.firstOrNull { it.name.endsWith(".tif") }
        assertTrue("expected .tif under $target", tif != null && tif.length() > 1_000_000)
    }

    @Test
    fun region_pbf_streams_when_missing() {
        val dataDir = dataDir()
        val pbf = File(dataDir, "ostlandet-latest.osm.pbf")
        require(pbf.isFile && pbf.length() > 1_000_000) { "missing ostlandet pbf for rename test" }
        val bak = File(dataDir, "ostlandet-latest.osm.pbf.bak-streamtest")
        if (bak.exists()) bak.delete()
        assertTrue(pbf.renameTo(bak))

        try {
            val report =
                provisionRegionData(
                    dataDir.absolutePath,
                    "https://download.geofabrik.de/europe/norway/ostlandet-latest.osm.pbf",
                    "ostlandet-latest.osm.pbf",
                    // Keep DEM from being re-fetched: leave null; tiles already on disk.
                    null,
                )
            assertTrue("provision report: $report", report.isNotBlank())
            assertTrue(pbf.isFile && pbf.length() > 1_000_000)
        } finally {
            if (!pbf.isFile && bak.isFile) {
                bak.renameTo(pbf)
            } else if (bak.isFile) {
                bak.delete()
            }
        }
    }

    @Test
    fun pmtiles_basemap_tiny_oslo_still_completes() {
        val dataDir = dataDir()
        File(dataDir, "pmtiles/test_oslo.pmtiles").delete()
        val job = pmtilesQueueRegion(dataDir.absolutePath, "test/oslo", pmtilesPlanetUrl())
        assertTrue(job.id.isNotBlank())
        val done = pmtilesRunJob(dataDir.absolutePath, job.id)
        assertEquals("completed", done.status)
        assertTrue(File(done.localPath).length() > 1000)
    }
}
