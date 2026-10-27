package no.navi.app

import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.min
import kotlin.math.sin

/**
 * Host-side IMU-assisted dead reckoning for GNSS gaps (tunnels, deep cuts).
 *
 * Not a wasm plugin concern: the host owns map position, guidance, and the
 * LocationManager / SensorManager bridges. Mount calibration (#162) is applied
 * by the caller when building [SensorFrame.headingDeg] /
 * [SensorFrame.forwardAccelMps2].
 *
 * Pure state machine — injectable clock via sample timestamps; no Android types.
 */
enum class DrMode {
    Gps,
    Estimating,
    PositionLost,
}

data class DrConfig(
    /** Enter estimating after this long without a usable fix. */
    val gpsLossTimeoutMs: Long = 2_000L,
    /** Sustained accuracy worse than this counts as degraded GNSS. */
    val gpsDegradeAccuracyM: Float = 40f,
    /** How long accuracy must stay degraded before entering DR. */
    val gpsDegradeHoldMs: Long = 1_500L,
    /** Accuracy at or below this is "good" enough to leave DR / finish blend. */
    val goodAccuracyM: Float = 25f,
    /** Give up after this long without a good fix. */
    val maxDrDurationMs: Long = 120_000L,
    /** Give up after this along-road distance without a good fix. */
    val maxDrDistanceM: Double = 3_000.0,
    /** Smooth hand-back duration when GPS returns with an offset. */
    val blendDurationMs: Long = 800L,
    /** Clamp integrated speed (m/s). */
    val minSpeedMps: Double = 0.5,
    val maxSpeedMps: Double = 50.0,
    /** Forward-accel deadband (m/s^2) to ignore noise. */
    val accelDeadbandMps2: Double = 0.3,
)

data class GpsSample(
    val tMs: Long,
    val lat: Double,
    val lon: Double,
    val accuracyM: Float? = null,
    val speedMps: Double? = null,
    val bearingDeg: Double? = null,
    /** False when the provider reports no fix / unavailable. */
    val available: Boolean = true,
)

/**
 * Vehicle-frame sensor snapshot after mount calibration (or identity if none).
 *
 * [headingDeg] is clockwise from north. [yawRateDegPerS] is used when absolute
 * heading is unavailable (gyro-only). [forwardAccelMps2] positive = accelerate.
 */
data class SensorFrame(
    val tMs: Long,
    val headingDeg: Double? = null,
    val yawRateDegPerS: Double? = null,
    val forwardAccelMps2: Double? = null,
)

/**
 * Read-only dead-reckoning state for diagnostics.
 *
 * [standstill] is null because [DeadReckoningEngine] has no zero-velocity
 * detector. Callers must not treat null as "moving".
 */
data class DrDiagSnapshot(
    val mode: DrMode,
    val seedLat: Double?,
    val seedLon: Double?,
    val seedKind: String,
    val integratedM: Double,
    val speedMps: Double,
    val bearingDeg: Double,
    val alongM: Double,
    val blending: Boolean,
    val standstill: Boolean?,
)

/** Polyline constraint (active route or a single graph-edge chain). */
data class DrConstraint(
    val samples: List<RouteSimSample>,
)

data class DrOutput(
    val mode: DrMode,
    val lat: Double,
    val lon: Double,
    val speedMps: Double,
    val bearingDeg: Double,
    val estimated: Boolean,
    val positionLost: Boolean,
    val alongM: Double? = null,
    /** Set once when a GPS hand-back completes; metres between estimate and return fix. */
    val exitErrorM: Double? = null,
)

