package no.navi.app

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import org.json.JSONArray
import org.json.JSONObject
import java.util.Locale

/**
 * Plan-result enumerations shared by every travel profile: tunnels, ferries,
 * rest places, nearby attractions, and wild-camping sites.
 *
 * Tunnel / ferry / rest-place counts come from the native plan report (0 when
 * the path has none). Attractions and wild camping are filled from existing
 * look-ahead / camping-plugin results — never invented.
 */
data class RoutePlanStats(
    val tunnelCount: Int = 0,
    val tunnelFp: String = "",
    val ferryLegCount: Int = 0,
    val ferryFp: String = "",
    val restPlaceCount: Int = 0,
    val restPlaceNames: List<String> = emptyList(),
    val attractionCount: Int = 0,
    val attractionByType: Map<String, Int> = emptyMap(),
    val wildCampingSiteCount: Int = 0,
    /** Distinct from count=0 when plugin off: e.g. UNAVAILABLE / ERROR kind. */
    val wildCampingStatusKind: String = "",
    val wildCampingStatusMessage: String = "",
)

/** Host-side camping suggest outcome kept when result JSON is absent (UNAVAILABLE). */
data class CampingSuggestStatus(
    val kind: String,
    val message: String,
)

fun parseReportUIntToken(
    report: String,
    key: String,
): Int? {
    val prefix = "$key="
    fun parseFrom(hay: String): Int? {
        val trimmed = hay.trim()
        if (trimmed.startsWith(prefix)) {
            return trimmed
                .removePrefix(prefix)
                .substringBefore(';')
                .trim()
                .toIntOrNull()
        }
        for (part in trimmed.split(';', ' ', '|')) {
            if (part.startsWith(prefix)) {
                return part.removePrefix(prefix).toIntOrNull()
            }
        }
        return null
    }
    var last: Int? = null
    var summary: Int? = null
    for (line in report.lineSequence()) {
        val trimmed = line.trim()
        if (trimmed.startsWith("plan_summary")) {
            summary = parseFrom(trimmed)
        }
        parseFrom(trimmed)?.let { last = it }
    }
    return summary ?: last
}

fun parseReportTokenValue(
    report: String,
    key: String,
): String? {
    val prefix = "$key="
    for (line in report.lineSequence()) {
        val trimmed = line.trim()
        if (trimmed.startsWith(prefix)) {
            return trimmed.removePrefix(prefix).trim()
        }
    }
    return null
}

fun restPlaceNamesFromJson(breakPoisJson: String): List<String> {
    if (breakPoisJson.isBlank() || breakPoisJson == "[]") return emptyList()
    return runCatching {
        val arr = JSONArray(breakPoisJson)
        buildList {
            for (i in 0 until arr.length()) {
                val o = arr.optJSONObject(i) ?: continue
                val name = o.optString("name").trim()
                if (name.isNotEmpty()) add(name)
            }
        }
    }.getOrDefault(emptyList())
}

fun wildCampingSiteCount(result: CampingSuggestResult?): Int {
    if (result == null) return 0
    val ids = linkedSetOf<String>()

    fun add(list: CampingSuggestionListModel) {
        for (c in list.cards) {
            if (!c.accepted) continue
            val id =
                c.locationId.ifBlank {
                    String.format(Locale.US, "%.5f,%.5f", c.lat, c.lon)
                }
            ids.add(id)
        }
    }
    add(result.list)
    add(result.onFootFromHere)
    return ids.size
}

fun uniqueAttractionTally(queryJsons: List<String>): Pair<Int, Map<String, Int>> {
    val byOsm = linkedMapOf<String, String>()
    for (raw in queryJsons) {
        val state = poiLookaheadHudFromQueryJson(raw)
        for (h in state.hits) {
            val id = if (h.osmId != 0L) "osm:${h.osmId}" else "lbl:${h.label}:${h.iconKey}"
            if (id !in byOsm) {
                byOsm[id] = h.iconKey.ifBlank { "attraction" }
            }
        }
    }
    val byType = linkedMapOf<String, Int>()
    for (kind in byOsm.values) {
        byType[kind] = byType.getOrDefault(kind, 0) + 1
    }
    return byOsm.size to byType.toMap()
}

fun routePlanStatsFromPlan(
    report: String,
    breakPoisJson: String,
): RoutePlanStats {
    val restNames = restPlaceNamesFromJson(breakPoisJson)
    val restFromReport = parseReportUIntToken(report, "rest_place_count")
    return RoutePlanStats(
        tunnelCount = parseReportUIntToken(report, "route_tunnel_count") ?: 0,
        tunnelFp = parseReportTokenValue(report, "route_tunnel_fp").orEmpty(),
        ferryLegCount = parseReportUIntToken(report, "route_ferry_legs") ?: 0,
        ferryFp = parseReportTokenValue(report, "route_ferry_fp").orEmpty(),
        restPlaceCount = restFromReport ?: restNames.size,
        restPlaceNames = restNames,
    )
}

