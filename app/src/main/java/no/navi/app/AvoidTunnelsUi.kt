package no.navi.app

import uniffi.navi.TravelProfile

/**
 * Avoid-tunnels toggle visibility and effective planner flag.
 *
 * Hiking / bicycle / e-bike keep unsafe tunnels out via the native HardAvoid
 * access rule (`NonMotorTunnelAccess`). Those profiles must not expose a user
 * switch that can turn tunnel safety off. Soft-avoid detours may still run as
 * a backup when a forbidden tunnel (`is_tunnel && !tunnel_ok`) appears.
 *
 * Motor profiles keep the optional soft-avoid toggle.
 */
object AvoidTunnelsUi {
    /** Profiles whose tunnel safety is automatic (no user toggle). */
    fun isAutoSafeProfile(profile: TravelProfile): Boolean =
        when (profile) {
            TravelProfile.HIKING,
            TravelProfile.BICYCLE,
            TravelProfile.BICYCLE_ELECTRIC,
            -> true
            else -> false
        }

    /** True when the profile panel should show the Avoid tunnels switch. */
    fun showsUserToggle(profile: TravelProfile): Boolean = !isAutoSafeProfile(profile)

    /**
     * Value passed into planners. Auto-safe profiles always enable soft-avoid
     * backup; motor profiles honor the user preference.
     */
    fun effectiveAvoidTunnels(
        profile: TravelProfile,
        userPreference: Boolean,
    ): Boolean = if (isAutoSafeProfile(profile)) true else userPreference

    /**
     * Whether a restored / persisted avoid-tunnels preference may update UI
     * state. Auto-safe profiles ignore stored values so a saved `false` cannot
     * disable tunnel safety.
     */
    fun acceptsPersistedPreference(profile: TravelProfile): Boolean = !isAutoSafeProfile(profile)

    /**
     * Rewrite the shared avoidance status block so hiking / bike / e-bike do
     * not show an "Avoid tunnels: ON/OFF" line that looks like a user switch.
     */
    fun rewriteAvoidanceReport(
        profile: TravelProfile,
        report: String,
    ): String {
        if (!isAutoSafeProfile(profile)) return report
        return report
            .lineSequence()
            .map { line ->
                if (line.startsWith("Avoid tunnels:")) {
                    "Unsafe tunnels: blocked automatically"
                } else {
                    line
                }
            }.joinToString("\n")
    }
}