class DeadReckoningEngine(
    private val config: DrConfig = DrConfig(),
) {
    private var mode: DrMode = DrMode.Gps
    private var lastGoodGps: GpsSample? = null
    private var lastAnyGps: GpsSample? = null
    private var degradeSinceMs: Long? = null

    private var estLat: Double = 0.0
    private var estLon: Double = 0.0
    private var estSpeedMps: Double = 0.0
    private var estBearingDeg: Double = 0.0
    private var estAlongM: Double = 0.0
    private var drStartedMs: Long = 0L
    private var drDistanceM: Double = 0.0
    private var lastTickMs: Long = 0L

    private var blendFromAlong: Double = 0.0
    private var blendToAlong: Double = 0.0
    private var blendStartMs: Long = 0L
    private var blending: Boolean = false
    private var pendingExitErrorM: Double? = null
    private var lastExitErrorM: Double? = null

    /** Captured at [enterEstimating]. Not a positioning input. */
    private var seedLat: Double? = null
    private var seedLon: Double? = null
    private var seedKind: String = "none"

    fun mode(): DrMode = mode

    /**
     * Read-only snapshot for location diagnostics. Does not alter integration.
     * [DrDiagSnapshot.standstill] is null: this engine has no zero-velocity detector.
     */
    fun diagSnapshot(): DrDiagSnapshot =
        DrDiagSnapshot(
            mode = mode,
            seedLat = seedLat,
            seedLon = seedLon,
            seedKind = seedKind,
            integratedM = drDistanceM,
            speedMps = estSpeedMps,
            bearingDeg = estBearingDeg,
            alongM = estAlongM,
            blending = blending,
            standstill = null,
        )

    fun lastExitErrorM(): Double? = lastExitErrorM

    fun reset() {
        mode = DrMode.Gps
        lastGoodGps = null
        lastAnyGps = null
        degradeSinceMs = null
        estLat = 0.0
        estLon = 0.0
        estSpeedMps = 0.0
        estBearingDeg = 0.0
        estAlongM = 0.0
        drStartedMs = 0L
        drDistanceM = 0.0
        lastTickMs = 0L
        blending = false
        pendingExitErrorM = null
        lastExitErrorM = null
        seedLat = null
        seedLon = null
        seedKind = "none"
    }

    /**
     * Ingest a GNSS sample. Returns the position the map should show (may still
     * be an estimate while blending back).
     */
    fun onGps(
        sample: GpsSample,
        constraint: DrConstraint? = null,
    ): DrOutput {
        lastAnyGps = sample
        val good = isGoodFix(sample)
        if (good) {
            degradeSinceMs = null
            lastGoodGps = sample
            if (sample.speedMps != null && sample.speedMps.isFinite() && sample.speedMps >= 0.0) {
                estSpeedMps = sample.speedMps.coerceIn(config.minSpeedMps, config.maxSpeedMps)
            }
            if (sample.bearingDeg != null && sample.bearingDeg.isFinite()) {
                estBearingDeg = normalizeDeg(sample.bearingDeg)
            }
            when (mode) {
                DrMode.Gps -> {
                    estLat = sample.lat
                    estLon = sample.lon
                    if (constraint != null) {
                        estAlongM = alongOf(constraint, sample.lat, sample.lon)
                    }
                    return output(sample.tMs, exitError = null)
                }
                DrMode.Estimating, DrMode.PositionLost -> {
                    return beginBlend(sample, constraint)
                }
            }
        } else {
            // Degraded or missing: maybe enter DR.
            if (mode == DrMode.Gps) {
                val degraded = isDegraded(sample)
                if (!sample.available || degraded) {
                    val since = degradeSinceMs ?: sample.tMs.also { degradeSinceMs = it }
                    val loss =
                        lastGoodGps?.let { sample.tMs - it.tMs >= config.gpsLossTimeoutMs }
                            ?: (sample.tMs - since >= config.gpsLossTimeoutMs)
                    val hold = sample.tMs - since >= config.gpsDegradeHoldMs
                    if (loss || (degraded && hold)) {
                        enterEstimating(sample.tMs, constraint)
                    }
                } else {
                    degradeSinceMs = null
                }
            }
        }
        return when (mode) {
            DrMode.Gps -> {
                // Still trusting last good / current weak fix for display.
                val lat = if (sample.available) sample.lat else (lastGoodGps?.lat ?: sample.lat)
                val lon = if (sample.available) sample.lon else (lastGoodGps?.lon ?: sample.lon)
                estLat = lat
                estLon = lon
                output(sample.tMs)
            }
            DrMode.Estimating, DrMode.PositionLost -> output(sample.tMs)
        }
    }

    /**
     * Advance the estimate using sensors. Call on a steady cadence while
     * [mode] is [DrMode.Estimating] (and during blend ticks if desired).
     */
    fun onSensorTick(
        frame: SensorFrame,
        constraint: DrConstraint? = null,
    ): DrOutput {
        if (mode == DrMode.Gps && !blending) {
            // Opportunistically keep heading warm from sensors.
            frame.headingDeg?.let { estBearingDeg = normalizeDeg(it) }
            return output(frame.tMs)
        }
        if (mode == DrMode.PositionLost && !blending) {
            return output(frame.tMs)
        }

        val dt = if (lastTickMs > 0L) (frame.tMs - lastTickMs) / 1000.0 else 0.0
        lastTickMs = frame.tMs
        if (dt <= 0.0 || dt > 2.0) {
            return output(frame.tMs)
        }

        // Heading update.
        when {
            frame.headingDeg != null -> estBearingDeg = normalizeDeg(frame.headingDeg)
            frame.yawRateDegPerS != null ->
                estBearingDeg = normalizeDeg(estBearingDeg + frame.yawRateDegPerS * dt)
        }

        // Speed update from forward accel, else carry.
        val accel = frame.forwardAccelMps2
        if (accel != null && kotlin.math.abs(accel) >= config.accelDeadbandMps2) {
            estSpeedMps =
                (estSpeedMps + accel * dt).coerceIn(config.minSpeedMps, config.maxSpeedMps)
        }
        // Prefer planned speed on route when carried speed is tiny and we have constraint.
        if (constraint != null && estSpeedMps < config.minSpeedMps + 0.1) {
            val planned = sampleAtAlong(constraint, estAlongM)?.speedKmh
            if (planned != null && planned > 1.0) {
                estSpeedMps = (planned / 3.6).coerceIn(config.minSpeedMps, config.maxSpeedMps)
            }
        }

        if (blending) {
            return tickBlend(frame.tMs, constraint)
        }

        val ds = estSpeedMps * dt
        drDistanceM += ds
        if (constraint != null && constraint.samples.size >= 2) {
            estAlongM = (estAlongM + ds).coerceAtMost(constraint.samples.last().cumM)
            val p = pointAtAlong(constraint, estAlongM)
            estLat = p.lat
            estLon = p.lon
            estBearingDeg = p.bearingDeg
        } else {
            // Free dead reckoning (last resort — no route / edge polyline).
            val moved = destinationPoint(estLat, estLon, estBearingDeg, ds)
            estLat = moved.first
            estLon = moved.second
        }

        if (shouldGiveUp(frame.tMs)) {
            mode = DrMode.PositionLost
        }
        return output(frame.tMs)
    }

    private fun enterEstimating(
        tMs: Long,
        constraint: DrConstraint?,
    ) {
        val seed = lastGoodGps ?: lastAnyGps ?: return
        mode = DrMode.Estimating
        drStartedMs = tMs
        drDistanceM = 0.0
        lastTickMs = tMs
        seedLat = seed.lat
        seedLon = seed.lon
        seedKind = if (lastGoodGps != null) "last_good_gps" else "last_any_gps"
        estLat = seed.lat
        estLon = seed.lon
        if (seed.speedMps != null && seed.speedMps.isFinite() && seed.speedMps > 0.0) {
            estSpeedMps = seed.speedMps.coerceIn(config.minSpeedMps, config.maxSpeedMps)
        } else if (estSpeedMps <= 0.0) {
            estSpeedMps = config.minSpeedMps
        }
        if (seed.bearingDeg != null) {
            estBearingDeg = normalizeDeg(seed.bearingDeg)
        }
        if (constraint != null) {
            estAlongM = alongOf(constraint, seed.lat, seed.lon)
            val planned = sampleAtAlong(constraint, estAlongM)?.speedKmh
            if (planned != null &&
                planned > 1.0 &&
                (seed.speedMps == null || seed.speedMps <= 0.0)
            ) {
                estSpeedMps = (planned / 3.6).coerceIn(config.minSpeedMps, config.maxSpeedMps)
            }
        }
    }

    private fun beginBlend(
        sample: GpsSample,
        constraint: DrConstraint?,
    ): DrOutput {
        val err =
            RouteProgressTracker.haversineM(estLat, estLon, sample.lat, sample.lon)
        pendingExitErrorM = err
        if (constraint != null && constraint.samples.size >= 2) {
            blendFromAlong = estAlongM
            blendToAlong = alongOf(constraint, sample.lat, sample.lon)
            blendStartMs = sample.tMs
            blending = true
            mode = DrMode.Estimating
            lastTickMs = sample.tMs
            // Seed speed/bearing from returning GPS.
            sample.speedMps?.let {
                if (it > 0.0) estSpeedMps = it.coerceIn(config.minSpeedMps, config.maxSpeedMps)
            }
            sample.bearingDeg?.let { estBearingDeg = normalizeDeg(it) }
            return tickBlend(sample.tMs, constraint)
        }
        // No constraint: snap to GPS (small jump accepted if blend window is zero distance).
        blending = false
        mode = DrMode.Gps
        estLat = sample.lat
        estLon = sample.lon
        lastExitErrorM = err
        pendingExitErrorM = null
        return output(sample.tMs, exitError = err)
    }

    private fun tickBlend(
        tMs: Long,
        constraint: DrConstraint?,
    ): DrOutput {
        if (constraint == null || !blending) {
            blending = false
            mode = DrMode.Gps
            val err = pendingExitErrorM
            pendingExitErrorM = null
            lastExitErrorM = err
            return output(tMs, exitError = err)
        }
        val elapsed = (tMs - blendStartMs).coerceAtLeast(0L)
        val alpha = min(1.0, elapsed.toDouble() / config.blendDurationMs.toDouble())
        estAlongM = blendFromAlong + (blendToAlong - blendFromAlong) * alpha
        val p = pointAtAlong(constraint, estAlongM)
        estLat = p.lat
        estLon = p.lon
        estBearingDeg = p.bearingDeg
        if (alpha >= 1.0) {
            blending = false
            mode = DrMode.Gps
            val err = pendingExitErrorM
            pendingExitErrorM = null
            lastExitErrorM = err
            // Lock to GPS end of blend target.
            estAlongM = blendToAlong
            return output(tMs, exitError = err)
        }
        return output(tMs)
    }

    private fun shouldGiveUp(tMs: Long): Boolean {
        if (mode != DrMode.Estimating) return false
        if (tMs - drStartedMs >= config.maxDrDurationMs) return true
        if (drDistanceM >= config.maxDrDistanceM) return true
        return false
    }

    private fun isGoodFix(sample: GpsSample): Boolean {
        if (!sample.available) return false
        if (sample.lat == 0.0 && sample.lon == 0.0) return false
        val acc = sample.accuracyM ?: return true
        return acc.isFinite() && acc > 0f && acc <= config.goodAccuracyM
    }

    private fun isDegraded(sample: GpsSample): Boolean {
        if (!sample.available) return true
        val acc = sample.accuracyM ?: return false
        return acc.isFinite() && acc > config.gpsDegradeAccuracyM
    }

    private fun output(
        tMs: Long,
        exitError: Double? = null,
    ): DrOutput {
        val estimated = mode == DrMode.Estimating || blending
        val lost = mode == DrMode.PositionLost
        return DrOutput(
            mode = mode,
            lat = estLat,
            lon = estLon,
            speedMps = estSpeedMps,
            bearingDeg = estBearingDeg,
            estimated = estimated && !lost,
            positionLost = lost,
            alongM = estAlongM.takeIf { it > 0.0 || mode != DrMode.Gps },
            exitErrorM = exitError,
        )
    }

    companion object {
        fun normalizeDeg(deg: Double): Double {
            var d = deg % 360.0
            if (d < 0) d += 360.0
            return d
        }

        fun alongOf(
            constraint: DrConstraint,
            lat: Double,
            lon: Double,
        ): Double {
            if (constraint.samples.isEmpty()) return 0.0
            var best = constraint.samples[0]
            var bestD = Double.MAX_VALUE
            for (s in constraint.samples) {
                val d = RouteProgressTracker.haversineM(lat, lon, s.lat, s.lon)
                if (d < bestD) {
                    bestD = d
                    best = s
                }
            }
            return best.cumM
        }

        fun sampleAtAlong(
            constraint: DrConstraint,
            alongM: Double,
        ): RouteSimSample? {
            if (constraint.samples.isEmpty()) return null
            var best = constraint.samples[0]
            var bestD = Double.MAX_VALUE
            for (s in constraint.samples) {
                val d = kotlin.math.abs(s.cumM - alongM)
                if (d < bestD) {
                    bestD = d
                    best = s
                }
            }
            return best
        }

        data class AlongPoint(
            val lat: Double,
            val lon: Double,
            val bearingDeg: Double,
        )

        fun pointAtAlong(
            constraint: DrConstraint,
            alongM: Double,
        ): AlongPoint {
            val samples = constraint.samples
            require(samples.isNotEmpty())
            if (samples.size == 1 || alongM <= samples.first().cumM) {
                val a = samples.first()
                val b = samples.getOrNull(1) ?: a
                return AlongPoint(
                    a.lat,
                    a.lon,
                    RouteProgressTracker.bearingDeg(a.lat, a.lon, b.lat, b.lon),
                )
            }
            if (alongM >= samples.last().cumM) {
                val b = samples.last()
                val a = samples[samples.lastIndex - 1]
                return AlongPoint(
                    b.lat,
                    b.lon,
                    RouteProgressTracker.bearingDeg(a.lat, a.lon, b.lat, b.lon),
                )
            }
            for (i in 0 until samples.lastIndex) {
                val a = samples[i]
                val b = samples[i + 1]
                if (alongM >= a.cumM && alongM <= b.cumM) {
                    val span = (b.cumM - a.cumM).coerceAtLeast(1e-6)
                    val t = ((alongM - a.cumM) / span).coerceIn(0.0, 1.0)
                    return AlongPoint(
                        a.lat + (b.lat - a.lat) * t,
                        a.lon + (b.lon - a.lon) * t,
                        RouteProgressTracker.bearingDeg(a.lat, a.lon, b.lat, b.lon),
                    )
                }
            }
            val last = samples.last()
            return AlongPoint(last.lat, last.lon, 0.0)
        }

        /** Destination from (lat,lon) travelling [distanceM] along [bearingDeg]. */
        fun destinationPoint(
            lat: Double,
            lon: Double,
            bearingDeg: Double,
            distanceM: Double,
        ): Pair<Double, Double> {
            val r = 6_378_100.0
            val br = Math.toRadians(bearingDeg)
            val lat1 = Math.toRadians(lat)
            val lon1 = Math.toRadians(lon)
            val ang = distanceM / r
            val lat2 =
                asin(
                    sin(lat1) * cos(ang) + cos(lat1) * sin(ang) * cos(br),
                )
            val lon2 =
                lon1 +
                    atan2(
                        sin(br) * sin(ang) * cos(lat1),
                        cos(ang) - sin(lat1) * sin(lat2),
                    )
            return Math.toDegrees(lat2) to Math.toDegrees(lon2)
        }

        private fun asin(x: Double): Double = kotlin.math.asin(x.coerceIn(-1.0, 1.0))
    }
}
