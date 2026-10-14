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

/**
 * True when the text is two numeric parts (lat, lon style), even if the
 * values are out of range. Used to block Nominatim from treating a stray
 * coordinate as a place name (`1280060.393,5.324`).
 */
fun looksLikeCoordinatePair(raw: String): Boolean {
    val q = raw.trim()
    if (q.length < 3) return false
    val normalized = q.replace(';', ',').replace('\t', ' ')
    val parts =
        when {
            normalized.contains(',') -> normalized.split(',').map { it.trim() }.filter { it.isNotEmpty() }
            else -> normalized.split(Regex("\\s+")).filter { it.isNotEmpty() }
        }
    if (parts.size != 2) return false
    if (parts[0].toDoubleOrNull() == null || parts[1].toDoubleOrNull() == null) return false
    return parts[0].contains('.') || parts[1].contains('.')
}

/** NFC/NFD-insensitive, Unicode lowercase, diacritics stripped. Matches core fold. */
fun foldSearchKey(s: String): String {
    val out = StringBuilder(s.length)
    for (ch in s) {
        val code = ch.code
        if (code in 0x0300..0x036F ||
            code in 0x1AB0..0x1AFF ||
            code in 0x1DC0..0x1DFF ||
            code in 0x20D0..0x20FF ||
            code in 0xFE20..0xFE2F
        ) {
            continue
        }
        if (ch.code < 128 && ch.isLetterOrDigit()) {
            out.append(ch.lowercaseChar())
            continue
        }
        if (ch.isWhitespace() || ch == '-' || ch == '\'') {
            out.append(ch)
            continue
        }
        for (lc in ch.lowercase()) {
            out.append(foldSearchLetter(lc))
        }
    }
    return out.toString()
}

private fun foldSearchLetter(ch: Char): String =
    when (ch) {
        'æ', 'ǽ', 'ǣ' -> "ae"
        'œ' -> "oe"
        'ø', 'ǿ' -> "o"
        'ß' -> "ss"
        'þ' -> "th"
        'ł' -> "l"
        'đ', 'ð' -> "d"
        'ħ' -> "h"
        'ı' -> "i"
        'ŋ' -> "n"
        'ŧ' -> "t"
        'å', 'ä', 'à', 'á', 'â', 'ã', 'ā', 'ă', 'ą' -> "a"
        'ç', 'ć', 'č', 'ĉ', 'ċ' -> "c"
        'è', 'é', 'ê', 'ë', 'ē', 'ĕ', 'ė', 'ę', 'ě' -> "e"
        'ì', 'í', 'î', 'ï', 'ī', 'ĭ', 'į', 'ĩ' -> "i"
        'ñ', 'ń', 'ņ', 'ň' -> "n"
        'ò', 'ó', 'ô', 'õ', 'ö', 'ō', 'ŏ', 'ő' -> "o"
        'ù', 'ú', 'û', 'ü', 'ū', 'ŭ', 'ů', 'ű', 'ų' -> "u"
        'ý', 'ÿ', 'ŷ' -> "y"
        'ś', 'š', 'ş', 'ŝ', 'ș' -> "s"
        'ź', 'ż', 'ž' -> "z"
        'ğ', 'ģ', 'ġ', 'ĝ' -> "g"
        else -> if (ch.isLetterOrDigit()) ch.toString() else ""
    }

/** Where the map frames a selected search hit. The Tools region stays put. */
data class SearchHitMapMove(
    val lat: Double,
    val lon: Double,
    val name: String,
)

fun searchHitMapMove(hit: uniffi.navi.PlaceHit): SearchHitMapMove = SearchHitMapMove(lat = hit.lat, lon = hit.lon, name = placeHitDisplayLabel(hit))

fun formatCoordWaypointName(
    lat: Double,
    lon: Double,
): String = "%.5f, %.5f".format(java.util.Locale.US, lat, lon)

/** Fallback label when no place/address is within [GPS_WAYPOINT_RESOLVE_RADIUS_M]. */
fun formatGpsWaypointFallback(
    lat: Double,
    lon: Double,
): String = "GPS (${formatCoordWaypointName(lat, lon)})"

/** Immediate waypoint label when tapping Use GPS (same as typed lat, lon). */
fun gpsImmediateCoordHit(
    lat: Double,
    lon: Double,
): uniffi.navi.PlaceHit =
    uniffi.navi.PlaceHit(
        osmId = 0L,
        name = formatCoordWaypointName(lat, lon),
        kind = "coordinate",
        lat = lat,
        lon = lon,
        subArea = "",
        municipality = "",
        regionId = "",
    )

