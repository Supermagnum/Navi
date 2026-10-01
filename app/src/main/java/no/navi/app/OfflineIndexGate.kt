package no.navi.app

import java.io.File

/**
 * Gates automatic place-index / indexed-maps work so cold start and Tools do not
 * pretend indexing is running when there is nothing to index.
 *
 * Auto-index requires at least one of: a real region PBF under [dataDir], installed
 * graph pack files, or a persisted pending region-download / extract job.
 * Fixture paths under `/data/local/tmp/navi_fixtures/` are for Plan tests only —
 * never for background convert / place-index.
 */
object OfflineIndexGate {
    /** Same floor as [RegionDownloadBackground.MIN_PBF_BYTES] (pack-install stubs are smaller). */
    const val MIN_INDEXABLE_PBF_BYTES = 1_000_000L

    private const val FIXTURE_PREFIX = "/data/local/tmp/navi_fixtures/"

    fun isFixturePath(file: File): Boolean =
        file.absolutePath.startsWith(FIXTURE_PREFIX)

    fun isIndexablePbf(file: File): Boolean =
        file.isFile &&
            file.name.endsWith(".osm.pbf") &&
            file.length() >= MIN_INDEXABLE_PBF_BYTES &&
            !isFixturePath(file)

    fun hasGraphPackMaterial(dataDir: File): Boolean {
        val files = dataDir.listFiles() ?: return false
        return files.any { f ->
            f.isFile &&
                f.length() > 0L &&
                f.name.contains(".navi-graph-") &&
                f.name.endsWith(".rkyv")
        }
    }

    /**
     * True when background indexing / convert may start. False on a fresh install
     * (no PBF, no packs, no pending extract) even if the Tools region picker has
     * a default Geofabrik path selected.
     */
    fun hasMaterialToIndex(dataDir: File): Boolean {
        if (!dataDir.isDirectory) return false
        // Prefer cheap file probes (host unit tests have no libnavi). Avoid
        // discoverPending — it maps PBF stems via UniFFI.
        if (RegionDownloadBackground.loadJob(dataDir) != null) return true
        if (RegionDownloadBackground.findPartialPbf(dataDir) != null) return true
        val files = dataDir.listFiles() ?: return false
        if (files.any { isIndexablePbf(it) }) return true
        return hasGraphPackMaterial(dataDir)
    }

    /**
     * PBF to use for automatic background index/convert. Prefers [regionPath]
     * when it resolves to a real local extract; never returns fixture files.
     * Pack-server installs may only have a small stub PBF beside graph packs —
     * those stubs are returned only when [hasGraphPackMaterial] is true.
     */
    fun resolveAutoIndexPbf(
        dataDir: File,
        regionPath: String,
    ): File? {
        val rid = regionPath.trim().trim('/')
        if (rid.isNotEmpty()) {
            PackRegionAvailability.resolvePbfForRegion(dataDir, rid)?.let { pbf ->
                if (isIndexablePbf(pbf)) return pbf
            }
        }
        dataDir
            .listFiles()
            ?.filter { isIndexablePbf(it) }
            ?.maxByOrNull { it.length() }
            ?.let { return it }
        if (!hasGraphPackMaterial(dataDir)) return null
        if (rid.isNotEmpty()) {
            val stub = File(dataDir, "${PackRegionAvailability.localStem(rid)}.osm.pbf")
            if (stub.isFile && !isFixturePath(stub)) return stub
        }
        return dataDir.listFiles()?.firstOrNull { f ->
            f.isFile && f.name.endsWith(".osm.pbf") && !isFixturePath(f)
        }
    }
}
