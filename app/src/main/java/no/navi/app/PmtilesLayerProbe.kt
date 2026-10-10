package no.navi.app

import uniffi.navi.pmtilesGetTile
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.ln
import kotlin.math.tan

/** Reads source-layer names from an archive tile at the camera. */
object PmtilesLayerProbe {
    fun layersAt(
        path: String,
        lat: Double,
        lon: Double,
        zoom: Double,
    ): Set<String> {
        val z = zoom.toInt().coerceIn(0, 15)
        val (x, y) = lonLatToTile(lat, lon, z)
        val bytes =
            runCatching {
                pmtilesGetTile(path, z.toUByte(), x.toUInt(), y.toUInt())
            }.getOrNull() ?: return emptySet()
        return mvtLayerNames(bytes)
    }

    fun hasRoads(layers: Set<String>): Boolean = layers.any { it == "roads" }

    fun hasWater(layers: Set<String>): Boolean = layers.any { it == "water" }

    fun hasLabels(layers: Set<String>): Boolean =
        layers.any { it == "places" || it == "pois" || it == "water" || it == "roads" }

    internal fun lonLatToTile(
        lat: Double,
        lon: Double,
        z: Int,
    ): Pair<Int, Int> {
        val n = 1 shl z
        val x = ((lon + 180.0) / 360.0 * n).toInt().coerceIn(0, n - 1)
        val latRad = lat.coerceIn(-85.0511287, 85.0511287) * PI / 180.0
        val y =
            (
                (1.0 - ln(tan(latRad) + 1.0 / cos(latRad)) / PI) / 2.0 * n
            ).toInt().coerceIn(0, n - 1)
        return x to y
    }

    /** Layer names from a Mapbox Vector Tile (Tile.layers[].name). */
    internal fun mvtLayerNames(bytes: ByteArray): Set<String> {
        val names = LinkedHashSet<String>()
        var i = 0
        while (i < bytes.size) {
            val (key, next) = readVarint(bytes, i) ?: break
            i = next
            val field = (key shr 3).toInt()
            val wire = (key and 7uL).toInt()
            if (wire != 2) {
                i = skip(bytes, i, wire) ?: break
                continue
            }
            val (len, afterLen) = readVarint(bytes, i) ?: break
            i = afterLen
            val end = i + len.toInt()
            if (end > bytes.size) break
            if (field == 3) {
                layerName(bytes, i, end)?.let { names.add(it) }
            }
            i = end
        }
        return names
    }

    private fun layerName(
        bytes: ByteArray,
        start: Int,
        end: Int,
    ): String? {
        var i = start
        while (i < end) {
            val (key, next) = readVarint(bytes, i) ?: return null
            i = next
            val field = (key shr 3).toInt()
            val wire = (key and 7uL).toInt()
            if (wire != 2) {
                i = skip(bytes, i, wire) ?: return null
                continue
            }
            val (len, afterLen) = readVarint(bytes, i) ?: return null
            i = afterLen
            val close = i + len.toInt()
            if (close > end) return null
            if (field == 1) {
                return String(bytes, i, len.toInt(), Charsets.UTF_8)
            }
            i = close
        }
        return null
    }

    private fun readVarint(
        bytes: ByteArray,
        start: Int,
    ): Pair<ULong, Int>? {
        var result = 0uL
        var shift = 0
        var i = start
        while (i < bytes.size && shift <= 63) {
            val b = bytes[i].toInt() and 0xFF
            i += 1
            result = result or ((b and 0x7F).toULong() shl shift)
            if (b and 0x80 == 0) return result to i
            shift += 7
        }
        return null
    }

    private fun skip(
        bytes: ByteArray,
        start: Int,
        wire: Int,
    ): Int? {
        return when (wire) {
            0 -> {
                val r = readVarint(bytes, start) ?: return null
                r.second
            }
            1 -> if (start + 8 <= bytes.size) start + 8 else null
            2 -> {
                val (len, after) = readVarint(bytes, start) ?: return null
                val end = after + len.toInt()
                if (end <= bytes.size) end else null
            }
            5 -> if (start + 4 <= bytes.size) start + 4 else null
            else -> null
        }
    }
}
