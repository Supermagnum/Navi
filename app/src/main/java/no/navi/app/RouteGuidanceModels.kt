package no.navi.app

import org.json.JSONArray

data class RouteSimSample(
    val lat: Double,
    val lon: Double,
    val cumM: Double,
    val speedKmh: Double,
    val highway: String?,
    val maxspeedPosted: Boolean,
    /** Posted OSM maxspeed (km/h) when present — for live limit resolution. */
    val maxspeedKmh: Double? = null,
    /** Raw OSM maxspeed:conditional for live time-based limit evaluation. */
    val maxspeedConditional: String? = null,
    /** OSM name, else ref; null → UI uses [highwayClassDisplayLabel]. */
    val street: String? = null,
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
    /** Explicit Navit icon stem from Rust when present; preferred over [kind] mapping. */
    val icon: String? = null,
    /** Optional next-maneuver hint when the following step is 30–60 m ahead. */
    val then: String? = null,
    /** 0-based intermediate-via ordinal when this is a via-reached marker. */
    val viaIndex: Int? = null,
) {
    fun iconKey(): String {
        icon?.takeIf { it.isNotBlank() }?.let { return it }
        return when (kind) {
            "slight_left" -> "nav_left_1"
            "left" -> "nav_left_2"
            "sharp_left" -> "nav_left_3"
            "slight_right" -> "nav_right_1"
            "right" -> "nav_right_2"
            "sharp_right" -> "nav_right_3"
            "u_turn" -> "nav_turnaround_left"
            "destination" -> "nav_destination"
            "roundabout" ->
                when (roundaboutExit) {
                    1 -> "nav_roundabout_r1"
                    2 -> "nav_roundabout_r2"
                    3 -> "nav_roundabout_r3"
                    4 -> "nav_roundabout_r4"
                    5 -> "nav_roundabout_r5"
                    6 -> "nav_roundabout_r6"
                    7 -> "nav_roundabout_r7"
                    8 -> "nav_roundabout_r8"
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
                        highway = o.optString("highway").takeIf { it.isNotBlank() && it != "null" },
                        maxspeedPosted = o.optBoolean("maxspeed_posted", false),
                        maxspeedKmh =
                            if (o.has("maxspeed_kmh") && !o.isNull("maxspeed_kmh")) {
                                o.optDouble("maxspeed_kmh").takeIf { it.isFinite() && it > 0.0 }
                            } else {
                                null
                            },
                        maxspeedConditional =
                            if (o.isNull("maxspeed_conditional")) {
                                null
                            } else {
                                o
                                    .optString("maxspeed_conditional")
                                    .takeIf { it.isNotBlank() && it != "null" }
                            },
                        street =
                            if (o.isNull("street")) {
                                null
                            } else {
                                o
                                    .optString("street")
                                    .takeIf { it.isNotBlank() && it != "null" }
                            },
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
                val streetRaw =
                    if (o.isNull("street")) {
                        null
                    } else {
                        o.optString("street").takeIf {
                            it.isNotBlank() && it != "null"
                        }
                    }
                val houseRaw =
                    if (o.isNull("housenumber")) {
                        null
                    } else {
                        o.optString("housenumber").takeIf { it.isNotBlank() && it != "null" }
                    }
                val postRaw =
                    if (o.isNull("postcode")) {
                        null
                    } else {
                        o.optString("postcode").takeIf { it.isNotBlank() && it != "null" }
                    }
                val (street, house, post) =
                    parseAddressDisplayLines(
                        street = streetRaw,
                        houseNumber = houseRaw,
                        postcode = postRaw,
                    )
                val exit = if (o.isNull("roundabout_exit")) null else o.optInt("roundabout_exit")
                val iconRaw =
                    if (o.isNull("icon")) {
                        null
                    } else {
                        o.optString("icon").takeIf { it.isNotBlank() && it != "null" }
                    }
                val thenRaw =
                    if (o.isNull("then")) {
                        null
                    } else {
                        o.optString("then").takeIf { it.isNotBlank() && it != "null" }
                    }
                val viaIndex =
                    if (o.has("via_index") && !o.isNull("via_index")) {
                        o.optInt("via_index").takeIf { it >= 0 }
                    } else {
                        null
                    }
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
                        icon = iconRaw,
                        then = thenRaw,
                        viaIndex = viaIndex,
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
            // Drop unmarked mid-leg destination markers (chunked leg ends). Keep
            // via-reached markers that carry via_index.
            if (m.kind == "destination" && idx < legs.lastIndex && m.viaIndex == null) {
                continue
            }
            out.add(m.copy(cumM = m.cumM + offset))
        }
        offset += lastCum
    }
    return thinRouteManeuvers(out)
}

/**
 * Thin stitched maneuvers toward continuous-route list density.
 * Mirrors `driver_break_core::routing::guidance_path::thin_route_maneuvers`.
 */
fun thinRouteManeuvers(mans: List<RouteManeuver>): List<RouteManeuver> {
    val longRouteM = 500_000.0
    val minGapFloorM = 8_000.0
    val targetSlots = 75.0
    val shortDedupM = 750.0
    val expectedMin = 55
    val expectedMax = 100

    fun isAnchor(m: RouteManeuver): Boolean {
        if (m.viaIndex != null) return true
        return m.kind == "destination"
    }

    fun thinWithGap(source: List<RouteManeuver>, minGap: Double): List<RouteManeuver> {
        val out = ArrayList<RouteManeuver>(source.size.coerceAtMost(128))
        var lastKeptCum = Double.NEGATIVE_INFINITY
        for (m in source) {
            if (m.kind == "straight") continue
            if (isAnchor(m)) {
                out.add(m)
                lastKeptCum = m.cumM
                continue
            }
            if (m.cumM - lastKeptCum < minGap) continue
            out.add(m)
            lastKeptCum = m.cumM
        }
        return out
    }

    val totalM = mans.lastOrNull()?.cumM?.coerceAtLeast(1.0) ?: 1.0
    if (totalM < longRouteM) {
        return thinWithGap(mans, shortDedupM)
    }

    var gap = (totalM / targetSlots).coerceAtLeast(minGapFloorM)
    var best = thinWithGap(mans, gap)
    repeat(12) {
        val n = best.size
        if (n in expectedMin..expectedMax) return best
        gap =
            if (n > expectedMax) {
                gap * 1.2
            } else {
                (gap * 0.82).coerceAtLeast(minGapFloorM * 0.5)
            }
        best = thinWithGap(mans, gap)
    }
    while (best.size > expectedMax) {
        gap *= 1.25
        best = thinWithGap(mans, gap)
        if (gap > totalM) break
    }
    return best
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
        "path", "footway", "cycleway", "bridleway", "steps" -> 10.0
        else -> 50.0
    }
}

