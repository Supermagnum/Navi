package no.navi.app

import java.io.File

/**
 * Standalone auto-index (launch poller / [PlaceIndexBackground]) for downloaded
 * regions that are missing or not intact. One region at a time; never the main
 * thread; never while [RoutePlanGate] holds a plan.
 *
 * General rule: any installed region that is not intact. Schema upgrades run
 * in-place in native [ensurePlaceIndex] open first. While user_version is old,
 * kick a writer open on a region that already has rows (cache-hit after migrate)
 * so a missing-region PBF rebuild cannot start until the PK migration finishes.
 * Vestlandet is offered first when several regions are missing and schema is current.
 */
object PlaceIndexAutoBuild {
    const val ENABLE_MISSING_REGION_AUTO_INDEX = true
    const val FIRST_MISSING_REGION = "europe/norway/vestlandet"

    /**
     * After Vestlandet, index corridor leaves before alphabetically-first extras
     * (finland / nord-norge / mecklenburg) that are not on the Bevensen trip.
     */
    private val CORRIDOR_INDEX_ORDER =
        listOf(
            "europe/germany/schleswig-holstein",
            "europe/norway/sorlandet",
            "europe/sweden/skane",
            "europe/sweden/halland",
            "europe/sweden/vastra_gotaland",
        )

    /** Prefer an already-populated region so migrate-then-cache-hit does not rebuild. */
    private val MIGRATE_KICKOFF_REGIONS =
        listOf(
            "europe/norway/ostlandet",
            "europe/denmark",
            "europe/germany/niedersachsen",
            "europe/germany/hamburg",
        )

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
        if (schemaNeedsMigrate(dataDir)) {
            // Only the migrate kickoff region — open() migrates; post-open cache
            // check must not rebuild protected rows.
            return rid == migrateKickoffRegion(dataDir)
        }
        // Never index a missing region onto an empty/stub DB (post-quarantine).
        if (!protectedSlicesPresent(dataDir)) return false
        if (PlaceIndexIntact.isIntact(dataDir, rid)) return false
        if (!ENABLE_MISSING_REGION_AUTO_INDEX) return false
        return true
    }

    /** First missing/not-intact installed region; Vestlandet before others. */
    fun nextRegion(dataDir: File): String? {
        if (PlaceIndexReady.deferWritesDuringPlan()) return null
        if (schemaNeedsMigrate(dataDir)) {
            return migrateKickoffRegion(dataDir)
        }
        if (!protectedSlicesPresent(dataDir)) return null
        val ids =
            InstalledMaps.current()?.regions?.keys?.toList().orEmpty().ifEmpty {
                return null
            }
        val normalized = ids.map { PackRegionAvailability.normalize(it) }.distinct()
        val ordered =
            listOf(FIRST_MISSING_REGION) +
                CORRIDOR_INDEX_ORDER +
                normalized
                    .filter {
                        it != FIRST_MISSING_REGION && it !in CORRIDOR_INDEX_ORDER
                    }.sorted()
        return ordered.distinct().firstOrNull { mayStart(dataDir, it) }
    }

    private fun migrateKickoffRegion(dataDir: File): String? {
        val installed =
            InstalledMaps.current()?.regions?.keys?.map { PackRegionAvailability.normalize(it) }
                ?.toSet()
                .orEmpty()
        for (rid in MIGRATE_KICKOFF_REGIONS) {
            if (installed.isNotEmpty() && rid !in installed) continue
            val pbf = OfflineIndexGate.resolveAutoIndexPbf(dataDir, rid) ?: continue
            if (!OfflineIndexGate.isIndexablePbf(pbf)) continue
            val probe = PlaceIndexIntact.probe(dataDir, rid)
            // schema_old probe has rowCount 0; check rows directly when version is old.
            if (probe.userVersion in 1 until PlaceIndexIntact.SCHEMA_VERSION) {
                if (regionHasRows(dataDir, rid)) return rid
            } else if (probe.rowCount > 0L) {
                return rid
            }
        }
        return null
    }

    /** True when all four protected regions still have rows (post-migrate safety). */
    fun protectedSlicesPresent(dataDir: File): Boolean {
        for (rid in MIGRATE_KICKOFF_REGIONS) {
            if (!regionHasRows(dataDir, rid)) return false
        }
        return true
    }

    private fun regionHasRows(
        dataDir: File,
        regionId: String,
    ): Boolean {
        val dbFile = File(dataDir, "place_index.db")
        if (!dbFile.isFile) return false
        return runCatching {
            android.database.sqlite.SQLiteDatabase
                .openDatabase(
                    dbFile.absolutePath,
                    null,
                    android.database.sqlite.SQLiteDatabase.OPEN_READONLY,
                ).use { db ->
                    db
                        .rawQuery(
                            "SELECT 1 FROM name_entries WHERE region_id = ? LIMIT 1",
                            arrayOf(regionId),
                        ).use { c -> c.moveToFirst() }
                }
        }.getOrDefault(false)
    }
}
