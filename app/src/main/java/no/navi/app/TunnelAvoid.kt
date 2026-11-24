package no.navi.app

import org.json.JSONArray
import org.json.JSONObject
import uniffi.navi.FfiComapsRoute
import java.util.Locale
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.sin
import kotlin.math.sqrt

/**
 * Routing flags that change a CoMaps from–to leg. Part of the tunnel-avoid
 * leg cache key so a different avoid / vehicle / locale cannot reuse a leg.
 */
internal data class TunnelLegFlags(
    val profile: String,
    val avoidToll: Boolean,
    val avoidMotorway: Boolean,
    val avoidFerry: Boolean,
    val avoidDirty: Boolean,
    val optimize: String,
    val wetlandHook: Boolean,
    val turnsLocale: String,
    val vehicleKey: String,
)

internal data class TunnelLegCacheKey(
    val fromLatE7: Long,
    val fromLonE7: Long,
    val toLatE7: Long,
    val toLonE7: Long,
    val flags: TunnelLegFlags,
)

/**
 * From–to legs reused for one tunnel-avoid search. A hit returns the same
 * points, turns, length, and travel time. [buildTimeS] is zero on a hit so
 * the summed build time stays the native work. Endpoints that differ miss.
 */
internal class TunnelLegCache {
    private val legs = HashMap<TunnelLegCacheKey, FfiComapsRoute>()

    var hits: Int = 0
        private set

    var misses: Int = 0
        private set

    val size: Int
        get() = legs.size

    fun get(key: TunnelLegCacheKey): FfiComapsRoute? {
        val found = legs[key]
        if (found == null) {
            misses++
            return null
        }
        hits++
        return found.copy(buildTimeS = 0.0)
    }

    fun put(
        key: TunnelLegCacheKey,
        route: FfiComapsRoute,
    ) {
        legs[key] = route
    }
}

internal fun tunnelLegCacheKey(
    fromLat: Double,
    fromLon: Double,
    toLat: Double,
    toLon: Double,
    flags: TunnelLegFlags,
): TunnelLegCacheKey =
    TunnelLegCacheKey(
        fromLatE7 = quantizeE7(fromLat),
        fromLonE7 = quantizeE7(fromLon),
        toLatE7 = quantizeE7(toLat),
        toLonE7 = quantizeE7(toLon),
        flags = flags,
    )

/** ~1 cm. Distinct detour vias miss; the same endpoint pair hits. */
private fun quantizeE7(deg: Double): Long = Math.round(deg * 1e7)

/**
 * One contiguous tunnel the soft-avoid search is spending its budget on.
 * [anchors] are lat/lon samples along the run (entrance / mid / exit).
 */
internal data class TunnelRunTarget(
    val lengthM: Double,
    val startM: Double,
    val endM: Double,
    val anchors: List<Pair<Double, Double>>,
)

/**
 * What to do after one detour attempt.
 * [ADVANCE_ROUND] keeps the improved geometry and spends the next round on
 * the new longest leftover. [FINISH] is only a zero-tunnel result.
 */
internal enum class TunnelRoundStep {
    KEEP_TRYING,
    ADVANCE_ROUND,
    FINISH,
}

internal fun tunnelRoundStep(
    beforeCount: Int,
    afterCount: Int,
    geometryChanged: Boolean,
): TunnelRoundStep {
    if (!geometryChanged || afterCount >= beforeCount) return TunnelRoundStep.KEEP_TRYING
    if (afterCount <= 0) return TunnelRoundStep.FINISH
    return TunnelRoundStep.ADVANCE_ROUND
}

/**
 * A round that only skipped already-seen primary-side vias is not a stall.
 * The opposite side of that same run has not been considered, so the search
 * must continue. A native attempt that failed to cut, or a round that already
 * walked the opposite side, is a stall.
 */
internal fun tunnelAvoidRoundIsStall(
    nativeCalls: Int,
    skippedPrimary: Int,
    oppositeConsidered: Boolean,
    hasOtherSide: Boolean,
): Boolean {
    val skipOnlyPrimary =
        nativeCalls == 0 && skippedPrimary > 0 && !oppositeConsidered && hasOtherSide
    return !skipOnlyPrimary
}

/**
 * Walks offsets on [primarySide] first, including the wide hiking offsets.
 * When a primary attempt leaves the route fingerprint unchanged, emits
 * [otherSide] at that same offset before the next primary offset.
 * After the primary offsets have all been offered without a cut, the same
 * offsets are emitted on [otherSide]. A caller that only skips already-seen
 * primary vias still receives that opposite side instead of a null stall.
 */
