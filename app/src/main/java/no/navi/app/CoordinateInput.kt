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
    val parts = when {
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

fun formatCoordWaypointName(lat: Double, lon: Double): String =
    "%.5f, %.5f".format(lat, lon)
