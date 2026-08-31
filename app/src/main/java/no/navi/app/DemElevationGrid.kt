package no.navi.app

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import kotlin.math.max

/**
 * Elevation samples on a regular lat/lon grid (row 0 = north).
 * Values are metres above sea level.
 */
data class DemElevationGrid(
    val width: Int,
    val height: Int,
    val elev: DoubleArray,
    val west: Double,
    val south: Double,
    val east: Double,
    val north: Double,
) {
    init {
        require(width >= 2 && height >= 2) { "grid must be at least 2x2" }
        require(elev.size == width * height) { "elev size ${elev.size} != ${width}x$height" }
    }
}

data class DemTileLatLngBounds(
    val west: Double,
    val south: Double,
    val east: Double,
    val north: Double,
)

object DemElevationGridDecoder {
    private fun bitmapFactoryNoScaleOpts(): BitmapFactory.Options =
        BitmapFactory.Options().apply {
            inPreferredConfig = Bitmap.Config.ARGB_8888
            inScaled = false
            @Suppress("DEPRECATION")
            inDensity = 0
            @Suppress("DEPRECATION")
            inTargetDensity = 0
        }

    /** Decode terrarium RGB (WebP/PNG bytes) into an elevation grid covering [bounds]. */
    fun fromTerrariumBytes(
        bytes: ByteArray,
        bounds: DemTileLatLngBounds,
    ): DemElevationGrid? = fromTerrariumBytes(bytes, bounds.west, bounds.south, bounds.east, bounds.north)

