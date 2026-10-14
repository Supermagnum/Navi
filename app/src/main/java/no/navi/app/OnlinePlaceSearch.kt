package no.navi.app

import android.content.Context
import android.util.Log
import kotlinx.coroutines.runInterruptible
import org.json.JSONArray
import uniffi.navi.PlaceHit
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.nio.charset.StandardCharsets

/**
 * Optional online place search against a Nominatim-compatible service.
 *
 * The product path is opt-in ([MapHudPrefs.ONLINE_PLACE_SEARCH_DEFAULT_ENABLED]
 * is false), submit-only, rate-limited, and cached. Offline FTS is unaffected.
 *
 * Optional OpenRouteService geocode remains available for tests / a key file,
 * but a submitted search sends at most one HTTP request (no automatic retry
 * onto ORS or extra Nominatim fallbacks).
 */
object OnlinePlaceSearch {
    private const val TAG = "OnlinePlaceSearch"
    private const val KEY_FILE = "ors_api_key.txt"

    internal val limiter = OnlinePlaceSearchLimiter()

    /** Test seam: when non-null, [search] returns this instead of hitting the network. */
    @Volatile
    internal var overrideForTests: ((String, Int) -> List<PlaceHit>)? = null

    /** Test seam for [reverse]. */
    @Volatile
    internal var reverseOverrideForTests: ((Double, Double) -> PlaceHit?)? = null

    /** JVM tests: force the setting without SharedPreferences. */
    @Volatile
    internal var enabledOverrideForTests: Boolean? = null

    /** JVM tests: force the base URL without SharedPreferences. */
    @Volatile
    internal var baseUrlOverrideForTests: String? = null

    /** JVM tests: inject HTTP (no real network). */
    @Volatile
    internal var httpOverrideForTests: OnlineHttpClient? = null

    private val requestLog = ArrayList<String>()

    fun snapshotRequestLog(): List<String> =
        synchronized(requestLog) {
            requestLog.toList()
        }

    fun clearRequestLog() {
        synchronized(requestLog) { requestLog.clear() }
        limiter.resetForTests()
    }

    fun resetOverridesForTests() {
        overrideForTests = null
        reverseOverrideForTests = null
        enabledOverrideForTests = null
        baseUrlOverrideForTests = null
        httpOverrideForTests = null
        clearRequestLog()
    }

    internal fun resolvedEnabled(context: Context): Boolean = enabledOverrideForTests ?: MapHudPrefs.loadOnlinePlaceSearchEnabled(context)

    internal fun resolvedBaseUrl(context: Context): String = baseUrlOverrideForTests ?: MapHudPrefs.loadNominatimBaseUrl(context)

    internal fun appUserAgent(): String = naviOnlinePlaceSearchUserAgent(BuildConfig.VERSION_NAME)

    suspend fun search(
        context: Context,
        query: String,
        limit: Int,
        addressMode: Boolean,
        explicitSubmit: Boolean = true,
    ): List<PlaceHit> {
        val q = query.trim()
        if (q.length < 2) return emptyList()
        if (looksLikeCoordinatePair(q) && parseLatLonQuery(q) == null) return emptyList()
        overrideForTests?.let { return it(q, limit) }
        val config =
            OnlinePlaceSearchConfig(
                enabled = resolvedEnabled(context),
                baseUrl = resolvedBaseUrl(context),
                userAgent = appUserAgent(),
                explicitSubmit = explicitSubmit,
            )
        if (!shouldSendOnlinePlaceSearch(config)) return emptyList()
        if (!BasemapStyleResolver.hasNetwork(context)) return emptyList()
        val client = httpOverrideForTests ?: defaultHttpClient()
        return searchWithConfig(q, limit, config, client, limiter, addressMode)
    }