/**
 * After a background [resolveLabelAt], upgrade the GPS waypoint only when the
 * user has not replaced it and a real name was found.
 */
fun gpsWaypointShouldUpgrade(
    currentLat: Double?,
    currentLon: Double?,
    currentName: String?,
    expectedLat: Double,
    expectedLon: Double,
    resolvedName: String,
    resolvedKind: String,
): Boolean {
    if (currentLat == null || currentLon == null) return false
    if (currentLat != expectedLat || currentLon != expectedLon) return false
    if (resolvedName.isBlank()) return false
    if (resolvedKind == "map-mark" || resolvedKind == "gps") return false
    if (resolvedName == currentName) return false
    return true
}

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
const val GPS_WAYPOINT_RESOLVE_RADIUS_M = 75.0

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

/**
 * True when every significant query token appears as a whole word in [name]
 * (not merely a prefix of a longer compound). Used to drop local FTS
 * prefix hits like `Kalmar*` → `Kalmargaten` (Bergen) when merging with
 * online geocode results for `Kalmar` (Sweden).
 */
fun placeNameContainsQueryTokens(
    name: String,
    query: String,
): Boolean {
    val tokens =
        foldSearchKey(query)
            .split(Regex("[\\s,]+"))
            .map { it.trim() }
            .filter { it.length >= 2 || it.any { c -> c.isDigit() } }
    if (tokens.isEmpty()) return true
    val hayWords = foldSearchKey(name).split(Regex("\\s+"))
    return tokens.all { tok ->
        hayWords.any { w -> w == tok }
    }
}

/**
 * True when every word of [query] is a whole word of the hit's name or label
 * after folding: `Kobenhavn` matches `København`, `Köln` does not match
 * `Kolnesholmane`.
 */
fun placeHitMatchesWholeQuery(
    hit: uniffi.navi.PlaceHit,
    query: String,
): Boolean {
    fun words(s: String) = foldSearchKey(s).split(Regex("[^\\p{L}\\p{N}]+")).filter { it.isNotEmpty() }
    val queryWords = words(query)
    if (queryWords.isEmpty()) return true
    val hay = (words(hit.name) + words(placeHitDisplayLabel(hit))).toSet()
    val hayCollapsed = hay.map(::foldCollapse).toSet()
    return queryWords.all { it in hay || foldCollapse(it) in hayCollapsed }
}

/**
 * Query as it reads while typing: every word but the last is a whole word of
 * the hit, and the last word starts one (`Domb` against `Dombås`).
 */
fun placeHitMatchesQueryPrefix(
    hit: uniffi.navi.PlaceHit,
    query: String,
): Boolean {
    fun words(s: String) = foldSearchKey(s).split(Regex("[^\\p{L}\\p{N}]+")).filter { it.isNotEmpty() }
    val queryWords = words(query)
    if (queryWords.isEmpty()) return true
    val hay = (words(hit.name) + words(placeHitDisplayLabel(hit))).toSet()
    val hayCollapsed = hay.map(::foldCollapse).toSet()
    val head = queryWords.dropLast(1).all { it in hay || foldCollapse(it) in hayCollapsed }
    val last = queryWords.last()
    return head &&
        (hay.any { it.startsWith(last) } || hayCollapsed.any { it.startsWith(foldCollapse(last)) })
}

/**
 * Merge online geocode hits ahead of local FTS, dropping offline prefix
 * expansions that do not contain the query as whole tokens.
 */
fun mergeOnlineAndOfflinePlaceHits(
    query: String,
    online: List<uniffi.navi.PlaceHit>,
    offline: List<uniffi.navi.PlaceHit>,
    biasLat: Double? = null,
    biasLon: Double? = null,
    visibleRegion: String = "",
): List<uniffi.navi.PlaceHit> {
    if (online.isEmpty() && offline.isEmpty()) return emptyList()
    val offlineKeep =
        if (online.isEmpty()) {
            offline
        } else {
            offline.filter { hit ->
                placeNameContainsQueryTokens(hit.name, query) ||
                    placeNameContainsQueryTokens(placeHitDisplayLabel(hit), query)
            }
        }
    val seen = HashSet<String>()
    val merged = ArrayList<uniffi.navi.PlaceHit>(online.size + offlineKeep.size)
    for (h in offlineKeep) {
        val key = "${h.name.lowercase()}|${"%.4f".format(h.lat)}|${"%.4f".format(h.lon)}"
        if (seen.add(key)) merged.add(h)
    }
    for (h in online) {
        if (offlineKeep.any { isSamePlace(it, h) }) continue
        val key = "${h.name.lowercase()}|${"%.4f".format(h.lat)}|${"%.4f".format(h.lon)}"
        if (seen.add(key)) merged.add(h)
    }
    return rankPlaceHits(query, merged, biasLat, biasLon, visibleRegion)
}

