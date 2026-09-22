package no.navi.app

import android.content.Context
import android.util.Log
import org.json.JSONArray
import uniffi.navi.PlaceHit
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.net.URLEncoder
import java.nio.charset.StandardCharsets

/**
 * Online place / address search when the offline FTS place index is empty or
 * missing. Primary: OpenStreetMap Nominatim (no API key). Optional secondary:
 * OpenRouteService geocode if a one-line key file is present.
 *
 * Key file (optional, least-preferred): first non-blank line of
 * `<app files>/ors_api_key.txt` or `<external files>/ors_api_key.txt`.
 */
object OnlinePlaceSearch {
    private const val TAG = "OnlinePlaceSearch"
    private const val NOMINATIM =
        "https://nominatim.openstreetmap.org/search"
    private const val NOMINATIM_REVERSE =
        "https://nominatim.openstreetmap.org/reverse"
    private const val ORS_GEOCODE =
        "https://api.openrouteservice.org/geocode/search"
    private const val USER_AGENT =
        "Navi/0.1 (https://github.com/Supermagnum/Navi; offline-nav app)"
    private const val KEY_FILE = "ors_api_key.txt"

    @Volatile
    private var lastNominatimMs: Long = 0L

    /** Test seam: when non-null, [search] returns this instead of hitting the network. */
    @Volatile
    internal var overrideForTests: ((String, Int) -> List<PlaceHit>)? = null

    /** Test seam for [reverse]. */
    @Volatile
    internal var reverseOverrideForTests: ((Double, Double) -> PlaceHit?)? = null

    fun search(
        context: Context,
        query: String,
        limit: Int,
        addressMode: Boolean,
    ): List<PlaceHit> {
        val q = query.trim()
        if (q.length < 2) return emptyList()
        overrideForTests?.let { return it(q, limit) }
        if (!BasemapStyleResolver.hasNetwork(context)) return emptyList()

        val lim = limit.coerceIn(1, 20)
        for (candidate in nominatimQueryFallbacks(q)) {
            val nominatim = searchNominatim(candidate, lim, addressMode)
            if (nominatim.isNotEmpty()) return nominatim
        }

        val key = readOrsApiKey(context)
        if (key.isNullOrBlank()) return emptyList()
        return searchOrs(q, lim, key)
    }

    /**
     * Nominatim often returns [] for very long CSV address strings. Try the
     * full query first, then shorter head/mid/country combinations.
     */
    internal fun nominatimQueryFallbacks(query: String): List<String> {
        val parts =
            query
                .split(',')
                .map { it.trim() }
                .filter { it.isNotEmpty() }
        if (parts.size <= 2) return listOf(query.trim())
        val out = LinkedHashSet<String>()
        out += query.trim()
        out += parts.joinToString(", ")
        out += "${parts.first()}, ${parts.last()}"
        out += "${parts.first()}, ${parts[parts.size - 2]}, ${parts.last()}"
        for (i in 1 until parts.size - 1) {
            val mid = parts[i]
            if (mid.any { it.isDigit() }) continue
            if (mid.startsWith("Kreis", ignoreCase = true)) continue
            if (mid.startsWith("North ", ignoreCase = true)) continue
            if (mid.startsWith("Samtgemeinde", ignoreCase = true)) continue
            out += "${parts.first()}, $mid, ${parts.last()}"
        }
        return out.toList()
    }

    /**
     * Reverse-geocode [lat]/[lon] to a street / place label (Nominatim).
     * Used by Use GPS / map-mark when the offline place index has no nearby addr.
     */
    fun reverse(
        context: Context,
        lat: Double,
        lon: Double,
    ): PlaceHit? {
        reverseOverrideForTests?.let { return it(lat, lon) }
        if (!BasemapStyleResolver.hasNetwork(context)) return null
        if (lat == 0.0 && lon == 0.0) return null
        throttleNominatim()
        val url =
            "$NOMINATIM_REVERSE?lat=$lat&lon=$lon&format=jsonv2&addressdetails=1&zoom=18"
        return runCatching {
            val body = httpGet(url, connectTimeoutMs = 6_000, readTimeoutMs = 8_000)
            parseNominatimReverseJson(body, lat, lon)
        }.onFailure { e ->
            Log.w(TAG, "Nominatim reverse failed: ${e.message}")
        }.getOrNull()
    }

