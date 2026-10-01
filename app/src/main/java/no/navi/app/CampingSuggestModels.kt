package no.navi.app

import org.json.JSONArray
import org.json.JSONObject

/** Spec disclaimer — must match [navi-right-to-roam-camping] `DISCLAIMER`. */
const val CAMPING_PLUGIN_DISCLAIMER: String =
    "This plugin provides informational guidance based on publicly described " +
        "right-to-roam / outdoor-access rules (including Norwegian allemannsretten). " +
        "It is not legal advice and not a compliance guarantee. Laws and local practice change; " +
        "municipal fire bans, private land, and seasonal restrictions can be stricter than these summaries. " +
        "The user remains responsible for checking official sources and complying with the law where they camp."

enum class CampingDeclineKind {
    CAMPSITES_ONLY,
    SVALBARD,
    HARD_FILTER,
    UNKNOWN,
}

enum class CampingTier {
    A,
    B,
    C,
    D,
    UNKNOWN,
}

data class CampingNotCheckedLayers(
    val protectedArea: Boolean,
    val landcover: Boolean,
)

data class CampingCardModel(
    val lat: Double,
    val lon: Double,
    val accepted: Boolean,
    val decline: CampingDeclineKind?,
    val rejectReason: String?,
    val tier: CampingTier,
    val countryIso: String,
    val subdivisionIso: String?,
    val legalBasis: String,
    val sources: List<String>,
    val fireText: String?,
    val bareRockNote: String?,
    val notes: List<String>,
    val notChecked: CampingNotCheckedLayers,
    val disclaimer: String,
    val locationId: String,
    val seedRoadHighway: String?,
    val walkM: Double?,
)

data class CampingSuggestionListModel(
    val cards: List<CampingCardModel>,
    val seedsConsidered: Int,
    val probesAccepted: Int,
    val probesRejected: Int,
    val disclaimer: String,
)

data class CampingSuggestResult(
    val disclaimer: String,
    val list: CampingSuggestionListModel,
    val vehicle: CampingSuggestionListModel,
    val onFootFromHere: CampingSuggestionListModel,
    val accepted: Int,
    val rejected: Int,
    val vehicleAccepted: Int,
    val onFootAccepted: Int,
    val via: String,
    val peakGuestMemoryBytes: Long,
)

private fun jsonStr(
    obj: JSONObject,
    key: String,
    default: String = "",
): String {
    if (!obj.has(key) || obj.isNull(key)) return default
    return obj.optString(key, default).ifBlank { default }
}

fun parseCampingSuggestResultJson(json: String): CampingSuggestResult {
    val root = JSONObject(json)
    return CampingSuggestResult(
        disclaimer = jsonStr(root, "disclaimer", CAMPING_PLUGIN_DISCLAIMER),
        list = parseSuggestionList(root.optJSONObject("list")),
        vehicle = parseSuggestionList(root.optJSONObject("vehicle")),
        onFootFromHere = parseSuggestionList(root.optJSONObject("on_foot_from_here")),
        accepted = root.optInt("accepted", 0),
        rejected = root.optInt("rejected", 0),
        vehicleAccepted = root.optInt("vehicle_accepted", 0),
        onFootAccepted = root.optInt("on_foot_accepted", 0),
        via = jsonStr(root, "via"),
        peakGuestMemoryBytes = root.optLong("peak_guest_memory_bytes", 0L),
    )
}

private fun parseSuggestionList(obj: JSONObject?): CampingSuggestionListModel {
    if (obj == null) {
        return CampingSuggestionListModel(
            cards = emptyList(),
            seedsConsidered = 0,
            probesAccepted = 0,
            probesRejected = 0,
            disclaimer = CAMPING_PLUGIN_DISCLAIMER,
        )
    }
    val cardsArr = obj.optJSONArray("cards") ?: JSONArray()
    val cards =
        buildList {
            for (i in 0 until cardsArr.length()) {
                add(parseCard(cardsArr.getJSONObject(i)))
            }
        }
    return CampingSuggestionListModel(
        cards = cards,
        seedsConsidered = obj.optInt("seeds_considered", 0),
        probesAccepted = obj.optInt("probes_accepted", 0),
        probesRejected = obj.optInt("probes_rejected", 0),
        disclaimer = jsonStr(obj, "disclaimer", CAMPING_PLUGIN_DISCLAIMER),
    )
}

