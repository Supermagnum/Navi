package no.navi.app

import android.content.Context
import android.util.Log
import java.io.File

/**
 * Resolves the on-device directory for large working data (PMTiles, DEM, OSM
 * extracts, elevation cache, graph cache, place index).
 *
 * Prefer **internal** [Context.getFilesDir] (`/data/user/<id>/<pkg>/files`), which
 * sits on the app-private `/data` volume. Do **not** prefer
 * [Context.getExternalFilesDir]: on some Automotive emulators the primary
 * “external” volume is a tiny emulated SD card (~510 MB) that cannot hold a
 * regional basemap+DEM, so downloads stall or fail without an obvious UI error.
 *
 * Existing data under the legacy external files tree is migrated into internal
 * storage once when present (same `/data` filesystem → rename when possible).
 */
object NaviAppData {
    private const val TAG = "NaviAppData"

    fun resolve(context: Context): File {
        val internal = context.filesDir.also { it.mkdirs() }
        migrateLegacyExternal(context, internal)
        Log.i(TAG, "app data dir=${internal.absolutePath}")
        return internal
    }

    private fun migrateLegacyExternal(context: Context, internal: File) {
        val external = context.getExternalFilesDir(null) ?: return
        if (external.absolutePath == internal.absolutePath) return
        val names = listOf(
            "pmtiles",
            "elevation",
            "graph-cache",
            "graph-cache-foot",
            "navi.db",
            "place_index.db",
            "region_meta.json",
        )
        for (name in names) {
            val src = File(external, name)
            if (!src.exists()) continue
            val dst = File(internal, name)
            if (dst.exists()) {
                val dstEmptyDir = dst.isDirectory && (dst.list()?.isEmpty() != false)
                val dstEmptyFile = dst.isFile && dst.length() == 0L
                if (!dstEmptyDir && !dstEmptyFile) continue
                if (dstEmptyDir) dst.delete()
                if (dstEmptyFile) dst.delete()
            }
            val ok = runCatching {
                if (src.isDirectory) {
                    src.copyRecursively(dst, overwrite = false)
                    // Leave external copy if delete fails (root-owned fixtures).
                    runCatching { src.deleteRecursively() }
                } else {
                    if (!src.renameTo(dst)) {
                        src.copyTo(dst, overwrite = false)
                        runCatching { src.delete() }
                    }
                }
                true
            }.getOrDefault(false)
            Log.i(TAG, "migrate $name from ${src.absolutePath} -> ${dst.absolutePath} ok=$ok")
        }
        // Loose region extracts (*.osm.pbf) left on external.
        external.listFiles()
            ?.filter { it.isFile && it.name.endsWith(".osm.pbf") }
            ?.forEach { src ->
                val dst = File(internal, src.name)
                if (dst.exists()) return@forEach
                val ok = runCatching {
                    if (!src.renameTo(dst)) {
                        src.copyTo(dst, overwrite = false)
                        src.delete()
                    }
                    true
                }.getOrDefault(false)
                Log.i(TAG, "migrate ${src.name} ok=$ok")
            }
    }
}
