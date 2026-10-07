package no.navi.app

import android.content.Context
import android.util.Log
import java.io.File
import java.io.FileInputStream
import java.io.FileOutputStream

/**
 * Place-index DB location follows the long-trip **pack volume** (same
 * app-files tree as [LongTripPackStorage.packDownloadDir]), not a separate
 * internal-only path.
 *
 * Rules:
 * - No SD / pack volume → internal app files (same as packs on internal).
 * - Pack volume removed or not writable → [Status.Unavailable]; do **not**
 *   build or open a substitute on another volume.
 * - Changing volume **moves** an existing intact DB; never rebuilds because
 *   of the move alone.
 */
object PlaceIndexStorage {
    private const val TAG = "PlaceIndexStorage"
    const val DB_NAME = "place_index.db"
    private const val LOC_MARKER = "place_index_volume.txt"

    sealed class Status {
        data class Ready(
            val indexDir: File,
            val dbFile: File,
            val volumeId: String,
        ) : Status()

        data class Unavailable(
            val volumeId: String,
            val reason: String,
        ) : Status()
    }

    fun dbName(): String = DB_NAME

    /** Volume id that owns packs + place index. */
    fun volumeId(context: Context): String = LongTripPackStorage.selectedVolumeId(context)

    /**
     * App-files directory on the pack volume (parent of `long-trip-packs`).
     * Null when the selected removable volume is not usable.
     */
    fun indexDirOrNull(context: Context): File? {
        val id = volumeId(context)
        if (id == NaviStorageVolumes.INTERNAL_ID) {
            return NaviAppData.resolve(context)
        }
        val vol = NaviStorageVolumes.findById(context, id) ?: return null
        if (!vol.mounted) return null
        val app = vol.appFilesDir ?: return null
        val packs = File(app, LongTripPackStorage.PACKS_SUBDIR)
        if (!NaviStorageVolumes.probeWritable(packs) && !NaviStorageVolumes.probeWritable(app)) {
            return null
        }
        return app
    }

    fun status(context: Context): Status {
        val id = volumeId(context)
        val dir = indexDirOrNull(context)
        if (dir == null) {
            return Status.Unavailable(
                volumeId = id,
                reason = "pack_volume_unavailable",
            )
        }
        return Status.Ready(
            indexDir = dir,
            dbFile = File(dir, DB_NAME),
            volumeId = id,
        )
    }

    fun dbFile(context: Context): File? =
        when (val s = status(context)) {
            is Status.Ready -> s.dbFile
            is Status.Unavailable -> null
        }

    fun indexDir(context: Context): File? =
        when (val s = status(context)) {
            is Status.Ready -> s.indexDir
            is Status.Unavailable -> null
        }

    /** Human-readable location line for InstalledMaps / logs. */
    fun locationSummary(context: Context): String =
        when (val s = status(context)) {
            is Status.Ready ->
                "place_index vol=${s.volumeId} path=${s.dbFile.absolutePath} " +
                    "bytes=${if (s.dbFile.isFile) s.dbFile.length() else 0}"
            is Status.Unavailable ->
                "place_index UNAVAILABLE vol=${s.volumeId} reason=${s.reason}"
        }

    /**
     * If an intact DB exists on a previous volume marker / internal fallback
     * and the active pack volume differs, move it (copy + verify + delete
     * source). Never rebuilds. No-op when unavailable or already in place.
     */
    fun ensureOnPackVolume(context: Context): Status {
        val ready = status(context)
        if (ready is Status.Unavailable) {
            Log.w(TAG, locationSummary(context))
            return ready
        }
        val target = ready as Status.Ready
        target.indexDir.mkdirs()
        val marker = File(target.indexDir, LOC_MARKER)
        val prevId = runCatching { marker.takeIf { it.isFile }?.readText()?.trim() }.getOrNull()
        if (target.dbFile.isFile && target.dbFile.length() >= PlaceIndexIntact.MIN_DB_BYTES) {
            runCatching { marker.writeText(target.volumeId + "\n") }
            return target
        }
        val candidates = linkedSetOf<File>()
        // Prior marker on another known volume.
        for (vol in NaviStorageVolumes.list(context)) {
            val app = vol.appFilesDir ?: continue
            val cand = File(app, DB_NAME)
            if (cand.isFile && cand.length() >= PlaceIndexIntact.MIN_DB_BYTES) {
                candidates.add(cand)
            }
        }
        val internal = File(NaviAppData.resolve(context), DB_NAME)
        if (internal.isFile && internal.length() >= PlaceIndexIntact.MIN_DB_BYTES) {
            candidates.add(internal)
        }
        val source =
            candidates.firstOrNull { it.absolutePath != target.dbFile.absolutePath }
        if (source == null) {
            runCatching { marker.writeText(target.volumeId + "\n") }
            Log.i(TAG, "no existing DB to move; ${locationSummary(context)}")
            return target
        }
        Log.i(
            TAG,
            "moving place_index ${source.absolutePath} -> ${target.dbFile.absolutePath} " +
                "(prevVol=$prevId newVol=${target.volumeId})",
        )
        if (!copyFileVerified(source, target.dbFile)) {
            Log.e(TAG, "move copy failed; leaving source intact")
            return target
        }
        // Drop WAL/SHM next to destination if we copied only the main file.
        for (suffix in listOf("-wal", "-shm")) {
            val s = File(source.path + suffix)
            val d = File(target.dbFile.path + suffix)
            if (s.isFile) {
                runCatching { copyFileVerified(s, d) }
            }
        }
        if (target.dbFile.length() < PlaceIndexIntact.MIN_DB_BYTES) {
            Log.e(TAG, "moved DB tiny; not deleting source")
            return target
        }
        runCatching { source.delete() }
        for (suffix in listOf("-wal", "-shm")) {
            runCatching { File(source.path + suffix).delete() }
        }
        runCatching { marker.writeText(target.volumeId + "\n") }
        Log.i(TAG, "move complete; ${locationSummary(context)}")
        return target
    }

    private fun copyFileVerified(
        from: File,
        to: File,
    ): Boolean {
        return runCatching {
            to.parentFile?.mkdirs()
            val tmp = File(to.path + ".partial")
            FileInputStream(from).use { inp ->
                FileOutputStream(tmp).use { out -> inp.copyTo(out) }
            }
            if (tmp.length() != from.length()) {
                tmp.delete()
                return false
            }
            if (to.exists()) to.delete()
            if (!tmp.renameTo(to)) {
                FileInputStream(tmp).use { inp ->
                    FileOutputStream(to).use { out -> inp.copyTo(out) }
                }
                tmp.delete()
            }
            to.length() == from.length()
        }.getOrDefault(false)
    }
}
