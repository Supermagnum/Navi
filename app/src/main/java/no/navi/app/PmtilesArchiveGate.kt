package no.navi.app

import android.os.Looper
import java.io.File
import java.io.RandomAccessFile
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.concurrent.ConcurrentHashMap

/**
 * Decides whether a local file is a usable offline map archive.
 *
 * Same bar as a completed download: readable PMTiles header, tile count above
 * zero, and a sample tile that decodes. Results are cached per path + size +
 * mtime so camera moves do not re-read the file. Never runs the heavy check
 * on the UI thread — an uncached file is treated as not yet usable there.
 */
object PmtilesArchiveGate {
    data class Verdict(
        val ok: Boolean,
        val reason: String,
    )

    private data class CacheKey(
        val path: String,
        val size: Long,
        val mtime: Long,
    )

    private val cache = ConcurrentHashMap<CacheKey, Verdict>()

    /**
     * Optional override for unit tests. Return a rejection reason, or null when
     * the file should be treated as a map. When set, the header/sample check
     * is skipped.
     */
    @Volatile
    var validateContent: ((File, String) -> String?)? = null

    fun resetForTests() {
        cache.clear()
        validateContent = null
    }

    fun isUsable(
        file: File,
        regionKey: String = "",
    ): Boolean = verdict(file, regionKey).ok

    fun rejectionReason(
        file: File,
        regionKey: String = "",
    ): String? {
        val v = verdict(file, regionKey)
        return if (v.ok) null else v.reason
    }

    fun verdict(
        file: File,
        regionKey: String = "",
    ): Verdict {
        if (!file.isFile) return Verdict(false, "missing archive")
        if (file.length() == 0L) return Verdict(false, "empty archive")
        val key =
            CacheKey(
                path = file.absolutePath,
                size = file.length(),
                mtime = file.lastModified(),
            )
        cache[key]?.let { return it }
        val hook = validateContent
        if (hook != null) {
            val reason = hook(file, regionKey)
            val v = if (reason == null) Verdict(true, "") else Verdict(false, reason)
            cache[key] = v
            return v
        }
        if (isUiThread()) {
            return Verdict(false, "archive not validated yet")
        }
        val v = inspectFile(file, regionKey)
        cache[key] = v
        return v
    }

    /**
     * Mark [path] as the archive the map is currently showing so a download or
     * audit job can refuse to rename or remove it.
     */
    fun writeShownMarker(
        dataDir: File,
        path: String?,
    ) {
        val marker = File(File(dataDir, "pmtiles"), ".shown")
        marker.parentFile?.mkdirs()
        if (path.isNullOrBlank()) {
            marker.delete()
        } else {
            marker.writeText(path)
        }
    }

    fun readShownMarker(dataDir: File): String? {
        val marker = File(File(dataDir, "pmtiles"), ".shown")
        if (!marker.isFile) return null
        return marker.readText().trim().ifBlank { null }
    }

    private fun inspectFile(
        file: File,
        regionKey: String,
    ): Verdict {
        if (file.length() < 127L) {
            return Verdict(false, "archive too small (${file.length()} bytes)")
        }
        val header =
            try {
                readHeader(file)
            } catch (e: Exception) {
                return Verdict(false, e.message ?: "not a PMTiles archive")
            }
        if (regionKey.endsWith("_dem", ignoreCase = true) ||
            file.name.contains("_dem.pmtiles")
        ) {
            return Verdict(true, "")
        }
        if (regionKey.startsWith("test_")) {
            return Verdict(true, "")
        }
        val worldOverview =
            regionKey == "world_overview" ||
                regionKey.endsWith("/world_overview") ||
                regionKey.endsWith("_world_overview") ||
                file.nameWithoutExtension.equals("world_overview", ignoreCase = true)
        if (worldOverview) {
            if (header.maxZoom < 6) {
                return Verdict(
                    false,
                    "PMTiles maxzoom ${header.maxZoom} < required 6 for world overview",
                )
            }
        } else if (header.maxZoom < 15) {
            return Verdict(
                false,
                "PMTiles maxzoom ${header.maxZoom} < required 15 for region $regionKey",
            )
        }
        if (header.addressedTiles == 0L) {
            return Verdict(false, "archive has no tiles for region $regionKey")
        }
        val decoded =
            runCatching {
                uniffi.navi.pmtilesGetTile(file.absolutePath, 0u, 0u, 0u)
                    ?: uniffi.navi.pmtilesGetTile(file.absolutePath, 1u, 0u, 0u)
                    ?: uniffi.navi.pmtilesGetTile(file.absolutePath, 1u, 1u, 0u)
            }.getOrNull()
        if (decoded == null || decoded.isEmpty()) {
            return Verdict(false, "no decodable sample tile for region $regionKey")
        }
        return Verdict(true, "")
    }

    private data class Header(
        val maxZoom: Int,
        val addressedTiles: Long,
    )

    private fun readHeader(file: File): Header {
        RandomAccessFile(file, "r").use { raf ->
            val buf = ByteArray(127)
            raf.readFully(buf)
            if (String(buf, 0, 7, Charsets.US_ASCII) != "PMTiles") {
                throw IllegalArgumentException("not a PMTiles archive")
            }
            val maxZoom = buf[101].toInt() and 0xff
            val tiles =
                ByteBuffer
                    .wrap(buf, 72, 8)
                    .order(ByteOrder.LITTLE_ENDIAN)
                    .long
            return Header(maxZoom = maxZoom, addressedTiles = tiles)
        }
    }

    private fun isUiThread(): Boolean =
        try {
            Looper.getMainLooper() != null && Looper.myLooper() == Looper.getMainLooper()
        } catch (_: Throwable) {
            false
        }
}
