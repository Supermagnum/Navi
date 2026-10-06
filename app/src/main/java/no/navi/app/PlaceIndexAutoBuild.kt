package no.navi.app

import java.io.File

/**
 * Standalone auto-index (launch poller / [PlaceIndexBackground]) for downloaded
 * regions that are missing or not intact. One region at a time; never the main
 * thread; never while [RoutePlanGate] holds a plan.
 *
 * Future writes keep the first osm_id owner (2a). Auto-build is allow-listed
 * so hamburg / niedersachsen / ostlandet / denmark rows are not rewritten.
 */
object PlaceIndexAutoBuild {
    const val ENABLE_MISSING_REGION_AUTO_INDEX = true

    private val AUTO_INDEX_REGIONS =
        setOf(
            "europe/germany/schleswig-holstein",
            "europe/norway/vestlandet",
            "europe/norway/sorlandet",
            "europe/germany/mecklenburg-vorpommern",
        )

    fun mayStart(
        dataDir: File,
        regionId: String,
    ): Boolean {
        val rid = PackRegionAvailability.normalize(regionId)
        if (rid.isEmpty()) return false
        if (RoutePlanGate.isRunning()) return false
        if (NaviMapTestHooks.pendingTripPlan != null) return false
        if (PlaceIndexIntact.isIntact(dataDir, rid)) return false
        if (rid !in AUTO_INDEX_REGIONS) return false
        if (!ENABLE_MISSING_REGION_AUTO_INDEX) return false
        return true
    }
}