fun RoutePlanStats.toReportJson(): JSONObject {
    val byType = JSONObject()
    for ((k, v) in attractionByType) {
        byType.put(k, v)
    }
    val rest = JSONArray()
    for (n in restPlaceNames) rest.put(n)
    return JSONObject()
        .put("route_tunnel_count", tunnelCount)
        .put("route_tunnel_fp", tunnelFp)
        .put("route_ferry_legs", ferryLegCount)
        .put("route_ferry_fp", ferryFp)
        .put("rest_place_count", restPlaceCount)
        .put("rest_place_names", rest)
        .put("attraction_count", attractionCount)
        .put("attraction_by_type", byType)
        .put("wild_camping_site_count", wildCampingSiteCount)
        .put("wild_camping_status_kind", wildCampingStatusKind)
        .put("wild_camping_status_message", wildCampingStatusMessage)
}

fun formatNamedFpList(fp: String): String {
    if (fp.isBlank()) return ""
    return fp
        .split('|')
        .mapNotNull { part ->
            val name = part.substringBefore('@').trim()
            name.takeIf { it.isNotEmpty() }
        }.joinToString(", ")
}

fun headingBetweenDeg(
    lat1: Double,
    lon1: Double,
    lat2: Double,
    lon2: Double,
): Double {
    val dLon = Math.toRadians(lon2 - lon1)
    val p1 = Math.toRadians(lat1)
    val p2 = Math.toRadians(lat2)
    val y = kotlin.math.sin(dLon) * kotlin.math.cos(p2)
    val x =
        kotlin.math.cos(p1) * kotlin.math.sin(p2) -
            kotlin.math.sin(p1) * kotlin.math.cos(p2) * kotlin.math.cos(dLon)
    return Math.toDegrees(kotlin.math.atan2(y, x))
}

@Composable
fun RoutePlanStatsCard(
    stats: RoutePlanStats,
    breakRemindersEnabled: Boolean = true,
    campingPluginEnabled: Boolean = false,
    campingSuggestStatus: CampingSuggestStatus? = null,
    modifier: Modifier = Modifier,
) {
    Surface(
        shape = RoundedCornerShape(10.dp),
        tonalElevation = 2.dp,
        modifier =
            modifier
                .fillMaxWidth()
                .testTag("route_plan_stats"),
    ) {
        Column(
            modifier = Modifier.padding(10.dp),
            verticalArrangement = Arrangement.spacedBy(2.dp),
        ) {
            Text(
                "Route enumerations",
                style = MaterialTheme.typography.titleSmall,
                modifier = Modifier.testTag("route_plan_stats_title"),
            )
            Text(
                "Tunnels: ${stats.tunnelCount}",
                style = MaterialTheme.typography.bodySmall,
                modifier = Modifier.testTag("route_plan_stats_tunnels"),
            )
            val ferryNames = formatNamedFpList(stats.ferryFp)
            val ferryLine =
                if (ferryNames.isNotBlank()) {
                    "Ferries: ${stats.ferryLegCount} ($ferryNames)"
                } else {
                    "Ferries: ${stats.ferryLegCount}"
                }
            Text(
                ferryLine,
                style = MaterialTheme.typography.bodySmall,
                modifier = Modifier.testTag("route_plan_stats_ferries"),
            )
            if (breakRemindersEnabled) {
                val restLine =
                    if (stats.restPlaceNames.isNotEmpty()) {
                        "Rest places: ${stats.restPlaceCount} (${stats.restPlaceNames.take(8).joinToString(", ")})"
                    } else {
                        "Rest places: ${stats.restPlaceCount}"
                    }
                Text(
                    restLine,
                    style = MaterialTheme.typography.bodySmall,
                    modifier = Modifier.testTag("route_plan_stats_rest"),
                )
            }
            val attrTypes =
                stats.attractionByType.entries
                    .sortedByDescending { it.value }
                    .joinToString(", ") { "${it.key} ${it.value}" }
            val attrLine =
                if (attrTypes.isNotBlank()) {
                    "Attractions: ${stats.attractionCount} ($attrTypes)"
                } else {
                    "Attractions: ${stats.attractionCount}"
                }
            Text(
                attrLine,
                style = MaterialTheme.typography.bodySmall,
                modifier = Modifier.testTag("route_plan_stats_attractions"),
            )
            Text(
                formatWildCampingStatsLine(
                    pluginEnabled = campingPluginEnabled,
                    siteCount = stats.wildCampingSiteCount,
                    status = campingSuggestStatus,
                ),
                style = MaterialTheme.typography.bodySmall,
                modifier = Modifier.testTag("route_plan_stats_wild_camping"),
            )
        }
    }
}

fun formatWildCampingStatsLine(
    pluginEnabled: Boolean,
    siteCount: Int,
    status: CampingSuggestStatus?,
): String {
    if (!pluginEnabled) return "Wild camping: plugin off"
    val kind = status?.kind.orEmpty()
    if (kind.isNotEmpty() && !kind.equals("OK", ignoreCase = true)) {
        val msg = status?.message.orEmpty().trim()
        val short =
            if (msg.length > 72) {
                msg.take(69) + "..."
            } else {
                msg
            }
        return if (short.isNotEmpty()) {
            "Wild camping: $kind ($short)"
        } else {
            "Wild camping: $kind"
        }
    }
    return "Wild camping sites: $siteCount"
}
