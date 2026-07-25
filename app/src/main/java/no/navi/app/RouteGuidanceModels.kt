package no.navi.app

import org.json.JSONArray

data class RouteSimSample(
    val lat: Double,
    val lon: Double,
    val cumM: Double,
    val speedKmh: Double,
    val highway: String?,
    val maxspeedPosted: Boolean,
)

data class RouteManeuver(
    val lat: Double,
    val lon: Double,
    val cumM: Double,
    val kind: String,
    val street: String?,
    val houseNumber: String? = null,
    val postcode: String? = null,
    val roundaboutExit: Int?,
) {
    fun iconKey(): String = when (kind) {
        "left", "slight_left" -> "nav_left_1"
        "sharp_left" -> "nav_left_3"
        "right", "slight_right" -> "nav_right_1"
        "sharp_right" -> "nav_right_3"
        "u_turn" -> "nav_turnaround_left"
        "destination" -> "nav_destination"
        "roundabout" -> when (roundaboutExit) {
            2 -> "nav_roundabout_r2"
            3 -> "nav_roundabout_r3"
            else -> "nav_roundabout_r1"
        }
        "keep_left" -> "nav_keep_left"
        "keep_right" -> "nav_keep_right"
        "exit_left" -> "nav_exit_left"
        "exit_right" -> "nav_exit_right"
        "merge_left" -> "nav_merge_left"
        "merge_right" -> "nav_merge_right"
        else -> "nav_straight"
    }
}

fun parseRouteSimSamples(json: String): List<RouteSimSample> {
    if (json.isBlank() || json == "[]") return emptyList()
    return runCatching {
        val arr = JSONArray(json)
        buildList {
            for (i in 0 until arr.length()) {
                val o = arr.getJSONObject(i)
                add(
                    RouteSimSample(
                        lat = o.getDouble("lat"),
                        lon = o.getDouble("lon"),
                        cumM = o.getDouble("cum_m"),
                        speedKmh = o.getDouble("speed_kmh"),
                        highway = o.optString("highway", null)?.takeIf { it.isNotBlank() && it != "null" },
                        maxspeedPosted = o.optBoolean("maxspeed_posted", false),
                    ),
                )
            }
        }
    }.getOrDefault(emptyList())
}

fun parseRouteManeuvers(json: String): List<RouteManeuver> {
    if (json.isBlank() || json == "[]") return emptyList()
    return runCatching {
        val arr = JSONArray(json)
        buildList {
            for (i in 0 until arr.length()) {
                val o = arr.getJSONObject(i)
                val streetRaw = if (o.isNull("street")) null else o.optString("street").takeIf {
                    it.isNotBlank() && it != "null"
                }
                val houseRaw = if (o.isNull("housenumber")) {
                    null
                } else {
                    o.optString("housenumber").takeIf { it.isNotBlank() && it != "null" }
                }
                val postRaw = if (o.isNull("postcode")) {
                    null
                } else {
                    o.optString("postcode").takeIf { it.isNotBlank() && it != "null" }
                }
                val (street, house, post) = parseAddressDisplayLines(
                    street = streetRaw,
                    houseNumber = houseRaw,
                    postcode = postRaw,
                )
                val exit = if (o.isNull("roundabout_exit")) null else o.optInt("roundabout_exit")
                add(
                    RouteManeuver(
                        lat = o.getDouble("lat"),
                        lon = o.getDouble("lon"),
                        cumM = o.getDouble("cum_m"),
                        kind = o.getString("kind"),
                        street = street ?: streetRaw,
                        houseNumber = house ?: houseRaw,
                        postcode = post ?: postRaw,
                        roundaboutExit = exit,
                    ),
                )
            }
        }
    }.getOrDefault(emptyList())
}

/** Merge leg samples with cumulative-metre offset (multi-via motor plans). */
fun mergeSimSamples(legs: List<List<RouteSimSample>>): List<RouteSimSample> {
    val out = ArrayList<RouteSimSample>()
    var offset = 0.0
    for (leg in legs) {
        if (leg.isEmpty()) continue
        for (s in leg) {
            out.add(s.copy(cumM = s.cumM + offset))
        }
        offset = out.last().cumM
    }
    return out
}

fun mergeManeuvers(legs: List<List<RouteManeuver>>): List<RouteManeuver> {
    val out = ArrayList<RouteManeuver>()
    var offset = 0.0
    for ((idx, leg) in legs.withIndex()) {
        if (leg.isEmpty()) continue
        val lastCum = leg.lastOrNull()?.cumM ?: 0.0
        for (m in leg) {
            // Drop mid-leg "destination" markers except on the final leg.
            if (m.kind == "destination" && idx < legs.lastIndex) continue
            out.add(m.copy(cumM = m.cumM + offset))
        }
        offset += lastCum
    }
    return out
}

/** Highway-class fallback table (mirrors core eta::highway_fallback_kmh). */
fun highwayFallbackKmh(highway: String?): Double {
    val h = highway?.trim()?.lowercase() ?: return 50.0
    val base = h.removeSuffix("_link")
    return when (base) {
        "motorway" -> 100.0
        "trunk" -> 80.0
        "primary" -> 70.0
        "secondary" -> 60.0
        "tertiary", "unclassified" -> 50.0
        "residential", "living_street" -> 40.0
        "service", "track", "road" -> 20.0
        else -> 50.0
    }
}
