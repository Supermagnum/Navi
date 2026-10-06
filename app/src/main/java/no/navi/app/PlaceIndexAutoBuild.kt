package no.navi.app

import java.io.File

/**
 * Standalone auto-index (launch poller / [PlaceIndexBackground]) for downloaded
 * regions that are missing or not intact. One region at a time; never the main
 * thread; never while [RoutePlanGate] holds a plan.
 *
 * General rule: any installed region that is not intact. Schema upgrades run
 * in-place in native open first — auto-build waits while user_version is old
 * so a PBF rebuild cannot race the PK migration. Vestlandet is offered first
 * when several regions are missing.
 */
object PlaceIndexAutoBuild {
    const val ENABLE_MISSING_REGION_AUTO_INDEX = true
    const val FIRST_MISSING_REGION = "europe/norway/vestlandet"

    fun schemaNeedsMigrate(dataDir: File): Boolean {
        val dbFile = File(dataDir, "place_index.db")
        if (!dbFile.isFile) return false
        val probe = PlaceIndexIntact.probe(dataDir, "europe/norway/ostlandet")
        return probe.userVersion in 1 until PlaceIndexIntact.SCHEMA_VERSION
    }

    fun mayStart(
        dataDir: File,
        regionId: String,
    ): Boolean {
        val rid = PackRegionAvailability.normalize(regionId)
        if (rid.isEmpty()) return false
        if (PlaceIndexReady.deferWritesDuringPlan()) return false
        if (schemaNeedsMigrate(dataDir)) return false
        if (PlaceIndexIntact.isIntact(dataDir, rid)) return false
        if (!ENABLE_MISSING_REGION_AUTO_INDEX) return false
        return true
    }

    /** First missing/not-intact installed region; Vestlandet before others. */
    fun nextRegion(dataDir: File): String? {
        if (PlaceIndexReady.deferWritesDuringPlan()) return null
        if (schemaNeedsMigrate(dataDir)) return null
        val ids =
            InstalledMaps.current()?.regions?.keys?.toList().orEmpty().ifEmpty {
                return null
            }
        val ordered =
            listOf(FIRST_MISSING_REGION) +
                ids.filter { PackRegionAvailability.normalize(it) != FIRST_MISSING_REGION }
                    .sorted()
        return ordered
            .map { PackRegionAvailability.normalize(it) }
            .distinct()
            .firstOrNull { mayStart(dataDir, it) }
    }
}
