package no.navi.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * Per-PR host guard for the download-completion gap: an mz12 / undersize staged
 * file must not pass as a full Ostlandet extract. On-device counterpart:
 * [no.navi.app.PmtilesCompletionGuardInstrumentedTest] (dispatch-only).
 */
class OfflinePmtilesBootstrapTest {
    @Test
    fun undersizeFileIsNotFullRegionBasemap() {
        val dir = File(createTempDir(), "staged").also { it.mkdirs() }
        val fake = writeFakePmtiles(File(dir, OfflinePmtilesBootstrap.DEFAULT_BASEMAP), maxzoom = 12, size = 192_023)
        assertTrue(fake.length() < OfflinePmtilesBootstrap.MIN_FULL_BASEMAP_BYTES)
        assertFalse(OfflinePmtilesBootstrap.isFullRegionBasemap(fake))
    }

    @Test
    fun mz12HeaderRejectedEvenWhenSizeFloorDisabled() {
        val dir = File(createTempDir(), "staged").also { it.mkdirs() }
        val fake = writeFakePmtiles(File(dir, "tiny.pmtiles"), maxzoom = 12, size = 4_096)
        assertFalse(OfflinePmtilesBootstrap.isFullRegionBasemap(fake, minBytes = 0L))
    }

    @Test
    fun mz15HeaderAcceptedWhenSizeFloorDisabled() {
        val dir = File(createTempDir(), "staged").also { it.mkdirs() }
        val fake = writeFakePmtiles(File(dir, "full-header.pmtiles"), maxzoom = 15, size = 4_096)
        assertTrue(OfflinePmtilesBootstrap.isFullRegionBasemap(fake, minBytes = 0L))
        assertEquals(15, OfflinePmtilesBootstrap.readPmtilesMaxZoom(fake))
    }

    @Test
    fun productionRestoreRejectsMz12FixtureWithoutCallingDownloader() {
        val root = createTempDir()
        val staged = File(root, "staged").also { it.mkdirs() }
        val dataDir = File(root, "data").also { it.mkdirs() }
        writeFakePmtiles(
            File(staged, OfflinePmtilesBootstrap.DEFAULT_BASEMAP),
            maxzoom = 12,
            size = 192_023,
        )
        File(staged, OfflinePmtilesBootstrap.DEFAULT_DEM).writeBytes(ByteArray(2_048))

        val report =
            OfflinePmtilesBootstrap.restoreOstlandetFromStaging(
                dataDir = dataDir,
                stagedDir = staged,
                forInstrumentedTests = false,
            )
        assertTrue("expected FAIL report, got: $report", report.startsWith("FAIL:"))
        assertTrue(
            "FAIL should mention full/maxzoom: $report",
            report.contains("full", ignoreCase = true) ||
                report.contains("maxzoom", ignoreCase = true),
        )
        val dest = File(dataDir, "pmtiles/${OfflinePmtilesBootstrap.DEFAULT_BASEMAP}")
        assertFalse("production restore must not copy the mz12 fixture", dest.isFile)
    }

    private fun createTempDir(): File =
        kotlin.io.path.createTempDirectory("navi-pmtiles-guard-").toFile().also {
            it.deleteOnExit()
        }

    private fun writeFakePmtiles(
        dest: File,
        maxzoom: Int,
        size: Int,
    ): File {
        val buf = ByteArray(size.coerceAtLeast(127))
        val magic = "PMTiles".toByteArray(Charsets.US_ASCII)
        magic.copyInto(buf)
        buf[101] = maxzoom.toByte()
        dest.writeBytes(buf)
        return dest
    }
}
