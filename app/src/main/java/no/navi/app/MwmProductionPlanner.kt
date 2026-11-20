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
import uniffi.navi.FfiSearchHit
import uniffi.navi.FfiVehicleLimits
import uniffi.navi.TravelProfile
import uniffi.navi.comapsCategoryQuery
import uniffi.navi.comapsNativeAvailable
import uniffi.navi.comapsSessionIsOpen
import uniffi.navi.comapsSessionRoute
import uniffi.navi.comapsSessionSetAvoidOptions
import uniffi.navi.comapsSessionSetBikeWetland
import uniffi.navi.comapsSessionSetOptimize
import uniffi.navi.comapsSessionSetTurnsLocale
import uniffi.navi.comapsSessionSetVehicle
import uniffi.navi.datexRefreshJson
import uniffi.navi.loadCarRestSettings
import uniffi.navi.loadVehicleLimits
import uniffi.navi.poiLookaheadIngestFromJson
import uniffi.navi.setMwmProductionPlannerActive
import java.util.Locale
import kotlin.math.absoluteValue
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
     * Profiles that never pass vehicle dimensions to the car estimator
     * (motorcycle). Truck / mobile home / car with height now apply sidecar
     * `vehicle_limits` when present.
     */
    fun limitsNotApplied(profile: TravelProfile): Boolean =
        when (profile) {
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
        /**
         * When true, CoMaps has no native tunnel RoutingOption — we soft-avoid by
         * replanning with perpendicular detour vias around tunnel midpoints
         * (same pattern as DATEX Block avoidance).
         */
        avoidTunnels: Boolean = false,
        /**
         * When true (default), a successful CoMaps geometry is re-planned with detour
         * vias if on-corridor DATEX **Block** situations are present. Nested replan
         * passes false so avoidance does not recurse.
         */
        applyDatexAvoidance: Boolean = true,
        /** Nested tunnel replans pass false so avoidance does not recurse. */
        applyTunnelAvoidance: Boolean = true,
        /**
         * Eco routing on the MWM/CoMaps path. CoMaps has no joule weights; eco maps
         * to [FfiComapsOptimize.PREFER_NON_MOTORWAY] (same lever as avoid-motorways)
         * so the router prefers the easier / less motorway corridor.
         */
        ecoEnabled: Boolean = false,
    ): CorridorRouteResult {
        if (!shouldUse(context)) {
            return failCorridor(
                "TEST_KIND=PLAN_MWM_ROUTE\nFAIL: MWM session not open (maps or native missing)\n" +
                    "PLANNER=mwm\ncomaps_route_code=NeedMoreMaps\n",
            )
        }
        // CoMaps IndexRouter aborts (SIGABRT) when numMwmIds is empty — only World
        // overlays registered. Refuse before the native route call when country
        // leaves are not exposed under {mwmRoot}/{version}/.
        if (!ComapsSessionBootstrap.countryMapsExposed(context)) {
            Log.w(TAG, "refuse plan: country leaf MWMs not exposed (VFAT link/mirror pending?)")
            return failCorridor(
                "TEST_KIND=PLAN_MWM_ROUTE\nFAIL: Country maps not registered yet\n" +
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
        // CoMaps eco lever: prefer non-motorway (easier / more economic corridor).
        val preferNonMotorway = ecoEnabled || avoidMotorways
        Log.i(
            TAG,
            "planner=mwm profile=$profile vias=${vias.size} eco=$ecoEnabled " +
                "optimize=${if (preferNonMotorway) "prefer_non_motorway" else "time"} " +
                "datex_avoid=$applyDatexAvoidance tunnel_avoid=$avoidTunnels/$applyTunnelAvoidance",
        )
        return try {
            comapsSessionSetTurnsLocale(turnsLocale.ifBlank { "nb" })
            comapsSessionSetBikeWetland(bikeWetland)
            // Vehicle dimensions for car estimator (motorcycle clears).
            val dataDir = NaviAppData.resolve(context)
            val stored =
                runCatching { loadVehicleLimits(dataDir.absolutePath) }.getOrNull()
            if (VehicleRestrictionUi.appliesStoredLimits(profile) && stored != null) {
                comapsSessionSetVehicle(stored)
            } else {
                comapsSessionSetVehicle(
                    FfiVehicleLimits(null, null, null, null, null, null),
                )
            }
            comapsSessionSetAvoidOptions(
                avoidToll = avoidTolls,
                // Eco also asks CoMaps to treat motorways as avoided (same as
                // PREFER_NON_MOTORWAY in ApplyAvoidSettings).
                avoidMotorway = preferNonMotorway,
                avoidFerry = avoidFerries,
                avoidDirty = false,
            )
            comapsSessionSetOptimize(
                if (preferNonMotorway) {
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
            val routeLeaves = LinkedHashSet<String>()

            for (i in 0 until legs.size - 1) {
                if (cancelled()) return cancelledCorridor()
                val (la0, lo0) = legs[i]
                val (la1, lo1) = legs[i + 1]
                collectLeavesAlong(la0, lo0, la1, lo1, routeLeaves)
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
                    return toCorridor(
                        leg,
                        endLat,
                        endLon,
                        profile,
                        vehicleLimitsCoverage(context, routeLeaves),
                    )
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

            lastTurnsJson = turns.toString()
            val ok =
                FfiComapsRoute(
                    code = FfiComapsRouteCode.OK,
                    lengthM = lengthM,
                    timeS = timeS,
                    buildTimeS = buildS,
                    points = allPoints,
                    errorMessage = null,
                    turnsJson = lastTurnsJson,
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
            var result =
                toCorridor(ok, endLat, endLon, profile, vehicleLimitsCoverage(context, routeLeaves))
            if (cancelled()) {
                return enrichMwmPlan(context, profile, result)
            }
            // Nested DATEX / tunnel replans: enrich once, no further avoidance.
            if (!applyDatexAvoidance) {
                return enrichMwmPlan(context, profile, result)
            }
            result =
                avoidDatexBlocks(
                    context = context,
                    profile = profile,
                    startLat = startLat,
                    startLon = startLon,
                    endLat = endLat,
                    endLon = endLon,
                    vias = vias,
                    avoidMotorways = avoidMotorways,
                    avoidTolls = avoidTolls,
                    avoidFerries = avoidFerries,
                    avoidTunnels = avoidTunnels,
                    turnsLocale = turnsLocale,
                    bikeWetland = bikeWetland,
                    ecoEnabled = ecoEnabled,
                    base = result,
                )
            if (avoidTunnels && applyTunnelAvoidance && result.report.contains("PASS")) {
                result =
                    avoidTunnelsViaDetours(
                        context = context,
                        profile = profile,
                        startLat = startLat,
                        startLon = startLon,
                        endLat = endLat,
                        endLon = endLon,
                        vias = vias,
                        avoidMotorways = avoidMotorways,
                        avoidTolls = avoidTolls,
                        avoidFerries = avoidFerries,
                        turnsLocale = turnsLocale,
                        bikeWetland = bikeWetland,
                        ecoEnabled = ecoEnabled,
                        base = result,
                    )
            }
            // Nested replan already ran enrichMwmPlan; do not duplicate soft-break lines.
            if (result.report.contains("datex_mwm=reroute_apply") ||
                result.report.contains("tunnel_mwm=reroute_apply")
            ) {
                return result
            }
            enrichMwmPlan(context, profile, result)
        } finally {
            setMwmProductionPlannerActive(false)
        }
    }

    /**
     * A route whose geometry is an imported track: the HUD, ETA, progress and simulation work on the
     * file's own line. No router is involved, so there are no turn instructions: the only maneuver is
     * the end of the track. Speed is the walking / cycling / driving average of [profile].
     */
    fun trackRoute(
        profile: TravelProfile,
        latlon: DoubleArray,
        lengthM: Double,
        ascentM: Double,
        endLabel: String,
    ): CorridorRouteResult {
        val n = latlon.size / 2
        if (n < 2) return failCorridor("TEST_KIND=PLAN_MWM_ROUTE\nFAIL: track has fewer than two points\nPLANNER=track\n")
        lastRouteStyle =
            when (profile) {
                TravelProfile.HIKING -> 1
                TravelProfile.BICYCLE, TravelProfile.BICYCLE_ELECTRIC -> 2
                else -> 0
            }
        val kmh =
            when (profile) {
                TravelProfile.HIKING -> 4.5
                TravelProfile.BICYCLE, TravelProfile.BICYCLE_ELECTRIC -> 16.0
                else -> 50.0
            }
        val dest =
            JSONObject()
                .put("kind", "destination")
                .put("dist_m", lengthM)
                .put("street", endLabel)
                .put("text_nb", "Du er ved slutten av sporet")
                .put("text_en", "You are at the end of the track")
                .put("voice_nb", "Du er ved slutten av sporet")
                .put("voice_en", "You are at the end of the track")
        val route =
            FfiComapsRoute(
                code = FfiComapsRouteCode.OK,
                lengthM = lengthM,
                timeS = lengthM / (kmh / 3.6),
                buildTimeS = 0.0,
                points = (0 until n).map { FfiComapsLatLon(latlon[2 * it], latlon[2 * it + 1]) },
                errorMessage = null,
                turnsJson = "[]",
                instructionsJson = JSONArray().put(dest).toString(),
                ascentM = ascentM,
                descentM = 0.0,
                elevationJson = "[]",
                routerMode = "track",
                speedLimitsJson = "[]",
            )
        return toCorridor(route, latlon[2 * (n - 1)], latlon[2 * (n - 1) + 1], profile, null)
    }

    fun toCorridor(
        route: FfiComapsRoute,
        endLat: Double,
        endLon: Double,
        profile: TravelProfile,
        vehicleLimitsCoverage: Pair<List<String>, List<String>>? = null,
    ): CorridorRouteResult {
        val codeName = route.code.name
        val polyline = pointsToPolyline(route.points)
        val (maneuvers, samples) = buildManeuversAndSamples(route)
        turnKey = polyline.hashCode()
        val missing = vehicleLimitsCoverage?.first
        val partial = vehicleLimitsCoverage?.second.orEmpty()
        if (route.code != FfiComapsRouteCode.OK) {
            val msg =
                route.errorMessage?.ifBlank { null }
                    ?: "CoMaps route failed: $codeName"
            return failCorridor(
                "TEST_KIND=PLAN_MWM_ROUTE\nFAIL: $msg\nPLANNER=mwm\n" +
                    "comaps_route_code=$codeName\n" +
                    vehicleLimitsReportLines(missing, partial),
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
        // Tunnel / ferry runs are appended in enrichMwmPlan via annotatePathFlagsFromTurns
        // (needs lastTurnsJson from the multi-leg aggregate).
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
                append(vehicleLimitsReportLines(missing, partial))
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

    /**
     * CoMaps has no DATEX edge weights. When a corridor **Block** sits on the
     * first-pass geometry, replan with perpendicular detour vias so the closed
     * segment changes the route (not only overlay counters).
     */
    private fun avoidDatexBlocks(
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
        avoidTunnels: Boolean,
        turnsLocale: String,
        bikeWetland: Boolean,
        ecoEnabled: Boolean,
        base: CorridorRouteResult,
    ): CorridorRouteResult {
        if (!base.report.contains("PASS")) return base
        if (!MapHudPrefs.loadDatexPluginEnabled(context)) return base
        val pts = decodePolylinePairs(base.routePolyline)
        if (pts.size < 2) return base
        val blocks = loadOnRouteDatexBlocks(context, pts)
        if (blocks.isEmpty()) {
            Log.i(TAG, "mwm datex avoid: no Block on first-pass corridor")
            return base
        }
        val beforeKm = base.distanceKm
        val beforeEta = base.etaMinutes
        val beforeHash = polylineFingerprint(base.routePolyline)
        Log.i(TAG, "mwm datex avoid: blocks=${blocks.size} before_km=$beforeKm hash=$beforeHash")
        // Cap attempts: each CoMaps replan is expensive on a long corridor.
        // Mid-sim / live nav: keep this to one offset×side so we never stack up
        // to six ProcessTask builds (native SIGABRT / std::length_error under load).
        val simulating = NaviMapTestHooks.simulatingActive
        val offsetsM =
            if (simulating) {
                listOf(2_500.0)
            } else {
                listOf(2_500.0, 5_000.0, 8_000.0)
            }
        val sides = if (simulating) listOf(1.0) else listOf(1.0, -1.0)
        val maxAttempts = if (simulating) 2 else 6
        var bestChanged: CorridorRouteResult? = null
        var bestChangedClearance = -1.0
        var bestMeta = ""
        var attempts = 0
        for (offsetM in offsetsM) {
            for (side in sides) {
                if (attempts >= maxAttempts) break
                attempts++
                val detours =
                    blocks.take(2).mapNotNull { b ->
                        detourViaForBlock(pts, b.lat, b.lon, offsetM, side)
                    }
                if (detours.isEmpty()) continue
                val merged = mergeViasByRouteOrder(startLat, startLon, endLat, endLon, vias, detours)
                val alt =
                    plan(
                        context = context,
                        profile = profile,
                        startLat = startLat,
                        startLon = startLon,
                        endLat = endLat,
                        endLon = endLon,
                        vias = merged,
                        avoidMotorways = avoidMotorways,
                        avoidTolls = avoidTolls,
                        avoidFerries = avoidFerries,
                        turnsLocale = turnsLocale,
                        bikeWetland = bikeWetland,
                        avoidTunnels = avoidTunnels,
                        applyDatexAvoidance = false,
                        applyTunnelAvoidance = false,
                        ecoEnabled = ecoEnabled,
                    )
                if (!alt.report.contains("PASS")) continue
                val clearance = minClearanceM(alt.routePolyline, blocks)
                val deltaKm = (alt.distanceKm - beforeKm).absoluteValue
                val afterHash = polylineFingerprint(alt.routePolyline)
                val changed = deltaKm >= 0.25 || afterHash != beforeHash
                val meta =
                    "clearance_m=${"%.0f".format(Locale.US, clearance)}" +
                        "; before_km=${"%.3f".format(Locale.US, beforeKm)}" +
                        "; after_km=${"%.3f".format(Locale.US, alt.distanceKm)}" +
                        "; before_eta_min=${"%.1f".format(Locale.US, beforeEta)}" +
                        "; after_eta_min=${"%.1f".format(Locale.US, alt.etaMinutes)}" +
                        "; before_poly=$beforeHash" +
                        "; after_poly=$afterHash" +
                        "; offset_m=${offsetM.toInt()}; side=${side.toInt()}"
                // Prefer a real miss of the closed point; keep best geometry change as fallback.
                if (clearance >= 150.0 && changed) {
                    val evidence =
                        buildString {
                            append("datex_mwm=reroute_apply; blocks_avoided=${blocks.size}; $meta\n")
                            for (b in blocks) {
                                append(
                                    "datex_avoided: id=${b.id}; impact=block; " +
                                        "lat=${b.lat}; lon=${b.lon}; xsi=${b.xsi}\n",
                                )
                            }
                        }
                    alt.report = alt.report.trimEnd() + "\n" + evidence
                    Log.i(TAG, "mwm datex avoid OK $meta")
                    markDatexRerouteHooks(beforeKm, alt.distanceKm, beforeHash, afterHash, blocks)
                    return alt
                }
                if (changed && clearance > bestChangedClearance) {
                    bestChangedClearance = clearance
                    bestChanged = alt
                    bestMeta = meta
                }
            }
        }
        val fallback = bestChanged
        if (fallback != null && bestChangedClearance >= 40.0) {
            val afterHash = polylineFingerprint(fallback.routePolyline)
            val evidence =
                buildString {
                    append(
                        "datex_mwm=reroute_apply; blocks_avoided=${blocks.size}; " +
                            "weak_clearance=true; $bestMeta\n",
                    )
                    for (b in blocks) {
                        append(
                            "datex_avoided: id=${b.id}; impact=block; " +
                                "lat=${b.lat}; lon=${b.lon}; xsi=${b.xsi}\n",
                        )
                    }
                }
            fallback.report = fallback.report.trimEnd() + "\n" + evidence
            Log.i(TAG, "mwm datex avoid WEAK $bestMeta")
            markDatexRerouteHooks(
                beforeKm,
                fallback.distanceKm,
                beforeHash,
                afterHash,
                blocks,
            )
            return fallback
        }
        val fail =
            buildString {
                append("datex_mwm=reroute_failed; blocks_seen=${blocks.size}")
                append("; before_km=${"%.3f".format(Locale.US, beforeKm)}")
                append("; before_poly=$beforeHash\n")
                for (b in blocks) {
                    append(
                        "datex_avoided: id=${b.id}; impact=block; " +
                            "lat=${b.lat}; lon=${b.lon}; xsi=${b.xsi}; status=unavoided\n",
                    )
                }
            }
        // Still mark Block ids so HUD auto-reroute does not loop on the same closure.
        markDatexRerouteHooks(beforeKm, beforeKm, beforeHash, beforeHash, blocks, applied = false)
        base.report = base.report.trimEnd() + "\n" + fail
        Log.w(TAG, "mwm datex avoid FAILED blocks=${blocks.size}")
        return base
    }

    /**
     * CoMaps [RoutingOptions] has no Tunnel flag. Soft-avoid by inserting
     * perpendicular detour vias at tunnel midpoints (from turns_json) and
     * accepting the first geometry that cuts the tunnel run count.
     */
    private fun avoidTunnelsViaDetours(
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
        bikeWetland: Boolean,
        ecoEnabled: Boolean,
        base: CorridorRouteResult,
    ): CorridorRouteResult {
        if (!base.report.contains("PASS")) return base
        val beforeCount = tunnelRunCount(lastTurnsJson)
        if (beforeCount <= 0) {
            Log.i(TAG, "mwm tunnel avoid: no tunnels on first-pass corridor")
            return base
        }
        val mids = tunnelMidpoints(lastTurnsJson, base.routePolyline)
        if (mids.isEmpty()) {
            base.report =
                base.report.trimEnd() +
                "\ntunnel_mwm=reroute_incomplete; tunnels_before=$beforeCount; reason=no_midpoints\n"
            return base
        }
        val pts = decodePolylinePairs(base.routePolyline)
        if (pts.size < 2) return base
        val beforeKm = base.distanceKm
        val beforeHash = polylineFingerprint(base.routePolyline)
        Log.i(TAG, "mwm tunnel avoid: tunnels=$beforeCount mids=${mids.size} before_km=$beforeKm")
        val simulating = NaviMapTestHooks.simulatingActive
        val offsetsM = if (simulating) listOf(1_500.0) else listOf(1_500.0, 3_000.0, 5_000.0)
        val sides = if (simulating) listOf(1.0) else listOf(1.0, -1.0)
        val maxAttempts = if (simulating) 2 else 6
        var attempts = 0
        var best: CorridorRouteResult? = null
        var bestCount = beforeCount
        for (offsetM in offsetsM) {
            for (side in sides) {
                if (attempts >= maxAttempts) break
                attempts++
                val detours =
                    mids.take(3).mapNotNull { (lat, lon) ->
                        detourViaForBlock(pts, lat, lon, offsetM, side)
                    }
                if (detours.isEmpty()) continue
                val merged = mergeViasByRouteOrder(startLat, startLon, endLat, endLon, vias, detours)
                val alt =
                    plan(
                        context = context,
                        profile = profile,
                        startLat = startLat,
                        startLon = startLon,
                        endLat = endLat,
                        endLon = endLon,
                        vias = merged,
                        avoidMotorways = avoidMotorways,
                        avoidTolls = avoidTolls,
                        avoidFerries = avoidFerries,
                        turnsLocale = turnsLocale,
                        bikeWetland = bikeWetland,
                        avoidTunnels = true,
                        applyDatexAvoidance = false,
                        applyTunnelAvoidance = false,
                        ecoEnabled = ecoEnabled,
                    )
                if (!alt.report.contains("PASS")) continue
                val afterCount = tunnelRunCount(lastTurnsJson)
                val changed =
                    polylineFingerprint(alt.routePolyline) != beforeHash ||
                        (alt.distanceKm - beforeKm).absoluteValue >= 0.25
                if (afterCount < bestCount && changed) {
                    bestCount = afterCount
                    best = alt
                }
                if (afterCount == 0 && changed) {
                    best = alt
                    bestCount = 0
                    break
                }
            }
            if (bestCount == 0) break
        }
        val chosen = best
        if (chosen != null && bestCount < beforeCount) {
            val evidence =
                "tunnel_mwm=reroute_apply; tunnels_before=$beforeCount; " +
                    "tunnels_after=$bestCount; midpoints=${mids.size}; attempts=$attempts\n"
            chosen.report = chosen.report.trimEnd() + "\n" + evidence
            Log.i(TAG, "mwm tunnel avoid OK before=$beforeCount after=$bestCount")
            return chosen
        }
        base.report =
            base.report.trimEnd() +
            "\ntunnel_mwm=reroute_incomplete; tunnels_before=$beforeCount; " +
            "tunnels_after=$beforeCount; midpoints=${mids.size}; attempts=$attempts\n"
        Log.w(TAG, "mwm tunnel avoid incomplete tunnels=$beforeCount")
        return base
    }

    /** Contiguous `is_tunnel` runs in CoMaps turns_json. */
    internal fun tunnelRunCount(turnsJson: String): Int {
        val source = runCatching { JSONArray(turnsJson.ifBlank { "[]" }) }.getOrElse { JSONArray() }
        var count = 0
        var prev = false
        for (i in 0 until source.length()) {
            val isTunnel = source.optJSONObject(i)?.optBoolean("is_tunnel", false) == true
            if (isTunnel && !prev) count++
            prev = isTunnel
        }
        return count
    }

    /**
     * Midpoints of contiguous tunnel runs, projected onto [polyline] by cumulative
     * turn `dist_m` (turns lack lat/lon).
     */
    internal fun tunnelMidpoints(
        turnsJson: String,
        polyline: String,
    ): List<Pair<Double, Double>> {
        val samples = samplePolylineKm(polyline)
        if (samples.size < 2) return emptyList()
        val source = runCatching { JSONArray(turnsJson.ifBlank { "[]" }) }.getOrElse { JSONArray() }
        val mids = ArrayList<Pair<Double, Double>>()
        var alongKm = 0.0
        var runStart = -1.0

        fun flush() {
            if (runStart < 0.0) return
            val mid = (runStart + alongKm) * 0.5
            mids.add(interpolateAtKm(samples, mid))
            runStart = -1.0
        }
        for (i in 0 until source.length()) {
            val o = source.optJSONObject(i) ?: continue
            val isTunnel = o.optBoolean("is_tunnel", false)
            val distKm = (o.optDouble("dist_m", 0.0) / 1000.0).coerceAtLeast(0.0)
            if (isTunnel) {
                if (runStart < 0.0) runStart = alongKm
                alongKm += distKm
            } else {
                flush()
                alongKm += distKm
            }
        }
        flush()
        return mids
    }

    private fun markDatexRerouteHooks(
        beforeKm: Double,
        afterKm: Double,
        beforeHash: String,
        afterHash: String,
        blocks: List<DatexBlockHit>,
        applied: Boolean = true,
    ) {
        NaviMapTestHooks.datexMwmRerouteApplied = applied
        NaviMapTestHooks.datexMwmRerouteBeforeKm = beforeKm
        NaviMapTestHooks.datexMwmRerouteAfterKm = afterKm
        NaviMapTestHooks.datexMwmRerouteBeforePoly = beforeHash
        NaviMapTestHooks.datexMwmRerouteAfterPoly = afterHash
        val ids = blocks.map { it.id }.filter { it.isNotBlank() }
        if (ids.isNotEmpty()) {
            val prev =
                NaviMapTestHooks.datexAutoRerouteBlockIds
                    .split(',')
                    .filter { it.isNotBlank() }
                    .toMutableSet()
            prev.addAll(ids)
            NaviMapTestHooks.datexAutoRerouteBlockIds = prev.joinToString(",")
        }
    }

    private data class DatexBlockHit(
        val id: String,
        val lat: Double,
        val lon: Double,
        val xsi: String,
    )

    private fun loadOnRouteDatexBlocks(
        context: Context,
        samples: List<Pair<Double, Double>>,
    ): List<DatexBlockHit> {
        val dataDir = NaviAppData.resolve(context)
        val cacheDir = java.io.File(dataDir, "datex_cache").absolutePath
        val raw =
            runCatching {
                datexRefreshJson(
                    enabled = true,
                    host = MapHudPrefs.loadDatexHost(context),
                    port = MapHudPrefs.loadDatexPort(context).toUInt(),
                    routeLatLonJson = latLonPairsToJson(samples),
                    wifiOnly = false,
                    onWifi = true,
                    useDiscoveryChain = true,
                    cacheDir = cacheDir,
                )
            }.getOrElse { e ->
                Log.w(TAG, "mwm datex avoid refresh failed: ${e.message}")
                return emptyList()
            }
        val o = runCatching { JSONObject(raw) }.getOrNull() ?: return emptyList()
        val active = o.optJSONArray("active") ?: return emptyList()
        val out = ArrayList<DatexBlockHit>()
        for (i in 0 until active.length()) {
            val s = active.optJSONObject(i) ?: continue
            if (s.optString("impact").lowercase(Locale.US) != "block") continue
            val lat = s.optDouble("lat", Double.NaN)
            val lon = s.optDouble("lon", Double.NaN)
            if (!lat.isFinite() || !lon.isFinite()) continue
            val nearest = samples.minOf { (la, lo) -> haversineM(la, lo, lat, lon) }
            // Slightly wider than DATEX_IMPACT_RADIUS_M (250) so snapped sits count.
            if (nearest > 400.0) continue
            out.add(
                DatexBlockHit(
                    id = s.optString("id").ifBlank { "block_$i" },
                    lat = lat,
                    lon = lon,
                    xsi = s.optString("xsi_type"),
                ),
            )
        }
        return out
    }

    private fun detourViaForBlock(
        pts: List<Pair<Double, Double>>,
        blockLat: Double,
        blockLon: Double,
        offsetM: Double,
        side: Double,
    ): Pair<Double, Double>? {
        if (pts.size < 2) return null
        var bestI = 0
        var bestD = Double.MAX_VALUE
        for (i in pts.indices) {
            val d = haversineM(pts[i].first, pts[i].second, blockLat, blockLon)
            if (d < bestD) {
                bestD = d
                bestI = i
            }
        }
        val i0 = (bestI - 1).coerceAtLeast(0)
        val i1 = (bestI + 1).coerceAtMost(pts.lastIndex)
        if (i0 == i1) return null
        val (la0, lo0) = pts[i0]
        val (la1, lo1) = pts[i1]
        val dLat = la1 - la0
        val dLon = lo1 - lo0
        val len = sqrt(dLat * dLat + dLon * dLon)
        if (len < 1e-9) return null
        // Perpendicular in lat/lon degrees; scale lon by cos(lat).
        val latRad = Math.toRadians(blockLat)
        val metersPerDegLat = 111_320.0
        val metersPerDegLon = 111_320.0 * cos(latRad).coerceAtLeast(0.2)
        val ux = dLon / len
        val uy = dLat / len
        // Perpendicular unit in degree-space ≈ (-uy, ux)
        val px = -uy
        val py = ux
        val outLat = blockLat + side * (offsetM / metersPerDegLat) * py
        val outLon = blockLon + side * (offsetM / metersPerDegLon) * px
        return outLat to outLon
    }

    private fun mergeViasByRouteOrder(
        startLat: Double,
        startLon: Double,
        endLat: Double,
        endLon: Double,
        userVias: List<Pair<Double, Double>>,
        detours: List<Pair<Double, Double>>,
    ): List<Pair<Double, Double>> {
        fun key(
            lat: Double,
            lon: Double,
        ): Double {
            val dStart = haversineM(startLat, startLon, lat, lon)
            val dEnd = haversineM(lat, lon, endLat, endLon)
            return dStart / (dStart + dEnd + 1.0)
        }
        return (userVias + detours).sortedBy { key(it.first, it.second) }
    }

    private fun minClearanceM(
        polyline: String,
        blocks: List<DatexBlockHit>,
    ): Double {
        val pts = decodePolylinePairs(polyline)
        if (pts.isEmpty() || blocks.isEmpty()) return Double.POSITIVE_INFINITY
        var min = Double.POSITIVE_INFINITY
        for (b in blocks) {
            val d = pts.minOf { (la, lo) -> haversineM(la, lo, b.lat, b.lon) }
            if (d < min) min = d
        }
        return min
    }

    private fun decodePolylinePairs(polyline: String): List<Pair<Double, Double>> =
        polyline.split(';').mapNotNull { part ->
            val bits = part.split(',')
            if (bits.size < 2) return@mapNotNull null
            val lo = bits[0].toDoubleOrNull() ?: return@mapNotNull null
            val la = bits[1].toDoubleOrNull() ?: return@mapNotNull null
            la to lo
        }

    private fun polylineFingerprint(polyline: String): String = "%08x:len=%d".format(Locale.US, polyline.hashCode(), polyline.length)

    /**
     * MWM plans previously skipped DATEX / soft-break / nearby-attractions that the
     * `.rkyv` planner fills. Annotate the PASS report and break pins from the same
     * on-disk DATEX stamp + rest settings, and seed look-ahead from CoMaps tourism
     * when Navi POI packs are absent.
     */
    private fun enrichMwmPlan(
        context: Context,
        profile: TravelProfile,
        base: CorridorRouteResult,
    ): CorridorRouteResult {
        if (!base.report.contains("PASS")) return base
        val dataDir = NaviAppData.resolve(context)
        val samples = downsamplePolyline(base.routePolyline, maxPoints = 120)
        val datexExtra = annotateDatex(context, dataDir, samples)
        val (breaksJson, restExtra) =
            softBreakPoisJson(dataDir, profile, base.routePolyline, base.distanceKm, base.etaMinutes)
        seedAttractionsFromComaps(samples)
        seedWaterFromComaps(samples)
        // Tunnel / ferry runs come from CoMaps turns_json (is_tunnel / is_ferry), which
        // the .rkyv path emits via path_tunnel_legs / ferry edges. Keep the same report
        // tokens so RoutePlanStats and the campaign corridor_counts stay populated.
        val pathExtra = annotatePathFlagsFromTurns(base)
        val report =
            buildString {
                append(base.report.trimEnd())
                append('\n')
                if (pathExtra.isNotBlank()) append(pathExtra)
                if (datexExtra.isNotBlank()) append(datexExtra)
                if (restExtra.isNotBlank()) append(restExtra)
            }
        Log.i(
            TAG,
            "mwm_enrich datex_lines=${datexExtra.lineSequence().count()} " +
                "break_pois=${runCatching { JSONArray(breaksJson).length() }.getOrDefault(0)} " +
                "path=${pathExtra.lineSequence().firstOrNull().orEmpty()}",
        )
        return CorridorRouteResult(
            report = report,
            distanceKm = base.distanceKm,
            etaMinutes = base.etaMinutes,
            cacheHit = base.cacheHit,
            coldBuildS = base.coldBuildS,
            warmLoadS = base.warmLoadS,
            routePolyline = base.routePolyline,
            poiLat = base.poiLat,
            poiLon = base.poiLon,
            poiName = base.poiName,
            poiIconKey = base.poiIconKey,
            breakPoisJson = breaksJson,
            daysJson = base.daysJson,
            simSamplesJson = base.simSamplesJson,
            maneuversJson = base.maneuversJson,
            priorityPathSharePct = base.priorityPathSharePct,
            routeSegmentsJson = base.routeSegmentsJson,
            offTrailAdvisory = base.offTrailAdvisory,
            tollPolicy = base.tollPolicy,
            padAttemptsJson = base.padAttemptsJson,
            searchExpansions = base.searchExpansions,
            searchTerminateReason = base.searchTerminateReason,
            tollAvoidanceIncomplete = base.tollAvoidanceIncomplete,
            routeUsesTolls = base.routeUsesTolls,
        )
    }

    /**
     * Count contiguous tunnel / ferry runs from CoMaps `turns_json` flags. Prefer the
     * raw turns array (still available via [lastTurnsJson]); fall back to maneuvers that
     * carry `is_tunnel` / `is_ferry` when a nested replan only kept maneuvers.
     */
    @Volatile
    private var lastTurnsJson: String = "[]"

    private fun annotatePathFlagsFromTurns(base: CorridorRouteResult): String {
        val turns =
            runCatching { JSONArray(lastTurnsJson.ifBlank { "[]" }) }.getOrElse { JSONArray() }
        val source =
            if (turns.length() > 0) {
                turns
            } else {
                runCatching { JSONArray(base.maneuversJson.ifBlank { "[]" }) }.getOrElse { JSONArray() }
            }
        var tunnelCount = 0
        var ferryCount = 0
        val tunnelFp = ArrayList<String>()
        val ferryFp = ArrayList<String>()
        var prevTunnel = false
        var prevFerry = false
        var tunnelAlongKm = 0.0
        var ferryAlongKm = 0.0
        for (i in 0 until source.length()) {
            val o = source.optJSONObject(i) ?: continue
            val isTunnel = o.optBoolean("is_tunnel", false)
            val isFerry = o.optBoolean("is_ferry", false)
            val distKm = o.optDouble("dist_m", 0.0) / 1000.0
            val label =
                o.optString("street").ifBlank {
                    o.optString("ref").ifBlank { o.optString("kind").ifBlank { "unnamed" } }
                }
            if (isTunnel) {
                if (!prevTunnel) {
                    tunnelCount++
                    tunnelAlongKm = 0.0
                }
                tunnelAlongKm += distKm
                if (tunnelFp.size < tunnelCount) {
                    tunnelFp.add("$label@${"%.2f".format(Locale.US, tunnelAlongKm)}")
                } else {
                    tunnelFp[tunnelCount - 1] = "$label@${"%.2f".format(Locale.US, tunnelAlongKm)}"
                }
            }
            if (isFerry) {
                if (!prevFerry) {
                    ferryCount++
                    ferryAlongKm = 0.0
                }
                ferryAlongKm += distKm
                if (ferryFp.size < ferryCount) {
                    ferryFp.add("$label@${"%.2f".format(Locale.US, ferryAlongKm)}")
                } else {
                    ferryFp[ferryCount - 1] = "$label@${"%.2f".format(Locale.US, ferryAlongKm)}"
                }
            }
            prevTunnel = isTunnel
            prevFerry = isFerry
        }
        return buildString {
            append("route_tunnel_count=$tunnelCount\n")
            append("route_tunnel_fp=${tunnelFp.joinToString("|")}\n")
            append("route_ferry_legs=$ferryCount\n")
            append("route_ferry_fp=${ferryFp.joinToString("|")}\n")
        }
    }

    private fun annotateDatex(
        context: Context,
        dataDir: java.io.File,
        samples: List<Pair<Double, Double>>,
    ): String {
        if (samples.size < 2) {
            return "datex_impacts=0; datex_block=0; datex_penalize=0\n"
        }
        if (!MapHudPrefs.loadDatexPluginEnabled(context)) {
            return "datex_impacts=0; datex_block=0; datex_penalize=0\ndatex_mwm=plugin_off\n"
        }
        val routeJson = latLonPairsToJson(samples)
        val cacheDir = java.io.File(dataDir, "datex_cache").absolutePath
        val raw =
            runCatching {
                datexRefreshJson(
                    enabled = true,
                    host = MapHudPrefs.loadDatexHost(context),
                    port = MapHudPrefs.loadDatexPort(context).toUInt(),
                    routeLatLonJson = routeJson,
                    wifiOnly = false,
                    onWifi = true,
                    useDiscoveryChain = true,
                    cacheDir = cacheDir,
                )
            }.getOrElse { e ->
                Log.w(TAG, "mwm datex refresh failed: ${e.message}")
                return "datex_impacts=0; datex_block=0; datex_penalize=0\ndatex_mwm=refresh_failed\n"
            }
        val o =
            runCatching { JSONObject(raw) }.getOrNull()
                ?: return "datex_impacts=0; datex_block=0; datex_penalize=0\ndatex_mwm=parse_failed\n"
        val active = o.optJSONArray("active") ?: JSONArray()
        var block = 0
        var penalize = 0
        val lines = StringBuilder()
        for (i in 0 until active.length()) {
            val s = active.optJSONObject(i) ?: continue
            // DatexImpact::as_str uses "penalty" (not "penalize").
            val impact = s.optString("impact").lowercase(Locale.US)
            when (impact) {
                "block" -> block++
                "penalty", "penalize" -> penalize++
                else -> continue
            }
            lines.append(
                "datex_constraint: id=${s.optString("id")}; impact=$impact; " +
                    "lat=${s.optDouble("lat")}; lon=${s.optDouble("lon")}; " +
                    "xsi=${s.optString("xsi_type")}\n",
            )
        }
        val impacts = block + penalize
        return buildString {
            append("datex_impacts=$impacts; datex_block=$block; datex_penalize=$penalize\n")
            append("datex_mwm=overlay_apply; active=${active.length()}; warning=${o.optString("warning")}\n")
            append(lines)
        }
    }

    private fun softBreakPoisJson(
        dataDir: java.io.File,
        profile: TravelProfile,
        polyline: String,
        distanceKm: Double,
        etaMinutes: Double,
    ): Pair<String, String> {
        if (!usesSoftBreaks(profile) || distanceKm < 1.0 || polyline.isBlank()) {
            return "[]" to "rest_place_count=0\nchunked_soft_breaks=skipped_profile\n"
        }
        val samples = samplePolylineKm(polyline)
        if (samples.size < 2) {
            return "[]" to "rest_place_count=0\nchunked_soft_breaks=no_samples\n"
        }
        val (intervalKm, intervalH, icon, kind) = softBreakSpacing(dataDir, profile, distanceKm, etaMinutes)
        val breaks = JSONArray()
        var next = intervalKm
        while (next < distanceKm - 0.5) {
            val (lat, lon) = interpolateAtKm(samples, next)
            val name =
                when (profile) {
                    TravelProfile.HIKING -> "Hiking rest"
                    TravelProfile.BICYCLE, TravelProfile.BICYCLE_ELECTRIC -> "Cycle rest"
                    else -> "Rest stop"
                }
            breaks.put(
                JSONObject()
                    .put("name", name)
                    .put("lat", lat)
                    .put("lon", lon)
                    .put("kind", kind)
                    .put("icon", icon)
                    .put("along_km", next),
            )
            next += intervalKm
        }
        val extra =
            "chunked_soft_breaks=true; break_interval_km=${"%.1f".format(Locale.US, intervalKm)}; " +
                "break_interval_h=${"%.2f".format(Locale.US, intervalH)}\n" +
                "rest_place_count=${breaks.length()}\n" +
                "chunked_rest_pauses=${breaks.length()}; break_pois_total=${breaks.length()}\n"
        return breaks.toString() to extra
    }

    /**
     * Motor soft breaks (~1.5 h). Hiking uses Scandinavian rast spacing
     * ([HIKING_MAIN_BREAK_KM]); bicycle a longer cycling rast.
     */
    private fun softBreakSpacing(
        dataDir: java.io.File,
        profile: TravelProfile,
        distanceKm: Double,
        etaMinutes: Double,
    ): SoftBreakSpacing {
        when (profile) {
            TravelProfile.HIKING -> {
                val intervalKm = HIKING_MAIN_BREAK_KM
                val etaH = (etaMinutes / 60.0).coerceAtLeast(1e-6)
                val speedKmh = (distanceKm / etaH).coerceAtLeast(1.0)
                return SoftBreakSpacing(intervalKm, intervalKm / speedKmh, "shelter", "rest")
            }
            TravelProfile.BICYCLE, TravelProfile.BICYCLE_ELECTRIC -> {
                val intervalKm = CYCLING_MAIN_BREAK_KM
                val etaH = (etaMinutes / 60.0).coerceAtLeast(1e-6)
                val speedKmh = (distanceKm / etaH).coerceAtLeast(1.0)
                return SoftBreakSpacing(intervalKm, intervalKm / speedKmh, "picnic", "rest")
            }
            else -> {
                val rest =
                    runCatching { loadCarRestSettings(dataDir.absolutePath) }.getOrNull()
                val hudIntervalH =
                    (rest?.breakIntervalHours ?: 2.0).coerceIn(1.0, 12.0)
                val intervalH =
                    when {
                        hudIntervalH >= 3.0 -> 1.5
                        else -> hudIntervalH
                    }.coerceIn(1.0, 12.0)
                val etaH = (etaMinutes / 60.0).coerceAtLeast(1e-6)
                val speedKmh = (distanceKm / etaH).coerceAtLeast(1.0)
                val intervalKm = (speedKmh * intervalH).coerceAtLeast(1.0)
                return SoftBreakSpacing(intervalKm, intervalH, "fuel", "amenity")
            }
        }
    }

    private data class SoftBreakSpacing(
        val intervalKm: Double,
        val intervalH: Double,
        val icon: String,
        val kind: String,
    )

    /** Matches `HIKING_MAIN_BREAK_DISTANCE_KM` in core defaults. */
    private const val HIKING_MAIN_BREAK_KM = 11.295

    /** Matches `CYCLING_MAIN_BREAK_DISTANCE_KM` in core defaults. */
    private const val CYCLING_MAIN_BREAK_KM = 28.24

    internal fun usesSoftBreaks(profile: TravelProfile): Boolean =
        when (profile) {
            TravelProfile.CAR,
            TravelProfile.CAR_ELECTRIC,
            TravelProfile.MOTORCYCLE,
            TravelProfile.MOTORCYCLE_ELECTRIC,
            TravelProfile.MOBILE_HOME,
            TravelProfile.HIKING,
            TravelProfile.BICYCLE,
            TravelProfile.BICYCLE_ELECTRIC,
            -> true
            else -> false
        }

    private fun seedAttractionsFromComaps(samples: List<Pair<Double, Double>>) {
        seedComapsCategoryPois(
            samples = samples,
            categoryKey = "category_tourism",
            fallbackQuery = "Tourism",
            ingestId = "mwm-corridor-tourism",
            logLabel = "mwm_attractions",
            tagBuilder = { h ->
                JSONObject()
                    .put("name", h.name)
                    .put("tourism", comapsHitToTourismTag(h.featureType, h.name))
            },
        )
    }

    private fun seedWaterFromComaps(samples: List<Pair<Double, Double>>) {
        seedComapsCategoryPois(
            samples = samples,
            categoryKey = "category_water",
            fallbackQuery = "Water",
            ingestId = "mwm-corridor-water",
            logLabel = "mwm_water",
            tagBuilder = { h ->
                JSONObject()
                    .put("name", h.name)
                    .put("amenity", "drinking_water")
                    .put("natural", if (comapsHitLooksLikeSpring(h.featureType, h.name)) "spring" else "")
            },
        )
    }

    private fun seedComapsCategoryPois(
        samples: List<Pair<Double, Double>>,
        categoryKey: String,
        fallbackQuery: String,
        ingestId: String,
        logLabel: String,
        tagBuilder: (FfiSearchHit) -> JSONObject,
    ) {
        if (samples.isEmpty() || !ComapsSearchBridge.enabled()) return
        val query =
            runCatching { comapsCategoryQuery(categoryKey, "nb") }
                .getOrNull()
                ?.trim()
                ?.ifBlank { null }
                ?: fallbackQuery
        val step = (samples.size / 8).coerceAtLeast(1)
        val picked = samples.filterIndexed { i, _ -> i % step == 0 }.take(8)
        val pois = JSONArray()
        var osm = -2_000_000L
        val seen = HashSet<String>()
        for ((lat, lon) in picked) {
            val hits =
                runCatching {
                    ComapsSearchBridge.searchCategory(
                        queryText = query,
                        biasLat = lat,
                        biasLon = lon,
                        limit = 20,
                        locale = "nb",
                    )
                }.getOrDefault(emptyList())
            for (h in hits) {
                if (!h.lat.isFinite() || !h.lon.isFinite()) continue
                if (h.name.isBlank()) continue
                val distM =
                    if (h.distanceM.isFinite() && h.distanceM >= 0.0) {
                        h.distanceM
                    } else {
                        haversineM(lat, lon, h.lat, h.lon)
                    }
                if (distM > 1_200.0) continue
                val key =
                    "${h.name.trim().lowercase(Locale.US)}|" +
                        "${"%.4f".format(Locale.US, h.lat)}|${"%.4f".format(Locale.US, h.lon)}"
                if (!seen.add(key)) continue
                osm -= 1
                pois.put(
                    JSONObject()
                        .put("osm_id", osm)
                        .put("lat", h.lat)
                        .put("lon", h.lon)
                        .put("name", h.name)
                        .put("tags", tagBuilder(h)),
                )
            }
        }
        if (pois.length() == 0) {
            Log.i(TAG, "$logLabel seed empty query=$query samples=${picked.size}")
            return
        }
        val stats =
            runCatching {
                poiLookaheadIngestFromJson(ingestId, pois.toString())
            }.getOrNull()
        Log.i(
            TAG,
            "$logLabel seeded=${pois.length()} records=${stats?.records} query=$query",
        )
    }

    internal fun comapsHitLooksLikeSpring(
        featureType: String?,
        name: String?,
    ): Boolean {
        val hay =
            listOfNotNull(featureType, name)
                .joinToString(" ")
                .lowercase(Locale.US)
        return hay.contains("spring") || hay.contains("kilde") || hay.contains("quelle")
    }

    /**
     * Map CoMaps category / feature_type (+ name hints) onto OSM tourism=* so
     * look-ahead icon keys preserve viewpoint / museum / artwork (not a flat
     * tourism=attraction dump that collapses every hit to one type).
     */
    internal fun comapsHitToTourismTag(
        featureType: String?,
        name: String?,
    ): String {
        val hay =
            listOfNotNull(featureType, name)
                .joinToString(" ")
                .lowercase(Locale.US)
        return when {
            hay.contains("viewpoint") ||
                hay.contains("utsikt") ||
                hay.contains("aussicht") ||
                hay.contains("scenic") ||
                hay.contains("lookout") ||
                hay.contains("overlook") -> "viewpoint"
            hay.contains("museum") -> "museum"
            hay.contains("artwork") ||
                hay.contains("sculpture") ||
                hay.contains("statue") ||
                hay.contains("memorial") -> "artwork"
            else -> "attraction"
        }
    }

    /** Lon,lat;… polyline → up to [maxPoints] evenly spaced (lat,lon) samples. */
    private fun downsamplePolyline(
        polyline: String,
        maxPoints: Int,
    ): List<Pair<Double, Double>> {
        val all = ArrayList<Pair<Double, Double>>()
        for (part in polyline.split(';')) {
            val bits = part.split(',')
            if (bits.size != 2) continue
            val lon = bits[0].toDoubleOrNull() ?: continue
            val lat = bits[1].toDoubleOrNull() ?: continue
            all.add(lat to lon)
        }
        if (all.size <= maxPoints) return all
        val step = ((all.size - 1).toDouble() / (maxPoints - 1)).coerceAtLeast(1.0)
        val out = ArrayList<Pair<Double, Double>>(maxPoints)
        var i = 0.0
        while (out.size < maxPoints) {
            out.add(all[i.toInt().coerceIn(0, all.lastIndex)])
            i += step
        }
        if (out.last() != all.last()) {
            out[out.lastIndex] = all.last()
        }
        return out
    }

    /** Wire format lon,lat;… → (lat, lon, cum_km). */
    private fun samplePolylineKm(polyline: String): List<Triple<Double, Double, Double>> {
        val out = ArrayList<Triple<Double, Double, Double>>()
        var cum = 0.0
        var prev: Pair<Double, Double>? = null
        for (part in polyline.split(';')) {
            val bits = part.split(',')
            if (bits.size != 2) continue
            val lon = bits[0].toDoubleOrNull() ?: continue
            val lat = bits[1].toDoubleOrNull() ?: continue
            if (prev != null) {
                cum += haversineM(prev.first, prev.second, lat, lon) / 1000.0
            }
            out.add(Triple(lat, lon, cum))
            prev = lat to lon
        }
        return out
    }

    private fun interpolateAtKm(
        samples: List<Triple<Double, Double, Double>>,
        targetKm: Double,
    ): Pair<Double, Double> {
        if (samples.isEmpty()) return 0.0 to 0.0
        if (targetKm <= samples.first().third) {
            return samples.first().first to samples.first().second
        }
        for (i in 0 until samples.size - 1) {
            val (lat0, lon0, k0) = samples[i]
            val (lat1, lon1, k1) = samples[i + 1]
            if (targetKm <= k1) {
                val t = if ((k1 - k0).absoluteValue < 1e-9) 0.0 else (targetKm - k0) / (k1 - k0)
                return (lat0 + (lat1 - lat0) * t) to (lon0 + (lon1 - lon0) * t)
            }
        }
        val last = samples.last()
        return last.first to last.second
    }

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
        val leaves = LinkedHashSet<String>()
        collectLeavesAlong(la0, lo0, la1, lo1, leaves)
        return missingResult(context, leaves)
    }

    private fun collectLeavesAlong(
        la0: Double,
        lo0: Double,
        la1: Double,
        lo1: Double,
        into: MutableSet<String>,
    ) {
        val dKm = haversineM(la0, lo0, la1, lo1) / 1000.0
        val n = (dKm / 8.0).toInt().coerceIn(2, 240)
        for (k in 0..n) {
            val f = k.toDouble() / n
            val leaf =
                runCatching {
                    uniffi.navi.comapsLeafForPoint(la0 + (la1 - la0) * f, lo0 + (lo1 - lo0) * f)
                }.getOrNull()
            if (!leaf.isNullOrBlank()) into.add(leaf)
        }
    }

    /**
     * Leaves along the route whose sidecars lack a `vehicle_limits` section,
     * plus leaves whose catalog match rate is below the configured threshold.
     */
    private fun vehicleLimitsCoverage(
        context: Context,
        leaves: Collection<String>,
    ): Pair<List<String>, List<String>> {
        if (leaves.isEmpty()) return emptyList<String>() to emptyList()
        val missing =
            runCatching {
                uniffi.navi.comapsLeavesMissingVehicleLimits(leaves.toList())
            }.getOrDefault(leaves.toList())
        val haveSection = leaves.filter { it.isNotBlank() && it !in missing }
        val partial =
            VehicleLimitsCoverageStore.partialLeaves(context, haveSection).filter { it !in missing }
        return missing to partial
    }

    private fun vehicleLimitsReportLines(
        missing: List<String>?,
        partial: List<String> = emptyList(),
    ): String {
        if (missing == null) return ""
        return buildString {
            append("vehicle_limits_ok=${missing.isEmpty() && partial.isEmpty()}\n")
            if (missing.isNotEmpty()) {
                append("vehicle_limits_missing=${missing.joinToString(",")}\n")
            }
            if (partial.isNotEmpty()) {
                append("vehicle_limits_partial=${partial.joinToString(",")}\n")
            }
        }
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
