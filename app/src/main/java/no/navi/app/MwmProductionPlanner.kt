package no.navi.app

import android.content.Context
import android.util.Log
import org.json.JSONArray
import org.json.JSONObject
import uniffi.navi.CorridorRouteResult
import uniffi.navi.FfiComapsLatLon
import uniffi.navi.FfiComapsOptimize
import uniffi.navi.FfiComapsRoute
import uniffi.navi.FfiComapsRouteCode
import uniffi.navi.TravelProfile
import uniffi.navi.comapsNativeAvailable
import uniffi.navi.comapsSessionIsOpen
import uniffi.navi.comapsSessionRoute
import uniffi.navi.comapsSessionSetAvoidOptions
import uniffi.navi.comapsSessionSetBikeWetland
import uniffi.navi.comapsSessionSetOptimize
import uniffi.navi.comapsSessionSetTurnsLocale
import uniffi.navi.setMwmProductionPlannerActive
import java.util.Locale
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.sin
import kotlin.math.sqrt

/**
 * Production planner when the MWM client flag is ON: CoMaps session end-to-end,
 * no `.rkyv` [uniffi.navi.planCarRoute] / [uniffi.navi.planHikingRoute].
 *
 * Sets [setMwmProductionPlannerActive] for the duration of a plan so a stray
 * `.rkyv` call fails with a logged assertion.
 */
object MwmProductionPlanner {
    private const val TAG = "NaviPlan"
    private const val MIN_SIM_KMH = 15.0
    private const val MAX_SIM_KMH = 110.0
    private const val DEFAULT_SIM_KMH = 60.0

    /** Drape route style of the last plan: 0 car, 1 pedestrian, 2 bicycle ([drapeRouteType]). */
    @Volatile
    var lastRouteStyle: Int = 0
        private set

    @Volatile
    private var turnKey: Int = 0

    @Volatile
    private var turnIdx: IntArray = IntArray(0)

    /** Polyline point indices of the last plan's turns, for drape turn arrows; null for another polyline. */
    fun lastTurnPointIndices(polyline: String): IntArray? = if (polyline.isNotBlank() && polyline.hashCode() == turnKey && turnIdx.isNotEmpty()) turnIdx else null

    /**
     * Profiles that CoMaps routes as a plain car without vehicle dimension / weight limits
     * (electric car, truck, mobile home, motorcycle): the UI says so next to the route options.
     */
    fun limitsNotApplied(profile: TravelProfile): Boolean =
        when (profile) {
            TravelProfile.CAR_ELECTRIC,
            TravelProfile.TRUCK,
            TravelProfile.TRUCK_ELECTRIC,
            TravelProfile.MOBILE_HOME,
            TravelProfile.MOTORCYCLE,
            TravelProfile.MOTORCYCLE_ELECTRIC,
            -> true
            else -> false
        }

    /** True when this process should use CoMaps for the live route screen. */
    fun shouldUse(context: Context): Boolean {
        if (!MwmClientBootstrap.isEnabled()) return false
        if (!comapsNativeAvailable()) return false
        if (!MwmClientBootstrap.nativeLoaded) return false
        // Instrumented tests open the session on a test root without bootstrap.
        if (comapsSessionIsOpen()) return true
        if (ComapsSessionBootstrap.isOpen()) return true
        return ComapsSessionBootstrap.ensureOpen(context, force = false)
    }

    /**
     * Bumped by [cancel]; a [plan] that started under an older generation returns
     * CANCELLED at the next leg boundary and discards its result.
     *
     * The native `navi_comaps_cancel` is only a pre-flight latch: the next route call
     * consumes it and a running `ProcessTask` is not interrupted. Using it from here
     * made a replan that was started right after `cancel()` fail with CANCELLED, so the
     * planner keeps cancellation on the Kotlin side and never arms the native latch.
     */
    private val cancelGeneration =
        java.util.concurrent.atomic
            .AtomicLong(0)

    /** Cancel the plan in flight (if any). Never affects a plan that starts afterwards. */
    fun cancel() {
        cancelGeneration.incrementAndGet()
    }

