package no.navi.app

/**
 * Host "Stay in Country" preference: when on, planning passes
 * `allowed_countries = listOf(startIso)` so the existing graph country filter
 * keeps the route inside the trip origin country.
 *
 * Start country must come from Natural Earth via [uniffi.navi.countryIsoAt] on a
 * background thread (never the main looper — cold polygon load can ANR).
 */
object StayInCountry {
    const val LABEL = "Stay in Country"

    const val SHORT_DESCRIPTION =
        "Avoid crossing international borders, even if a foreign route is faster."

    const val DETAILS =
        "When on, Navi only routes through roads inside your starting country.\n" +
            "The route may be longer or slower, but never crosses a border - useful\n" +
            "when carrying pets, plants, or goods that need customs paperwork or are\n" +
            "subject to quarantine rules in a neighboring country.\n" +
            "Example: Drammen to Kautokeino normally routes through Sweden and\n" +
            "Finland, which is faster. With Stay in Country on, the route stays\n" +
            "entirely within Norway."

    /**
     * Planning argument for [uniffi.navi.planCarRoute].
     *
     * @return `listOf(iso)` when enabled and [startCountryIso] is a 2-letter code;
     *   `null` when off or the origin country could not be resolved.
     */
    fun allowedCountriesForPlan(
        enabled: Boolean,
        startCountryIso: String?,
    ): List<String>? {
        if (!enabled) return null
        val iso =
            startCountryIso
                ?.trim()
                ?.lowercase()
                ?.takeIf { it.length == 2 && it.all(Char::isLetter) }
                ?: return null
        return listOf(iso)
    }

    fun noRouteMessage(countryLabel: String): String {
        val name = countryLabel.trim().ifBlank { "the starting country" }
        return "No route found that stays within $name with Stay in Country on. " +
            "Try turning it off, or add a via point."
    }

    /** Prefer English label from [countryLabelForIso]; fall back to uppercase ISO. */
    fun countryLabelFromIso(iso: String?): String {
        val cleaned = iso?.trim()?.lowercase().orEmpty()
        if (cleaned.isEmpty()) return "the starting country"
        val labeled = countryLabelForIso(cleaned)
        return labeled.ifBlank { cleaned.uppercase() }
    }
}
