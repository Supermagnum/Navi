package no.navi.app

import android.database.sqlite.SQLiteDatabase
import android.util.Log
import java.io.File

/**
 * Regions whose place-index rows are safe to search.
 *
 * Written only after a region's downloads finish and its place index build
 * returns PASS. Cleared when a new download for that region starts so From/Via/To
 * never surfaces partial or in-progress index rows.
 *
 * Search also requires the region to be present on disk (downloaded packs /
 * extract). Hits outside downloaded ∪ ready regions are never shown.
 */
object PlaceIndexReady {
    const val READY_FILE = "place-index-ready.json"
    private const val TAG = "PlaceIndexReady"

    fun readyFile(dataDir: File): File = File(dataDir, READY_FILE)

    fun load(dataDir: File): Set<String> {
        healReadyFromDownloads(dataDir)
        val f = readyFile(dataDir)
        if (f.isFile) {
            return parseJsonStringArray(f.readText())
                .map { PackRegionAvailability.normalize(it) }
                .filter { it.isNotEmpty() }
                .toSet()
        }
        // Legacy installs (no stamp file yet): treat every region_id already in
        // the DB as ready, and persist the stamp so later clears stick.
        val discovered = discoverRegionIdsFromDb(dataDir)
        if (discovered.isNotEmpty()) {
            save(dataDir, discovered)
        }
        return discovered
    }

    /**
     * When the stamp is empty/`[]` but the DB already has rows for downloaded
     * regions (background index finished without [markReady], or stamp lost),
     * adopt those region ids so search works again.
     *
     * Skips while a download or place-index job is running so mid-build rows
     * are not stamped ready early.
     */
    fun healReadyFromDownloads(dataDir: File) {
        if (RegionDownloadBackground.isRunning()) return
        if (PlaceIndexBackground.isRunning()) return
        val f = readyFile(dataDir)
        val stamped =
            if (f.isFile) {
                parseJsonStringArray(f.readText())
                    .map { PackRegionAvailability.normalize(it) }
                    .filter { it.isNotEmpty() }
                    .toSet()
            } else {
                null
            }
        if (stamped != null && stamped.isNotEmpty()) return

        val downloaded =
            RegionCoverage
                .downloadedGeofabrikPaths(dataDir)
                .map { PackRegionAvailability.normalize(it) }
                .filter { it.isNotEmpty() }
                .toSet()
        val inDb = discoverRegionIdsFromDb(dataDir)
        if (inDb.isEmpty()) return

        val healed =
            if (downloaded.isEmpty()) {
                // No PBF/manifest heuristic (unit tests / fixtures): trust DB.
                inDb
            } else {
                inDb
                    .filter { id ->
                        downloaded.any { d -> regionMatches(d, id) }
                    }.toSet()
            }
        if (healed.isEmpty()) return
        if (healed == (stamped ?: emptySet<String>())) return
        save(dataDir, healed)
        runCatching { Log.i(TAG, "healed ready stamp regions=$healed") }
    }

    fun markReady(
        dataDir: File,
        regionId: String,
    ) {
        val id = PackRegionAvailability.normalize(regionId)
        if (id.isEmpty()) return
        // Read stamp without heal side effects so an empty [] stays authoritative
        // until we add this id (heal could otherwise race mid-clear).
        val current = loadStampOnly(dataDir).toMutableSet()
        current.add(id)
        save(dataDir, current)
        runCatching { Log.i(TAG, "mark ready region=$id") }
    }

    fun clearReady(
        dataDir: File,
        regionId: String,
    ) {
        val id = PackRegionAvailability.normalize(regionId)
        if (id.isEmpty()) return
        val next = loadStampOnly(dataDir).toMutableSet()
        next.remove(id)
        // Always persist so the stamp file becomes authoritative (even as []).
        save(dataDir, next)
        clearRegionRows(dataDir, id)
        runCatching { Log.i(TAG, "clear ready region=$id") }
    }