    /**
     * Plan start→vias→end on CoMaps. Publishes climb for HUD when OK.
     * Report always contains `PLANNER=mwm` (and `comaps_route_code=` on failure).
     */
    fun plan(
        context: Context,
        profile: TravelProfile,
        startLat: Double,
        startLon: Double,
        endLat: Double,
        endLon: Double,
        vias: List<Pair<Double, Double>>,
        avoidMotorways: Boolean,
        avoidTolls: Boolean,
        avoidFerries: Boolean,
        turnsLocale: String,
        bikeWetland: Boolean = false,
    ): CorridorRouteResult {
        if (!shouldUse(context)) {
            return failCorridor(
                "TEST_KIND=PLAN_MWM_ROUTE\nFAIL: MWM session not open (maps or native missing)\n" +
                    "PLANNER=mwm\ncomaps_route_code=NeedMoreMaps\n",
            )
        }
        val generation = cancelGeneration.get()

        fun cancelled() = cancelGeneration.get() != generation
        lastRouteStyle =
            when (profile) {
                TravelProfile.HIKING -> 1
                TravelProfile.BICYCLE, TravelProfile.BICYCLE_ELECTRIC -> 2
                else -> 0
            }
        // Missing maps: name the leaves that cover the start / vias / end before the router
        // reports a misleading "not on a routable road".
        val endpoints = ArrayList<Pair<Double, Double>>(2 + vias.size)
        endpoints.add(startLat to startLon)
        vias.forEach { endpoints.add(it) }
        endpoints.add(endLat to endLon)
        missingLeavesFor(context, endpoints)?.let { return it }
        setMwmProductionPlannerActive(true)
        Log.i(TAG, "planner=mwm profile=$profile vias=${vias.size}")
        return try {
            comapsSessionSetTurnsLocale(turnsLocale.ifBlank { "nb" })
            comapsSessionSetBikeWetland(bikeWetland)
            comapsSessionSetAvoidOptions(
                avoidToll = avoidTolls,
                avoidMotorway = avoidMotorways,
                avoidFerry = avoidFerries,
                avoidDirty = false,
            )
            comapsSessionSetOptimize(
                if (avoidMotorways) {
                    FfiComapsOptimize.PREFER_NON_MOTORWAY
                } else {
                    FfiComapsOptimize.TIME
                },
            )
            val comapsProfile = SnapChoiceFlow.comapsProfileFor(profile)
            val legs = ArrayList<Pair<Double, Double>>(2 + vias.size)
            legs.add(startLat to startLon)
            vias.forEach { legs.add(it) }
            legs.add(endLat to endLon)

            val allPoints = ArrayList<FfiComapsLatLon>()
            var lengthM = 0.0
            var timeS = 0.0
            var buildS = 0.0
            var ascent = 0.0
            var descent = 0.0
            var turns = JSONArray()
            var instructions = JSONArray()
            var elev = JSONArray()
            val speedLimits = JSONArray()
            var mode = "Unknown"
            var last: FfiComapsRoute? = null

            for (i in 0 until legs.size - 1) {
                if (cancelled()) return cancelledCorridor()
                val (la0, lo0) = legs[i]
                val (la1, lo1) = legs[i + 1]
                val leg =
                    comapsSessionRoute(
                        profile = comapsProfile,
                        fromLat = la0,
                        fromLon = lo0,
                        toLat = la1,
                        toLon = lo1,
                        wetlandHook = profile == TravelProfile.HIKING || bikeWetland,
                    )
                last = leg
                if (cancelled()) return cancelledCorridor()
                if (leg.code != FfiComapsRouteCode.OK) {
                    RouteClimbSource.publishIfOk(leg)
                    if (leg.code == FfiComapsRouteCode.NEED_MORE_MAPS) {
                        // The route crosses a leaf that is not installed: name it.
                        missingLeavesAlong(context, la0, lo0, la1, lo1)?.let { return it }
                    }
                    return toCorridor(leg, endLat, endLon, profile)
                }
                RouteClimbSource.publishIfOk(leg)
                lengthM += leg.lengthM
                timeS += leg.timeS
                buildS += leg.buildTimeS
                ascent += leg.ascentM
                descent += leg.descentM
                mode = leg.routerMode
                val pts = leg.points
                val startIdx = if (allPoints.isEmpty()) 0 else 1
                for (j in startIdx until pts.size) {
                    allPoints.add(pts[j])
                }
                appendJsonArray(turns, leg.turnsJson)
                appendJsonArray(instructions, leg.instructionsJson)
                appendJsonArray(elev, leg.elevationJson)
                appendJsonArray(speedLimits, leg.speedLimitsJson)
            }

            val ok =
                FfiComapsRoute(
                    code = FfiComapsRouteCode.OK,
                    lengthM = lengthM,
                    timeS = timeS,
                    buildTimeS = buildS,
                    points = allPoints,
                    errorMessage = null,
                    turnsJson = turns.toString(),
                    instructionsJson = instructions.toString(),
                    ascentM = ascent,
                    descentM = descent,
                    elevationJson = elev.toString(),
                    routerMode = mode,
                    speedLimitsJson = speedLimits.toString(),
                )
            // Climb already published per leg; re-publish aggregate for HUD.
            RouteClimbSource.publishComaps(
                RouteClimbSource.fromComaps(ascent, descent, elev.toString()),
            )
            toCorridor(ok, endLat, endLon, profile)
        } finally {
            setMwmProductionPlannerActive(false)
        }
    }

