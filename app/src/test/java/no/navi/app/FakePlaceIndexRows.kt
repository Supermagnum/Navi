package no.navi.app

import java.io.File
import java.util.concurrent.ConcurrentHashMap

/**
 * Place-index rows per data dir for host JVM tests (no Android SQLite).
 * [install] replaces the database read in [PlaceIndexIntact]; [reset] restores it.
 */
object FakePlaceIndexRows : PlaceIndexIntact.RowSource {
    private val dbs = ConcurrentHashMap<String, ConcurrentHashMap<String, Long>>()

    fun install() {
        dbs.clear()
        PlaceIndexIntact.rowSourceForTests = this
        PlaceIndexReady.invalidateAllowedRegionsCache()
    }

    fun reset() {
        PlaceIndexIntact.rowSourceForTests = null
        dbs.clear()
        PlaceIndexReady.invalidateAllowedRegionsCache()
    }

    fun putRows(
        dataDir: File,
        regionId: String,
        rows: Long,
    ) {
        dbs
            .getOrPut(dataDir.absolutePath) { ConcurrentHashMap() }[PackRegionAvailability.normalize(regionId)] = rows
        PlaceIndexReady.invalidateAllowedRegionsCache()
    }

    override fun rows(dataDir: File): Map<String, Long>? = dbs[dataDir.absolutePath]?.toMap()

    override fun clearRegion(
        dataDir: File,
        regionId: String,
    ) {
        dbs[dataDir.absolutePath]?.remove(PackRegionAvailability.normalize(regionId))
    }
}