    fun isReady(
        dataDir: File,
        regionId: String,
    ): Boolean {
        val id = PackRegionAvailability.normalize(regionId)
        if (id.isEmpty()) return false
        return load(dataDir).any {
            PackRegionAvailability.regionIdsMatchForCatalog(it, id)
        }
    }

    /**
     * Regions allowed in From/Via/To: ready stamp, restricted to downloaded
     * regions when any are on disk. Hits must match by [PlaceHit.regionId]
     * (preferred) or lat/lon Geofabrik suggestion (legacy empty region_id).
     */
    fun searchAllowedRegions(dataDir: File): Set<String> {
        val ready = load(dataDir)
        if (ready.isEmpty()) return emptySet()
        val downloaded =
            RegionCoverage
                .downloadedGeofabrikPaths(dataDir)
                .map { PackRegionAvailability.normalize(it) }
                .filter { it.isNotEmpty() }
                .toSet()
        if (downloaded.isEmpty()) return ready
        return ready
            .filter { r ->
                downloaded.any { d -> regionMatches(d, r) }
            }.toSet()
    }

    /**
     * Keep hits that belong to a ready (and, when known, downloaded) region.
     * Never surface places outside those regions.
     */
    fun filterHitsToReadyRegions(
        dataDir: File,
        hits: List<uniffi.navi.PlaceHit>,
    ): List<uniffi.navi.PlaceHit> {
        val allowed = searchAllowedRegions(dataDir)
        if (allowed.isEmpty()) return emptyList()
        return hits.filter { hit ->
            val fromDb =
                PackRegionAvailability
                    .normalize(hit.regionId)
                    .takeIf { it.isNotEmpty() }
            val path =
                fromDb
                    ?: runCatching { RegionCoverage.suggestGeofabrikPath(hit.lat, hit.lon) }
                        .getOrNull()
                        ?.let { PackRegionAvailability.normalize(it) }
                        .orEmpty()
            if (path.isEmpty()) return@filter false
            allowed.any { r -> regionMatches(r, path) }
        }
    }

    private fun regionMatches(
        a: String,
        b: String,
    ): Boolean =
        PackRegionAvailability.regionIdsMatchForCatalog(a, b) ||
            a.startsWith("$b/") ||
            b.startsWith("$a/")

    private fun loadStampOnly(dataDir: File): Set<String> {
        val f = readyFile(dataDir)
        if (!f.isFile) return emptySet()
        return parseJsonStringArray(f.readText())
            .map { PackRegionAvailability.normalize(it) }
            .filter { it.isNotEmpty() }
            .toSet()
    }

    private fun save(
        dataDir: File,
        ids: Set<String>,
    ) {
        dataDir.mkdirs()
        val ordered = ids.map { PackRegionAvailability.normalize(it) }.filter { it.isNotEmpty() }.sorted()
        readyFile(dataDir).writeText(
            ordered.joinToString(prefix = "[", postfix = "]") { jsonQuote(it) },
        )
    }

    private fun discoverRegionIdsFromDb(dataDir: File): Set<String> {
        val dbFile = File(dataDir, "place_index.db")
        if (!dbFile.isFile || dbFile.length() < 100L) return emptySet()
        return runCatching {
            SQLiteDatabase.openDatabase(dbFile.absolutePath, null, SQLiteDatabase.OPEN_READONLY).use { db ->
                db
                    .rawQuery(
                        "SELECT DISTINCT region_id FROM name_entries WHERE region_id != ''",
                        null,
                    ).use { c ->
                        buildSet {
                            while (c.moveToNext()) {
                                val id = PackRegionAvailability.normalize(c.getString(0) ?: "")
                                if (id.isNotEmpty()) add(id)
                            }
                        }
                    }
            }
        }.getOrDefault(emptySet())
    }