    fun toCorridor(
        route: FfiComapsRoute,
        endLat: Double,
        endLon: Double,
        profile: TravelProfile,
    ): CorridorRouteResult {
        val codeName = route.code.name
        val polyline = pointsToPolyline(route.points)
        val (maneuvers, samples) = buildManeuversAndSamples(route)
        turnKey = polyline.hashCode()
        if (route.code != FfiComapsRouteCode.OK) {
            val msg =
                route.errorMessage?.ifBlank { null }
                    ?: "CoMaps route failed: $codeName"
            return failCorridor(
                "TEST_KIND=PLAN_MWM_ROUTE\nFAIL: $msg\nPLANNER=mwm\n" +
                    "comaps_route_code=$codeName\n",
            )
        }
        val km = route.lengthM / 1000.0
        val etaMin =
            if (route.timeS > 0.0) {
                route.timeS / 60.0
            } else {
                when (profile) {
                    TravelProfile.HIKING -> km * 16.0
                    TravelProfile.BICYCLE, TravelProfile.BICYCLE_ELECTRIC -> km * 4.0
                    else -> km * 1.2
                }
            }
        val report =
            buildString {
                append("TEST_KIND=PLAN_MWM_ROUTE\n")
                append("PLANNER=mwm\n")
                append("PASS\n")
                append("distance_km=${"%.3f".format(Locale.US, km)}\n")
                append("eta_minutes=${"%.1f".format(Locale.US, etaMin)}\n")
                append("router_mode=${route.routerMode}\n")
                append("ascent_m=${route.ascentM}\n")
                append("descent_m=${route.descentM}\n")
                append("build_s=${route.buildTimeS}\n")
                append("comaps_route_code=$codeName\n")
            }
        return CorridorRouteResult(
            report = report,
            distanceKm = km,
            etaMinutes = etaMin,
            cacheHit = false,
            coldBuildS = route.buildTimeS,
            warmLoadS = 0.0,
            routePolyline = polyline,
            poiLat = endLat,
            poiLon = endLon,
            poiName = "",
            poiIconKey = "",
            breakPoisJson = "[]",
            daysJson = "[]",
            simSamplesJson = samples,
            maneuversJson = maneuvers,
            priorityPathSharePct = 0.0,
            routeSegmentsJson = "[]",
            offTrailAdvisory = "",
            tollPolicy = "allow",
            padAttemptsJson = "[]",
            searchExpansions = 0u,
            searchTerminateReason = "ok",
            tollAvoidanceIncomplete = false,
            routeUsesTolls = false,
        )
    }

    private fun cancelledCorridor(): CorridorRouteResult =
        failCorridor(
            "TEST_KIND=PLAN_MWM_ROUTE\nFAIL: Cancelled\nPLANNER=mwm\ncomaps_route_code=CANCELLED\n",
        )

