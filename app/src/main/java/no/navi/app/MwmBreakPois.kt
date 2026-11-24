package no.navi.app

import android.os.SystemClock
import android.util.Log
import org.json.JSONArray
import org.json.JSONObject
import uniffi.navi.FfiSearchHit
import uniffi.navi.TravelProfile
import uniffi.navi.comapsCategoryQuery
import uniffi.navi.comapsSearchCancel
import uniffi.navi.comapsSearchClearViewport
import uniffi.navi.comapsSearchSetLocale
import uniffi.navi.comapsSearchSetPosition
import uniffi.navi.comapsSearchSetViewport
import uniffi.navi.comapsSearchStart
import uniffi.navi.comapsSessionIsOpen
import java.util.Locale
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicLong
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.sin
import kotlin.math.sqrt

/**
 * Soft-break / rest-place pins for the MWM production planner.
 *
 * [MwmProductionPlanner] does not load Ready POI packs. When a CoMaps session is
 * open, this collects cabin / shelter / picnic (and motor rest) hits from
 * installed MWM leaves along the planned polyline, then places one pin near each
 * rast interval — same role as `build_break_pois_json` on the PBF path.
 */
object MwmBreakPois {
    private const val TAG = "NaviPlan"

    /** Matches `HIKING_MAIN_BREAK_DISTANCE_KM` in core defaults. */
    const val HIKING_INTERVAL_KM = 11.295

    /** Default Drive hike/cycle cabin search radius. */
    const val DEFAULT_SEARCH_RADIUS_M = 10_500.0

    /** Coarser corridor sweeps before assigning marks (keeps plan latency bounded). */
    const val SEGMENT_KM = 35.0

    const val DEDUP_M = 2_000.0

    private const val READY_TIMEOUT_MS = 8_000L
    private const val PROBE_TIMEOUT_MS = 2_500L
    private const val POLL_MS = 80L
    private const val MAX_SEGMENTS = 24
    private const val HITS_PER_QUERY = 40

    data class Candidate(
        val name: String,
        val lat: Double,
        val lon: Double,
        val kind: String,
        val icon: String,
        val featureType: String = "",
    )

    data class Mark(
        val lat: Double,
        val lon: Double,
        val alongKm: Double,
    )

    data class BuildResult(
        val breakPoisJson: String,
        val restPlaceCount: Int,
        val candidateCount: Int,
        val segmentQueries: Int,
    )

    /**
     * Collect MWM rest candidates along [polyline] and emit break-POI JSON for
     * the profile's rast interval. Empty when search is unavailable or the
     * polyline is too short.
     */
    fun buildAlongPolyline(
        polyline: String,
        profile: TravelProfile,
        distanceKm: Double,
        locale: String? = null,
        searchRadiusM: Double = DEFAULT_SEARCH_RADIUS_M,
        stillActive: () -> Boolean = { true },
    ): BuildResult {
        val intervalKm =
            when (profile) {
                TravelProfile.HIKING -> HIKING_INTERVAL_KM
                TravelProfile.BICYCLE, TravelProfile.BICYCLE_ELECTRIC -> HIKING_INTERVAL_KM * 2.0
                else -> motorIntervalKm(distanceKm)
            }
        val marks = sampleMarks(polyline, intervalKm)
        if (marks.isEmpty()) {
            return BuildResult("[]", 0, 0, 0)
        }
        if (!ComapsSearchBridge.enabled()) {
            Log.i(TAG, "mwm_break_pois skipped: search disabled")
            return BuildResult("[]", 0, 0, 0)
        }
        val hiking =
            profile == TravelProfile.HIKING ||
                profile == TravelProfile.BICYCLE ||
                profile == TravelProfile.BICYCLE_ELECTRIC
        val (candidates, segments) =
            collectCandidates(
                polyline = polyline,
                distanceKm = distanceKm.coerceAtLeast(marks.last().alongKm),
                hiking = hiking,
                searchRadiusM = searchRadiusM,
                locale = locale,
                stillActive = stillActive,
            )
        val stops = assignNearest(marks, candidates, searchRadiusM)
        val json = toBreakPoisJson(stops)
        Log.i(
            TAG,
            "mwm_break_pois marks=${marks.size} candidates=${candidates.size} " +
                "stops=${stops.size} segments=$segments interval_km=$intervalKm",
        )
        return BuildResult(
            breakPoisJson = json,
            restPlaceCount = stops.size,
            candidateCount = candidates.size,
            segmentQueries = segments,
        )
    }

