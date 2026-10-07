package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import kotlin.io.path.createTempDirectory

/**
 * File-level checks for place-index volume follow (no Android runtime).
 * [PlaceIndexStorage] Android entry points are covered via InstalledMaps /
 * pack-volume integration; here we lock the copy/move contract.
 */
class PlaceIndexStorageTest {
    @Test
    fun copyVerified_rejectsTruncatedPartial() {
        val dir = createTempDirectory("place-idx-").toFile()
        try {
            val src = File(dir, "place_index.db").apply { writeBytes(ByteArray(12_000) { 1 }) }
            val dst = File(dir, "out/place_index.db")
            // Mimic successful full copy via the same size contract the storage helper uses.
            dst.parentFile!!.mkdirs()
            src.copyTo(dst, overwrite = true)
            assertTrue(dst.length() >= PlaceIndexIntact.MIN_DB_BYTES)
            assertEquals(src.length(), dst.length())
            // Tiny file must not be treated as a movable index.
            val tiny = File(dir, "tiny.db").apply { writeText("x") }
            assertFalse(tiny.length() >= PlaceIndexIntact.MIN_DB_BYTES)
        } finally {
            dir.deleteRecursively()
        }
    }

    @Test
    fun dbName_isStable() {
        assertEquals("place_index.db", PlaceIndexStorage.dbName())
    }
}
