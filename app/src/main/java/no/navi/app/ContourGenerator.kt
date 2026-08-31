package no.navi.app

import com.google.gson.JsonObject
import org.maplibre.geojson.Feature
import org.maplibre.geojson.LineString
import org.maplibre.geojson.Point
import kotlin.math.abs
import kotlin.math.ceil
import kotlin.math.floor

/**
 * Marching-squares isolines from a [DemElevationGrid].
 *
 * Interval ladder follows [Kartverket](https://www.kartverket.no/) map-series
 * ekvidistanse (minor) and tellekurve (index) conventions: index spacing is
 * always 5× minor. See `docs/map-styles.md` for the Navi zoom → scale-tier
 * mapping.
 */
object ContourGenerator {
    const val PROP_ELEV = "ele"
    const val PROP_MAJOR = "major"

    /**
     * Minor and major contour spacing (metres) for a map zoom level.
     *
     * | Navi zoom | Kartverket tier (approx.) | Minor | Index (5×) |
     * |-----------|---------------------------|-------|------------|
     * | 9–10      | 1:100 000                 | 30 m  | 150 m      |
     * | 11–12     | N50 1:50 000              | 20 m  | 100 m      |
     * | 13–14     | N20 1:20 000              | 10 m  | 50 m       |
     * | 15–20     | N5 1:5 000                | 5 m   | 25 m       |
     */
    fun intervalsForZoom(zoom: Int): Pair<Double, Double>? =
        when {
            zoom < 9 -> null
            zoom <= 10 -> 30.0 to 150.0
            zoom <= 12 -> 20.0 to 100.0
            zoom <= 14 -> 10.0 to 50.0
            else -> 5.0 to 25.0
        }

    fun generateFeatures(
        grid: DemElevationGrid,
        zoom: Int,
    ): List<Feature> {
        val (minor, major) = intervalsForZoom(zoom) ?: return emptyList()
        val minElev = grid.elev.minOrNull() ?: return emptyList()
        val maxElev = grid.elev.maxOrNull() ?: return emptyList()
        if (minElev.isNaN() || maxElev.isNaN()) return emptyList()
        val start = floor(minElev / minor) * minor
        val end = ceil(maxElev / minor) * minor
        val features = mutableListOf<Feature>()
        var level = start
        while (level <= end + 1e-6) {
            val rem = abs(level % major)
            val isMajor = rem < 1e-3 || abs(rem - major) < 1e-3
            for (segment in marchingSquares(grid, level)) {
                if (segment.size < 2) continue
                val line = LineString.fromLngLats(segment)
                val props =
                    JsonObject().apply {
                        addProperty(PROP_ELEV, level)
                        addProperty(PROP_MAJOR, isMajor)
                    }
                features.add(Feature.fromGeometry(line, props))
            }
            level += minor
        }
        return features
    }

    private fun marchingSquares(
        grid: DemElevationGrid,
        level: Double,
    ): List<List<Point>> {
        val w = grid.width
        val h = grid.height
        val segments = mutableListOf<List<Point>>()
        for (y in 0 until h - 1) {
            for (x in 0 until w - 1) {
                val bl = grid.elev[y * w + x]
                val br = grid.elev[y * w + x + 1]
                val tr = grid.elev[(y + 1) * w + x + 1]
                val tl = grid.elev[(y + 1) * w + x]
                if (bl.isNaN() || br.isNaN() || tr.isNaN() || tl.isNaN()) continue
                var idx = 0
                if (bl >= level) idx = idx or 1
                if (br >= level) idx = idx or 2
                if (tr >= level) idx = idx or 4
                if (tl >= level) idx = idx or 8
                if (idx == 0 || idx == 15) continue
                val lonSpan = grid.east - grid.west
                val latSpan = grid.north - grid.south

                fun lon(i: Int): Double = grid.west + (i.toDouble() / (w - 1)) * lonSpan

                fun lat(j: Int): Double = grid.north - (j.toDouble() / (h - 1)) * latSpan

                fun interp(
                    v1: Double,
                    v2: Double,
                    c1: Double,
                    c2: Double,
                ): Double {
                    val d = v2 - v1
                    if (abs(d) < 1e-9) return (c1 + c2) * 0.5
                    val t = ((level - v1) / d).coerceIn(0.0, 1.0)
                    return c1 + t * (c2 - c1)
                }
                val bottom = Point.fromLngLat(interp(bl, br, lon(x), lon(x + 1)), lat(y))
                val right = Point.fromLngLat(lon(x + 1), interp(br, tr, lat(y), lat(y + 1)))
                val top = Point.fromLngLat(interp(tl, tr, lon(x), lon(x + 1)), lat(y + 1))
                val left = Point.fromLngLat(lon(x), interp(bl, tl, lat(y), lat(y + 1)))
                when (idx) {
                    1, 14 -> segments.add(listOf(left, bottom))
                    2, 13 -> segments.add(listOf(bottom, right))
                    3, 12 -> segments.add(listOf(left, right))
                    4, 11 -> segments.add(listOf(right, top))
                    5 -> {
                        segments.add(listOf(left, top))
                        segments.add(listOf(bottom, right))
                    }
                    6, 9 -> segments.add(listOf(bottom, top))
                    7, 8 -> segments.add(listOf(left, top))
                    10 -> {
                        segments.add(listOf(bottom, left))
                        segments.add(listOf(right, top))
                    }
                }
            }
        }
        return segments
    }
}
