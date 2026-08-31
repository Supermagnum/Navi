package no.navi.app

import org.maplibre.geojson.Feature
import org.maplibre.geojson.LineString

/**
 * LRU cache for marching-squares output keyed by DEM tile + map zoom.
 *
 * MapLibre [CustomGeometrySource] also retains loaded geometry tiles until
 * zoom/bounds invalidation, but that cache is opaque — this layer avoids
 * repeating DEM decode + marching squares when the user pans back over an
 * area or when adjacent viewport tiles share the same parent DEM tile.
 */
object ContourGenerationCache {
    private const val MAX_ENTRIES = 72

    private val lock = Any()
    private val store =
        object : LinkedHashMap<String, List<Feature>>(MAX_ENTRIES, 0.75f, true) {
            override fun removeEldestEntry(eldest: MutableMap.MutableEntry<String, List<Feature>>): Boolean = size > MAX_ENTRIES
        }

    fun key(
        demZ: Int,
        tx: Int,
        ty: Int,
        mapZoom: Int,
    ): String = "$demZ/$tx/$ty@$mapZoom"

    fun get(key: String): List<Feature>? =
        synchronized(lock) {
            val hit = store[key]
            if (hit != null) {
                NaviMapTestHooks.contourGenCacheHits++
            }
            hit
        }

    fun put(
        key: String,
        features: List<Feature>,
    ) {
        synchronized(lock) { store[key] = features }
    }

    fun clear() {
        synchronized(lock) { store.clear() }
    }
}

/** Clip contour segments to a viewport tile; keeps lines whose bbox intersects. */
object ContourFeatureClip {
    fun clipToBounds(
        features: List<Feature>,
        west: Double,
        south: Double,
        east: Double,
        north: Double,
    ): List<Feature> {
        val out = ArrayList<Feature>(features.size)
        for (f in features) {
            val geom = f.geometry()
            if (geom !is LineString) continue
            val coords = geom.coordinates()
            if (coords.isEmpty()) continue
            var minLon = coords[0].longitude()
            var maxLon = minLon
            var minLat = coords[0].latitude()
            var maxLat = minLat
            for (i in 1 until coords.size) {
                val lon = coords[i].longitude()
                val lat = coords[i].latitude()
                if (lon < minLon) minLon = lon
                if (lon > maxLon) maxLon = lon
                if (lat < minLat) minLat = lat
                if (lat > maxLat) maxLat = lat
            }
            if (maxLon < west || minLon > east || maxLat < south || minLat > north) continue
            out.add(f)
        }
        return out
    }
}

/** List DEM (z/x/y) tiles intersecting geographic bounds at [demZ]. */
object DemTileCover {
    fun tilesIntersecting(
        bounds: DemTileLatLngBounds,
        demZ: Int,
    ): List<Pair<Int, Int>> {
        val n = 1 shl demZ
        val x0 = DemTileBounds.tileXY(demZ, bounds.west, bounds.north).first
        val x1 = DemTileBounds.tileXY(demZ, bounds.east, bounds.north).first
        val yN = DemTileBounds.tileXY(demZ, bounds.west, bounds.north).second
        val yS = DemTileBounds.tileXY(demZ, bounds.west, bounds.south).second
        val minX = minOf(x0, x1).coerceIn(0, n - 1)
        val maxX = maxOf(x0, x1).coerceIn(0, n - 1)
        val minY = minOf(yN, yS).coerceIn(0, n - 1)
        val maxY = maxOf(yN, yS).coerceIn(0, n - 1)
        val tiles = ArrayList<Pair<Int, Int>>((maxX - minX + 1) * (maxY - minY + 1))
        for (y in minY..maxY) {
            for (x in minX..maxX) {
                tiles.add(x to y)
            }
        }
        return tiles
    }
}
