package no.navi.app

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject
import uniffi.navi.CorridorRouteResult
import uniffi.navi.FfiTollPolicy
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
        val preferred =
            listOf(
                File(dataDir, "ostlandet-latest.osm.pbf"),
                File(dataDir, "espa-atnbrufossen-corridor.osm.pbf"),
                File(dataDir, "oppland-latest.osm.pbf"),
                File("/data/local/tmp/navi_fixtures/ostlandet-latest.osm.pbf"),
                File("/data/local/tmp/navi_fixtures/espa-atnbrufossen-corridor.osm.pbf"),
                File("/data/local/tmp/navi_fixtures/oppland-latest.osm.pbf"),
            ).firstOrNull { it.isFile && it.length() > 10_000L }
        if (preferred != null) return preferred

        // Pack-server installs leave a small stub PBF + leaf-stem packs.
        val meta = File(dataDir, "region_meta.json")
        if (meta.isFile) {
            runCatching {
                val name =
                    JSONObject(meta.readText())
                        .optString("pbf_filename")
                        .trim()
                if (name.isNotEmpty()) {
                    val f = File(dataDir, name)
                    if (f.isFile) return f
                }
            }
        }
        return dataDir.listFiles()?.firstOrNull { f ->
            f.isFile &&
                f.name.endsWith(".osm.pbf") &&
                f.length() > 10_000L &&
                File(dataDir, f.name.removeSuffix(".osm.pbf") + ".navi-manifest.json").isFile
        }
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
            require(waypoints.size <= 6) {
                "at most 4 via points allowed (got ${waypoints.size - 2} vias)"
            }
            onProgress(10, "planning")
            val start = waypoints.first()
            val end = waypoints.last()
            val vias =
                waypoints.drop(1).dropLast(1).map { uniffi.navi.FfiLatLon(it.lat, it.lon) }
            val result =
                planCarRoute(
                    pbfPath = pbf.absolutePath,
                    elevDir = elev,
                    cacheDir = cacheDir.absolutePath,
                    startLat = start.lat,
                    startLon = start.lon,
                    endLat = end.lat,
                    endLon = end.lon,
                    useEco = useEco,
                    profile = profile,
                    avoidMotorways = avoidMotorways,
                    tollPolicy =
                        if (avoidTolls) {
                            FfiTollPolicy.PENALIZE
                        } else {
                            FfiTollPolicy.ALLOW
                        },
                    avoidFerries = avoidFerries,
                    vehicle = vehicle,
                    preferOfficialNetworks = preferOfficialNetworks,
                    dataDir = dataDir.absolutePath,
                    viaPoints = vias,
                )
            onProgress(100, "done")
            if (!result.report.contains("PASS") || result.routePolyline.isBlank()) {
                return@withContext result
            }
            result.copy(
                poiLat = end.lat,
                poiLon = end.lon,
                poiName = end.name,
            )
        }
}