/**
 * The online geocoder's copy of a local feature (same folded name within
 * 300 m). The local hit is kept so it ranks as local.
 */
private fun isSamePlace(
    local: uniffi.navi.PlaceHit,
    online: uniffi.navi.PlaceHit,
): Boolean =
    foldSearchKey(local.name.trim()) == foldSearchKey(online.name.trim()) &&
        placeDistanceDecikm(online, local.lat, local.lon) < 3

fun rankPlaceHits(
    query: String,
    hits: List<uniffi.navi.PlaceHit>,
    biasLat: Double? = null,
    biasLon: Double? = null,
    visibleRegion: String = "",
): List<uniffi.navi.PlaceHit> {
    val q = query.trim()
    val vis = visibleRegion.trim().trim('/')
    val matchClass = hits.associateWith { placeMatchClass(it.name, q) }
    return hits.sortedWith(
        compareBy<uniffi.navi.PlaceHit> { placeNameTier(matchClass.getValue(it)) }
            .then { a, b -> compareOnlinePromote(a, b, placeNameTier(matchClass.getValue(a))) }
            .thenBy { matchClass.getValue(it) }
            .thenBy { placeSourceClass(it.kind) }
            .thenBy { placeRegionClass(it.regionId, vis) }
            .thenBy { placeKindClass(it.kind) }
            .thenBy { placeDistanceDecikm(it, biasLat, biasLon) }
            .thenBy { it.name.length },
    )
}

/**
 * Between two hits whose names both equal the query (tier 0): an online city,
 * town or village comes before a local minor feature of that name. Otherwise 0,
 * so the exact-as-typed class and then local-before-online decide.
 */
internal fun compareOnlinePromote(
    a: uniffi.navi.PlaceHit,
    b: uniffi.navi.PlaceHit,
    nameTier: Int,
): Int {
    if (nameTier != 0) return 0
    val aOn = placeSourceClass(a.kind) == 1
    val bOn = placeSourceClass(b.kind) == 1
    if (aOn == bOn) return 0
    val online = if (aOn) a else b
    val local = if (aOn) b else a
    if (!shouldPromoteOnlineOverLocal(online, local)) return 0
    return if (aOn) -1 else 1
}

/**
 * Online city or town over a local minor feature anywhere; an online village
 * only on the local feature's continent (Rødby in Denmark over the Asker
 * hamlet, but not villages named Aga in Benin over the Ullensvang hamlet).
 */
internal fun shouldPromoteOnlineOverLocal(
    online: uniffi.navi.PlaceHit,
    local: uniffi.navi.PlaceHit,
): Boolean {
    if (!isMinorPlaceKind(local.kind)) return false
    if (isCityOrTownKind(online.kind)) return true
    if (placeKindToken(online.kind) != "village") return false
    val onlineContinent = regionContinent(online.regionId)
    return onlineContinent.isNotEmpty() && onlineContinent == regionContinent(local.regionId)
}

private fun regionContinent(regionId: String): String = regionId.trim('/', ' ').substringBefore('/')

internal fun placeKindToken(kind: String): String {
    val k = kind.lowercase().substringBefore('|')
    if (k.contains("place:")) {
        return k.substringAfter("place:").substringBefore('/').substringBefore('|')
    }
    if (k.contains("/place/")) {
        return k.substringAfter("/place/").substringBefore('/').substringBefore('|')
    }
    return k.substringAfterLast('/').substringAfter(':')
}

internal fun isCityOrTownKind(kind: String): Boolean {
    val t = placeKindToken(kind)
    return t == "city" || t == "town" || t == "municipality"
}

/**
 * Local features that an online settlement of the same name outranks: farm,
 * hamlet, isolated dwelling, locality, bus stop, building, ferry and other
 * route relations, shops and other points of interest.
 */
