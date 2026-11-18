package no.navi.app

import uniffi.navi.TravelProfile

/**
 * Vehicle clearance messaging on the MWM path.
 *
 * When sidecars carry `vehicle_limits`, truck / mobile-home / car-with-height
 * routes apply restrictions in the car estimator. Warnings are for missing
 * data or leaves whose match rate is below the configured threshold (default
 * 0.90); matched restrictions are still applied in those regions.
 */
object VehicleRestrictionUi {
    /** Profiles that pass stored dimensions into the car estimator. */
    fun appliesStoredLimits(profile: TravelProfile): Boolean =
        when (profile) {
            TravelProfile.CAR,
            TravelProfile.CAR_ELECTRIC,
            TravelProfile.TRUCK,
            TravelProfile.TRUCK_ELECTRIC,
            TravelProfile.MOBILE_HOME,
            -> true
            else -> false
        }

    /** Motorcycle never applies dimension limits on MWM. */
    fun motorcycleNoLimits(profile: TravelProfile): Boolean =
        profile == TravelProfile.MOTORCYCLE || profile == TravelProfile.MOTORCYCLE_ELECTRIC

    /**
     * Temporary STEP-1 style "not applied" — kept for callers that still need
     * a honesty note when vehicle_limits data is known missing for the route.
     * Prefer [warnDataUnavailable] when leaf names are known.
     */
    @Deprecated("Use warnDataUnavailable when leaf coverage is known")
    fun alwaysWarnOnMwm(profile: TravelProfile): Boolean =
        when (profile) {
            TravelProfile.TRUCK,
            TravelProfile.TRUCK_ELECTRIC,
            TravelProfile.MOBILE_HOME,
            -> true
            else -> false
        }

    /**
     * When vehicle_limits data is present and every route leaf meets the
     * coverage threshold, do not warn. When [dataAvailable] is false, warn for
     * truck/mobile-home and for car when a height is stored.
     */
    fun warnOnPlannedRoute(
        profile: TravelProfile,
        heightM: Double?,
        /** False until the planner confirms full coverage for the route leaves. */
        dataAvailable: Boolean = false,
    ): Boolean {
        if (dataAvailable) return false
        if (motorcycleNoLimits(profile)) return false
        if (alwaysWarnOnMwm(profile)) return true
        return when (profile) {
            TravelProfile.CAR,
            TravelProfile.CAR_ELECTRIC,
            -> heightM != null
            else -> false
        }
    }

    fun showProfilePanelNote(
        profile: TravelProfile,
        heightM: Double?,
        dataAvailable: Boolean = false,
    ): Boolean =
        warnOnPlannedRoute(profile, heightM, dataAvailable) ||
            (motorcycleNoLimits(profile) && MwmProductionPlanner.limitsNotApplied(profile))

    fun appendPlanWarning(
        statusLine: String,
        warning: String,
        profile: TravelProfile,
        heightM: Double?,
        dataAvailable: Boolean = false,
    ): String {
        if (!warnOnPlannedRoute(profile, heightM, dataAvailable)) return statusLine
        val w = warning.trim()
        if (w.isEmpty()) return statusLine
        if (statusLine.contains(w)) return statusLine
        return if (statusLine.isBlank()) w else "$statusLine\n$w"
    }

    /** Name regions where vehicle_limits are missing / mismatched. */
    fun warnDataUnavailable(
        regionNames: List<String>,
        unavailableTemplate: String,
    ): String? {
        val names = regionNames.map { it.trim() }.filter { it.isNotEmpty() }
        if (names.isEmpty()) return null
        val joined = names.distinct().joinToString(", ") { it.replace('_', ' ') }
        return unavailableTemplate.replace("%1\$s", joined).replace("%s", joined)
    }

    /**
     * Coverage parsed from a MWM plan report (`vehicle_limits_ok=` /
     * `vehicle_limits_missing=` / `vehicle_limits_partial=`).
     */
    data class Coverage(
        val known: Boolean,
        /** True when every leaf has vehicle_limits and meets the match-rate threshold. */
        val dataAvailable: Boolean,
        val missingLeaves: List<String>,
        val partialLeaves: List<String>,
    )

    fun parseCoverage(report: String): Coverage {
        val okLine =
            report.lineSequence().firstOrNull { it.startsWith("vehicle_limits_ok=") }
                ?: return Coverage(
                    known = false,
                    dataAvailable = false,
                    missingLeaves = emptyList(),
                    partialLeaves = emptyList(),
                )
        val ok = okLine.substringAfter('=').trim().equals("true", ignoreCase = true)
        fun listAfter(prefix: String): List<String> =
            report
                .lineSequence()
                .firstOrNull { it.startsWith(prefix) }
                ?.substringAfter('=')
                ?.split(',')
                ?.map { it.trim() }
                ?.filter { it.isNotEmpty() }
                .orEmpty()
        val missing = listAfter("vehicle_limits_missing=")
        val partial = listAfter("vehicle_limits_partial=")
        return Coverage(
            known = true,
            dataAvailable = ok && missing.isEmpty() && partial.isEmpty(),
            missingLeaves = missing,
            partialLeaves = partial,
        )
    }

    /**
     * Banner / status text when [warnOnPlannedRoute] is true. Prefers a named
     * partial-coverage message, then missing regions, then the generic banner.
     */
    fun routeWarningText(
        profile: TravelProfile,
        heightM: Double?,
        dataAvailable: Boolean,
        missingLeaves: List<String>,
        genericBanner: String,
        regionTemplate: String,
        partialLeaves: List<String> = emptyList(),
        partialTemplate: String = regionTemplate,
    ): String? {
        if (!warnOnPlannedRoute(profile, heightM, dataAvailable)) return null
        warnDataUnavailable(partialLeaves, partialTemplate)?.let { return it }
        return warnDataUnavailable(missingLeaves, regionTemplate) ?: genericBanner
    }
}
