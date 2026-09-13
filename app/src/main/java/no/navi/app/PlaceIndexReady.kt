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
 */
object PlaceIndexReady {
    const val READY_FILE = "place-index-ready.json"
    private const val TAG = "PlaceIndexReady"

    fun readyFile(dataDir: File): File = File(dataDir, READY_FILE)

    fun load(dataDir: File): Set<String> {
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

    fun markReady(
        dataDir: File,
        regionId: String,
    ) {
        val id = PackRegionAvailability.normalize(regionId)
        if (id.isEmpty()) return
        val next = load(dataDir).toMutableSet()
        next.add(id)
        save(dataDir, next)
        runCatching { Log.i(TAG, "mark ready region=$id") }
    }

    fun clearReady(
        dataDir: File,
        regionId: String,
    ) {
        val id = PackRegionAvailability.normalize(regionId)
        if (id.isEmpty()) return
        val next = load(dataDir).toMutableSet()
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
     * Keep hits whose lat/lon fall in a ready Geofabrik region. Hits outside any
     * known landsdel bbox are dropped when a ready set exists.
     */
    fun filterHitsToReadyRegions(
        dataDir: File,
        hits: List<uniffi.navi.PlaceHit>,
    ): List<uniffi.navi.PlaceHit> {
        val ready = load(dataDir)
        if (ready.isEmpty()) return emptyList()
        return hits.filter { hit ->
            val path =
                runCatching { RegionCoverage.suggestGeofabrikPath(hit.lat, hit.lon) }
                    .getOrNull()
                    ?.let { PackRegionAvailability.normalize(it) }
                    .orEmpty()
            if (path.isEmpty()) return@filter false
            ready.any { r ->
                PackRegionAvailability.regionIdsMatchForCatalog(r, path) ||
                    path.startsWith("$r/") ||
                    r.startsWith("$path/")
            }
        }
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