    internal fun parseNominatimReverseJson(
        body: String,
        fallbackLat: Double,
        fallbackLon: Double,
    ): PlaceHit? {
        val o = org.json.JSONObject(body)
        if (o.has("error")) return null
        val lat = o.optString("lat").toDoubleOrNull() ?: fallbackLat
        val lon = o.optString("lon").toDoubleOrNull() ?: fallbackLon
        val addr = o.optJSONObject("address")
        val display = o.optString("display_name")
        val name = formatReverseAddress(addr, display).ifBlank { return null }
        val osmId = o.optLong("osm_id", 0L)
        val cls = o.optString("category").ifBlank { o.optString("class") }.ifBlank { "place" }
        val type = o.optString("type").ifBlank { "reverse" }
        val municipality =
            addr
                ?.optString("municipality")
                ?.ifBlank { addr.optString("city") }
                ?.ifBlank { addr.optString("town") }
                ?.ifBlank { addr.optString("village") }
                .orEmpty()
        val subArea =
            addr
                ?.optString("suburb")
                ?.ifBlank { addr.optString("neighbourhood") }
                ?.ifBlank { addr.optString("hamlet") }
                .orEmpty()
        return PlaceHit(
            osmId = osmId,
            name = name,
            kind = "online/$cls/$type",
            lat = lat,
            lon = lon,
            subArea = subArea,
            municipality = municipality,
            regionId = "",
        )
    }

    /** Prefer "Road 12" over the full Nominatim display_name CSV. */
    internal fun formatReverseAddress(
        addr: org.json.JSONObject?,
        displayName: String,
    ): String {
        if (addr != null) {
            val road =
                sequenceOf("road", "pedestrian", "footway", "path", "residential", "street")
                    .map { addr.optString(it) }
                    .firstOrNull { it.isNotBlank() }
                    .orEmpty()
            val num = addr.optString("house_number").trim()
            if (road.isNotBlank() && num.isNotEmpty()) return "$road $num"
            if (road.isNotBlank()) return road
            val place =
                sequenceOf("amenity", "building", "shop", "tourism", "office")
                    .map { addr.optString(it) }
                    .firstOrNull { it.isNotBlank() }
            if (!place.isNullOrBlank()) return place
        }
        val short = displayName.substringBefore(',').trim()
        return short.ifBlank { displayName.trim() }
    }

    fun readOrsApiKey(context: Context): String? {
        // Prefer one-line file (easy to find via adb / file manager).
        val candidates =
            listOfNotNull(
                File(NaviAppData.resolve(context), KEY_FILE),
                context.getExternalFilesDir(null)?.let { File(it, KEY_FILE) },
            )
        for (f in candidates) {
            if (!f.isFile) continue
            val line =
                runCatching {
                    f.readText(StandardCharsets.UTF_8)
                        .lineSequence()
                        .map { it.trim() }
                        .firstOrNull { it.isNotEmpty() && !it.startsWith("#") }
                }.getOrNull()
            if (!line.isNullOrBlank()) return line
        }
        // Optional prefs fallback if a settings UI ever wrote a key.
        return MapHudPrefs.loadOrsApiKey(context).trim().ifBlank { null }
    }

    internal fun parseNominatimJson(
        body: String,
        limit: Int,
    ): List<PlaceHit> {
        val arr = JSONArray(body)
        val out = ArrayList<PlaceHit>(minOf(arr.length(), limit))
        for (i in 0 until arr.length()) {
            if (out.size >= limit) break
            val o = arr.optJSONObject(i) ?: continue
            val lat = o.optString("lat").toDoubleOrNull() ?: continue
            val lon = o.optString("lon").toDoubleOrNull() ?: continue
            val addr = o.optJSONObject("address")
            val display = o.optString("display_name")
            val name =
                formatReverseAddress(addr, display)
                    .ifBlank { o.optString("name") }
                    .ifBlank { formatCoordWaypointName(lat, lon) }
            val osmId = o.optLong("osm_id", 0L)
            val cls = o.optString("class").ifBlank { o.optString("category") }.ifBlank { "place" }
            val type = o.optString("type").ifBlank { "online" }
            val kind = "online/$cls/$type"
            val regionId =
                runCatching { RegionCoverage.suggestGeofabrikPath(lat, lon) }
                    .getOrNull()
                    .orEmpty()
            val municipality =
                addr?.optString("municipality")
                    ?.ifBlank { addr.optString("city") }
                    ?.ifBlank { addr.optString("town") }
                    ?.ifBlank { addr.optString("village") }
                    .orEmpty()
            val subArea =
                addr?.optString("suburb")
                    ?.ifBlank { addr.optString("neighbourhood") }
                    ?.ifBlank { addr.optString("hamlet") }
                    .orEmpty()
            out.add(
                PlaceHit(
                    osmId = osmId,
                    name = name,
                    kind = kind,
                    lat = lat,
                    lon = lon,
                    subArea = subArea,
                    municipality = municipality,
                    regionId = regionId,
                ),
            )
        }
        return out
    }