internal fun isMinorPlaceKind(kind: String): Boolean {
    val t = placeKindToken(kind)
    val k = kind.lowercase()
    return t == "farm" ||
        t == "hamlet" ||
        t == "isolated_dwelling" ||
        t == "locality" ||
        t == "bus_stop" ||
        t == "building" ||
        k.contains("bus_stop") ||
        k.contains("building") ||
        k.contains("ferry") ||
        MINOR_KIND_PREFIXES.any { k.startsWith(it) }
}

private val MINOR_KIND_PREFIXES =
    listOf("route:", "shop:", "amenity:", "tourism:", "leisure:", "historic:", "office:", "craft:", "man_made:")

/** 0 = local / installed-region hit, 1 = online (Nominatim or ORS). */
internal fun placeSourceClass(kind: String): Int = if (kind.startsWith("online/", ignoreCase = true)) 1 else 0

/**
 * 0 = name equals the query as typed, accents and special letters included
 * (case ignored); 1 = equal after case and accent folding; 2 = prefix; 3 = other.
 * A query typed without special letters cannot tell Åre from Are, so for it
 * classes 0 and 1 are one class and kind, size and distance decide.
 */
internal fun placeMatchClass(
    name: String,
    query: String,
): Int {
    val n = name.trim().lowercase()
    val q = query.trim().lowercase()
    val nf = foldSearchKey(name)
    val qf = foldSearchKey(q)
    val nCollapse = foldCollapse(nf)
    val qCollapse = foldCollapse(qf)
    val typedSpecial = q.any { it.isLetter() && it.code > 0x7f }
    return when {
        n == q && typedSpecial -> 0
        n == q || nf == qf || nCollapse == qCollapse -> 1
        n.startsWith(q) || nf.startsWith(qf) || nCollapse.startsWith(qCollapse) -> 2
        else -> 3
    }
}

/** Exact-as-typed and folded-exact names share tier 0; prefix and other keep their class. */
internal fun placeNameTier(matchClass: Int): Int = if (matchClass <= 1) 0 else matchClass

private fun foldCollapse(folded: String): String = folded.replace("aa", "a").replace("ue", "u").replace("oe", "o")

internal fun placeKindClass(kind: String): Int {
    val k = kind.lowercase()
    val place = placeKindToken(k)
    return when (place) {
        "city", "municipality" -> 0
        "town" -> 1
        "suburb", "borough", "quarter" -> 2
        "village" -> 3
        "hamlet" -> 4
        "locality", "isolated_dwelling" -> 5
        "farm" -> 7
        "bus_stop" -> 8
        "building" -> 9
        else -> {
            when {
                k.contains("bus_stop") -> 8
                k.contains("building") -> 9
                k.contains("tourism") ||
                    k.contains("amenity") ||
                    k.contains("leisure") ||
                    k.contains("natural") -> 10
                k.contains("addr") || k.contains("highway") || k.contains("street") -> 20
                k.contains("place:") || k.contains("/place/") -> 6
                else -> 15
            }
        }
    }
}

internal fun placeRegionClass(
    regionId: String,
    visible: String,
): Int {
    if (visible.isEmpty()) return 1
    val rid = regionId.trim().trim('/')
    if (rid == visible || rid.startsWith("$visible/")) return 0
    val visCountry = visible.split('/').getOrNull(1).orEmpty()
    val hitCountry = rid.split('/').getOrNull(1).orEmpty()
    if (visCountry.isNotEmpty() && visCountry == hitCountry) return 1
    return 2
}

internal fun placeDistanceDecikm(
    hit: uniffi.navi.PlaceHit,
    biasLat: Double?,
    biasLon: Double?,
): Int {
    if (biasLat == null || biasLon == null) return 0
    val dlat = Math.toRadians(hit.lat - biasLat)
    val dlon = Math.toRadians(hit.lon - biasLon)
    val la1 = Math.toRadians(biasLat)
    val la2 = Math.toRadians(hit.lat)
    val h =
        Math.sin(dlat / 2) * Math.sin(dlat / 2) +
            Math.cos(la1) * Math.cos(la2) * Math.sin(dlon / 2) * Math.sin(dlon / 2)
    val km = 2.0 * 6371.0 * Math.asin(Math.sqrt(h))
    return (km * 10.0).toInt()
}