    private fun failCorridor(report: String): CorridorRouteResult =
        CorridorRouteResult(
            report = report,
            distanceKm = 0.0,
            etaMinutes = 0.0,
            cacheHit = false,
            coldBuildS = 0.0,
            warmLoadS = 0.0,
            routePolyline = "",
            poiLat = 0.0,
            poiLon = 0.0,
            poiName = "",
            poiIconKey = "",
            breakPoisJson = "[]",
            daysJson = "[]",
            simSamplesJson = "[]",
            maneuversJson = "[]",
            priorityPathSharePct = 0.0,
            routeSegmentsJson = "[]",
            offTrailAdvisory = "",
            tollPolicy = "allow",
            padAttemptsJson = "[]",
            searchExpansions = 0u,
            searchTerminateReason = "fail",
            tollAvoidanceIncomplete = false,
            routeUsesTolls = false,
        )

    private fun pointsToPolyline(points: List<FfiComapsLatLon>): String {
        if (points.isEmpty()) return ""
        val sb = StringBuilder(points.size * 24)
        points.forEachIndexed { i, p ->
            if (i > 0) sb.append(';')
            sb.append(p.lon).append(',').append(p.lat)
        }
        return sb.toString()
    }

    private fun appendJsonArray(
        dest: JSONArray,
        json: String,
    ) {
        val arr = runCatching { JSONArray(json.ifBlank { "[]" }) }.getOrNull() ?: return
        for (i in 0 until arr.length()) dest.put(arr.get(i))
    }

    private fun buildManeuversAndSamples(route: FfiComapsRoute): Pair<String, String> {
        val pts = route.points
        val cum = DoubleArray(pts.size)
        for (i in 1 until pts.size) {
            cum[i] = cum[i - 1] + haversineM(pts[i - 1].lat, pts[i - 1].lon, pts[i].lat, pts[i].lon)
        }
        val samples = JSONArray()
        // The CoMaps shim exposes no per-segment speed or posted limit, so playback and HUD
        // pace use the route average (CoMaps ETA over length). RouteSimSample requires a
        // numeric speed; a null here made parseRouteSimSamples drop the whole route.
        val avgKmh =
            if (route.timeS > 0.0 && route.lengthM > 0.0) {
                (route.lengthM / route.timeS * 3.6).coerceIn(MIN_SIM_KMH, MAX_SIM_KMH)
            } else {
                DEFAULT_SIM_KMH
            }
        // Posted maxspeed per segment from the MWM maxspeeds section (car router only; -1 = none).
        val limits =
            runCatching { JSONArray(route.speedLimitsJson.ifBlank { "[]" }) }.getOrElse { JSONArray() }

        // A sample covers `step` segments; use the lowest posted limit among them so a short
        // limit change is not skipped by the subsampling.
        fun limitAt(
            i: Int,
            step: Int,
        ): Int {
            var best = -1
            val to = minOf(i + step, limits.length())
            for (k in i until to) {
                val v = limits.optInt(k, -1)
                if (v > 0 && (best < 0 || v < best)) best = v
            }
            if (best < 0 && i < limits.length() && limits.optInt(i, -1) == -2) return -2
            return best
        }

        fun sampleJson(
            i: Int,
            step: Int,
        ): JSONObject {
            val lim = limitAt(i.coerceAtMost((limits.length() - 1).coerceAtLeast(0)), step)
            val o =
                JSONObject()
                    .put("lat", pts[i].lat)
                    .put("lon", pts[i].lon)
                    .put("cum_m", cum[i])
                    .put("speed_kmh", avgKmh)
                    .put("highway", JSONObject.NULL)
                    .put("maxspeed_posted", lim > 0)
            if (lim > 0) o.put("maxspeed_kmh", lim.toDouble())
            return o
        }

        // Subsample long polylines for the HUD / sim path.
        val step = (pts.size / 800).coerceAtLeast(1)
        var i = 0
        while (i < pts.size) {
            samples.put(sampleJson(i, step))
            i += step
        }
        if (pts.isNotEmpty() && (pts.size - 1) % step != 0) {
            samples.put(sampleJson(pts.size - 1, 1))
        }

        val instructions =
            runCatching { JSONArray(route.instructionsJson.ifBlank { "[]" }) }
                .getOrElse { JSONArray() }
        val maneuvers = JSONArray()
        val turnPoints = ArrayList<Int>()
        var along = 0.0
        for (j in 0 until instructions.length()) {
            val step = instructions.optJSONObject(j) ?: continue
            val dist = step.optDouble("dist_m", 0.0)
            along += dist
            val idx = indexAtCum(cum, along)
            if (step.optString("kind") != "destination") turnPoints.add(idx)
            val p = pts.getOrNull(idx) ?: pts.lastOrNull()
            maneuvers.put(
                JSONObject()
                    .put("lat", p?.lat ?: 0.0)
                    .put("lon", p?.lon ?: 0.0)
                    .put("cum_m", along)
                    .put("kind", step.optString("kind", "unknown"))
                    .put("street", step.optString("street", ""))
                    .put(
                        "roundabout_exit",
                        if (step.isNull("roundabout_exit")) {
                            JSONObject.NULL
                        } else {
                            step.opt("roundabout_exit")
                        },
                    ).put("text_nb", step.optString("text_nb", ""))
                    .put("text_en", step.optString("text_en", ""))
                    .put("voice_nb", step.optString("voice_nb", step.optString("voice", "")))
                    .put("voice_en", step.optString("voice_en", step.optString("voice", ""))),
            )
        }
        turnIdx = turnPoints.distinct().filter { it in 1 until pts.size }.toIntArray()
        return maneuvers.toString() to samples.toString()
    }

