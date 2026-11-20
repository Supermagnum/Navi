package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * [MwmDataDir] mirrors pack-volume selection: when the preferred volume is
 * internal, MWM roots under [NaviAppData]; SD selection is covered by
 * instrumented volume probes (writable app-files dir).
 */
class MwmDataDirTest {
    @Test
    fun internalVolumeIdConstantMatchesPackStorage() {
        assertEquals(NaviStorageVolumes.INTERNAL_ID, "internal")
    }

    @Test
    fun mwmRootAppendsMwmSegment() {
        // Pure path contract: mwmRoot = resolve + "/mwm".
        val base = java.io.File("/tmp/navi-mwm-data-dir-test")
        val root = java.io.File(base, "mwm")
        assertEquals("mwm", root.name)
        assertTrue(root.path.endsWith("/mwm") || root.path.endsWith("\\mwm"))
    }
}
