package no.navi.app

import uniffi.navi.TravelProfile

/**
 * Temporary MWM-path honesty for vehicle clearance (STEP 1).
 *
 * Until the sidecar hook is live, truck / mobile-home / electric-truck routes
 * must warn that height / width / length / weight limits are not applied.
 * Car (and electric car) warn only when a height value is stored.
 */
object VehicleRestrictionUi {
    /** Profiles that always need the temporary "not yet applied" notice on MWM. */
    fun alwaysWarnOnMwm(profile: TravelProfile): Boolean =
        when (profile) {
            TravelProfile.TRUCK,
            TravelProfile.TRUCK_ELECTRIC,
            TravelProfile.MOBILE_HOME,
            -> true
            else -> false
        }

    /**
     * True when a planned route / navigation start must show the temporary
     * restriction warning (MWM path). Motorcycle is excluded here; it still
     * uses [MwmProductionPlanner.limitsNotApplied] in the profile panel.
     */
    fun warnOnPlannedRoute(
        profile: TravelProfile,
        heightM: Double?,
    ): Boolean {
        if (alwaysWarnOnMwm(profile)) return true
        return when (profile) {
            TravelProfile.CAR,
            TravelProfile.CAR_ELECTRIC,
            -> heightM != null
            else -> false
        }
    }

    /** Profile-panel note: planned-route cases plus motorcycle / electric motorcycle. */
    fun showProfilePanelNote(
        profile: TravelProfile,
        heightM: Double?,
    ): Boolean =
        warnOnPlannedRoute(profile, heightM) ||
            MwmProductionPlanner.limitsNotApplied(profile)

    /** Append the localized warning to a plan status line when needed. */
    fun appendPlanWarning(
        statusLine: String,
        warning: String,
        profile: TravelProfile,
        heightM: Double?,
    ): String {
        if (!warnOnPlannedRoute(profile, heightM)) return statusLine
        val w = warning.trim()
        if (w.isEmpty()) return statusLine
        if (statusLine.contains(w)) return statusLine
        return if (statusLine.isBlank()) w else "$statusLine\n$w"
    }
}
