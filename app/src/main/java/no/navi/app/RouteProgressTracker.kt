package no.navi.app

import uniffi.navi.approachHideM
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.sin
import kotlin.math.sqrt

data class RouteProgressSnapshot(
    val alongM: Double,
    val distanceToManeuverM: Double,
    val maneuverIndex: Int,
    val maneuver: RouteManeuver?,
    val sample: RouteSimSample?,
    val remainingEtaMinutes: Double,
    /**
     * Planned driving hours from route start to the snapped sample, integrating
     * each segment at that sample's speed (same method as remaining ETA).
     * Used for break countdown — never distance / instantaneous speed alone.
     */
    val elapsedDrivingHours: Double,
    val viaIndexReached: Int,
    val arrivedAtEnd: Boolean,
    val bearingDeg: Double,
)

/**
 * Snap GPS (or simulated) position onto the planned sample chain and derive
 * next-maneuver distance, via/end arrival, and remaining ETA.
 *
 * [hideDistanceM] is metres past which a maneuver is treated as passed (approach
 * box hide). Default is UniFFI [approachHideM] — same value as Rust
 * `APPROACH_HIDE_M` / docs/approach-instructions.md (25 m). Do not hardcode
 * a second magic number here.
 */
class RouteProgressTracker(
    private val samples: List<RouteSimSample>,
    private val maneuvers: List<RouteManeuver>,
    private val viaPoints: List<Waypoint>,
    private val endPoint: Waypoint,
    private val viaRadiusM: Double = 80.0,
    private val endRadiusM: Double = 60.0,
    private val hideDistanceM: Double = approachHideM(),
) {
    private var maneuverCursor = 0
    private var viaReached = -1

    fun reset() {
        maneuverCursor = 0
        viaReached = -1
    }

    fun update(
        lat: Double,
        lon: Double,
    ): RouteProgressSnapshot {
        if (samples.isEmpty()) {
            return RouteProgressSnapshot(
                alongM = 0.0,
                distanceToManeuverM = Double.POSITIVE_INFINITY,
                maneuverIndex = -1,
                maneuver = null,
                sample = null,
                remainingEtaMinutes = 0.0,
                elapsedDrivingHours = 0.0,
                viaIndexReached = viaReached,
                arrivedAtEnd = false,
                bearingDeg = 0.0,
            )
        }
        val nearest = nearestSampleIndex(lat, lon)
        val sample = samples[nearest]
        val along = sample.cumM
        // Advance past maneuvers within hide distance (metres).
        while (maneuverCursor < maneuvers.size) {
            val m = maneuvers[maneuverCursor]
            if (along + hideDistanceM >= m.cumM) {
                maneuverCursor++
            } else {
                break
            }
        }
        val man = maneuvers.getOrNull(maneuverCursor)
        val distToMan = if (man != null) (man.cumM - along).coerceAtLeast(0.0) else Double.POSITIVE_INFINITY

        for (i in viaPoints.indices) {
            if (i <= viaReached) continue
            val v = viaPoints[i]
            if (haversineM(lat, lon, v.lat, v.lon) <= viaRadiusM) {
                viaReached = i
            }
        }
        val arrived =
            haversineM(lat, lon, endPoint.lat, endPoint.lon) <= endRadiusM ||
                along >= (samples.last().cumM - endRadiusM)

        val bearing =
            if (nearest + 1 < samples.size) {
                bearingDeg(sample.lat, sample.lon, samples[nearest + 1].lat, samples[nearest + 1].lon)
            } else if (nearest > 0) {
                bearingDeg(samples[nearest - 1].lat, samples[nearest - 1].lon, sample.lat, sample.lon)
            } else {
                0.0
            }

        return RouteProgressSnapshot(
            alongM = along,
            distanceToManeuverM = distToMan,
            maneuverIndex = maneuverCursor,
            maneuver = man,
            sample = sample,
            remainingEtaMinutes = remainingEtaMinutes(nearest),
            elapsedDrivingHours = elapsedDrivingHours(nearest),
            viaIndexReached = viaReached,
            arrivedAtEnd = arrived,
            bearingDeg = bearing,
        )
    }

    /** Hours to drive from sample 0 through [toIdx] at each segment's planned speed. */
    private fun elapsedDrivingHours(toIdx: Int): Double {
        if (toIdx <= 0 || samples.size < 2) return 0.0
        var hours = 0.0
        val end = toIdx.coerceAtMost(samples.lastIndex)
        for (i in 0 until end) {
            val a = samples[i]
            val b = samples[i + 1]
            val dm = (b.cumM - a.cumM).coerceAtLeast(0.0)
            val speed = a.speedKmh.coerceAtLeast(1.0)
            hours += (dm / 1000.0) / speed
        }
        return hours
    }

    private fun remainingEtaMinutes(fromIdx: Int): Double {
        var hours = 0.0
        for (i in fromIdx until samples.lastIndex) {
            val a = samples[i]
            val b = samples[i + 1]
            val dm = (b.cumM - a.cumM).coerceAtLeast(0.0)
            val speed = a.speedKmh.coerceAtLeast(1.0)
            hours += (dm / 1000.0) / speed
        }
        return hours * 60.0
    }

    private fun nearestSampleIndex(
        lat: Double,
        lon: Double,
    ): Int {
        var best = 0
        var bestD = Double.MAX_VALUE
        for (i in samples.indices) {
            val s = samples[i]
            val d = haversineM(lat, lon, s.lat, s.lon)
            if (d < bestD) {
                bestD = d
                best = i
            }
        }
        return best
    }

    companion object {
        fun haversineM(
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
            val h =
                sin(dp / 2) * sin(dp / 2) +
                    cos(p1) * cos(p2) * sin(dl / 2) * sin(dl / 2)
            return 2 * r * atan2(sqrt(h), sqrt(1 - h))
        }

        fun bearingDeg(
            lat1: Double,
            lon1: Double,
            lat2: Double,
            lon2: Double,
        ): Double {
            val p1 = Math.toRadians(lat1)
            val p2 = Math.toRadians(lat2)
            val dl = Math.toRadians(lon2 - lon1)
            val y = sin(dl) * cos(p2)
            val x = cos(p1) * sin(p2) - sin(p1) * cos(p2) * cos(dl)
            var brng = Math.toDegrees(atan2(y, x))
            if (brng < 0) brng += 360.0
            return brng
        }
    }
}
