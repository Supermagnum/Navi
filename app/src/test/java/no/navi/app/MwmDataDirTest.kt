package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * [MwmDataDir] searches SD and internal for World.mwm; SD wins when present.
 */
class MwmDataDirTest {
    @Test
    fun internalVolumeIdConstantMatchesPackStorage() {
        assertEquals(NaviStorageVolumes.INTERNAL_ID, "internal")
    }

    @Test
    fun mwmRootAppendsMwmSegment() {
        val base = File("/tmp/navi-mwm-data-dir-test")
        val root = File(base, "mwm")
        assertEquals("mwm", root.name)
        assertTrue(root.path.endsWith("/mwm") || root.path.endsWith("\\mwm"))
    }

    @Test
    fun hasWorldMwmRequiresWorldFileUnderMwm() {
        val base =
            File.createTempFile("navi-mwm-world", null).apply {
                delete()
                mkdirs()
            }
        try {
            assertFalse(MwmDataDir.hasWorldMwm(base))
            val mwm = File(base, "mwm").also { it.mkdirs() }
            assertFalse(MwmDataDir.hasWorldMwm(base))
            File(mwm, "World.mwm").writeText("x")
            assertTrue(MwmDataDir.hasWorldMwm(base))
        } finally {
            base.deleteRecursively()
        }
    }

    @Test
    fun repeaterDataDirNameIsStable() {
        assertEquals("repeater-data", CatBootstrap.REPEATER_DATA_DIR_NAME)
    }
}
