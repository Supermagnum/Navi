package no.navi.app

import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.downloadProgressSnapshot
import uniffi.navi.pmtilesPlanetUrl
import uniffi.navi.pmtilesQueueDemRegion
import uniffi.navi.pmtilesQueueRegion
import uniffi.navi.pmtilesRunJob
import java.io.File

/**
 * SM-P613 verification for PMTiles/DEM download progress fixes (Luxembourg bbox).
 * Logs progress samples to logcat tag LuxembourgProgress.
 */
@RunWith(AndroidJUnit4::class)
class LuxembourgDownloadProgressInstrumentedTest {
    private companion object {
        const val TAG = "LuxembourgProgress"
        const val REGION = "europe/luxembourg"
        const val STALE_PROTOMAPS = "https://build.protomaps.com/20260813.pmtiles"
    }

    private fun dataDir(): File {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        return NaviAppData.resolve(context)
    }

    private fun cleanupLuxembourgArtifacts(dataDir: File) {
        val pmtilesDir = File(dataDir, "pmtiles")
        pmtilesDir.listFiles()?.forEach { f ->
            if (f.name.contains("luxembourg", ignoreCase = true)) {
                f.deleteRecursively()
            }
        }
    }

    private data class ProgressSample(
        val label: String,
        val done: ULong,
        val total: ULong?,
    )

    @Test
    fun stale_protomaps_url_resolves_and_basemap_completes() {
        val dataDir = dataDir()
        cleanupLuxembourgArtifacts(dataDir)

        val resolved = pmtilesPlanetUrl()
        assertTrue("planet url should be protomaps build", resolved.contains("build.protomaps.com"))
        assertFalse("resolved url must not be stale 404 build", resolved.contains("20260813"))
        Log.i(TAG, "resolved_planet_url=$resolved")

        val job = pmtilesQueueRegion(dataDir.absolutePath, REGION, STALE_PROTOMAPS)
        assertTrue(job.id.isNotBlank())
        assertEquals(STALE_PROTOMAPS, job.url)

        val samples = mutableListOf<ProgressSample>()
        val runner =
            Thread {
                val done = pmtilesRunJob(dataDir.absolutePath, job.id)
                Log.i(TAG, "basemap_status=${done.status} bytes=${done.bytesReceived}")
                assertEquals("completed", done.status)
            }
        runner.start()

        val deadline = System.currentTimeMillis() + 900_000
        var sawPlanning = false
        var sawWritePhase = false
        var writeTotal: ULong? = null
        while (runner.isAlive && System.currentTimeMillis() < deadline) {
            val snap = runCatching { downloadProgressSnapshot() }.getOrNull()
            if (snap != null && snap.label.isNotBlank()) {
                val sample = ProgressSample(snap.label, snap.unitsDone, snap.unitsTotal)
                if (samples.isEmpty() ||
                    samples.last().label != sample.label ||
                    samples.last().done != sample.done ||
                    samples.last().total != sample.total
                ) {
                    samples.add(sample)
                    Log.i(
                        TAG,
                        "progress label=${sample.label} done=${sample.done} total=${sample.total}",
                    )
                }
                if (sample.label.contains("Planning extract")) sawPlanning = true
                if (sample.label.contains("Writing map archive")) {
                    sawWritePhase = true
                    writeTotal = sample.total
                }
            }
            Thread.sleep(400)
        }
        runner.join(60_000)
        assertFalse("job thread still running after join", runner.isAlive)

        assertTrue("expected Planning extract phase", sawPlanning)
        assertFalse(
            "should not flash 0 / ? during planning window",
            samples.any { it.label.contains("map tiles") && it.total == null && it.done == 0uL },
        )
        if (sawWritePhase && writeTotal != null) {
            val lastWrite = samples.lastOrNull { it.label.contains("Writing map archive") }
            assertTrue(
                "write phase should reach ~100% (denominator=tiles written, not bbox tile-id count)",
                lastWrite != null && lastWrite.done >= writeTotal!! * 95uL / 100uL,
            )
        }
        assertTrue(File(job.localPath).length() > 1000)
    }

    @Test
    fun dem_download_reports_smooth_byte_progress() {
        val dataDir = dataDir()
        val demPath = File(dataDir, "pmtiles/europe_luxembourg_dem.pmtiles")
        demPath.delete()
        val staging = File("${demPath.absolutePath}.chunks")
        staging.deleteRecursively()

        val job = pmtilesQueueDemRegion(dataDir.absolutePath, REGION)
        assertTrue(job.id.isNotBlank())

        val samples = mutableListOf<ProgressSample>()
        val runner =
            Thread {
                val done = pmtilesRunJob(dataDir.absolutePath, job.id)
                Log.i(TAG, "dem_status=${done.status} bytes=${done.bytesReceived}")
                assertEquals("completed", done.status)
            }
        runner.start()

        var lastDone = 0uL
        var monotonicSteps = 0
        val deadline = System.currentTimeMillis() + 900_000
        while (runner.isAlive && System.currentTimeMillis() < deadline) {
            val snap = runCatching { downloadProgressSnapshot() }.getOrNull()
            if (snap != null && snap.label.contains("DEM")) {
                val sample = ProgressSample(snap.label, snap.unitsDone, snap.unitsTotal)
                if (samples.isEmpty() || samples.last().done != sample.done) {
                    samples.add(sample)
                    Log.i(
                        TAG,
                        "dem progress done=${sample.done} total=${sample.total}",
                    )
                    if (sample.done > lastDone) {
                        monotonicSteps++
                        lastDone = sample.done
                    }
                }
            }
            Thread.sleep(400)
        }
        runner.join(60_000)
        assertFalse(runner.isAlive)
        assertTrue(
            "DEM byte progress should advance in multiple steps within chunks",
            monotonicSteps >= 3,
        )
        assertTrue(demPath.length() > 1000)
    }
}
