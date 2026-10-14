package no.navi.app

/**
 * Policy for optional online place search (Nominatim-compatible).
 *
 * Requests are allowed only when the setting is on, the user explicitly
 * submitted the query, and the configured base URL is non-empty.
 */
data class OnlinePlaceSearchConfig(
    val enabled: Boolean,
    val baseUrl: String,
    val userAgent: String,
    val explicitSubmit: Boolean,
)

data class OnlineHttpRequest(
    val url: String,
    val headers: Map<String, String>,
)

sealed class OnlineHttpResult {
    data class Ok(
        val body: String,
    ) : OnlineHttpResult()

    data class Failure(
        val code: Int,
        val message: String = "",
    ) : OnlineHttpResult()
}

fun interface OnlineHttpClient {
    suspend fun execute(request: OnlineHttpRequest): OnlineHttpResult
}

const val OSM_PLACE_SEARCH_ATTRIBUTION = "Data © OpenStreetMap contributors"

const val NAVI_PROJECT_PAGE = "https://github.com/Supermagnum/Navi"

fun shouldSendOnlinePlaceSearch(config: OnlinePlaceSearchConfig): Boolean =
    config.enabled &&
        config.explicitSubmit &&
        config.baseUrl.trim().isNotEmpty()

fun naviOnlinePlaceSearchUserAgent(versionName: String): String {
    val ver = versionName.trim().ifBlank { "0" }
    return "Navi/$ver ($NAVI_PROJECT_PAGE)"
}

fun normalizeNominatimBaseUrl(raw: String): String = raw.trim().trimEnd('/')

fun nominatimSearchUrl(
    baseUrl: String,
    query: String,
    limit: Int,
    countryIso: String? = null,
): String {
    val base = normalizeNominatimBaseUrl(baseUrl)
    if (base.isEmpty() || query.isBlank()) return ""
    val enc = java.net.URLEncoder.encode(query, "UTF-8")
    val cc =
        countryIso
            ?.trim()
            ?.lowercase()
            ?.takeIf { it.length == 2 }
    val ccParam = if (cc != null) "&countrycodes=$cc" else ""
    return "$base/search?q=$enc&format=jsonv2&limit=$limit&addressdetails=1$ccParam"
}

fun nominatimReverseUrl(
    baseUrl: String,
    lat: Double,
    lon: Double,
): String {
    val base = normalizeNominatimBaseUrl(baseUrl)
    if (base.isEmpty()) return ""
    return "$base/reverse?lat=$lat&lon=$lon&format=jsonv2&addressdetails=1&zoom=18"
}
