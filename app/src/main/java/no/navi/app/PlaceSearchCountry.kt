package no.navi.app

/**
 * Query-side country disambiguation: detect an explicit country name/ISO in a
 * place query and hard-filter candidates to that country.
 *
 * Result-side helpers derive a stable ISO + English country label from a
 * [uniffi.navi.PlaceHit] (Nominatim `|cc` kind suffix, or Geofabrik [regionId]).
 */
data class CountryQualifiedQuery(
    /** Query with country tokens removed (for geocode / FTS). */
    val placeQuery: String,
    /** Lowercase ISO 3166-1 alpha-2 when a country token was recognized. */
    val countryIso: String?,
)

private data class CountryAlias(
    val iso: String,
    val label: String,
    val tokens: Set<String>,
)

private val COUNTRY_ALIASES: List<CountryAlias> =
    listOf(
        CountryAlias("no", "Norway", setOf("no", "norway", "norge", "noreg")),
        CountryAlias("se", "Sweden", setOf("se", "sweden", "sverige")),
        CountryAlias("dk", "Denmark", setOf("dk", "denmark", "danmark")),
        CountryAlias(
            "de",
            "Germany",
            setOf("de", "germany", "deutschland", "federal republic of germany"),
        ),
        CountryAlias(
            "nl",
            "Netherlands",
            setOf("nl", "netherlands", "nederland", "holland"),
        ),
        CountryAlias(
            "be",
            "Belgium",
            setOf("be", "belgium", "belgie", "belgië", "belgique"),
        ),
        CountryAlias("at", "Austria", setOf("at", "austria", "österreich", "osterreich")),
        CountryAlias("fi", "Finland", setOf("fi", "finland", "suomi")),
        CountryAlias("pl", "Poland", setOf("pl", "poland", "polska")),
        CountryAlias("fr", "France", setOf("fr", "france")),
        CountryAlias("gb", "United Kingdom", setOf("gb", "uk", "united kingdom", "britain", "england", "scotland", "wales")),
        CountryAlias("us", "United States", setOf("us", "usa", "united states", "united states of america", "america")),
        CountryAlias("cz", "Czechia", setOf("cz", "czechia", "czech republic", "cesko", "česko")),
    )

private val TOKEN_TO_ISO: Map<String, String> =
    COUNTRY_ALIASES
        .flatMap { alias -> alias.tokens.map { tok -> tok.lowercase() to alias.iso } }
        .toMap()

private val ISO_TO_LABEL: Map<String, String> =
    COUNTRY_ALIASES.associate { it.iso to it.label }

/** Split "Bergen, Germany" / "Bergen Germany" into place + ISO filter. */
fun splitCountryQualifiedQuery(query: String): CountryQualifiedQuery {
    val trimmed = query.trim()
    if (trimmed.isEmpty()) return CountryQualifiedQuery("", null)

    // Comma form: last segment is often the country.
    val commaParts =
        trimmed
            .split(',')
            .map { it.trim() }
            .filter { it.isNotEmpty() }
    if (commaParts.size >= 2) {
        val last = commaParts.last()
        val iso = resolveCountryToken(last)
        if (iso != null) {
            val place = commaParts.dropLast(1).joinToString(", ")
            if (place.isNotBlank()) return CountryQualifiedQuery(place, iso)
        }
    }

    // Space form: trailing multi-word or single-word country token.
    val lower = trimmed.lowercase()
    for (alias in COUNTRY_ALIASES.sortedByDescending { a ->
        a.tokens.maxOf { it.length }
    }) {
        for (tok in alias.tokens.sortedByDescending { it.length }) {
            val suffix = Regex("""(?iu)(?:^|[\s,]+)""" + Regex.escape(tok) + """\s*$""")
            if (suffix.containsMatchIn(lower) && lower != tok) {
                val place =
                    trimmed
                        .replace(Regex("(?iu)[\\s,]*" + Regex.escape(tok) + """\s*$"""), "")
                        .trim()
                        .trim(',')
                        .trim()
                if (place.isNotBlank()) return CountryQualifiedQuery(place, alias.iso)
            }
        }
    }
    return CountryQualifiedQuery(trimmed, null)
}

fun resolveCountryToken(raw: String): String? {
    val t = raw.trim().lowercase()
    if (t.isEmpty()) return null
    TOKEN_TO_ISO[t]?.let { return it }
    // Allow "DE" / "NO" two-letter codes already lowercase in map.
    return null
}

fun countryLabelForIso(iso: String?): String = iso?.lowercase()?.let { ISO_TO_LABEL[it] } ?: ""