internal class TunnelSideEscalation(
    private val offsetsM: List<Double>,
    private val primarySide: Double,
    private val otherSide: Double?,
) {
    private var index = 0
    private var onOtherSide = false

    fun next(lastPrimaryUnchanged: Boolean): Pair<Double, Double>? {
        if (!onOtherSide && lastPrimaryUnchanged && otherSide != null && index > 0) {
            return offsetsM[index - 1] to otherSide
        }
        if (!onOtherSide) {
            if (index < offsetsM.size) {
                val offset = offsetsM[index]
                index += 1
                return offset to primarySide
            }
            if (otherSide == null) return null
            onOtherSide = true
            index = 0
        }
        if (otherSide == null || index >= offsetsM.size) return null
        val offset = offsetsM[index]
        index += 1
        return offset to otherSide
    }
}

/** Polyline of detour vias (`lon,lat;…`, six decimals) for the skip key. */
internal fun detourViaPolyline(vias: List<Pair<Double, Double>>): String =
    vias.joinToString(";") { (lat, lon) ->
        String.format(Locale.US, "%.6f,%.6f", lon, lat)
    }

internal fun detourViaFingerprint(vias: List<Pair<Double, Double>>): String {
    val poly = detourViaPolyline(vias)
    return String.format(Locale.US, "%08x:len=%d", poly.hashCode(), poly.length)
}

/**
 * @return true when [fingerprint] was already routed and the native call
 * should be skipped. The first sighting is recorded and returns false.
 */
internal fun skipKnownDetour(
    seen: MutableSet<String>,
    fingerprint: String,
): Boolean = !seen.add(fingerprint)

/**
 * Short window around a tunnel that sits on one leg: no user via inside the
 * window, and the window is a small slice of a longer route. Null when a
 * splice would jump a via or replace most of the corridor.
 */
internal fun spliceWindowMeters(
    runStartM: Double,
    runEndM: Double,
    routeLengthM: Double,
    viaAlongM: List<Double>,
    marginM: Double = 200.0,
): Pair<Double, Double>? {
    if (runEndM <= runStartM || routeLengthM <= 0.0) return null
    val start = (runStartM - marginM).coerceAtLeast(0.0)
    val end = (runEndM + marginM).coerceAtMost(routeLengthM)
    val window = end - start
    if (window < 50.0 || window > 15_000.0) return null
    if (window > routeLengthM * 0.35) return null
    for (viaM in viaAlongM) {
        if (viaM > start + 30.0 && viaM < end - 30.0) return null
    }
    return start to end
}

internal data class SplicedGeometry(
    val polyline: String,
    val turnsJson: String,
    val lengthM: Double,
)

/**
 * Replace the [windowStartM, windowEndM] slice of [polyline] / [turnsJson]
 * with a short window route. A failed or empty window returns null; that is
 * not evidence the tunnel is unavoidable.
 */
internal fun spliceTunnelWindow(
    polyline: String,
    turnsJson: String,
    windowStartM: Double,
    windowEndM: Double,
    windowPolyline: String,
    windowTurnsJson: String,
    windowLengthM: Double,
): SplicedGeometry? {
    if (windowEndM <= windowStartM) return null
    if (windowLengthM <= 0.0 || windowLengthM > 80_000.0) return null
    val base = polylineSamples(polyline)
    val windowPts = polylineSamples(windowPolyline)
    if (base.size < 2 || windowPts.size < 2) return null
    val prefix = ArrayList<Pair<Double, Double>>()
    val suffix = ArrayList<Pair<Double, Double>>()
    for (p in base) {
        if (p.third <= windowStartM) prefix.add(p.first to p.second)
        if (p.third >= windowEndM) suffix.add(p.first to p.second)
    }
    val merged = ArrayList<Pair<Double, Double>>(prefix.size + windowPts.size + suffix.size)
    merged.addAll(prefix)
    var winStart = 0
    if (merged.isNotEmpty() && samePoint(merged.last(), windowPts.first().first to windowPts.first().second)) {
        winStart = 1
    }
    for (i in winStart until windowPts.size) {
        merged.add(windowPts[i].first to windowPts[i].second)
    }
    var sufStart = 0
    if (suffix.isNotEmpty() && merged.isNotEmpty() && samePoint(merged.last(), suffix.first())) {
        sufStart = 1
    }
    for (i in sufStart until suffix.size) merged.add(suffix[i])
    if (merged.size < 2) return null
    val turns = spliceTurns(turnsJson, windowStartM, windowEndM, windowTurnsJson) ?: return null
    return SplicedGeometry(
        polyline = merged.joinToString(";") { (lat, lon) -> "$lon,$lat" },
        turnsJson = turns,
        lengthM = polylineLengthM(merged),
    )
}

/**
 * Accept a spliced turn list only when the tunnel-run count drops and every
 * remaining run label was already present. A new run rejects the splice.
 * Rejection is not proof the tunnel is unavoidable.
 */
