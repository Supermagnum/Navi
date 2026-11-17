package no.navi.app

import uniffi.navi.TravelProfile

/**
 * Vehicle clearance messaging on the MWM path.
 *
 * When sidecars carry `vehicle_limits`, truck / mobile-home / car-with-height
 * routes apply restrictions in the car estimator. Warnings are only for
 * missing data (or motorcycle, which never carries dimensions).
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
    fun motorcycleNoLimits(profile: TravelProfile): Boolean = profile == TravelProfile.MOTORCYCLE || profile == TravelProfile.MOTORCYCLE_ELECTRIC

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
     * When vehicle_limits data is present for the route leaves, do not warn
     * "not applied". When [dataAvailable] is false, warn for truck/mobile-home
     * and for car when a height is stored.
     */
    fun warnOnPlannedRoute(
        profile: TravelProfile,
        heightM: Double?,
        /** False until the planner confirms `vehicle_limits` for the route leaves. */
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
     * `vehicle_limits_missing=`). When the keys are absent, [known] is false
     * and callers keep the conservative default (warn).
     */
    data class Coverage(
        val known: Boolean,
        val dataAvailable: Boolean,
        val missingLeaves: List<String>,
    )

    fun parseCoverage(report: String): Coverage {
        val okLine =
            report.lineSequence().firstOrNull { it.startsWith("vehicle_limits_ok=") }
                ?: return Coverage(known = false, dataAvailable = false, missingLeaves = emptyList())
        val ok = okLine.substringAfter('=').trim().equals("true", ignoreCase = true)
        val missing =
            report
                .lineSequence()
                .firstOrNull { it.startsWith("vehicle_limits_missing=") }
                ?.substringAfter('=')
                ?.split(',')
                ?.map { it.trim() }
                ?.filter { it.isNotEmpty() }
                .orEmpty()
        return Coverage(known = true, dataAvailable = ok && missing.isEmpty(), missingLeaves = missing)
    }

    /**
     * Banner / status text when [warnOnPlannedRoute] is true. Prefers a named
     * region list when [missingLeaves] is non-empty.
     */
    fun routeWarningText(
        profile: TravelProfile,
        heightM: Double?,
        dataAvailable: Boolean,
        missingLeaves: List<String>,
        genericBanner: String,
        regionTemplate: String,
    ): String? {
        if (!warnOnPlannedRoute(profile, heightM, dataAvailable)) return null
        return warnDataUnavailable(missingLeaves, regionTemplate) ?: genericBanner
    }
}