    /**
     * Context-free search used by unit tests with a fake [OnlineHttpClient].
     * Never opens a real socket.
     */
    internal suspend fun searchWithConfig(
        query: String,
        limit: Int,
        config: OnlinePlaceSearchConfig,
        client: OnlineHttpClient,
        limiter: OnlinePlaceSearchLimiter = this.limiter,
        @Suppress("UNUSED_PARAMETER") addressMode: Boolean = false,
    ): List<PlaceHit> {
        val q = query.trim()
        if (q.length < 2) return emptyList()
        if (looksLikeCoordinatePair(q) && parseLatLonQuery(q) == null) return emptyList()
        if (!shouldSendOnlinePlaceSearch(config)) return emptyList()
        val base = normalizeNominatimBaseUrl(config.baseUrl)
        if (base.isEmpty()) return emptyList()
        val split = splitCountryQualifiedQuery(q)
        val placeQ = split.placeQuery.ifBlank { q }
        val countryIso = split.countryIso
        val searchQ = if (countryIso != null) placeQ else q
        val lim = limit.coerceIn(1, 20)
        val url = nominatimSearchUrl(base, searchQ, lim, countryIso)
        if (url.isEmpty()) return emptyList()
        val headers =
            mapOf(
                "User-Agent" to config.userAgent,
                "Accept" to "application/json",
            )
        val cacheKey = "$base\n$searchQ\n$lim\n${countryIso.orEmpty()}"
        val result = limiter.get(cacheKey, OnlineHttpRequest(url, headers), client)
        return when (result) {
            is OnlineHttpResult.Ok -> {
                runCatching { parseNominatimJson(result.body, lim) }.getOrDefault(emptyList())
            }
            is OnlineHttpResult.Failure -> emptyList()
        }
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
     * Not used for GPS / map-mark / nearby lookups. Gated by the same setting
     * and base URL as forward search.
     */
    suspend fun reverse(
        context: Context,
        lat: Double,
        lon: Double,
    ): PlaceHit? {
        reverseOverrideForTests?.let { return it(lat, lon) }
        val config =
            OnlinePlaceSearchConfig(
                enabled = resolvedEnabled(context),
                baseUrl = resolvedBaseUrl(context),
                userAgent = appUserAgent(),
                explicitSubmit = true,
            )
        if (!shouldSendOnlinePlaceSearch(config)) return null
        if (!BasemapStyleResolver.hasNetwork(context)) return null
        if (lat == 0.0 && lon == 0.0) return null
        val url = nominatimReverseUrl(config.baseUrl, lat, lon)
        if (url.isEmpty()) return null
        val client = httpOverrideForTests ?: defaultHttpClient()
        val headers =
            mapOf(
                "User-Agent" to config.userAgent,
                "Accept" to "application/json",
            )
        val cacheKey = "reverse\n${config.baseUrl}\n$lat\n$lon"
        val result =
            limiter.get(cacheKey, OnlineHttpRequest(url, headers), client)
        return when (result) {
            is OnlineHttpResult.Ok ->
                runCatching { parseNominatimReverseJson(result.body, lat, lon) }.getOrNull()
            is OnlineHttpResult.Failure -> null
        }
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
        val candidates =
            listOfNotNull(
                File(NaviAppData.resolve(context), KEY_FILE),
                context.getExternalFilesDir(null)?.let { File(it, KEY_FILE) },
            )
        for (f in candidates) {
            if (!f.isFile) continue
            val line =
                runCatching {
                    f
                        .readText(StandardCharsets.UTF_8)
                        .lineSequence()
                        .map { it.trim() }
                        .firstOrNull { it.isNotEmpty() && !it.startsWith("#") }
                }.getOrNull()
            if (!line.isNullOrBlank()) return line
        }
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
            // A named place (restaurant, town) keeps its name; houses have none.
            val name =
                o
                    .optString("name")
                    .trim()
                    .ifBlank { formatReverseAddress(addr, display) }
                    .ifBlank { formatCoordWaypointName(lat, lon) }
            val osmId = o.optLong("osm_id", 0L)
            val cls = o.optString("class").ifBlank { o.optString("category") }.ifBlank { "place" }
            val type = o.optString("type").ifBlank { "online" }
            val kind = "online/$cls/$type"
            val regionId =
                runCatching { RegionCoverage.suggestGeofabrikPath(lat, lon) }
                    .getOrNull()
                    .orEmpty()
            val countryCc =
                addr
                    ?.optString("country_code")
                    ?.trim()
                    ?.lowercase()
                    .orEmpty()
            val municipality =
                addr
                    ?.optString("municipality")
                    ?.ifBlank { addr.optString("city") }
                    ?.ifBlank { addr.optString("town") }
                    ?.ifBlank { addr.optString("village") }
                    .orEmpty()
            val state =
                addr
                    ?.optString("state")
                    ?.ifBlank { addr.optString("county") }
                    .orEmpty()
            val subArea =
                addr
                    ?.optString("suburb")
                    ?.ifBlank { addr.optString("neighbourhood") }
                    ?.ifBlank { addr.optString("hamlet") }
                    ?.ifBlank { state }
                    .orEmpty()
            out.add(
                PlaceHit(
                    osmId = osmId,
                    name = name,
                    kind = kindWithCountryIso(kind, countryCc.ifBlank { null }),
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

    internal fun defaultHttpClient(): OnlineHttpClient =
        OnlineHttpClient { request ->
            recordRequest(request)
            runInterruptible {
                httpGet(request)
            }
        }

    private fun recordRequest(request: OnlineHttpRequest) {
        synchronized(requestLog) { requestLog.add(request.url) }
        Log.i(TAG, "http ${request.url}")
        NaviMapTestHooks.lastOnlinePlaceSearchCount = synchronized(requestLog) { requestLog.size }
        NaviMapTestHooks.lastOnlinePlaceSearchUrls = snapshotRequestLog()
        NaviMapTestHooks.lastOnlinePlaceSearchUserAgent =
            request.headers["User-Agent"].orEmpty()
    }

    private fun httpGet(request: OnlineHttpRequest): OnlineHttpResult {
        val conn = (URL(request.url).openConnection() as HttpURLConnection)
        conn.connectTimeout = 12_000
        conn.readTimeout = 20_000
        conn.requestMethod = "GET"
        for ((k, v) in request.headers) {
            conn.setRequestProperty(k, v)
        }
        return try {
            val code = conn.responseCode
            val stream =
                if (code in 200..299) {
                    conn.inputStream
                } else {
                    conn.errorStream ?: conn.inputStream
                }
            val body = stream.bufferedReader(StandardCharsets.UTF_8).use { it.readText() }
            if (code == 429 || code !in 200..299) {
                OnlineHttpResult.Failure(code, body.take(200))
            } else {
                OnlineHttpResult.Ok(body)
            }
        } catch (e: Exception) {
            OnlineHttpResult.Failure(-1, e.message ?: "error")
        } finally {
            conn.disconnect()
        }
    }
}