    fun appendReportTokens(
        report: StringBuilder,
        built: BuildResult,
    ) {
        report.append("rest_place_count=${built.restPlaceCount}\n")
        report.append("mwm_break_pois=true\n")
        report.append("mwm_break_candidates=${built.candidateCount}\n")
        report.append("mwm_break_segments=${built.segmentQueries}\n")
    }

    /** Evenly spaced rast marks along a lon,lat;lon,lat polyline. */
    fun sampleMarks(
        polyline: String,
        intervalKm: Double,
    ): List<Mark> {
        if (intervalKm <= 0.0) return emptyList()
        val samples = samplePolylineKm(polyline)
        if (samples.size < 2) return emptyList()
        val total = samples.last().alongKm
        if (total < intervalKm) return emptyList()
        val out = ArrayList<Mark>()
        var next = intervalKm
        while (next < total - 0.5) {
            val (lat, lon) = interpolateAtKm(samples, next)
            out.add(Mark(lat, lon, next))
            next += intervalKm
        }
        return out
    }

    /**
     * Map a CoMaps [featureType] to a rest-place kind/icon, or null when the
     * type is not a soft-break / overnight candidate (e.g. plain hotel on hike).
     */
    fun classifyRestFeature(
        featureType: String,
        hiking: Boolean,
    ): Pair<String, String>? {
        val t =
            featureType
                .trim()
                .lowercase(Locale.US)
                .replace('_', ' ')
                .replace('-', ' ')
                .replace('|', ' ')
                .replace(Regex("\\s+"), " ")
        if (t.isEmpty()) return null
        return when {
            t.contains("alpine hut") ||
                t.contains("wilderness hut") ||
                t.contains("basic hut") ||
                (t.contains("chalet") && hiking) -> "hut" to "cabin"
            t.contains("lean to") ||
                (t.contains("shelter") && !t.contains("animal")) ||
                t.contains("gapahuk") -> "shelter" to "shelter"
            t.contains("camp site") ||
                t.contains("campsite") ||
                t.contains("camp pitch") -> "tent" to "tourism-camp_site"
            t.contains("picnic") -> "picnic" to "tourism-picnic_site"
            t.contains("rest area") || t.contains("services") -> "rest_area" to "highway-rest_area"
            !hiking &&
                (
                    t.contains("hotel") ||
                        t.contains("motel") ||
                        t.contains("guest house") ||
                        t.contains("hostel") ||
                        t.contains("apartment")
                ) -> "lodging" to "tourism-hotel"
            !hiking && t.contains("parking") -> "rest_area" to "highway-rest_area"
            hiking && t.contains("hostel") -> "hut" to "cabin"
            else -> null
        }
    }

    fun candidateFromHit(
        hit: FfiSearchHit,
        hiking: Boolean,
    ): Candidate? {
        if (!hit.lat.isFinite() || !hit.lon.isFinite()) return null
        val classified =
            classifyRestFeature(hit.featureType, hiking)
                ?: classifyRestFeature(hit.name, hiking)
                ?: return null
        val name =
            hit.name.trim().ifBlank {
                when (classified.first) {
                    "hut" -> "Hut"
                    "shelter" -> "Shelter"
                    "tent" -> "Tent site"
                    "picnic" -> "Picnic"
                    "rest_area" -> "Rest area"
                    "lodging" -> "Lodging"
                    else -> "Rest"
                }
            }
        return Candidate(
            name = name,
            lat = hit.lat,
            lon = hit.lon,
            kind = classified.first,
            icon = classified.second,
            featureType = hit.featureType,
        )
    }

    data class AlongStop(
        val name: String,
        val lat: Double,
        val lon: Double,
        val kind: String,
        val icon: String,
        val alongKm: Double,
    )