private fun parseCard(o: JSONObject): CampingCardModel =
    CampingCardModel(
        lat = o.getDouble("lat"),
        lon = o.getDouble("lon"),
        accepted = o.optBoolean("accepted", false),
        decline = parseDecline(jsonStr(o, "decline").takeIf { it.isNotBlank() }),
        rejectReason = jsonStr(o, "reject_reason").ifBlank { null },
        tier = parseTier(jsonStr(o, "tier").takeIf { it.isNotBlank() }),
        countryIso = jsonStr(o, "country_iso"),
        subdivisionIso = jsonStr(o, "subdivision_iso").ifBlank { null },
        legalBasis = jsonStr(o, "legal_basis"),
        sources = jsonStringList(o.optJSONArray("sources")),
        fireText = jsonStr(o, "fire_text").ifBlank { null },
        bareRockNote = jsonStr(o, "bare_rock_note").ifBlank { null },
        notes = jsonStringList(o.optJSONArray("notes")),
        notChecked =
            CampingNotCheckedLayers(
                protectedArea = o.optJSONObject("not_checked")?.optBoolean("protected_area", false) == true,
                landcover = o.optJSONObject("not_checked")?.optBoolean("landcover", false) == true,
            ),
        disclaimer = jsonStr(o, "disclaimer", CAMPING_PLUGIN_DISCLAIMER),
        locationId = jsonStr(o, "location_id"),
        seedRoadHighway = jsonStr(o, "seed_road_highway").ifBlank { null },
        walkM =
            if (o.has("walk_m") && !o.isNull("walk_m")) {
                o.getDouble("walk_m")
            } else {
                null
            },
    )

private fun jsonStringList(arr: JSONArray?): List<String> {
    if (arr == null) return emptyList()
    return buildList {
        for (i in 0 until arr.length()) {
            val s = arr.optString(i, "").trim()
            if (s.isNotEmpty()) add(s)
        }
    }
}

private fun parseDecline(raw: String?): CampingDeclineKind? {
    if (raw.isNullOrBlank()) return null
    return when (raw.lowercase()) {
        "campsites_only" -> CampingDeclineKind.CAMPSITES_ONLY
        "svalbard" -> CampingDeclineKind.SVALBARD
        "hard_filter" -> CampingDeclineKind.HARD_FILTER
        else -> CampingDeclineKind.UNKNOWN
    }
}

private fun parseTier(raw: String?): CampingTier =
    when (raw?.lowercase()) {
        "a" -> CampingTier.A
        "b" -> CampingTier.B
        "c" -> CampingTier.C
        "d" -> CampingTier.D
        else -> CampingTier.UNKNOWN
    }

/** Sample corridor polyline `"lat,lon;..."` into `[lat, lon]` pairs for native suggest. */
fun sampleCampingCorridorWaypoints(
    polyline: String,
    maxPoints: Int = 80,
): List<DoubleArray> {
    if (polyline.isBlank()) return emptyList()
    val raw =
        polyline.split(';').mapNotNull { seg ->
            val parts = seg.trim().split(',')
            if (parts.size != 2) return@mapNotNull null
            val lat = parts[0].trim().toDoubleOrNull() ?: return@mapNotNull null
            val lon = parts[1].trim().toDoubleOrNull() ?: return@mapNotNull null
            doubleArrayOf(lat, lon)
        }
    if (raw.isEmpty()) return emptyList()
    if (raw.size <= maxPoints) return raw
    val step = (raw.size / maxPoints).coerceAtLeast(1)
    return raw.filterIndexed { idx, _ -> idx % step == 0 }
}

fun campingWaypointsJson(waypoints: List<DoubleArray>): String {
    val arr = JSONArray()
    for (w in waypoints) {
        arr.put(JSONArray().put(w[0]).put(w[1]))
    }
    return arr.toString()
}

fun allCampingPinLatLon(result: CampingSuggestResult): List<Pair<Double, Double>> {
    val out = linkedSetOf<Pair<Double, Double>>()

    fun add(list: CampingSuggestionListModel) {
        for (c in list.cards) {
            out.add(c.lat to c.lon)
        }
    }
    add(result.list)
    add(result.vehicle)
    add(result.onFootFromHere)
    return out.toList()
}

fun isMotorisedTravelProfile(profile: uniffi.navi.TravelProfile): Boolean =
    when (profile) {
        uniffi.navi.TravelProfile.HIKING,
        uniffi.navi.TravelProfile.BICYCLE,
        uniffi.navi.TravelProfile.BICYCLE_ELECTRIC,
        -> false
        else -> true
    }
