package no.navi.app

import uniffi.navi.downloadProgressClear

/**
 * Guard for the shared native Download progress slot. A route plan, DEM extract,
 * or a finishing standalone place-index must not blank a live region-pipeline
 * or place-index label.
 */
object DownloadProgressClear {
    internal fun shouldClear(
        regionRunning: Boolean,
        placeIndexRunning: Boolean,
    ): Boolean = !regionRunning && !placeIndexRunning

    /**
     * Clear the Download slot only when neither the region pipeline nor a
     * standalone place-index is running. [runCatching] so JVM unit tests without
     * libnavi still compile. Returns true when the native clear ran.
     */
    fun clearIfIdle(): Boolean {
        if (!shouldClear(
                regionRunning = RegionDownloadBackground.isRunning(),
                placeIndexRunning = PlaceIndexBackground.isRunning(),
            )
        ) {
            return false
        }
        return runCatching {
            downloadProgressClear()
            true
        }.getOrDefault(false)
    }
}
