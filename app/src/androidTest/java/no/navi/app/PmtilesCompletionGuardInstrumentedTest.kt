package no.navi.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.pmtilesListJobs
import uniffi.navi.pmtilesQueueRegion
import uniffi.navi.pmtilesRunJob
import java.io.File

/**
 * Completion-guard regression: mz12 staged fixture must not become a Completed
 * production Ostlandet job; a real maxzoom-15 archive on disk must still short-circuit OK.
 */
@RunWith(AndroidJUnit4::class)
class PmtilesCompletionGuardInstrumentedTest {
    private fun dataDir(): File = NaviAppData.resolve(InstrumentationRegistry.getInstrumentation().targetContext)

    @Test
    fun production_restore_rejects_mz12_fixture() {
        val dir = dataDir()
        val staged = File(OfflineDataIntegrity.STAGED_FIXTURES_DIR, OfflinePmtilesBootstrap.DEFAULT_BASEMAP)
        assertTrue("need staged mz12 fixture", staged.isFile)
        assertTrue(
            "fixture should be below full-size floor",
            staged.length() < OfflinePmtilesBootstrap.MIN_FULL_BASEMAP_BYTES,
        )
        assertFalse(OfflinePmtilesBootstrap.isFullRegionBasemap(staged))

        val report =
            OfflinePmtilesBootstrap.restoreOstlandetFromStaging(
                dataDir = dir,
                forInstrumentedTests = false,
            )
        assertTrue("expected FAIL report, got: $report", report.startsWith("FAIL:"))
        assertTrue(
            "FAIL should mention full/maxzoom: $report",
            report.contains("full", ignoreCase = true) ||
                report.contains("maxzoom", ignoreCase = true),
        )

        val inspect =
            OfflineDataIntegrity.inspect(
                InstrumentationRegistry.getInstrumentation().targetContext,
                dir,
            )
        assertFalse(inspect.canRestoreFromStaging)
    }

    @Test
    fun existing_full_ostlandet_archive_still_completes() {
        val dir = dataDir()
        val basemap = File(dir, "pmtiles/europe_norway_ostlandet.pmtiles")
        org.junit.Assume.assumeTrue(
            "need prior full Ostlandet extract on device",
            OfflinePmtilesBootstrap.isFullRegionBasemap(basemap),
        )
        val job = pmtilesQueueRegion(dir.absolutePath, "europe/norway/ostlandet", null)
        assertTrue(job.id.isNotBlank())
        val done = pmtilesRunJob(dir.absolutePath, job.id)
        assertEquals("completed", done.status)
        assertTrue(done.bytesReceived.toLong() >= OfflinePmtilesBootstrap.MIN_FULL_BASEMAP_BYTES)
        val jobs = pmtilesListJobs(dir.absolutePath)
        assertTrue(
            jobs.any {
                it.regionKey == "europe_norway_ostlandet" && it.status == "completed"
            },
        )
    }
}