    fun fromTerrariumBytes(
        bytes: ByteArray,
        west: Double,
        south: Double,
        east: Double,
        north: Double,
    ): DemElevationGrid? {
        val bmp =
            BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bitmapFactoryNoScaleOpts())
                ?: return null
        val w = bmp.width
        val h = bmp.height
        if (w < 2 || h < 2) {
            bmp.recycle()
            return null
        }
        val pixels = IntArray(w * h)
        bmp.getPixels(pixels, 0, w, 0, 0, w, h)
        bmp.recycle()
        val elev = DoubleArray(w * h)
        for (i in pixels.indices) {
            val c = pixels[i]
            val r = (c ushr 16) and 0xff
            val g = (c ushr 8) and 0xff
            val b = c and 0xff
            elev[i] = LocalDemTileServer.terrariumElevM(r, g, b)
        }
        return DemElevationGrid(w, h, elev, west, south, east, north)
    }

    /**
     * Bilinear-resample [source] onto a grid covering [bounds] with at most
     * [maxDim] samples per axis (performance LOD for high camera zoom).
     */
    fun resample(
        source: DemElevationGrid,
        bounds: DemTileLatLngBounds,
        maxDim: Int,
    ): DemElevationGrid = resample(source, bounds.west, bounds.south, bounds.east, bounds.north, maxDim)

    fun resample(
        source: DemElevationGrid,
        boundsWest: Double,
        boundsSouth: Double,
        boundsEast: Double,
        boundsNorth: Double,
        maxDim: Int,
    ): DemElevationGrid {
        val spanLon = max(1e-9, boundsEast - boundsWest)
        val spanLat = max(1e-9, boundsNorth - boundsSouth)
        val srcLon = max(1e-9, source.east - source.west)
        val srcLat = max(1e-9, source.north - source.south)
        val aspect = spanLon / spanLat
        val outW =
            if (aspect >= 1.0) {
                maxDim
            } else {
                max(2, (maxDim * aspect).toInt())
            }
        val outH =
            if (aspect >= 1.0) {
                max(2, (maxDim / aspect).toInt())
            } else {
                maxDim
            }
        val out = DoubleArray(outW * outH)
        for (j in 0 until outH) {
            val lat = boundsNorth - (j.toDouble() / (outH - 1)) * spanLat
            val fy = ((source.north - lat) / srcLat) * (source.height - 1)
            for (i in 0 until outW) {
                val lon = boundsWest + (i.toDouble() / (outW - 1)) * spanLon
                val fx = ((lon - source.west) / srcLon) * (source.width - 1)
                out[j * outW + i] = sampleBilinear(source, fx, fy)
            }
        }
        return DemElevationGrid(outW, outH, out, boundsWest, boundsSouth, boundsEast, boundsNorth)
    }

    private fun sampleBilinear(
        grid: DemElevationGrid,
        fx: Double,
        fy: Double,
    ): Double {
        val x = fx.coerceIn(0.0, (grid.width - 1).toDouble())
        val y = fy.coerceIn(0.0, (grid.height - 1).toDouble())
        val x0 = x.toInt().coerceAtMost(grid.width - 2)
        val y0 = y.toInt().coerceAtMost(grid.height - 2)
        val tx = x - x0
        val ty = y - y0

        fun at(
            xi: Int,
            yi: Int,
        ): Double = grid.elev[yi * grid.width + xi]
        val v00 = at(x0, y0)
        val v10 = at(x0 + 1, y0)
        val v01 = at(x0, y0 + 1)
        val v11 = at(x0 + 1, y0 + 1)
        if (v00.isNaN() || v10.isNaN() || v01.isNaN() || v11.isNaN()) return Double.NaN
        val a = v00 * (1 - tx) + v10 * tx
        val b = v01 * (1 - tx) + v11 * tx
        return a * (1 - ty) + b * ty
    }

    /**
     * Mosaic same-size DEM tiles (512x512) into one grid. [tiles] must share
     * [DemElevationGrid.width]/height and form a contiguous xyz block.
     */
    fun stitch(tiles: List<DemElevationGrid>): DemElevationGrid? {
        if (tiles.isEmpty()) return null
        if (tiles.size == 1) return tiles[0]
        val tileW = tiles.first().width
        val tileH = tiles.first().height
        if (tiles.any { it.width != tileW || it.height != tileH }) return null
        // Sort west→east, north→south using geographic bounds.
        val sorted =
            tiles.sortedWith(
                compareBy<DemElevationGrid>({ -it.north }).thenBy { it.west },
            )
        val west = sorted.minOf { it.west }
        val east = sorted.maxOf { it.east }
        val south = sorted.minOf { it.south }
        val north = sorted.maxOf { it.north }
        val latTol = (north - south) * 0.001 + 1e-9
        val rows = sorted.groupBy { tile -> (tile.north / latTol).toLong() }
        val rowKeys = rows.keys.sortedDescending()
        val cols = rowKeys.firstOrNull()?.let { rows[it]?.size } ?: return null
        if (rowKeys.any { (rows[it]?.size ?: 0) != cols }) return null
        val outW = cols * tileW
        val outH = rowKeys.size * tileH
        val out = DoubleArray(outW * outH)
        for ((rowIdx, rowKey) in rowKeys.withIndex()) {
            val rowTiles = rows[rowKey]!!.sortedBy { it.west }
            for ((colIdx, tile) in rowTiles.withIndex()) {
                for (j in 0 until tileH) {
                    for (i in 0 until tileW) {
                        val src = tile.elev[j * tileW + i]
                        out[(rowIdx * tileH + j) * outW + colIdx * tileW + i] = src
                    }
                }
            }
        }
        return DemElevationGrid(outW, outH, out, west, south, east, north)
    }
}

/** Web-Mercator tile bounds (xyz). */
object DemTileBounds {
    fun xyz(
        z: Int,
        x: Int,
        y: Int,
    ): DemTileLatLngBounds {
        val n = 1 shl z
        val west = x / n.toDouble() * 360.0 - 180.0
        val east = (x + 1) / n.toDouble() * 360.0 - 180.0
        val northRad = Math.PI * (1.0 - 2.0 * y / n.toDouble())
        val southRad = Math.PI * (1.0 - 2.0 * (y + 1) / n.toDouble())
        val north = Math.toDegrees(Math.atan(Math.sinh(northRad)))
        val south = Math.toDegrees(Math.atan(Math.sinh(southRad)))
        return DemTileLatLngBounds(west, south, east, north)
    }

    fun tileXY(
        z: Int,
        lon: Double,
        lat: Double,
    ): Pair<Int, Int> {
        val n = 1 shl z
        val x = ((lon + 180.0) / 360.0 * n).toInt().coerceIn(0, n - 1)
        val latRad = Math.toRadians(lat.coerceIn(-85.0511287, 85.0511287))
        val y =
            (
                (1.0 - Math.log(Math.tan(latRad) + 1.0 / Math.cos(latRad)) / Math.PI) /
                    2.0 * n
            ).toInt().coerceIn(0, n - 1)
        return x to y
    }
}
