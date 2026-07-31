package no.navi.app

import kotlin.math.cos
import kotlin.math.ln
import kotlin.math.max

/**
 * Camera (lat, lon, zoom) that frames an entire lon,lat;… overlay polyline
 * on a portrait tablet map viewport.
 */
object RouteCameraFit {
    fun fromPolyline(
        polyline: String,
        pad: Double = 1.45,
        minZoom: Double = 5.0,
        maxZoom: Double = 16.0,
    ): Triple<Double, Double, Double> {
        var minLat = 90.0
        var maxLat = -90.0
        var minLon = 180.0
        var maxLon = -180.0
        var n = 0
        for (part in polyline.split(';')) {
            val bits = part.split(',')
            if (bits.size < 2) continue
            val lon = bits[0].trim().toDoubleOrNull() ?: continue
            val lat = bits[1].trim().toDoubleOrNull() ?: continue
            if (lat < minLat) minLat = lat
            if (lat > maxLat) maxLat = lat
            if (lon < minLon) minLon = lon
            if (lon > maxLon) maxLon = lon
            n++
        }
        check(n >= 2) { "polyline needs >=2 points" }
        val cLat = (minLat + maxLat) / 2.0
        val cLon = (minLon + maxLon) / 2.0
        val latSpan = (maxLat - minLat).coerceAtLeast(1e-5) * pad
        val lonSpan = (maxLon - minLon).coerceAtLeast(1e-5) * pad
        val lonAdj = lonSpan * cos(Math.toRadians(cLat)).coerceAtLeast(0.2)
        val span = max(latSpan, lonAdj)
        val zoom = (ln(360.0 / span) / ln(2.0)).coerceIn(minZoom, maxZoom)
        return Triple(cLat, cLon, zoom)
    }
}