    private fun searchNominatim(
        query: String,
        limit: Int,
        addressMode: Boolean,
    ): List<PlaceHit> {
        throttleNominatim()
        val enc = URLEncoder.encode(query, StandardCharsets.UTF_8.name())
        // Nominatim has no layer= filter (that is Photon). Address mode just
        // uses the same free-text search; callers may bias the query string.
        val url =
            "$NOMINATIM?q=$enc&format=jsonv2&limit=$limit&addressdetails=1"
        return runCatching {
            val body = httpGet(url)
            parseNominatimJson(body, limit)
        }.onFailure { e ->
            Log.w(TAG, "Nominatim search failed: ${e.message}")
        }.getOrDefault(emptyList())
    }

    private fun searchOrs(
        query: String,
        limit: Int,
        apiKey: String,
    ): List<PlaceHit> {
        val enc = URLEncoder.encode(query, StandardCharsets.UTF_8.name())
        val url = "$ORS_GEOCODE?text=$enc&size=$limit"
        return runCatching {
            val body = httpGet(url, mapOf("Authorization" to apiKey))
            parseOrsJson(body, limit)
        }.onFailure { e ->
            Log.w(TAG, "ORS geocode failed: ${e.message}")
        }.getOrDefault(emptyList())
    }

    internal fun parseOrsJson(
        body: String,
        limit: Int,
    ): List<PlaceHit> {
        val root = org.json.JSONObject(body)
        val features = root.optJSONArray("features") ?: return emptyList()
        val out = ArrayList<PlaceHit>(minOf(features.length(), limit))
        for (i in 0 until features.length()) {
            if (out.size >= limit) break
            val f = features.optJSONObject(i) ?: continue
            val geom = f.optJSONObject("geometry") ?: continue
            val coords = geom.optJSONArray("coordinates") ?: continue
            if (coords.length() < 2) continue
            val lon = coords.optDouble(0)
            val lat = coords.optDouble(1)
            val props = f.optJSONObject("properties")
            val name =
                props?.optString("label")?.ifBlank { props.optString("name") }
                    ?: formatCoordWaypointName(lat, lon)
            val regionId =
                runCatching { RegionCoverage.suggestGeofabrikPath(lat, lon) }
                    .getOrNull()
                    .orEmpty()
            out.add(
                PlaceHit(
                    osmId = 0L,
                    name = name,
                    kind = "online/ors/geocode",
                    lat = lat,
                    lon = lon,
                    subArea = props?.optString("locality").orEmpty(),
                    municipality = props?.optString("county").orEmpty(),
                    regionId = regionId,
                ),
            )
        }
        return out
    }

    private fun throttleNominatim() {
        val now = System.currentTimeMillis()
        val wait = 1100L - (now - lastNominatimMs)
        if (wait > 0) {
            try {
                Thread.sleep(wait)
            } catch (_: InterruptedException) {
                Thread.currentThread().interrupt()
            }
        }
        lastNominatimMs = System.currentTimeMillis()
    }

    private fun httpGet(
        url: String,
        extraHeaders: Map<String, String> = emptyMap(),
        connectTimeoutMs: Int = 12_000,
        readTimeoutMs: Int = 20_000,
    ): String {
        val conn = (URL(url).openConnection() as HttpURLConnection)
        conn.connectTimeout = connectTimeoutMs
        conn.readTimeout = readTimeoutMs
        conn.requestMethod = "GET"
        conn.setRequestProperty("User-Agent", USER_AGENT)
        conn.setRequestProperty("Accept", "application/json")
        for ((k, v) in extraHeaders) {
            conn.setRequestProperty(k, v)
        }
        try {
            val code = conn.responseCode
            val stream =
                if (code in 200..299) {
                    conn.inputStream
                } else {
                    conn.errorStream ?: conn.inputStream
                }
            val body = stream.bufferedReader(StandardCharsets.UTF_8).use { it.readText() }
            if (code !in 200..299) {
                error("HTTP $code: ${body.take(200)}")
            }
            return body
        } finally {
            conn.disconnect()
        }
    }
}