internal fun acceptTunnelSplice(
    beforeTurns: String,
    splicedTurns: String,
): Boolean {
    val beforeCount = MwmProductionPlanner.tunnelRunCount(beforeTurns)
    val afterCount = MwmProductionPlanner.tunnelRunCount(splicedTurns)
    if (afterCount >= beforeCount) return false
    val remaining = tunnelRunLabels(beforeTurns).toMutableList()
    for (label in tunnelRunLabels(splicedTurns)) {
        val at = remaining.indexOf(label)
        if (at < 0) return false
        remaining.removeAt(at)
    }
    return true
}

internal fun tunnelRunLabels(turnsJson: String): List<String> {
    val source = jsonTurns(turnsJson)
    val labels = ArrayList<String>()
    var prev = false
    for (i in 0 until source.length()) {
        val o = source.optJSONObject(i) ?: continue
        val isTunnel = isForbiddenTunnelTurn(o)
        if (!isTunnel) {
            prev = false
            continue
        }
        val street =
            o.optString("street").ifBlank {
                o.optString("ref").ifBlank { "unnamed" }
            }
        if (!prev) {
            labels.add(street)
        } else if (o.optString("street").isNotBlank()) {
            labels[labels.lastIndex] = street
        }
        prev = true
    }
    return labels
}

private fun spliceTurns(
    turnsJson: String,
    windowStartM: Double,
    windowEndM: Double,
    windowTurnsJson: String,
): String? {
    val source = jsonTurns(turnsJson)
    val out = JSONArray()
    var cum = 0.0
    for (i in 0 until source.length()) {
        val o = source.optJSONObject(i) ?: continue
        val dist = o.optDouble("dist_m", 0.0).coerceAtLeast(0.0)
        val start = cum
        val end = cum + dist
        cum = end
        when {
            end <= windowStartM + 0.5 -> out.put(JSONObject(o.toString()))
            start < windowStartM - 0.5 && end > windowStartM + 0.5 -> {
                val keep = windowStartM - start
                if (keep > 1.0) {
                    out.put(JSONObject(o.toString()).put("dist_m", keep))
                }
            }
        }
    }
    val window = jsonTurns(windowTurnsJson)
    var appended = 0
    for (i in 0 until window.length()) {
        val o = window.optJSONObject(i) ?: continue
        if (o.optString("kind") == "destination") continue
        out.put(JSONObject(o.toString()))
        appended++
    }
    if (appended == 0) return null
    cum = 0.0
    for (i in 0 until source.length()) {
        val o = source.optJSONObject(i) ?: continue
        val dist = o.optDouble("dist_m", 0.0).coerceAtLeast(0.0)
        val start = cum
        val end = cum + dist
        cum = end
        when {
            start >= windowEndM - 0.5 -> out.put(JSONObject(o.toString()))
            start < windowEndM - 0.5 && end > windowEndM + 0.5 -> {
                val keep = end - windowEndM
                if (keep > 1.0) {
                    out.put(JSONObject(o.toString()).put("dist_m", keep))
                }
            }
        }
    }
    return out.toString()
}

private fun jsonTurns(json: String): JSONArray = runCatching { JSONArray(json.ifBlank { "[]" }) }.getOrElse { JSONArray() }

/** lat, lon, cumulative meters. */
private fun polylineSamples(polyline: String): List<Triple<Double, Double, Double>> {
    val out = ArrayList<Triple<Double, Double, Double>>()
    var cum = 0.0
    var prevLat = 0.0
    var prevLon = 0.0
    var hasPrev = false
    for (part in polyline.split(';')) {
        val bits = part.split(',')
        if (bits.size != 2) continue
        val lon = bits[0].toDoubleOrNull() ?: continue
        val lat = bits[1].toDoubleOrNull() ?: continue
        if (hasPrev) cum += haversineM(prevLat, prevLon, lat, lon)
        out.add(Triple(lat, lon, cum))
        prevLat = lat
        prevLon = lon
        hasPrev = true
    }
    return out
}

private fun polylineLengthM(points: List<Pair<Double, Double>>): Double {
    var sum = 0.0
    for (i in 1 until points.size) {
        sum += haversineM(points[i - 1].first, points[i - 1].second, points[i].first, points[i].second)
    }
    return sum
}

private fun samePoint(
    a: Pair<Double, Double>,
    b: Pair<Double, Double>,
): Boolean = (a.first - b.first).absoluteValue() < 1e-5 && (a.second - b.second).absoluteValue() < 1e-5

private fun Double.absoluteValue(): Double = if (this < 0.0) -this else this

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
    val a = sin(dphi / 2) * sin(dphi / 2) + cos(p1) * cos(p2) * sin(dl / 2) * sin(dl / 2)
    return 2 * r * atan2(sqrt(a), sqrt(1 - a))
}