    /** Nearest unused candidate within [searchRadiusM] for each rast mark. */
    fun assignNearest(
        marks: List<Mark>,
        candidates: List<Candidate>,
        searchRadiusM: Double,
    ): List<AlongStop> {
        if (marks.isEmpty() || candidates.isEmpty()) return emptyList()
        val used = BooleanArray(candidates.size)
        val stops = ArrayList<AlongStop>()
        for (mark in marks) {
            var bestIdx = -1
            var bestDist = Double.POSITIVE_INFINITY
            for (i in candidates.indices) {
                if (used[i]) continue
                val c = candidates[i]
                val d = haversineM(mark.lat, mark.lon, c.lat, c.lon)
                if (d <= searchRadiusM && d < bestDist) {
                    bestDist = d
                    bestIdx = i
                }
            }
            if (bestIdx < 0) continue
            val pick = candidates[bestIdx]
            val dup =
                stops.any { s ->
                    haversineM(pick.lat, pick.lon, s.lat, s.lon) < DEDUP_M
                }
            if (dup) continue
            used[bestIdx] = true
            stops.add(
                AlongStop(
                    name = pick.name,
                    lat = pick.lat,
                    lon = pick.lon,
                    kind = pick.kind,
                    icon = pick.icon,
                    alongKm = mark.alongKm,
                ),
            )
        }
        return stops
    }

    fun toBreakPoisJson(stops: List<AlongStop>): String {
        val arr = JSONArray()
        for (s in stops) {
            arr.put(
                JSONObject()
                    .put("name", s.name)
                    .put("lat", s.lat)
                    .put("lon", s.lon)
                    .put("kind", s.kind)
                    .put("icon", s.icon)
                    .put("icon_key", s.icon)
                    .put("along_km", s.alongKm),
            )
        }
        return arr.toString()
    }

    private fun collectCandidates(
        polyline: String,
        distanceKm: Double,
        hiking: Boolean,
        searchRadiusM: Double,
        locale: String?,
        stillActive: () -> Boolean,
    ): Pair<List<Candidate>, Int> {
        val samples = samplePolylineKm(polyline)
        if (samples.size < 2) return emptyList<Candidate>() to 0
        val lang =
            locale?.trim()?.takeIf { it.isNotEmpty() }
                ?: Locale.getDefault().language.ifBlank { "en" }
        // Pair(queryText, isCategory).
        val queries = ArrayList<Pair<String, Boolean>>()
        queries.add(
            (
                runCatching { comapsCategoryQuery("category_hotel", lang) }
                    .getOrNull()
                    ?.takeIf { it.isNotBlank() }
                    ?: "Hotel"
            ) to true,
        )
        if (hiking) {
            // Shelters / gapahuks are not in a CoMaps category group.
            queries.add("gapahuk" to false)
            queries.add("picnic" to false)
        } else {
            queries.add(
                (
                    runCatching { comapsCategoryQuery("category_parking", lang) }
                        .getOrNull()
                        ?.takeIf { it.isNotBlank() }
                        ?: "Parking"
                ) to true,
            )
        }
        val segmentStep = SEGMENT_KM.coerceAtLeast(searchRadiusM / 1000.0)
        val nSeg =
            ((distanceKm / segmentStep).toInt() + 1)
                .coerceIn(1, MAX_SEGMENTS)
        val padDeg = (searchRadiusM / 111_000.0).coerceIn(0.02, 0.18)
        val seen = HashSet<String>()
        val out = ArrayList<Candidate>()
        var queriesRun = 0
        for (s in 0 until nSeg) {
            if (!stillActive()) break
            val km = (s + 0.5) * (distanceKm / nSeg).coerceAtLeast(segmentStep * 0.5)
            val (lat, lon) = interpolateAtKm(samples, km.coerceAtMost(distanceKm))
            for ((queryText, isCategory) in queries) {
                if (!stillActive()) break
                val hits =
                    searchViewport(
                        query = queryText,
                        isCategory = isCategory,
                        lat = lat,
                        lon = lon,
                        padDeg = padDeg,
                        locale = lang,
                        stillActive = stillActive,
                    )
                queriesRun++
                for (h in hits) {
                    val c = candidateFromHit(h, hiking) ?: continue
                    val d = haversineM(lat, lon, c.lat, c.lon)
                    if (d > searchRadiusM * 1.35) continue
                    val key =
                        "${c.name.lowercase(Locale.US)}|" +
                            "%.4f".format(Locale.US, c.lat) + "|" +
                            "%.4f".format(Locale.US, c.lon)
                    if (!seen.add(key)) continue
                    out.add(c)
                }
            }
        }
        runCatching { comapsSearchClearViewport() }
        return out to queriesRun
    }

