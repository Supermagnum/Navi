package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

/**
 * Regression: long-trip packs under [LongTripPackStorage.PACKS_SUBDIR] must be
 * visible to [RegionCoverage.resolvePlanPbf] even when [dataDir] top-level is empty.
 * Non-long-trip planning (packs only under dataDir) must keep working unchanged.
 */
class ResolvePlanPbfPackDirTest {
    @get:Rule
    val tmp = TemporaryFolder()

    private fun writePbf(
        dir: File,
        name: String,
        bytes: Int = 2_000_000,
    ): File {
        val f = File(dir, name)
        f.writeBytes(ByteArray(bytes))
        return f
    }

    private val hamar =
        RegionCoverage.Waypoint(role = "From", name = "Hamar", lat = 60.7945, lon = 11.0680)

    @Test
    fun resolvePlanPbf_findsExtractOnlyUnderLongTripPacks() {
        val dataDir = tmp.newFolder("files")
        val packDir = File(dataDir, LongTripPackStorage.PACKS_SUBDIR).also { it.mkdirs() }
        val pbf = writePbf(packDir, "ostlandet-latest.osm.pbf")
        assertNull(
            "top-level alone must not see subdirectory packs",
            RegionCoverage.resolvePlanPbf(dataDir, listOf(hamar)),
        )
        val found = RegionCoverage.resolvePlanPbf(dataDir, listOf(hamar), packDir)
        assertNotNull(found)
        assertEquals(pbf.absolutePath, found!!.absolutePath)
    }

    @Test
    fun resolvePlanPbf_stillPrefersTopLevelWhenNoPackDir() {
        val dataDir = tmp.newFolder("files-only")
        val pbf = writePbf(dataDir, "ostlandet-latest.osm.pbf")
        val found = RegionCoverage.resolvePlanPbf(dataDir, listOf(hamar), packDir = null)
        assertNotNull(found)
        assertEquals(pbf.absolutePath, found!!.absolutePath)
    }

    @Test
    fun resolvePlanPbf_topLevelUnchangedWhenPackDirAlsoPresent() {
        val dataDir = tmp.newFolder("both")
        val packDir = File(dataDir, LongTripPackStorage.PACKS_SUBDIR).also { it.mkdirs() }
        val top = writePbf(dataDir, "ostlandet-latest.osm.pbf")
        writePbf(packDir, "denmark-latest.osm.pbf")
        val found = RegionCoverage.resolvePlanPbf(dataDir, listOf(hamar), packDir)
        assertNotNull(found)
        assertEquals(
            "Hamar is in Ostlandet; top-level Ostlandet extract must win",
            top.absolutePath,
            found!!.absolutePath,
        )
    }
}
