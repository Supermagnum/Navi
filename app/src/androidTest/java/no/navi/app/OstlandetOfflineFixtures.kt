package no.navi.app

import uniffi.navi.pmtilesQueueRegion
import uniffi.navi.pmtilesRunJob
import java.io.File

/**
 * Installs Ostlandet Protomaps + Mapterhorn DEM from
 * `/data/local/tmp/navi_fixtures/` into the app [dataDir] `pmtiles/` tree and
 * registers a completed job so [BasemapStyleResolver] can attach offline 3D
 * without network.
 *
 * Without this, opt-in 3D falls through to online Mapterhorn TileJSON and the
 * status chip shows “3D terrain needs network” when Wi‑Fi is off.
 */
object OstlandetOfflineFixtures {
    private const val REGION = "europe/norway/ostlandet"
    private const val BASEMAP = "europe_norway_ostlandet.pmtiles"
    private const val DEM = "europe_norway_ostlandet_dem.pmtiles"

    fun ensureInstalled(
        dataDir: File,
        stagedDir: File = File("/data/local/tmp/navi_fixtures"),
    ) {
        val basemap = File(stagedDir, BASEMAP)
        val dem = File(stagedDir, DEM)
        check(basemap.isFile) { "missing ${basemap.absolutePath}" }
        check(dem.isFile) { "missing ${dem.absolutePath}" }

        val pmDir = File(dataDir, "pmtiles").also { it.mkdirs() }
        copyIfNeeded(basemap, File(pmDir, BASEMAP))
        copyIfNeeded(dem, File(pmDir, DEM))

        val job = pmtilesQueueRegion(dataDir.absolutePath, REGION, null)
        check(job.id.isNotBlank()) { "pmtilesQueueRegion returned empty id" }
        val done = pmtilesRunJob(dataDir.absolutePath, job.id)
        check(done.status == "completed") {
            "expected completed Ostlandet job, got ${done.status}"
        }
        check(File(pmDir, DEM).isFile && File(pmDir, DEM).length() > 1_000L) {
            "DEM not installed beside basemap under ${pmDir.absolutePath}"
        }
    }

    private fun copyIfNeeded(src: File, dst: File) {
        if (dst.isFile && dst.length() == src.length()) return
        src.copyTo(dst, overwrite = true)
    }
}