/** Nominatim kind suffix `|cc` or Geofabrik region path → ISO. */
fun placeHitCountryIso(hit: uniffi.navi.PlaceHit): String? {
    val kindCc =
        hit.kind
            .substringAfterLast('|', missingDelimiterValue = "")
            .trim()
            .lowercase()
    if (kindCc.length == 2 && kindCc.all { it.isLetter() }) return kindCc
    return countryIsoFromRegionId(hit.regionId)
}

fun countryIsoFromRegionId(regionId: String): String? {
    val norm = GeofabrikDownloadCatalog.canonicalizePath(regionId)
    if (norm.isEmpty()) return null
    GeofabrikDownloadCatalog
        .findByPath(norm)
        ?.iso
        ?.lowercase()
        ?.let { return it }
    // Walk parents: europe/norway/vestlandet → europe/norway
    var cur = norm
    while (true) {
        val slash = cur.lastIndexOf('/')
        if (slash <= 0) break
        cur = cur.substring(0, slash)
        GeofabrikDownloadCatalog
            .findByPath(cur)
            ?.iso
            ?.lowercase()
            ?.let { return it }
    }
    // Fallback slug: europe/germany/... → de via known map
    val parts = norm.split('/')
    if (parts.size >= 2) {
        resolveCountryToken(parts[1].replace('-', ' '))?.let { return it }
        // germany, norway, sweden already in aliases as full names — also try slug
        val slug = parts[1].lowercase()
        when (slug) {
            "germany" -> return "de"
            "norway" -> return "no"
            "sweden" -> return "se"
            "denmark" -> return "dk"
            "netherlands" -> return "nl"
            "belgium" -> return "be"
            "austria" -> return "at"
            "finland" -> return "fi"
            "poland" -> return "pl"
            "france" -> return "fr"
            "czech-republic" -> return "cz"
        }
    }
    return null
}

/** Admin / leaf region label from Geofabrik path (e.g. vestlandet → Vestlandet). */
fun regionLabelFromRegionId(regionId: String): String {
    val norm = GeofabrikDownloadCatalog.canonicalizePath(regionId)
    if (norm.isEmpty()) return ""
    val leaf = norm.substringAfterLast('/')
    if (leaf.isEmpty()) return ""
    // Skip bare continent or country slug when it's the whole path country.
    val country = GeofabrikDownloadCatalog.findByPath(norm)
    if (country != null && country.path == norm) return ""
    return leaf
        .split('-', '_')
        .filter { it.isNotEmpty() }
        .joinToString(" ") { part ->
            part.replaceFirstChar { ch -> ch.titlecase() }
        }
}

/**
 * Search-row label. When [disambiguate] is true (multi-candidate lists), always
 * append region + country so duplicate place names are distinguishable.
 */
fun placeHitSearchLabel(
    hit: uniffi.navi.PlaceHit,
    disambiguate: Boolean,
): String {
    val base = placeHitDisplayLabel(hit)
    if (!disambiguate) return base
    val parts = mutableListOf<String>()

    fun add(raw: String) {
        val t = raw.trim()
        if (t.isEmpty()) return
        if (parts.any { it.equals(t, ignoreCase = true) }) return
        // Avoid "Bergen, Bergen, Norway"
        if (parts.any { it.equals(t, ignoreCase = true) }) return
        parts.add(t)
    }

    add(base)
    val region = regionLabelFromRegionId(hit.regionId)
    // Prefer municipality/subArea already in base; add leaf region if new.
    if (region.isNotBlank() &&
        !base.contains(region, ignoreCase = true) &&
        !hit.municipality.equals(region, ignoreCase = true)
    ) {
        add(region)
    }
    val country = countryLabelForIso(placeHitCountryIso(hit))
    if (country.isNotBlank() && !base.contains(country, ignoreCase = true)) {
        add(country)
    }
    return parts.joinToString(", ")
}

fun placeHitMatchesCountryIso(
    hit: uniffi.navi.PlaceHit,
    iso: String,
): Boolean {
    val want = iso.lowercase()
    val got = placeHitCountryIso(hit) ?: return false
    return got == want
}

fun filterHitsByCountryIso(
    hits: List<uniffi.navi.PlaceHit>,
    iso: String?,
): List<uniffi.navi.PlaceHit> {
    if (iso.isNullOrBlank()) return hits
    return hits.filter { placeHitMatchesCountryIso(it, iso) }
}

/** Encode ISO into PlaceHit.kind as `…|cc` (online geocode). */
fun kindWithCountryIso(
    kind: String,
    iso: String?,
): String {
    val base = kind.substringBefore('|')
    val cc = iso?.trim()?.lowercase().orEmpty()
    return if (cc.length == 2) "$base|$cc" else base
}
