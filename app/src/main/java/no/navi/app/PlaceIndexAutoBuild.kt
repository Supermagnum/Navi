package no.navi.app

import java.io.File

/**
 * Standalone auto-index (launch poller / [PlaceIndexBackground]) must never
 * start a multi-hour build just because a downloaded region has 0 rows.
 *
 * Allowed: resume an in-progress `name_index_build.complete=0` write.
 * Intact slices are never opened for write. Missing-empty regions are listed
 * by [InstalledMaps], not built, until the user/download pipeline asks.
 */
object PlaceIndexAutoBuild {
    fun mayStart(
        dataDir: File,
        regionId: String,
    ): Boolean {
        val rid = PackRegionAvailability.normalize(regionId)
        if (rid.isEmpty()) return false
        if (PlaceIndexIntact.isIntact(dataDir, rid)) return false
        return RegionDownloadBackground.placeIndexBuildIncomplete(dataDir, rid)
    }
}
