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

    @Test
    fun localDemBesideBasemap_null_when_stem_mismatched_or_too_small() {
        val dir = tmp.newFolder("beside-miss")
        val vector = File(dir, "europe_norway_ostlandet.pmtiles").also { it.writeBytes(ByteArray(2000)) }
        // Different stem than the vector basename — not discovered.
        File(dir, "ostlandet_dem.pmtiles").writeBytes(ByteArray(2000))
        assertNull(MapterhornTerrain.localDemBesideBasemap(vector.absolutePath))
        // Matching name but under the 1000-byte gate — treated as missing.
        File(dir, "europe_norway_ostlandet_dem.pmtiles").writeBytes(ByteArray(500))
        assertNull(MapterhornTerrain.localDemBesideBasemap(vector.absolutePath))
    }

    @Test
    fun prefer3d_without_local_dem_keeps_offline_flat_not_online3d() {
        val flags =
            BasemapStyleResolver.offlineCoveringFlags(
                want3d = true,
                localDemPresent = false,
            )
        assertFalse(
            "must not request offline DEM hillshade without a local DEM",
            flags.offline3d,
        )
        assertEquals(0.0, flags.cameraPitch, 0.0)
        assertEquals(
            "3D hillshade needs the terrain download; showing offline map in 2D",
            flags.note,
        )
        // resolve() keeps StyleKind.OfflineProtomaps when a covering exists; only
        // these flags degrade 3D. Online3d is reserved for no covering at all.
        assertFalse(flags.offline3d)
        assertTrue(flags.cameraPitch == 0.0)
    }

    @Test
    fun prefer3d_with_local_dem_enables_offline_hillshade() {
        val flags =
            BasemapStyleResolver.offlineCoveringFlags(
                want3d = true,
                localDemPresent = true,
            )
        assertTrue(flags.offline3d)
        assertEquals(BasemapStyleResolver.TERRAIN_VIEW_TILT, flags.cameraPitch, 0.0)
        assertEquals("Offline Protomaps + Mapterhorn DEM hillshade", flags.note)
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
