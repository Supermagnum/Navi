package no.navi.app

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.navi.CorridorRouteResult
import uniffi.navi.FfiVehicleLimits
import uniffi.navi.TravelProfile
import uniffi.navi.planCarRoute
import uniffi.navi.planHikingRoute
import java.io.File

/**
 * Shared planning entry for initial Plan and off-route recalculation.
 * Reuses the same UniFFI pipeline — no second router.
 */
object RouteReplan {
    fun resolvePbf(dataDir: File): File? {
        NaviMapTestHooks.forcePlanPbfPath?.let { path ->
            val f = File(path)
            if (f.isFile) return f
        }
        return listOf(
            File(dataDir, "ostlandet-latest.osm.pbf"),
            File(dataDir, "espa-atnbrufossen-corridor.osm.pbf"),
            File(dataDir, "oppland-latest.osm.pbf"),
            File("/data/local/tmp/navi_fixtures/ostlandet-latest.osm.pbf"),
            File("/data/local/tmp/navi_fixtures/espa-atnbrufossen-corridor.osm.pbf"),
            File("/data/local/tmp/navi_fixtures/oppland-latest.osm.pbf"),
        ).firstOrNull { it.isFile && it.length() > 10_000L }
    }

    suspend fun plan(
        dataDir: File,
        profile: TravelProfile,
        waypoints: List<Waypoint>,
        useEco: Boolean,
        avoidMotorways: Boolean,
        avoidTolls: Boolean,
        avoidFerries: Boolean,
        vehicle: FfiVehicleLimits,
        preferOfficialNetworks: Boolean,
        preferPilgrimRoutes: Boolean,
        onProgress: (pct: Int, detail: String) -> Unit = { _, _ -> },
    ): CorridorRouteResult =
        withContext(Dispatchers.IO) {
            require(waypoints.size >= 2) { "need start and end" }
            NaviMapTestHooks.rerouteResultOverride?.let { return@withContext it }

            val pbf =
                resolvePbf(dataDir)
                    ?: error("No region PBF available for replan")
            val elev = File(dataDir, "elevation").absolutePath
            if (profile == TravelProfile.HIKING) {
                onProgress(20, "hiking_graph")
                val wpsJson =
                    waypoints.joinToString(",", "[", "]") {
                        """{"name":${org.json.JSONObject.quote(it.name)},"lat":${it.lat},"lon":${it.lon}}"""
                    }
                val hike =
                    planHikingRoute(
                        pbf.absolutePath,
                        elev,
                        File(dataDir, "graph-cache-foot").absolutePath,
                        wpsJson,
                        preferOfficialNetworks,
                        preferPilgrimRoutes,
                        dataDir.absolutePath,
                    )
                onProgress(100, "hiking_done")
                return@withContext hike
            }

            val graphTag =
                when (profile) {
                    TravelProfile.BICYCLE, TravelProfile.BICYCLE_ELECTRIC -> "bicycle"
                    TravelProfile.TRUCK, TravelProfile.TRUCK_ELECTRIC, TravelProfile.MOBILE_HOME -> "truck"
                    else -> "car"
                }
            val cacheDir =
                File(dataDir, "graph-cache-${pbf.nameWithoutExtension}-$graphTag")
            var poly = ""
            var dist = 0.0
            var etaSum = 0.0
            var shareWeighted = 0.0
            var last: CorridorRouteResult? = null
            val legSamples = mutableListOf<List<RouteSimSample>>()
            val legManeuvers = mutableListOf<List<RouteManeuver>>()
            val legTotal = waypoints.size - 1
            for (i in 0 until legTotal) {
                val a = waypoints[i]
                val b = waypoints[i + 1]
                val pct = ((i * 100) / legTotal).coerceIn(0, 99)
                onProgress(pct, "leg_${i + 1}_of_$legTotal")
                val leg =
                    planCarRoute(
                        pbfPath = pbf.absolutePath,
                        elevDir = elev,
                        cacheDir = cacheDir.absolutePath,
                        startLat = a.lat,
                        startLon = a.lon,
                        endLat = b.lat,
                        endLon = b.lon,
                        useEco = useEco,
                        profile = profile,
                        avoidMotorways = avoidMotorways,
                        avoidTolls = avoidTolls,
                        avoidFerries = avoidFerries,
                        vehicle = vehicle,
                        preferOfficialNetworks = preferOfficialNetworks,
                        dataDir = dataDir.absolutePath,
                    )
                if (!leg.report.contains("PASS") || leg.routePolyline.isBlank()) {
                    return@withContext leg
                }
                last = leg
                if (poly.isNotEmpty() && leg.routePolyline.isNotEmpty()) poly += ";"
                poly += leg.routePolyline
                dist += leg.distanceKm
                etaSum += leg.etaMinutes
                shareWeighted += leg.priorityPathSharePct * leg.distanceKm
                legSamples.add(parseRouteSimSamples(leg.simSamplesJson))
                legManeuvers.add(parseRouteManeuvers(leg.maneuversJson))
            }
            val merged = last!!
            val share = if (dist > 0) shareWeighted / dist else merged.priorityPathSharePct
            onProgress(100, "done")
            CorridorRouteResult(
                report = merged.report,
                distanceKm = dist,
                etaMinutes = etaSum,
                cacheHit = merged.cacheHit,
                coldBuildS = merged.coldBuildS,
                warmLoadS = merged.warmLoadS,
                routePolyline = poly,
                poiLat = waypoints.last().lat,
                poiLon = waypoints.last().lon,
                poiName = waypoints.last().name,
                poiIconKey = merged.poiIconKey,
                breakPoisJson = merged.breakPoisJson,
                daysJson = merged.daysJson,
                simSamplesJson = samplesToJson(mergeSimSamples(legSamples)),
                maneuversJson = maneuversToJson(mergeManeuvers(legManeuvers)),
                priorityPathSharePct = share,
                routeSegmentsJson = merged.routeSegmentsJson,
                offTrailAdvisory = merged.offTrailAdvisory,
            )
        }

    private fun samplesToJson(samples: List<RouteSimSample>): String {
        if (samples.isEmpty()) return "[]"
        return samples.joinToString(",", "[", "]") { s ->
            val street =
                s.street?.let { org.json.JSONObject.quote(it) } ?: "null"
            val hwy = s.highway?.let { org.json.JSONObject.quote(it) } ?: "null"
            val cond =
                s.maxspeedConditional?.let { org.json.JSONObject.quote(it) } ?: "null"
            val posted =
                s.maxspeedKmh?.takeIf { it.isFinite() }?.toString() ?: "null"
            """{"lat":${s.lat},"lon":${s.lon},"cum_m":${s.cumM},"speed_kmh":${s.speedKmh},""" +
                """"highway":$hwy,"maxspeed_posted":${s.maxspeedPosted},""" +
                """"maxspeed_kmh":$posted,"maxspeed_conditional":$cond,"street":$street}"""
        }
    }

    private fun maneuversToJson(maneuvers: List<RouteManeuver>): String {
        if (maneuvers.isEmpty()) return "[]"
        return maneuvers.joinToString(",", "[", "]") { m ->
            val street = m.street?.let { org.json.JSONObject.quote(it) } ?: "null"
            val exit = m.roundaboutExit?.toString() ?: "null"
            """{"lat":${m.lat},"lon":${m.lon},"cum_m":${m.cumM},"kind":${org.json.JSONObject.quote(m.kind)},""" +
                """"street":$street,"roundabout_exit":$exit""" +
                (m.icon?.let { ""","icon":${org.json.JSONObject.quote(it)}""" } ?: "") +
                "}"
        }
    }
}
