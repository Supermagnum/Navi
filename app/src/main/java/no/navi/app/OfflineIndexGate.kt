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

    fun isFixturePath(file: File): Boolean = file.absolutePath.startsWith(FIXTURE_PREFIX)

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
     * PBF to use for automatic place-index. Prefers [regionPath] when it
     * resolves to that region's **own** leaf extract; never returns fixture
     * files or another region's PBF (e.g. hamburg/finland must not pick
     * `sweden-latest`). Missing/stub leaf → null ("cannot index yet").
     *
     * When [regionPath] is blank, returns null — callers must name a region
     * rather than indexing the largest file on disk.
     */
    fun resolveAutoIndexPbf(
        dataDir: File,
        regionPath: String,
    ): File? {
        val rid = regionPath.trim().trim('/')
        if (rid.isEmpty()) return null
        PackRegionAvailability.resolvePlaceIndexPbf(dataDir, rid)?.let { pbf ->
            if (isIndexablePbf(pbf) &&
                PackRegionAvailability.pbfMatchesRegionForPlaceIndex(pbf, rid)
            ) {
                return pbf
            }
        }
        return null
    }

    /** Stable status when a region has packs but no indexable leaf extract. */
    const val CANNOT_INDEX_YET = "cannot index yet"
}
