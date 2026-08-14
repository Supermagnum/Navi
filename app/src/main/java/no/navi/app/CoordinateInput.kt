package no.navi.app

/**
 * Parse user-entered WGS84 coordinates for From / Via / To.
 *
 * Accepted forms (lat then lon):
 * - `60.562480, 11.256282`
 * - `60.562480,11.256282`
 * - `60.562480 11.256282`
 *
 * Returns null when the text is not a coordinate pair (fall through to place search).
 */
fun parseLatLonQuery(raw: String): Pair<Double, Double>? {
    val q = raw.trim()
    if (q.length < 3) return null
    // Reject obvious place names early (letters other than e/E in scientific notation).
    val normalized = q.replace(';', ',').replace('\t', ' ')
    val parts =
        when {
            normalized.contains(',') -> normalized.split(',').map { it.trim() }.filter { it.isNotEmpty() }
            else -> normalized.split(Regex("\\s+")).filter { it.isNotEmpty() }
        }
    if (parts.size != 2) return null
    val lat = parts[0].toDoubleOrNull() ?: return null
    val lon = parts[1].toDoubleOrNull() ?: return null
    if (lat !in -90.0..90.0 || lon !in -180.0..180.0) return null
    // Avoid treating tiny integers like "1 2" as coords when typed as place fragments —
    // require at least one decimal point in the pair for confidence.
    if (!parts[0].contains('.') && !parts[1].contains('.')) return null
    return lat to lon
}

fun formatCoordWaypointName(
    lat: Double,
    lon: Double,
): String = "%.5f, %.5f".format(java.util.Locale.US, lat, lon)

/** Fallback label when no place/address is within [GPS_WAYPOINT_RESOLVE_RADIUS_M]. */
fun formatGpsWaypointFallback(
    lat: Double,
    lon: Double,
): String = "GPS (${formatCoordWaypointName(lat, lon)})"

/**
 * Preferred max distance (metres) between a chosen From / Via / To and its
 * on-route map pin. Pins are always projected onto the planned corridor so they
 * sit on the red line; when the place centroid is farther than this budget a
 * warning is logged (POI often sits off the road graph).
 */
const val WAYPOINT_ROUTE_PIN_MAX_M = 12.0

data class RoutePinSnap(
    val lat: Double,
    val lon: Double,
    val distM: Double,
)

/**
 * Nearest point on the densified route polyline to `(lat, lon)`, including
 * projection onto each segment (not only vertices).
 */
fun nearestPointOnPolyline(
    pts: List<Pair<Double, Double>>,
    lat: Double,
    lon: Double,
): RoutePinSnap? {
    if (pts.isEmpty()) return null
    if (pts.size == 1) {
        val (plat, plon) = pts[0]
        return RoutePinSnap(plat, plon, haversineMApprox(lat, lon, plat, plon))
    }
    var bestLat = pts[0].first
    var bestLon = pts[0].second
    var bestD = Double.POSITIVE_INFINITY
    for (i in 0 until pts.lastIndex) {
        val (alat, alon) = pts[i]
        val (blat, blon) = pts[i + 1]
        val (plat, plon, d) = projectPointToSegmentM(lat, lon, alat, alon, blat, blon)
        if (d < bestD) {
            bestD = d
            bestLat = plat
            bestLon = plon
        }
    }
    return RoutePinSnap(bestLat, bestLon, bestD)
}

/**
 * Project `(lat, lon)` onto segment A→B in a local equirectangular frame.
 * Returns (projLat, projLon, distanceM).
 */
fun projectPointToSegmentM(
    lat: Double,
    lon: Double,
    aLat: Double,
    aLon: Double,
    bLat: Double,
    bLon: Double,
): Triple<Double, Double, Double> {
    val midLat = Math.toRadians((aLat + bLat) * 0.5)
    val mPerDegLat = 111_320.0
    val mPerDegLon = 111_320.0 * kotlin.math.cos(midLat).coerceAtLeast(0.2)
    val ax = (aLon) * mPerDegLon
    val ay = (aLat) * mPerDegLat
    val bx = (bLon) * mPerDegLon
    val by = (bLat) * mPerDegLat
    val px = (lon) * mPerDegLon
    val py = (lat) * mPerDegLat
    val abx = bx - ax
    val aby = by - ay
    val apx = px - ax
    val apy = py - ay
    val ab2 = abx * abx + aby * aby
    val t =
        if (ab2 < 1e-6) {
            0.0
        } else {
            ((apx * abx + apy * aby) / ab2).coerceIn(0.0, 1.0)
        }
    val qx = ax + t * abx
    val qy = ay + t * aby
    val qLon = qx / mPerDegLon
    val qLat = qy / mPerDegLat
    val dx = px - qx
    val dy = py - qy
    val dist = kotlin.math.sqrt(dx * dx + dy * dy)
    return Triple(qLat, qLon, dist)
}

/**
 * On-route pin for a chosen waypoint: nearest corridor point. Prefer snaps
 * within [WAYPOINT_ROUTE_PIN_MAX_M]; farther snaps are still returned so the pin
 * stays on the red line (caller may log).
 */
fun snapWaypointToRoutePolyline(
    pts: List<Pair<Double, Double>>,
    lat: Double,
    lon: Double,
): RoutePinSnap? = nearestPointOnPolyline(pts, lat, lon)

/** Max distance (metres) for resolving a GPS From/Via/To label to a nearby name. */
const val GPS_WAYPOINT_RESOLVE_RADIUS_M = 12.0

/**
 * Pick a human waypoint label from [nearbyPlaces] hits already filtered to
 * [GPS_WAYPOINT_RESOLVE_RADIUS_M] (nearest-first).
 *
 * Prefers address hits (`addr:*`), then any non-blank name. Returns null when
 * nothing usable is in range — caller should use [formatGpsWaypointFallback].
 */
fun pickNearbyPlaceNameForGpsWaypoint(hits: List<uniffi.navi.PlaceHit>): String? {
    if (hits.isEmpty()) return null

    fun usable(hit: uniffi.navi.PlaceHit): String? = hit.name.trim().takeIf { it.isNotEmpty() }
    val addr =
        hits.firstNotNullOfOrNull { hit ->
            val k = hit.kind.lowercase()
            if (k.contains("addr") || k.contains("housenumber")) usable(hit) else null
        }
    if (addr != null) return addr
    return hits.firstNotNullOfOrNull { usable(it) }
}

/**
 * Search-result label: `Place, Sub-area, Municipality`. Empty and duplicate
 * parts are omitted so unique names stay readable (`Espa, Stange`) while
 * identical names stay distinguishable (`Båberg, Brattberg, Gjøvik`).
 */
fun placeHitDisplayLabel(hit: uniffi.navi.PlaceHit): String {
    val parts = mutableListOf<String>()

    fun add(raw: String) {
        val t = raw.trim()
        if (t.isEmpty()) return
        if (parts.any { it.equals(t, ignoreCase = true) }) return
        parts.add(t)
    }

    add(hit.name)
    add(hit.subArea)
    add(hit.municipality)
    return parts.joinToString(", ")
}
