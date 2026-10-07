package no.navi.app

import java.io.File

/**
 * Standalone auto-index (launch poller / [PlaceIndexBackground]) for downloaded
 * regions that are missing or not intact. One region at a time; never the main
 * thread; never while [RoutePlanGate] holds a plan.
 *
 * General rules (no product hard-coding of test corridor region ids):
 * - Schema upgrades run in-place in native [ensurePlaceIndex] open first.
 * - While user_version is old, kick a writer open on any region that already
 *   has rows so a missing-region PBF rebuild cannot race the PK migration.
 * - Never start a missing-region build while migration is incomplete or the DB
 *   fails a basic integrity check (empty/stub/unreadable/schema-old).
 */
object PlaceIndexAutoBuild {
    const val ENABLE_MISSING_REGION_AUTO_INDEX = true

    fun schemaNeedsMigrate(dataDir: File): Boolean {
        val dbFile = File(dataDir, "place_index.db")
        if (!dbFile.isFile || dbFile.length() < PlaceIndexIntact.MIN_DB_BYTES) return false
        val v = readUserVersion(dataDir) ?: return false
        return v in 1 until PlaceIndexIntact.SCHEMA_VERSION
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
            // check must not rebuild while schema is still old.
            return rid == migrateKickoffRegion(dataDir)
        }
        if (!dbReadyForMissingRegionBuilds(dataDir)) return false
        if (PlaceIndexIntact.isIntact(dataDir, rid)) return false
        if (!ENABLE_MISSING_REGION_AUTO_INDEX) return false
        return true
    }

    /** First missing/not-intact installed region (stable sorted order). */
    fun nextRegion(dataDir: File): String? {
        if (PlaceIndexReady.deferWritesDuringPlan()) return null
        if (schemaNeedsMigrate(dataDir)) {
            return migrateKickoffRegion(dataDir)
        }
        if (!dbReadyForMissingRegionBuilds(dataDir)) return null
        val ids =
            InstalledMaps.current()?.regions?.keys?.map { PackRegionAvailability.normalize(it) }
                ?.filter { it.isNotEmpty() }
                ?.distinct()
                ?.sorted()
                .orEmpty()
        if (ids.isEmpty()) return null
        return ids.firstOrNull { mayStart(dataDir, it) }
    }

    /**
     * Any installed region that already has rows (or any populated region_id in
     * the DB when the install list is empty). Used only to open() for migrate.
     */
    private fun migrateKickoffRegion(dataDir: File): String? {
        val installed =
            InstalledMaps.current()?.regions?.keys?.map { PackRegionAvailability.normalize(it) }
                ?.filter { it.isNotEmpty() }
                ?.distinct()
                .orEmpty()
        for (rid in installed) {
            val pbf = OfflineIndexGate.resolveAutoIndexPbf(dataDir, rid) ?: continue
            if (!OfflineIndexGate.isIndexablePbf(pbf)) continue
            if (regionHasRows(dataDir, rid)) return rid
        }
        // Fallback: any region_id already present in the DB.
        return firstPopulatedRegionId(dataDir)?.takeIf { rid ->
            OfflineIndexGate.resolveAutoIndexPbf(dataDir, rid)?.let {
                OfflineIndexGate.isIndexablePbf(it)
            } == true
        }
    }

    /**
     * True when the shared DB is at the current schema, name_entries already has
     * the (region_id, osm_id) primary key (migrate finished), and the DB is not
     * an empty quarantine stub. Does not name any product/test region.
     */
    fun dbReadyForMissingRegionBuilds(dataDir: File): Boolean {
        val dbFile = File(dataDir, "place_index.db")
        if (!dbFile.isFile || dbFile.length() < PlaceIndexIntact.MIN_DB_BYTES) return false
        return runCatching {
            android.database.sqlite.SQLiteDatabase
                .openDatabase(
                    dbFile.absolutePath,
                    null,
                    android.database.sqlite.SQLiteDatabase.OPEN_READONLY,
                ).use { db ->
                    val userVersion =
                        db.rawQuery("PRAGMA user_version", null).use { c ->
                            if (c.moveToFirst()) c.getInt(0) else 0
                        }
                    if (userVersion < PlaceIndexIntact.SCHEMA_VERSION) return@use false
                    // After migrate, name_entries_pk is renamed away; require the
                    // live table DDL to carry the composite PK instead.
                    val ddl =
                        db
                            .rawQuery(
                                "SELECT sql FROM sqlite_master WHERE type='table' AND name=? LIMIT 1",
                                arrayOf("name_entries"),
                            ).use { c ->
                                if (c.moveToFirst()) c.getString(0).orEmpty() else ""
                            }
                    val hasPk =
                        ddl.contains("PRIMARY KEY (region_id, osm_id)") ||
                            ddl.contains("PRIMARY KEY(region_id, osm_id)")
                    if (!hasPk) return@use false
                    db.rawQuery("SELECT 1 FROM name_entries LIMIT 1", null).use { it.moveToFirst() }
                }
        }.getOrDefault(false)
    }

    /** @deprecated Use [dbReadyForMissingRegionBuilds]. Kept for existing call sites/tests. */
    fun protectedSlicesPresent(dataDir: File): Boolean = dbReadyForMissingRegionBuilds(dataDir)

    private fun readUserVersion(dataDir: File): Int? {
        val dbFile = File(dataDir, "place_index.db")
        if (!dbFile.isFile) return null
        return runCatching {
            android.database.sqlite.SQLiteDatabase
                .openDatabase(
                    dbFile.absolutePath,
                    null,
                    android.database.sqlite.SQLiteDatabase.OPEN_READONLY,
                ).use { db ->
                    db.rawQuery("PRAGMA user_version", null).use { c ->
                        if (c.moveToFirst()) c.getInt(0) else 0
                    }
                }
        }.getOrNull()
    }

    private fun firstPopulatedRegionId(dataDir: File): String? {
        val dbFile = File(dataDir, "place_index.db")
        if (!dbFile.isFile) return null
        return runCatching {
            android.database.sqlite.SQLiteDatabase
                .openDatabase(
                    dbFile.absolutePath,
                    null,
                    android.database.sqlite.SQLiteDatabase.OPEN_READONLY,
                ).use { db ->
                    db
                        .rawQuery(
                            "SELECT region_id FROM name_entries WHERE region_id IS NOT NULL " +
                                "AND length(region_id) > 0 LIMIT 1",
                            null,
                        ).use { c ->
                            if (c.moveToFirst()) c.getString(0) else null
                        }
                }
        }.getOrNull()
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