    /** Mirror core NameIndex::clear_region_rows (entries + best-effort FTS). */
    private fun clearRegionRows(
        dataDir: File,
        regionId: String,
    ) {
        val dbFile = File(dataDir, "place_index.db")
        if (!dbFile.isFile) return
        runCatching {
            SQLiteDatabase.openDatabase(dbFile.absolutePath, null, SQLiteDatabase.OPEN_READWRITE).use { db ->
                db.beginTransaction()
                try {
                    val osmIds = mutableListOf<Long>()
                    db
                        .rawQuery(
                            "SELECT osm_id FROM name_entries WHERE region_id = ?",
                            arrayOf(regionId),
                        ).use { c ->
                            while (c.moveToNext()) {
                                osmIds.add(c.getLong(0))
                            }
                        }
                    // Android framework SQLite may lack the FTS5 module even when
                    // rusqlite created name_fts. Never let FTS failure roll back
                    // the content-table delete — stale searchable rows are worse.
                    var ftsCleared = 0
                    var ftsUnavailable = false
                    for (osmId in osmIds) {
                        try {
                            db.execSQL(
                                "INSERT INTO name_fts(name_fts, rowid, name, kind) " +
                                    "VALUES('delete', ?, NULL, NULL)",
                                arrayOf(osmId),
                            )
                            ftsCleared++
                        } catch (t: Throwable) {
                            val msg = t.message.orEmpty()
                            if (msg.contains("fts5", ignoreCase = true) ||
                                msg.contains("no such module", ignoreCase = true)
                            ) {
                                ftsUnavailable = true
                                break
                            }
                            Log.w(TAG, "FTS delete failed osm_id=$osmId: ${t.message}")
                        }
                    }
                    db.execSQL(
                        "DELETE FROM name_entries WHERE region_id = ?",
                        arrayOf(regionId),
                    )
                    db.setTransactionSuccessful()
                    if (ftsUnavailable) {
                        Log.w(
                            TAG,
                            "clearRegionRows: FTS5 unavailable on device SQLite; " +
                                "deleted ${osmIds.size} name_entries for $regionId " +
                                "(fts_cleared=$ftsCleared). Re-index via rusqlite " +
                                "rebuilds FTS; orphan FTS rows may linger until then.",
                        )
                    }
                } finally {
                    db.endTransaction()
                }
            }
        }.onFailure { Log.w(TAG, "clearRegionRows failed for $regionId: ${it.message}") }
    }

    internal fun prioritizePaths(
        paths: List<String>,
        userLat: Double?,
        userLon: Double?,
    ): List<String> {
        if (paths.size <= 1) return paths
        val normalized =
            paths
                .map { PackRegionAvailability.normalize(it) }
                .filter { it.isNotEmpty() }
                .distinct()
        if (userLat == null || userLon == null || !userLat.isFinite() || !userLon.isFinite()) {
            return normalized
        }
        val here =
            runCatching { RegionCoverage.suggestGeofabrikPath(userLat, userLon) }
                .getOrNull()
                ?.let { PackRegionAvailability.normalize(it) }
                .orEmpty()
        if (here.isEmpty()) return normalized
        val match =
            normalized.firstOrNull {
                PackRegionAvailability.regionIdsMatchForCatalog(it, here) ||
                    here.startsWith("$it/") ||
                    it.startsWith("$here/")
            } ?: return normalized
        return listOf(match) + normalized.filter { it != match }
    }

    internal fun parseJsonStringArray(text: String): List<String> {
        val out = mutableListOf<String>()
        var i = 0
        while (i < text.length) {
            val start = text.indexOf('"', i)
            if (start < 0) break
            var j = start + 1
            val buf = StringBuilder()
            while (j < text.length) {
                val c = text[j++]
                when {
                    c == '\\' && j < text.length -> buf.append(text[j++])
                    c == '"' -> {
                        out.add(buf.toString())
                        break
                    }
                    else -> buf.append(c)
                }
            }
            i = j
        }
        return out
    }

    private fun jsonQuote(s: String): String =
        buildString {
            append('"')
            for (c in s) {
                when (c) {
                    '\\' -> append("\\\\")
                    '"' -> append("\\\"")
                    else -> append(c)
                }
            }
            append('"')
        }
}
