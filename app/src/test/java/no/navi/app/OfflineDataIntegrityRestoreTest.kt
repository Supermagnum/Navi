package no.navi.app

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * Per-PR host guard: mz12 staged fixtures must not surface as a production
 * restore. On-device counterparts stay dispatch-only
 * ([no.navi.app.RestoreOstlandetFixturesInstrumentedTest],
 * [no.navi.app.OfflineDataMismatchInstrumentedTest]).
 */
class OfflineDataIntegrityRestoreTest {
    @Test
    fun mz12FixtureDoesNotOfferProductionRestore() {
        val staged = File(createTempDir(), "staged").also { it.mkdirs() }
        writeFakePmtiles(
            File(staged, OfflinePmtilesBootstrap.DEFAULT_BASEMAP),
            maxzoom = 12,
            size = 192_023,
        )
        assertFalse(
            OfflineDataIntegrity.canOfferProductionRestore(staged, appPmtilesEmpty = true),
        )
    }

    @Test
    fun emptyStagingDoesNotOfferRestore() {
        val staged = File(createTempDir(), "staged").also { it.mkdirs() }
        assertFalse(
            OfflineDataIntegrity.canOfferProductionRestore(staged, appPmtilesEmpty = true),
        )
    }

    @Test
    fun occupiedAppStorageDoesNotOfferRestore() {
        val staged = File(createTempDir(), "staged").also { it.mkdirs() }
        writeFakePmtiles(File(staged, "header-only.pmtiles"), maxzoom = 15, size = 4_096)
        assertFalse(
            OfflineDataIntegrity.canOfferProductionRestore(staged, appPmtilesEmpty = false),
        )
    }

    @Test
    fun userMessageNamesOfflineDataWhenRestoreWouldBeOffered() {
        val report =
            OfflineDataIntegrity.Report(
                stagedBasemapNames = listOf(OfflinePmtilesBootstrap.DEFAULT_BASEMAP),
                canRestoreFromStaging = true,
            )
        val msg = report.userMessage()
        assertTrue(!msg.isNullOrBlank())
        assertTrue(
            "message should mention offline/reinstall: $msg",
            msg!!.contains("Offline data", ignoreCase = true) ||
                msg.contains("reinstall", ignoreCase = true) ||
                msg.contains("restore", ignoreCase = true),
        )
    }

    @Test
    fun mz12FixtureReportDoesNotClaimRestore() {
        val report =
            OfflineDataIntegrity.Report(
                stagedBasemapNames = listOf(OfflinePmtilesBootstrap.DEFAULT_BASEMAP),
                canRestoreFromStaging = false,
                missingRememberedRegions = listOf("europe_norway_ostlandet"),
            )
        assertFalse(report.canRestoreFromStaging)
        assertTrue(report.hasIssue)
        val msg = report.userMessage()
        assertTrue(!msg.isNullOrBlank())
        assertFalse(
            "mz12 must not prompt staged restore: $msg",
            msg!!.contains("restore staged", ignoreCase = true),
        )
    }

    private fun createTempDir(): File =
        kotlin.io.path.createTempDirectory("navi-integrity-").toFile().also {
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
