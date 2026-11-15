package no.navi.app

import org.json.JSONArray
import org.json.JSONObject
import uniffi.navi.FfiComapsRoute
import uniffi.navi.FfiComapsRouteCode

/**
 * Total climb / descent for a planned route and where it came from.
 *
 * With the MWM client flag ON the numbers come from the CoMaps route
 * (`ascent_m` / `descent_m` and the elevation profile, sidecar EL02 values
 * already applied by the shim). Otherwise they are read from the `.rkyv` plan
 * report (`eco_climb_m` / `eco_descent_m`).
 */
data class RouteClimb(
    val climbM: Double,
    val descentM: Double,
    val source: Source,
    /** Elevation in metres along the route polyline; null where unknown. */
    val profileM: List<Double?> = emptyList(),
) {
    enum class Source(
        val label: String,
    ) {
        COMAPS_MWM("mwm"),
        PLAN_REPORT("plan_report"),
    }
}

object RouteClimbSource {
    @Volatile
    private var latestComaps: RouteClimb? = null

    /** Remember the climb of the CoMaps route that was just built. */
    fun publishComaps(climb: RouteClimb?) {
        latestComaps = climb
    }

    /** Publish the climb of a finished CoMaps route (`ascentM` / `elevationJson`). */
    fun publishFromRoute(route: FfiComapsRoute) {
        publishComaps(fromComaps(route.ascentM, route.descentM, route.elevationJson))
    }

    /**
     * Entry point for every code path that obtains a CoMaps route. Publishes the
     * climb of an OK route and returns it; any other code clears a stale value
     * so a failed route never feeds the next plan's eco line.
     */
    fun publishIfOk(route: FfiComapsRoute): RouteClimb? {
        if (route.code != FfiComapsRouteCode.OK) {
            publishComaps(null)
            return null
        }
        publishFromRoute(route)
        return latestComaps
    }

    /** Take (and clear) the published CoMaps climb. */
    fun takeComaps(): RouteClimb? {
        val c = latestComaps
        latestComaps = null
        return c
    }

    /**
     * Climb from a CoMaps route result. The totals from the shim are used when
     * valid; otherwise they are summed from the elevation profile. Null when
     * neither is usable.
     */
    fun fromComaps(
        ascentM: Double,
        descentM: Double,
        elevationJson: String,
    ): RouteClimb? {
        val profile = parseProfile(elevationJson)
        if (ascentM.isFinite() && descentM.isFinite() && ascentM >= 0.0 && descentM >= 0.0) {
            return RouteClimb(ascentM, descentM, RouteClimb.Source.COMAPS_MWM, profile)
        }
        return sumProfile(profile)?.let { (up, down) ->
            RouteClimb(up, down, RouteClimb.Source.COMAPS_MWM, profile)
        }
    }

    /** Climb from the `.rkyv` plan report (`eco_climb_m`, `eco_descent_m`). */
    fun fromReport(report: String): RouteClimb? {
        val climb = reportDouble(report, "eco_climb_m") ?: return null
        val descent = reportDouble(report, "eco_descent_m") ?: 0.0
        return RouteClimb(climb, descent, RouteClimb.Source.PLAN_REPORT)
    }

    /**
     * MWM flag ON and a CoMaps climb available: use it. Anything else falls
     * back to the plan report, so flag OFF behaves exactly as before.
     */
    fun resolve(
        mwmEnabled: Boolean,
        comaps: RouteClimb?,
        report: String,
    ): RouteClimb? = if (mwmEnabled && comaps != null) comaps else fromReport(report)

    /** `{"points":[{ele_m}|null,...]}` from the shim, or a legacy bare array. */
    internal fun parseProfile(json: String): List<Double?> {
        val t = json.trim()
        if (t.isEmpty()) return emptyList()
        return runCatching {
            val arr =
                if (t.startsWith("{")) {
                    JSONObject(t).optJSONArray("points") ?: JSONArray()
                } else {
                    JSONArray(t)
                }
            (0 until arr.length()).map { i ->
                when (val v = arr.opt(i)) {
                    null, JSONObject.NULL -> null
                    is JSONObject -> if (v.has("ele_m") && !v.isNull("ele_m")) v.optDouble("ele_m") else null
                    is Number -> v.toDouble()
                    else -> null
                }
            }
        }.getOrDefault(emptyList())
    }

    private fun sumProfile(profile: List<Double?>): Pair<Double, Double>? {
        var up = 0.0
        var down = 0.0
        var pairs = 0
        for (i in 1 until profile.size) {
            val a = profile[i - 1]
            val b = profile[i]
            if (a == null || b == null) continue
            pairs++
            val d = b - a
            if (d > 0) up += d else down -= d
        }
        return if (pairs == 0) null else up to down
    }

    private fun reportDouble(
        report: String,
        key: String,
    ): Double? =
        Regex("""(?:^|[;\s])$key=([-+0-9.eE]+)""")
            .find(report)
            ?.groupValues
            ?.getOrNull(1)
            ?.toDoubleOrNull()
}
