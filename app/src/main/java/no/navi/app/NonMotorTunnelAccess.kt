package no.navi.app

/**
 * Product rule for hiking / bicycle / e-bike tunnel access.
 *
 * A tunnel is allowed only when at least one of:
 * - foot=yes or foot=designated (hiking); bicycle=yes/designated for bike profiles
 * - a sidewalk tag (OSM sidewalk=* / sidewalk:left|right|both, or mapped footway=sidewalk)
 * - highway=footway, highway=cycleway, or highway=path
 *
 * Otherwise treat as foot=no / bicycle=no (deny) or apply a HardAvoid-style
 * penalty so the router will not pick it.
 *
 * CoMaps MWM encodes the same facts as feature types (not raw OSM strings):
 * - `hwtag|yesfoot` ← foot=yes/designated, or positive sidewalk=* when foot unset
 * - `hwtag|yesbicycle` ← bicycle=yes/designated
 * - `highway|footway|sidewalk` ← separately mapped sidewalk ways
 * - `highway|footway|tunnel`, `highway|cycleway|tunnel`, `highway|path|tunnel`
 *
 * Turns JSON only exposes `is_tunnel` unless the CoMaps patch also emits
 * `tunnel_ok`. This pure rule is the shared allow/deny oracle for unit tests
 * and for interpreting those encoded flags.
 */
internal enum class NonMotorTunnelProfile {
    HIKING,
    BICYCLE,
}

/**
 * Snapshot of OSM / MWM signals for the tunnel-access decision.
 * Raw OSM strings and CoMaps-encoded booleans are OR'd where both are set.
 */
internal data class NonMotorTunnelTags(
    /** OSM `tunnel=*` (use `"yes"` when the feature is already known to be a tunnel). */
    val tunnel: String? = null,
    val highway: String? = null,
    val foot: String? = null,
    val bicycle: String? = null,
    val sidewalk: String? = null,
    val footway: String? = null,
    /** CoMaps `hwtag|yesfoot` on the feature. */
    val hasYesFoot: Boolean = false,
    /** CoMaps `hwtag|yesbicycle` on the feature. */
    val hasYesBicycle: Boolean = false,
    /** CoMaps `highway|footway|sidewalk` (or equivalent sidewalk type). */
    val hasSidewalkType: Boolean = false,
)

/** Wetland / vehicle HardAvoid multiplier used by the CoMaps estimator patch. */
internal const val NON_MOTOR_TUNNEL_HARD_AVOID_MULT = 1.0e6

internal fun isTunnelTag(raw: String?): Boolean {
    val t = raw?.trim().orEmpty()
    return t.isNotEmpty() && !t.equals("no", ignoreCase = true)
}

internal fun isAccessYesOrDesignated(raw: String?): Boolean {
    val t = raw?.trim()?.lowercase().orEmpty()
    return t == "yes" ||
        t == "designated" ||
        t == "permissive" ||
        t == "official" ||
        t == "true" ||
        t == "1"
}

/** Positive OSM sidewalk=* / sidewalk:left|right|both (not no / none / separate). */
internal fun isPositiveSidewalkTag(raw: String?): Boolean {
    val t = raw?.trim()?.lowercase().orEmpty()
    if (t.isEmpty() || t == "no" || t == "none" || t == "separate") return false
    return t == "yes" ||
        t == "both" ||
        t == "left" ||
        t == "right" ||
        t == "shared" ||
        t == "shared_lane"
}

internal fun isPedestrianOrCyclePathHighway(highway: String?): Boolean {
    val h = highway?.trim()?.lowercase().orEmpty()
    return h == "footway" || h == "cycleway" || h == "path"
}

/**
 * True when a tunnel may be used by [profile]. Non-tunnels (`tunnel` absent /
 * `no`) are unrestricted by this rule.
 */
internal fun nonMotorTunnelAllowed(
    profile: NonMotorTunnelProfile,
    tags: NonMotorTunnelTags,
): Boolean {
    if (!isTunnelTag(tags.tunnel)) return true

    if (isPedestrianOrCyclePathHighway(tags.highway)) return true
    if (tags.hasSidewalkType) return true
    if (isPositiveSidewalkTag(tags.sidewalk)) return true
    if (tags.footway?.equals("sidewalk", ignoreCase = true) == true) return true

    return when (profile) {
        NonMotorTunnelProfile.HIKING ->
            tags.hasYesFoot || isAccessYesOrDesignated(tags.foot)
        NonMotorTunnelProfile.BICYCLE ->
            tags.hasYesBicycle || isAccessYesOrDesignated(tags.bicycle)
    }
}

/**
 * Weight multiplier for a tunnel edge. `1.0` when allowed / not a tunnel;
 * [NON_MOTOR_TUNNEL_HARD_AVOID_MULT] when forbidden.
 */
internal fun nonMotorTunnelCostMultiplier(
    profile: NonMotorTunnelProfile,
    tags: NonMotorTunnelTags,
): Double {
    if (!isTunnelTag(tags.tunnel)) return 1.0
    return if (nonMotorTunnelAllowed(profile, tags)) 1.0 else NON_MOTOR_TUNNEL_HARD_AVOID_MULT
}

/**
 * Turns JSON: a forbidden tunnel step is `is_tunnel` without `tunnel_ok`.
 * Legacy turns (no `tunnel_ok`) count as forbidden so soft-avoid still runs.
 */
internal fun isForbiddenTunnelTurn(turn: org.json.JSONObject): Boolean {
    if (!turn.optBoolean("is_tunnel", false)) return false
    return !turn.optBoolean("tunnel_ok", false)
}