/**
 * Human highway-class label when name/ref are missing.
 * Mirrors `driver_break_core::routing::eta::highway_class_display_label` — keep in sync.
 */
fun highwayClassDisplayLabel(highway: String?): String {
    val h = highway?.trim()?.lowercase() ?: return "Road"
    val base = h.removeSuffix("_link")
    return when (base) {
        "motorway" -> "Motorway"
        "trunk" -> "Trunk road"
        "primary" -> "Primary road"
        "secondary" -> "Secondary road"
        "tertiary" -> "Tertiary road"
        "unclassified" -> "Unclassified road"
        "residential" -> "Residential road"
        "living_street" -> "Living street"
        "service" -> "Service road"
        "track" -> "Track"
        "road" -> "Road"
        "path" -> "Path"
        "footway" -> "Footway"
        "cycleway" -> "Cycleway"
        "bridleway" -> "Bridleway"
        "steps" -> "Steps"
        "pedestrian" -> "Pedestrian street"
        else -> "Road"
    }
}

/**
 * Bottom-HUD current-road label: sample street (name/ref), else highway-class words.
 * Mirrors `driver_break_core::current_road_label`.
 */
fun formatCurrentRoadLabel(
    street: String?,
    highway: String?,
): String {
    val s = street?.trim().orEmpty()
    if (s.isNotEmpty()) return s
    return highwayClassDisplayLabel(highway)
}

/**
 * Fallback street label from nearby place-index hits when no region PBF / graph
 * edge snap is available (idle GPS).
 *
 * Prefers `addr:*` entries. Uses the **most common** street name among nearby
 * addresses (not only the single nearest house). Weaker than nearest OSM way
 * at junctions — prefer `roadLabelNear` when a PBF is present.
 */
fun streetLabelFromNearbyPlaces(hits: List<uniffi.navi.PlaceHit>): String? {
    if (hits.isEmpty()) return null
    val addr =
        hits.filter {
            val k = it.kind.lowercase()
            k.contains("addr") || k.contains("housenumber")
        }
    val pool = addr.ifEmpty { hits }
    val labels =
        pool.mapNotNull { hit ->
            val (street, _, _) = parseAddressDisplayLines(combined = hit.name)
            street?.trim()?.takeIf { it.isNotEmpty() }
                ?: hit.name.trim().takeIf { it.isNotEmpty() }
        }
    if (labels.isEmpty()) return null
    val counts = LinkedHashMap<String, Int>()
    for (label in labels) {
        counts[label] = (counts[label] ?: 0) + 1
    }
    return counts.entries
        .sortedWith(
            compareByDescending<Map.Entry<String, Int>> { it.value }
                .thenBy { labels.indexOf(it.key) },
        ).first()
        .key
}

/**
 * Localized via-reached label for toast / approach box.
 * Uses [viaIndex0Based] when present; otherwise a generic "Via point reached".
 */
fun viaReachedDisplayLabel(
    resources: android.content.res.Resources,
    viaIndex0Based: Int?,
    viaCount: Int,
): String =
    if (viaIndex0Based != null && viaCount > 0) {
        resources.getString(
            R.string.via_point_n_of_m_reached,
            viaIndex0Based + 1,
            viaCount,
        )
    } else {
        resources.getString(R.string.via_point_reached)
    }

/** Approximate great-circle distance in metres (HUD throttle helpers). */
fun haversineMApprox(
    lat1: Double,
    lon1: Double,
    lat2: Double,
    lon2: Double,
): Double {
    val r = 6_378_100.0
    val p1 = Math.toRadians(lat1)
    val p2 = Math.toRadians(lat2)
    val dp = Math.toRadians(lat2 - lat1)
    val dl = Math.toRadians(lon2 - lon1)
    val a =
        kotlin.math.sin(dp / 2) * kotlin.math.sin(dp / 2) +
            kotlin.math.cos(p1) * kotlin.math.cos(p2) *
            kotlin.math.sin(dl / 2) * kotlin.math.sin(dl / 2)
    return 2 * r * kotlin.math.asin(kotlin.math.sqrt(a))
}