    /**
     * Missing-maps check for the route endpoints: a point whose CoMaps country (leaf) is not
     * installed gets a NEED_MORE_MAPS result that names the leaves, instead of the router's
     * "not on a routable road". Null when every endpoint is covered (or the leaf is unknown).
     */
    private fun missingLeavesFor(
        context: Context,
        points: List<Pair<Double, Double>>,
    ): CorridorRouteResult? {
        val leaves = LinkedHashSet<String>()
        for ((la, lo) in points) {
            val leaf = runCatching { uniffi.navi.comapsLeafForPoint(la, lo) }.getOrNull()
            if (!leaf.isNullOrBlank()) leaves.add(leaf)
        }
        return missingResult(context, leaves)
    }

    /** Leaves crossed by the straight line between two route points, for a NeedMoreMaps result. */
    private fun missingLeavesAlong(
        context: Context,
        la0: Double,
        lo0: Double,
        la1: Double,
        lo1: Double,
    ): CorridorRouteResult? {
        val dKm = haversineM(la0, lo0, la1, lo1) / 1000.0
        val n = (dKm / 8.0).toInt().coerceIn(2, 240)
        val leaves = LinkedHashSet<String>()
        for (k in 0..n) {
            val f = k.toDouble() / n
            val leaf =
                runCatching { uniffi.navi.comapsLeafForPoint(la0 + (la1 - la0) * f, lo0 + (lo1 - lo0) * f) }.getOrNull()
            if (!leaf.isNullOrBlank()) leaves.add(leaf)
        }
        return missingResult(context, leaves)
    }

    private fun missingResult(
        context: Context,
        leaves: Collection<String>,
    ): CorridorRouteResult? {
        if (leaves.isEmpty()) return null
        val missing = MwmMapDownloader.missing(context, leaves)
        if (missing.isEmpty()) return null
        Log.i(TAG, "missing maps: ${missing.joinToString(",")}")
        return failCorridor(
            "TEST_KIND=PLAN_MWM_ROUTE\nFAIL: Missing maps: ${missing.joinToString(", ") { it.replace('_', ' ') }}\n" +
                "PLANNER=mwm\ncomaps_route_code=NEED_MORE_MAPS\nmissing_leaves=${missing.joinToString(",")}\n",
        )
    }

    private fun indexAtCum(
        cum: DoubleArray,
        target: Double,
    ): Int {
        if (cum.isEmpty()) return 0
        var lo = 0
        var hi = cum.size - 1
        while (lo < hi) {
            val mid = (lo + hi) / 2
            if (cum[mid] < target) lo = mid + 1 else hi = mid
        }
        return lo
    }

    private fun haversineM(
        lat1: Double,
        lon1: Double,
        lat2: Double,
        lon2: Double,
    ): Double {
        val r = 6_371_000.0
        val p1 = Math.toRadians(lat1)
        val p2 = Math.toRadians(lat2)
        val dphi = Math.toRadians(lat2 - lat1)
        val dl = Math.toRadians(lon2 - lon1)
        val a =
            sin(dphi / 2) * sin(dphi / 2) +
                cos(p1) * cos(p2) * sin(dl / 2) * sin(dl / 2)
        return 2 * r * atan2(sqrt(a), sqrt(1 - a))
    }
}
