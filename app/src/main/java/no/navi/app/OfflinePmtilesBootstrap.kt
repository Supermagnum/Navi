package no.navi.app

import uniffi.navi.pmtilesQueueRegion
import uniffi.navi.pmtilesRunJob
import java.io.File
import java.io.FileInputStream

/**
 * Copies host-staged Ostlandet Protomaps + DEM into app [dataDir] and registers
 * a completed download job so [BasemapStyleResolver] can use offline 3D.
 *
 * Used by instrumented tests ([forInstrumentedTests]=true) and by Tools when
 * [OfflineDataIntegrity] finds a **full** staged extract after a reinstall wiped
 * app-private storage. Truncated mz12 fixtures are rejected by the shared
 * completion guard for production region keys.
 */
object OfflinePmtilesBootstrap {
    const val DEFAULT_REGION = "europe/norway/ostlandet"

    /** `test_` region key — mz12 fixtures may register Completed for screenshots only. */
    const val INSTRUMENTED_FIXTURE_REGION = "test/ostlandet_fixture"

    const val DEFAULT_BASEMAP = "europe_norway_ostlandet.pmtiles"
    const val FULL_BASEMAP = "europe_norway_ostlandet_full.pmtiles"
    const val DEFAULT_DEM = "europe_norway_ostlandet_dem.pmtiles"

    /** Same floor as Rust [MIN_FULL_REGION_BASEMAP_BYTES] / reprovision tests. */
    const val MIN_FULL_BASEMAP_BYTES = 500_000_000L

    const val REQUIRED_VECTOR_MAXZOOM = 15

    fun restoreOstlandetFromStaging(
        dataDir: File,
        stagedDir: File = OfflineDataIntegrity.STAGED_FIXTURES_DIR,
        region: String = DEFAULT_REGION,
        basemapName: String = DEFAULT_BASEMAP,
        demName: String = DEFAULT_DEM,
        forInstrumentedTests: Boolean = false,
    ): String {
        val preferredFull = File(stagedDir, FULL_BASEMAP)
        val defaultBasemap = File(stagedDir, basemapName)
        val basemap =
            when {
                preferredFull.isFile && preferredFull.length() >= MIN_FULL_BASEMAP_BYTES ->
                    preferredFull
                defaultBasemap.isFile -> defaultBasemap
                else -> return "FAIL: missing ${defaultBasemap.absolutePath}"
            }
        val dem = File(stagedDir, demName)
        if (!dem.isFile) return "FAIL: missing ${dem.absolutePath}"

        val regionPath = if (forInstrumentedTests) INSTRUMENTED_FIXTURE_REGION else region
        if (!forInstrumentedTests && !isFullRegionBasemap(basemap)) {
            return "FAIL: staged basemap is not a full maxzoom-$REQUIRED_VECTOR_MAXZOOM " +
                "extract (bytes=${basemap.length()}, maxzoom=${readPmtilesMaxZoom(basemap)}). " +
                "Re-download from Tools, or stage $FULL_BASEMAP (≥${MIN_FULL_BASEMAP_BYTES} bytes)."
        }

        val pmDir = File(dataDir, "pmtiles").also { it.mkdirs() }
        val destBasemapName =
            if (forInstrumentedTests) {
                "test_ostlandet_fixture.pmtiles"
            } else {
                DEFAULT_BASEMAP
            }
        copyIfNeeded(basemap, File(pmDir, destBasemapName))
        copyIfNeeded(dem, File(pmDir, demName))

        val job = pmtilesQueueRegion(dataDir.absolutePath, regionPath, null)
        if (job.id.isBlank()) return "FAIL: pmtilesQueueRegion returned empty id"
        val done = pmtilesRunJob(dataDir.absolutePath, job.id)
        if (done.status != "completed") {
            return "FAIL: expected completed Ostlandet job, got ${done.status} " +
                "(staged basemap rejected or incomplete — re-download from Tools)"
        }
        val demOut = File(pmDir, demName)
        if (!demOut.isFile || demOut.length() <= 1_000L) {
            return "FAIL: DEM not installed under ${pmDir.absolutePath}"
        }
        return "OK: restored $destBasemapName + $demName (${done.regionKey})"
    }

    fun isFullRegionBasemap(
        file: File,
        minBytes: Long = MIN_FULL_BASEMAP_BYTES,
    ): Boolean {
        if (!file.isFile || file.length() < minBytes) return false
        val mz = readPmtilesMaxZoom(file) ?: return false
        return mz >= REQUIRED_VECTOR_MAXZOOM
    }

    fun readPmtilesMaxZoom(pmFile: File): Int? {
        if (!pmFile.isFile || pmFile.length() < 127L) return null
        val header = ByteArray(127)
        FileInputStream(pmFile).use { input ->
            var off = 0
            while (off < header.size) {
                val n = input.read(header, off, header.size - off)
                if (n <= 0) return null
                off += n
            }
        }
        if (String(header, 0, 7, Charsets.US_ASCII) != "PMTiles") return null
        return header[101].toInt() and 0xFF
    }

    private fun copyIfNeeded(
        src: File,
        dst: File,
    ) {
        if (dst.isFile && dst.length() == src.length()) return
        src.copyTo(dst, overwrite = true)
    }
}