    private fun searchViewport(
        query: String,
        isCategory: Boolean,
        lat: Double,
        lon: Double,
        padDeg: Double,
        locale: String,
        stillActive: () -> Boolean,
    ): List<FfiSearchHit> {
        return runCatching {
            comapsSearchSetLocale(locale)
            comapsSearchSetViewport(lat - padDeg, lon - padDeg, lat + padDeg, lon + padDeg)
            comapsSearchSetPosition(lat, lon, true)
            val collector = HitCollector()
            val t0 = SystemClock.elapsedRealtime()
            var id = 0uL
            while (stillActive() && SystemClock.elapsedRealtime() - t0 < READY_TIMEOUT_MS) {
                if (!runCatching { comapsSessionIsOpen() }.getOrDefault(false)) {
                    Thread.sleep(POLL_MS)
                    continue
                }
                id = comapsSearchStart(query, isCategory, HITS_PER_QUERY.toUInt(), collector)
                if (id != 0uL) break
                Thread.sleep(POLL_MS)
            }
            if (id == 0uL) return@runCatching emptyList()
            val started = SystemClock.elapsedRealtime()
            while (!collector.done.await(POLL_MS, TimeUnit.MILLISECONDS)) {
                if (!stillActive() || SystemClock.elapsedRealtime() - started > PROBE_TIMEOUT_MS) {
                    comapsSearchCancel(id)
                    collector.done.await(500L, TimeUnit.MILLISECONDS)
                    break
                }
            }
            collector.hits.toList()
        }.getOrElse { e ->
            Log.w(TAG, "mwm_break_pois search failed q=$query: ${e.message}")
            emptyList()
        }
    }

    private class HitCollector : uniffi.navi.FfiSearchListener {
        val hits = CopyOnWriteArrayList<FfiSearchHit>()
        val done = CountDownLatch(1)
        private val firstHitAt = AtomicLong(-1L)
        private val startedAt = SystemClock.elapsedRealtime()

        override fun onBatch(batch: uniffi.navi.FfiSearchBatch) {
            if (batch.hits.isNotEmpty()) {
                if (firstHitAt.get() < 0L) {
                    firstHitAt.compareAndSet(-1L, SystemClock.elapsedRealtime() - startedAt)
                }
                hits.addAll(batch.hits)
            }
            if (batch.isFinal) done.countDown()
        }
    }

    private data class PolySample(
        val lat: Double,
        val lon: Double,
        val alongKm: Double,
    )

    private fun samplePolylineKm(polyline: String): List<PolySample> {
        val parts = polyline.split(';').map { it.trim() }.filter { it.isNotEmpty() }
        if (parts.size < 2) return emptyList()
        val out = ArrayList<PolySample>(parts.size)
        var along = 0.0
        var prevLat = Double.NaN
        var prevLon = Double.NaN
        for (p in parts) {
            val comma = p.indexOf(',')
            if (comma <= 0) continue
            val lon = p.substring(0, comma).toDoubleOrNull() ?: continue
            val lat = p.substring(comma + 1).toDoubleOrNull() ?: continue
            if (!lat.isFinite() || !lon.isFinite()) continue
            if (prevLat.isFinite()) {
                along += haversineM(prevLat, prevLon, lat, lon) / 1000.0
            }
            out.add(PolySample(lat, lon, along))
            prevLat = lat
            prevLon = lon
        }
        return out
    }

    private fun interpolateAtKm(
        samples: List<PolySample>,
        km: Double,
    ): Pair<Double, Double> {
        if (samples.isEmpty()) return 0.0 to 0.0
        if (km <= samples.first().alongKm) return samples.first().lat to samples.first().lon
        if (km >= samples.last().alongKm) return samples.last().lat to samples.last().lon
        for (i in 1 until samples.size) {
            val a = samples[i - 1]
            val b = samples[i]
            if (km <= b.alongKm) {
                val span = (b.alongKm - a.alongKm).coerceAtLeast(1e-9)
                val t = ((km - a.alongKm) / span).coerceIn(0.0, 1.0)
                return (a.lat + (b.lat - a.lat) * t) to (a.lon + (b.lon - a.lon) * t)
            }
        }
        return samples.last().lat to samples.last().lon
    }

    private fun motorIntervalKm(distanceKm: Double): Double {
        // Soft car-style spacing: ~1.5 h at ~80 km/h ≈ 120 km, clamped for short trips.
        return when {
            distanceKm < 40.0 -> distanceKm
            else -> 120.0.coerceAtMost(distanceKm / 2.0).coerceAtLeast(40.0)
        }
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
