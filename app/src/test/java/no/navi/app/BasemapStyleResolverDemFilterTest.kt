package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import uniffi.navi.FfiPmtilesJob
import java.io.File

/**
 * Regression: a completed DEM job newer than the vector extract must not win
 * basemap style selection ([BasemapStyleResolver.selectVectorCoveringJob]).
 */
class BasemapStyleResolverDemFilterTest {
    @get:Rule
    val tmp = TemporaryFolder()

    @Test
    fun isDemArchive_detects_region_key_and_path() {
        assertTrue(BasemapStyleResolver.isDemArchive("europe_norway_ostlandet_dem"))
        assertTrue(
            BasemapStyleResolver.isDemArchive(
                "europe_norway_ostlandet",
                "/data/pmtiles/europe_norway_ostlandet_dem.pmtiles",
            ),
        )
        assertFalse(BasemapStyleResolver.isDemArchive("europe_norway_ostlandet"))
        assertFalse(
            BasemapStyleResolver.isDemArchive(
                "europe_norway_ostlandet",
                "/data/pmtiles/europe_norway_ostlandet.pmtiles",
            ),
        )
    }

    @Test
    fun selectVectorCoveringJob_prefers_vector_when_dem_is_newer() {
        val dir = tmp.newFolder("pmtiles")
        val vector = File(dir, "europe_norway_ostlandet.pmtiles").also { it.writeText("vector") }
        val dem = File(dir, "europe_norway_ostlandet_dem.pmtiles").also { it.writeText("dem") }
        // DEM first — same order as pmtiles_jobs ORDER BY created_at DESC after DEM completes.
        val jobs =
            listOf(
                job(
                    id = "dem-newer",
                    regionKey = "europe_norway_ostlandet_dem",
                    localPath = dem.absolutePath,
                ),
                job(
                    id = "vector-older",
                    regionKey = "europe_norway_ostlandet",
                    localPath = vector.absolutePath,
                ),
            )
        val picked = BasemapStyleResolver.selectVectorCoveringJob(jobs)
        assertEquals("vector-older", picked?.id)
        assertEquals(vector.absolutePath, picked?.localPath)
        assertFalse(BasemapStyleResolver.isDemArchive(picked!!.regionKey, picked.localPath))
    }

    @Test
    fun selectVectorCoveringJob_skips_dem_only_cover() {
        val dir = tmp.newFolder("pmtiles-dem-only")
        val dem = File(dir, "europe_norway_ostlandet_dem.pmtiles").also { it.writeText("dem") }
        val jobs =
            listOf(
                job(
                    id = "dem-only",
                    regionKey = "europe_norway_ostlandet_dem",
                    localPath = dem.absolutePath,
                ),
            )
        assertNull(BasemapStyleResolver.selectVectorCoveringJob(jobs))
    }

    @Test
    fun localDemBesideBasemap_null_when_path_already_dem() {
        val dir = tmp.newFolder("beside")
        val dem = File(dir, "europe_norway_ostlandet_dem.pmtiles").also { it.writeBytes(ByteArray(2000)) }
        assertNull(MapterhornTerrain.localDemBesideBasemap(dem.absolutePath))
        // No double-suffix file is created or required.
        assertFalse(File(dir, "europe_norway_ostlandet_dem_dem.pmtiles").exists())
    }

    @Test
    fun localDemBesideBasemap_finds_sibling_of_vector() {
        val dir = tmp.newFolder("beside-ok")
        val vector = File(dir, "europe_norway_ostlandet.pmtiles").also { it.writeBytes(ByteArray(2000)) }
        val dem = File(dir, "europe_norway_ostlandet_dem.pmtiles").also { it.writeBytes(ByteArray(2000)) }
        assertEquals(dem.absolutePath, MapterhornTerrain.localDemBesideBasemap(vector.absolutePath)?.absolutePath)
    }

    private fun job(
        id: String,
        regionKey: String,
        localPath: String,
    ): FfiPmtilesJob =
        FfiPmtilesJob(
            id = id,
            regionKey = regionKey,
            url = "https://example.invalid",
            localPath = localPath,
            bytesReceived = 1uL,
            totalBytes = 1uL,
            status = "completed",
            paused = false,
            minLat = 59.0,
            minLon = 10.0,
            maxLat = 61.0,
            maxLon = 12.0,
        )
}
