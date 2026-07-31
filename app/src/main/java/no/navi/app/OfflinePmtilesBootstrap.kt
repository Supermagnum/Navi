package no.navi.app

import uniffi.navi.pmtilesQueueRegion
import uniffi.navi.pmtilesRunJob
import java.io.File

/**
 * Copies host-staged Ostlandet Protomaps + DEM into app [dataDir] and registers
 * a completed download job so [BasemapStyleResolver] can use offline 3D.
 *
 * Used by instrumented tests and by Tools when [OfflineDataIntegrity] finds
 * staged files after a reinstall wiped app-private storage.
 */
object OfflinePmtilesBootstrap {
    const val DEFAULT_REGION = "europe/norway/ostlandet"
    const val DEFAULT_BASEMAP = "europe_norway_ostlandet.pmtiles"
    const val DEFAULT_DEM = "europe_norway_ostlandet_dem.pmtiles"

    fun restoreOstlandetFromStaging(
        dataDir: File,
        stagedDir: File = OfflineDataIntegrity.STAGED_FIXTURES_DIR,
        region: String = DEFAULT_REGION,
        basemapName: String = DEFAULT_BASEMAP,
        demName: String = DEFAULT_DEM,
    ): String {
        val basemap = File(stagedDir, basemapName)
        val dem = File(stagedDir, demName)
        if (!basemap.isFile) return "FAIL: missing ${basemap.absolutePath}"
        if (!dem.isFile) return "FAIL: missing ${dem.absolutePath}"

        val pmDir = File(dataDir, "pmtiles").also { it.mkdirs() }
        copyIfNeeded(basemap, File(pmDir, basemapName))
        copyIfNeeded(dem, File(pmDir, demName))

        val job = pmtilesQueueRegion(dataDir.absolutePath, region, null)
        if (job.id.isBlank()) return "FAIL: pmtilesQueueRegion returned empty id"
        val done = pmtilesRunJob(dataDir.absolutePath, job.id)
        if (done.status != "completed") {
            return "FAIL: expected completed Ostlandet job, got ${done.status}"
        }
        val demOut = File(pmDir, demName)
        if (!demOut.isFile || demOut.length() <= 1_000L) {
            return "FAIL: DEM not installed under ${pmDir.absolutePath}"
        }
        return "OK: restored $basemapName + $demName (${done.regionKey})"
    }

    private fun copyIfNeeded(
        src: File,
        dst: File,
    ) {
        if (dst.isFile && dst.length() == src.length()) return
        src.copyTo(dst, overwrite = true)
    }
}
