package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.rules.TemporaryFolder
import org.junit.Rule
import java.io.File

class LongTripPackStorageTest {
    @get:Rule
    val tmp = TemporaryFolder()

    @Test
    fun scrub_keeps_partial_deletes_truncated_final_when_partial_exists() {
        val dir = tmp.newFolder("packs")
        File(dir, "ostlandet-latest.osm.pbf.partial").writeText("partial-bytes")
        File(dir, "ostlandet-latest.osm.pbf").writeText("trunc")
        val stems = LongTripPackStorage.scrubIncompletePacks(dir)
        assertTrue(stems.any { it.contains("ostlandet") })
        assertTrue(File(dir, "ostlandet-latest.osm.pbf.partial").isFile)
        assertFalse(
            "truncated final must be removed while .partial remains",
            File(dir, "ostlandet-latest.osm.pbf").exists(),
        )
    }

    @Test
    fun scrub_leaves_complete_final_without_partial() {
        val dir = tmp.newFolder("packs2")
        File(dir, "hamburg-latest.navi-manifest.json").writeText("{}")
        File(dir, "hamburg-latest.osm.pbf").writeBytes(ByteArray(2_000_000))
        val stems = LongTripPackStorage.scrubIncompletePacks(dir)
        assertTrue(stems.isEmpty())
        assertTrue(File(dir, "hamburg-latest.navi-manifest.json").isFile)
        assertTrue(File(dir, "hamburg-latest.osm.pbf").isFile)
    }

    @Test
    fun format_bytes_short_scales() {
        assertEquals("512 B", formatBytesShort(512))
        assertEquals("1 KB", formatBytesShort(1024))
        assertEquals("1.0 GB", formatBytesShort(1L * 1024 * 1024 * 1024))
    }

    @Test
    fun reuse_internal_when_manifest_present() {
        // Pure availability check used by resolvePackTarget's reuse branch.
        val internal = tmp.newFolder("internal")
        File(internal, "ostlandet-latest.navi-manifest.json").writeText("{}")
        assertTrue(PackRegionAvailability.localBakeReady(internal, "europe/norway/ostlandet"))
        val empty = tmp.newFolder("empty")
        assertFalse(PackRegionAvailability.localBakeReady(empty, "europe/norway/ostlandet"))
    }
}
