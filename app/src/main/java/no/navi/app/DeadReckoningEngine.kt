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
    /**
     * Give up after this long without a good fix.
     * A 3 km tunnel at 50 km/h takes about 216 s. 360 s leaves 144 s (40%)
     * so the estimate is still running at the exit. The previous 120 s
     * froze the puck inside that tunnel and was the steady-cruise lag.
     */
    val maxDrDurationMs: Long = 360_000L,
    /**
     * Give up after this along-road distance without a good fix.
     * 15 km is five times the 3 km simulated tunnel.
     */
    val maxDrDistanceM: Double = 15_000.0,
    /** Smooth hand-back duration when GPS returns with an offset. */
    val blendDurationMs: Long = 800L,
    /** Clamp integrated speed (m/s). */
    val minSpeedMps: Double = 0.5,
    val maxSpeedMps: Double = 50.0,
    /**
     * |low-passed longitudinal accel| at or above this changes speed (m/s²).
     * 0.20 is ten times a ±0.02 m/s² bias and above a zero-mean cabin
     * vibration of 0.40 m/s² RMS after the 0.5 s smoother. A sustained
     * 0.30 m/s² pull-away sits 0.10 above it.
     */
    val accelDeadbandMps2: Double = 0.20,
    /**
     * Smoother for longitudinal accel before the deadband (seconds).
     * Zero-mean vibration averages out; a real pull or brake remains.
     */
    val longitudinalFilterTauS: Double = 0.5,
    /**
     * |forward accel| below this is still (m/s^2).
     * The indoor tablet log stayed under 0.003 m/s^2 mean and 0.04 m/s^2 std.
     * 0.15 is above that noise and below the 0.3 integration deadband, so a
     * resting device latches and a real acceleration still counts as motion.
     */
    val standstillAccelMps2: Double = 0.15,
    /**
     * |yaw rate| below this is still (deg/s).
     * Indoor gyro noise was about 0.06 deg/s std. 2 deg/s sits above that and
     * below a deliberate turn.
     */
    val standstillGyroDegPerS: Double = 2.0,
    /**
     * Quiet IMU must last this long after a braking event before the
     * zero-velocity gate latches. A reported GNSS speed of 0 still zeros
     * immediately. Quiet without braking never latches (steady cruise).
     */
    val standstillHoldMs: Long = 1_000L,
    /**
     * After braking, estimated speed must fall below this before quiet can
     * latch. 0.5 m/s is below a 3–5 km/h crawl so a crawl does not latch.
     */
    val standstillSpeedMps: Double = 0.5,
    /**
     * Forward accel (device frame, after [forwardSign]) at or below this
     * counts as braking (m/s²). −0.75 sits below the strongest cabin-vibration
     * step (0.40 m/s² RMS) and above a light brake (−1.5 to −3); margin to
     * vib 0.40 is about 0.25 m/s².
     */
    val brakeAccelMps2: Double = -0.75,
    /**
     * Time constant for the logged speed uncertainty after GNSS loss
     * (seconds). Uncertainty grows as exp(age / τ). It is never applied to
     * the speed. 8 s matches the previous trust constant and sits above the
     * 2 s loss timeout.
     */
    val speedTrustTauS: Double = 8.0,
    /**
     * Smoothed forward accel at or above this can release standstill (m/s²).
     * Release also requires the raw sample to stay above [accelDeadbandMps2]
     * for [pullAwaySustainMs], so a spike or a zero-mean idle does not clear
     * the latch. 0.22 is below a sustained 0.30 m/s² pull.
     */
    val pullAwayAccelMps2: Double = 0.22,
    /**
     * How long the smoothed accel must stay above [pullAwayAccelMps2], with
     * the raw sample still above the deadband, before the latch releases.
     * 200 ms keeps a 0.30 m/s² pull inside the 1 s limit (the smoother
     * crosses 0.22 at about 0.8 s) and rejects a single slam or a 200 ms rev.
     */
    val pullAwaySustainMs: Long = 200L,
    /**
     * Time constant for the forward-axis accelerometer bias (seconds).
     * Slow enough that cabin vibration averages out on the approach, fast
     * enough that a ±0.1 m/s² bias is learned before a tunnel a kilometre ahead.
     */
    val accelBiasTauS: Double = 4.0,
    /**
     * |IMU − reference| above this is not a bias observation (m/s²).
     * A door slam or a one-sample GNSS lag sits above it; a ±0.1 bias does not.
     */
    val accelBiasOutlierMps2: Double = 1.0,
    /**
     * While the estimate is still on a tunnel span of the active route, give-up
     * waits until along-track has passed that span's end by this much (metres).
     * Outside any tunnel the 6 min / 15 km limits apply. No route means no
     * tunnel span, so those limits apply immediately.
     */
    val tunnelExitMarginM: Double = 300.0,
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
    /**
     * Vehicle-longitudinal accel when the caller already applied mount
     * calibration. Ignored for speed and braking once a device-frame fit
     * is in use.
     */
    val forwardAccelMps2: Double? = null,
    /** Device-frame linear acceleration (gravity already removed), m/s². */
    val linAccelX: Double? = null,
    val linAccelY: Double? = null,
    val linAccelZ: Double? = null,
    /** Device-frame specific force of gravity, m/s². Null skips the projection. */
    val gravityX: Double? = null,
    val gravityY: Double? = null,
    val gravityZ: Double? = null,
)

/**
 * Read-only dead-reckoning state for diagnostics.
 *
 * [standstill] is true while the zero-velocity gate is holding the position.
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
    /**
     * Growth of speed doubt after GNSS loss. Logged only; never subtracted
     * from [speedMps].
     */
    val speedUncertaintyMps: Double = 0.0,
    /**
     * `scalar` — caller already supplied longitudinal accel.
     * `learning` — device-frame samples seen, fit not confident yet, so
     * speed is held and braking is not detected.
     * `confident` — fitted forward axis is in use.
     */
    val forwardAxisState: String = "scalar",
    /**
     * Estimated accelerometer bias along the forward axis (m/s²).
     * Subtracted before the deadband and before integration.
     */
    val accelBiasMps2: Double = 0.0,
    /** 0 until bias observations accumulate, approaching 1. */
    val accelBiasConfidence: Double = 0.0,
    /**
     * `linear`, `accel_minus_gravity`, `accel_minus_lowpass`, `scalar`, or `none`.
     * Set by the controller. The engine does not choose the sensor.
     */
    val linearAccelSource: String = "scalar",
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

    /**
     * Last GNSS speed that was actually reported. Null means the speed was
     * missing (or the report aged past [DrConfig.gpsLossTimeoutMs]), which is
     * the only case that may adopt the planned route speed as a fallback.
     */
    private var lastGnssSpeedMps: Double? = null
    private var lastGnssReportMs: Long = 0L
    private var quietSinceMs: Long? = null
    private var standstill: Boolean = false
    private var imuMissing: Boolean = true
    private var imuMotion: Boolean = false

    /** True after a braking event since the current GNSS-loss / DR entry. */
    private var brakeSeenSinceLoss: Boolean = false

    /**
     * Fitted vehicle-forward unit vector in the device frame, projected onto
     * the horizontal plane. Learned from GNSS Δv/Δt while fixes are good and
     * kept across DR entry. Reset only in [reset] (a new mount session).
     */
    private var fwdX: Double = 0.0
    private var fwdY: Double = 1.0
    private var fwdZ: Double = 0.0
    private var axisSumX: Double = 0.0
    private var axisSumY: Double = 0.0
    private var axisSumZ: Double = 0.0
    private var axisAccelCount: Int = 0
    private var axisBrakeCount: Int = 0
    private var forwardAxisConfident: Boolean = false
    private var sawDeviceAccel: Boolean = false
    private var lastHorizX: Double? = null
    private var lastHorizY: Double? = null
    private var lastHorizZ: Double? = null
    private var prevGnssSpeedMps: Double? = null
    private var prevGnssSpeedMs: Long = 0L
    private var lastGnssAccelMps2: Double? = null
    private var filteredLongAccel: Double = 0.0
    private var speedUncertaintyMps: Double = 0.0
    private var pullSustainSinceMs: Long? = null
    private var accelBiasMps2: Double = 0.0
    private var accelBiasConfidence: Double = 0.0
    private var biasObsCount: Int = 0
    private var lastBiasMs: Long = 0L
    private var linearAccelSource: String = "scalar"

    fun mode(): DrMode = mode

    /**
     * Test/diagnostics. +1 when the fitted horizontal forward has a positive
     * device-Y component (portrait, yaw near 0), −1 otherwise. 0 before a fit.
     */
    fun forwardSign(): Double =
        if (!forwardAxisConfident) {
            0.0
        } else if (fwdY >= 0.0) {
            1.0
        } else {
            -1.0
        }

    /** `scalar`, `learning`, or `confident`. See [DrDiagSnapshot.forwardAxisState]. */
    fun forwardAxisState(): String =
        when {
            forwardAxisConfident -> "confident"
            sawDeviceAccel -> "learning"
            else -> "scalar"
        }

    fun forwardAxisConfident(): Boolean = forwardAxisConfident

    /** Horizontal forward unit vector in the device frame, or null while learning. */
    fun forwardAxisDevice(): DoubleArray? = if (forwardAxisConfident) doubleArrayOf(fwdX, fwdY, fwdZ) else null

    /** Test/diagnostics: whether a braking event has been seen since DR entry. */
    fun brakeSeenSinceLoss(): Boolean = brakeSeenSinceLoss

    /** Read-only snapshot for location diagnostics. Does not alter integration. */
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
            standstill = standstill,
            speedUncertaintyMps = speedUncertaintyMps,
            forwardAxisState = forwardAxisState(),
            accelBiasMps2 = accelBiasMps2,
            accelBiasConfidence = accelBiasConfidence,
            linearAccelSource = linearAccelSource,
        )

    /** Controller reports which accelerometer path built the device vector. */
    fun noteLinearAccelSource(source: String) {
        linearAccelSource = source
    }

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
        lastGnssSpeedMps = null
        lastGnssReportMs = 0L
        quietSinceMs = null
        standstill = false
        imuMissing = true
        imuMotion = false
        brakeSeenSinceLoss = false
        fwdX = 0.0
        fwdY = 1.0
        fwdZ = 0.0
        axisSumX = 0.0
        axisSumY = 0.0
        axisSumZ = 0.0
        axisAccelCount = 0
        axisBrakeCount = 0
        forwardAxisConfident = false
        sawDeviceAccel = false
        lastHorizX = null
        lastHorizY = null
        lastHorizZ = null
        prevGnssSpeedMps = null
        prevGnssSpeedMs = 0L
        lastGnssAccelMps2 = null
        filteredLongAccel = 0.0
        speedUncertaintyMps = 0.0
        pullSustainSinceMs = null
        accelBiasMps2 = 0.0
        accelBiasConfidence = 0.0
        biasObsCount = 0
        lastBiasMs = 0L
        linearAccelSource = "scalar"
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
            lastGnssReportMs = sample.tMs
            if (sample.speedMps != null && sample.speedMps.isFinite() && sample.speedMps >= 0.0) {
                applyReportedSpeed(sample.speedMps)
                updateGnssAccel(sample)
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
                // An available portal fix (accuracy between good and degraded)
                // moves along-track too, so the puck is not stuck at the last
                // good sample while lat/lon have already moved.
                val lat = if (sample.available) sample.lat else (lastGoodGps?.lat ?: sample.lat)
                val lon = if (sample.available) sample.lon else (lastGoodGps?.lon ?: sample.lon)
                estLat = lat
                estLon = lon
                if (
                    sample.available &&
                    constraint != null &&
                    constraint.samples.size >= 2 &&
                    !(lat == 0.0 && lon == 0.0)
                ) {
                    estAlongM = alongOf(constraint, lat, lon)
                }
                output(sample.tMs)
            }
            DrMode.Estimating, DrMode.PositionLost -> output(sample.tMs)
        }
    }

    /**
     * A silent provider never delivers an unavailable fix. Once the last good
     * fix is older than [DrConfig.gpsLossTimeoutMs], treat that silence as
     * loss so dead reckoning can start. A fix newer than the timeout is a
     * no-op. No fix yet is a no-op.
     */
    fun onGnssSilence(tMs: Long) {
        if (mode != DrMode.Gps || blending) return
        val last = lastGoodGps ?: return
        if (tMs - last.tMs < config.gpsLossTimeoutMs) return
        onGps(
            GpsSample(
                tMs = tMs,
                lat = 0.0,
                lon = 0.0,
                available = false,
            ),
        )
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
            // Opportunistically keep heading warm and learn the forward axis.
            frame.headingDeg?.let { estBearingDeg = normalizeDeg(it) }
            observeForwardAxis(frame)
            learnAccelBias(frame)
            // The loss timeout has not fired yet. Hold the last GNSS speed
            // and keep moving, instead of freezing until Estimating starts.
            if (degradeSinceMs != null) {
                coastAtHeldSpeed(frame, constraint)
            }
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

        // Mark braking from this frame before integrating so the speed update
        // and the latch see the same brake/speed state. Bias is updated first
        // so the correction uses the latest estimate.
        learnAccelBias(frame)
        noteBraking(frame)
        smoothLongitudinal(frame, dt)
        val reportedStop = lastGnssSpeedMps != null && lastGnssSpeedMps!! <= 0.0
        if (standstill || reportedStop) {
            // A reported 0, or the zero-velocity gate, is 0. Do not raise it to
            // minSpeedMps and do not replace it with the planned route speed.
            estSpeedMps = 0.0
            noteSpeedUncertainty(frame.tMs)
        } else {
            updateSpeedFromImu(frame, dt, constraint)
        }
        observeImu(frame)

        if (blending) {
            return tickBlend(frame.tMs, constraint)
        }
        if (standstill || reportedStop) {
            return output(frame.tMs)
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

        if (shouldGiveUp(frame.tMs, constraint)) {
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
        // Position is already the last fix, plus any coast during the loss
        // timeout. Do not rewind to the stale fix.
        seedLat = estLat
        seedLon = estLon
        seedKind = if (lastGoodGps != null) "last_good_gps" else "last_any_gps"
        quietSinceMs = null
        standstill = false
        filteredLongAccel = 0.0
        // The fitted forward axis belongs to the mount session. Do not reset
        // it here — a gap must keep the axis learned on the way in.
        if (seed.speedMps != null && seed.speedMps.isFinite() && seed.speedMps > 0.0) {
            lastGnssSpeedMps = seed.speedMps
            lastGnssReportMs = seed.tMs
            // Constant-velocity seed. Later ticks change it only by integrating
            // longitudinal accel above the deadband.
            estSpeedMps = seed.speedMps.coerceAtMost(config.maxSpeedMps).coerceAtLeast(0.0)
            brakeSeenSinceLoss = estSpeedMps <= config.standstillSpeedMps
        } else if (seed.speedMps != null && seed.speedMps.isFinite()) {
            lastGnssSpeedMps = 0.0
            lastGnssReportMs = seed.tMs
            estSpeedMps = 0.0
            // Already stopped at entry — treat as braking satisfied.
            brakeSeenSinceLoss = true
        } else {
            // Speed was not reported. Leave 0 here; a later tick may adopt the
            // planned speed only when no IMU is present.
            lastGnssSpeedMps = null
            estSpeedMps = 0.0
            brakeSeenSinceLoss = true
        }
        if (seed.bearingDeg != null) {
            estBearingDeg = normalizeDeg(seed.bearingDeg)
        }
        if (constraint != null && estAlongM <= 0.0 && constraint.samples.size >= 2) {
            estAlongM = alongOf(constraint, estLat, estLon)
        }
    }

    /**
     * Move at the held GNSS speed while a loss is pending and [mode] is still
     * [DrMode.Gps]. Speed is not changed here.
     */
    private fun coastAtHeldSpeed(
        frame: SensorFrame,
        constraint: DrConstraint?,
    ) {
        val dt = if (lastTickMs > 0L) (frame.tMs - lastTickMs) / 1000.0 else 0.0
        lastTickMs = frame.tMs
        if (dt <= 0.0 || dt > 2.0 || estSpeedMps <= 0.0) return
        val ds = estSpeedMps * dt
        if (constraint != null && constraint.samples.size >= 2) {
            estAlongM = (estAlongM + ds).coerceAtMost(constraint.samples.last().cumM)
            val p = pointAtAlong(constraint, estAlongM)
            estLat = p.lat
            estLon = p.lon
            estBearingDeg = p.bearingDeg
        }
    }

    /** A reported speed of 0 stays 0. A positive speed may release standstill. */
    private fun applyReportedSpeed(speed: Double) {
        lastGnssSpeedMps = speed
        lastGnssReportMs = lastGnssReportMs // kept by caller; also set in onGps
        if (speed <= 0.0) {
            estSpeedMps = 0.0
            brakeSeenSinceLoss = true
            return
        }
        estSpeedMps = speed.coerceIn(config.minSpeedMps, config.maxSpeedMps)
        if (speed > config.minSpeedMps) {
            standstill = false
            quietSinceMs = null
            brakeSeenSinceLoss = false
        }
    }

    /**
     * While GNSS is available, longitudinal accel from successive speeds is
     * paired with the latest horizontal device acceleration to fit the
     * vehicle forward axis. Both speeding up and braking are required.
     */
    private fun updateGnssAccel(sample: GpsSample) {
        val v = sample.speedMps ?: return
        val prev = prevGnssSpeedMps
        val prevT = prevGnssSpeedMs
        if (prev != null && sample.tMs > prevT) {
            val dt = (sample.tMs - prevT) / 1000.0
            if (dt in 0.05..3.0) {
                lastGnssAccelMps2 = (v - prev) / dt
                val hx = lastHorizX
                val hy = lastHorizY
                val hz = lastHorizZ
                if (hx != null && hy != null && hz != null) {
                    accumulateForwardAxis(lastGnssAccelMps2!!, doubleArrayOf(hx, hy, hz))
                }
            }
        }
        prevGnssSpeedMps = v
        prevGnssSpeedMs = sample.tMs
    }

    /**
     * Store the gravity-free horizontal acceleration. Does not reset a fit
     * that is already confident.
     */
    private fun observeForwardAxis(frame: SensorFrame) {
        val horiz = horizontalDeviceAccel(frame) ?: return
        sawDeviceAccel = true
        lastHorizX = horiz[0]
        lastHorizY = horiz[1]
        lastHorizZ = horiz[2]
    }

    /**
     * Linear acceleration with gravity removed, then projected onto the plane
     * perpendicular to gravity. Without a gravity vector, X/Y are treated as
     * already horizontal (the linear-acceleration path has removed gravity).
     */
    private fun horizontalDeviceAccel(frame: SensorFrame): DoubleArray? {
        val ax = frame.linAccelX ?: return null
        val ay = frame.linAccelY ?: return null
        val az = frame.linAccelZ ?: 0.0
        val gx = frame.gravityX
        val gy = frame.gravityY
        val gz = frame.gravityZ
        if (gx == null || gy == null || gz == null) {
            return doubleArrayOf(ax, ay, 0.0)
        }
        val g2 = gx * gx + gy * gy + gz * gz
        if (g2 < 1.0) return doubleArrayOf(ax, ay, 0.0)
        val inv = 1.0 / kotlin.math.sqrt(g2)
        val nx = gx * inv
        val ny = gy * inv
        val nz = gz * inv
        val dot = ax * nx + ay * ny + az * nz
        return doubleArrayOf(ax - dot * nx, ay - dot * ny, az - dot * nz)
    }

    private fun accumulateForwardAxis(
        aGnss: Double,
        horiz: DoubleArray,
    ) {
        val mag =
            kotlin.math.sqrt(
                horiz[0] * horiz[0] + horiz[1] * horiz[1] + horiz[2] * horiz[2],
            )
        if (kotlin.math.abs(aGnss) < 0.5 || mag < 0.2) return
        val sign = if (aGnss >= 0.0) 1.0 else -1.0
        axisSumX += sign * horiz[0] / mag
        axisSumY += sign * horiz[1] / mag
        axisSumZ += sign * horiz[2] / mag
        if (aGnss >= 0.5) axisAccelCount++ else axisBrakeCount++
        val n = axisAccelCount + axisBrakeCount
        val sm =
            kotlin.math.sqrt(
                axisSumX * axisSumX + axisSumY * axisSumY + axisSumZ * axisSumZ,
            )
        // Both signs, and the samples must agree (mean resultant ≥ 0.75).
        if (axisAccelCount >= 3 && axisBrakeCount >= 3 && sm / n >= 0.75) {
            val inv = 1.0 / sm
            fwdX = axisSumX * inv
            fwdY = axisSumY * inv
            fwdZ = axisSumZ * inv
            forwardAxisConfident = true
        }
    }

    /**
     * Longitudinal accel in the vehicle frame, or null when a device-frame
     * fit is still learning (speed then holds, and braking is not detected).
     * A scalar [SensorFrame.forwardAccelMps2] is used only when no device
     * vector is present — the caller already resolved the mount.
     */
    private fun longitudinalAccel(frame: SensorFrame): Double? {
        val horiz = horizontalDeviceAccel(frame)
        if (horiz != null) {
            if (!forwardAxisConfident) return null
            return horiz[0] * fwdX + horiz[1] * fwdY + horiz[2] * fwdZ
        }
        return frame.forwardAccelMps2
    }

    /** Longitudinal accel with the learned bias removed. Null while the axis is learning. */
    private fun correctedLongitudinal(frame: SensorFrame): Double? {
        val raw = longitudinalAccel(frame) ?: return null
        return raw - accelBiasMps2
    }

    /**
     * Bias along the forward axis. While GNSS is good the reference is Δv/Δt.
     * While standstill is latched the reference is zero. A large residual is
     * a spike, not a bias, and is ignored. The estimate is kept across DR entry.
     */
    private fun learnAccelBias(frame: SensorFrame) {
        val raw = longitudinalAccel(frame) ?: return
        val dt =
            if (lastBiasMs > 0L) {
                ((frame.tMs - lastBiasMs) / 1000.0).coerceIn(0.0, 1.0)
            } else {
                0.1
            }
        lastBiasMs = frame.tMs
        if (dt <= 0.0) return
        val gnssAccel = lastGnssAccelMps2
        val gnssFresh =
            mode == DrMode.Gps &&
                degradeSinceMs == null &&
                gnssAccel != null &&
                kotlin.math.abs(gnssAccel) < 0.8 &&
                frame.tMs - prevGnssSpeedMs <= 1_500L
        val reference =
            when {
                // A latched stop has true accel 0, but only the quiet samples
                // are bias. A pull-away is real motion and must not be absorbed.
                standstill && kotlin.math.abs(raw) <= config.accelDeadbandMps2 -> 0.0
                gnssFresh -> lastGnssAccelMps2
                else -> null
            } ?: return
        val err = raw - reference
        if (kotlin.math.abs(err) > config.accelBiasOutlierMps2) return
        val tau = config.accelBiasTauS.coerceAtLeast(0.5)
        val alpha = (dt / (tau + dt)).coerceIn(0.0, 1.0)
        accelBiasMps2 += alpha * (err - accelBiasMps2)
        biasObsCount += 1
        accelBiasConfidence =
            (1.0 - kotlin.math.exp(-biasObsCount / 40.0)).coerceIn(0.0, 1.0)
    }

    private fun smoothLongitudinal(
        frame: SensorFrame,
        dt: Double,
    ) {
        val raw = correctedLongitudinal(frame) ?: return
        val tau = config.longitudinalFilterTauS.coerceAtLeast(0.05)
        val alpha = (dt / (tau + dt)).coerceIn(0.0, 1.0)
        filteredLongAccel += alpha * (raw - filteredLongAccel)
    }

    private fun noteSpeedUncertainty(tMs: Long) {
        if (drStartedMs <= 0L) return
        val ageS = ((tMs - drStartedMs).coerceAtLeast(0L)) / 1000.0
        speedUncertaintyMps =
            (0.5 * kotlin.math.exp(ageS / config.speedTrustTauS)).coerceAtMost(50.0)
    }

    /**
     * Constant-velocity speed. Holds the GNSS seed. Changes only when both the
     * current sample and the low-pass gate are outside [DrConfig.accelDeadbandMps2].
     * Planned route speed is a fallback only when no IMU exists and no GNSS
     * speed was ever reported. It never pulls the estimate toward itself.
     */
    private fun updateSpeedFromImu(
        frame: SensorFrame,
        dt: Double,
        constraint: DrConstraint?,
    ) {
        noteSpeedUncertainty(frame.tMs)
        val raw = correctedLongitudinal(frame)
        val noDevice =
            raw == null &&
                frame.forwardAccelMps2 == null &&
                frame.linAccelX == null &&
                frame.yawRateDegPerS == null
        if (noDevice) {
            if (
                constraint != null &&
                lastGnssSpeedMps == null &&
                estSpeedMps < config.minSpeedMps + 0.1
            ) {
                val planned = sampleAtAlong(constraint, estAlongM)?.speedKmh
                if (planned != null && planned > 1.0) {
                    estSpeedMps = (planned / 3.6).coerceIn(config.minSpeedMps, config.maxSpeedMps)
                }
            }
            return
        }
        if (raw == null) {
            // Learning a device-frame axis, or no longitudinal sample: hold.
            return
        }
        // The smoother is only a gate. Once it is open, integrate the current
        // sample so a lagged average cannot keep braking after the taper.
        // A sample inside the deadband never changes speed, even if the
        // smoother has not decayed yet.
        if (
            kotlin.math.abs(raw) >= config.accelDeadbandMps2 &&
            kotlin.math.abs(filteredLongAccel) >= config.accelDeadbandMps2
        ) {
            estSpeedMps =
                (estSpeedMps + raw * dt)
                    .coerceAtLeast(0.0)
                    .coerceAtMost(config.maxSpeedMps)
        }
        // Missing GNSS speed plus real motion: planned speed is the fallback
        // and the cap. A GNSS-seeded speed is not pulled toward the plan.
        if (lastGnssSpeedMps == null && kotlin.math.abs(filteredLongAccel) >= config.accelDeadbandMps2) {
            val planned =
                constraint?.let { sampleAtAlong(it, estAlongM)?.speedKmh }?.let { it / 3.6 }
            if (planned != null && planned > 1.0 && estSpeedMps > planned) {
                estSpeedMps = planned
            } else if (
                planned != null &&
                planned > 1.0 &&
                estSpeedMps < config.minSpeedMps
            ) {
                estSpeedMps = planned.coerceAtMost(config.maxSpeedMps)
            }
        }
    }

    private fun noteBraking(frame: SensorFrame) {
        val accel = correctedLongitudinal(frame) ?: return
        if (accel <= config.brakeAccelMps2) {
            brakeSeenSinceLoss = true
            return
        }
        // A slow-down that settles into a crawl (above latch speed, quiet accel)
        // is not a stop — clear the brake flag so we do not ZUPT mid-crawl.
        if (
            brakeSeenSinceLoss &&
            !standstill &&
            estSpeedMps > config.standstillSpeedMps &&
            kotlin.math.abs(accel) < config.standstillAccelMps2
        ) {
            brakeSeenSinceLoss = false
            quietSinceMs = null
        }
    }

    /**
     * Braking-aware standstill: latch only after a braking event has brought
     * estimated speed below [DrConfig.standstillSpeedMps], followed by IMU
     * quiet for the hold time. Quiet alone never latches (smooth cruise).
     * Missing IMU does not latch and does not clear an existing latch.
     * Release on IMU motion above threshold; speed then restarts from 0.
     */
    private fun observeImu(frame: SensorFrame) {
        observeForwardAxis(frame)
        val accel = correctedLongitudinal(frame)
        val gyro = frame.yawRateDegPerS
        if (accel == null && gyro == null) {
            imuMissing = true
            imuMotion = false
            quietSinceMs = null
            return
        }
        imuMissing = false

        // Release only while smoothed accel stays above the pull threshold and
        // the raw sample stays above the deadband, for [DrConfig.pullAwaySustainMs].
        // A door slam, a short rev, or zero-mean idle fails the sustain. A
        // 0.30 m/s² pull holds both and still releases inside 1 s.
        val directional =
            accel != null &&
                accel >= config.accelDeadbandMps2 &&
                filteredLongAccel >= config.pullAwayAccelMps2
        if (directional) {
            if (pullSustainSinceMs == null) pullSustainSinceMs = frame.tMs
        } else {
            pullSustainSinceMs = null
        }
        val sustained =
            pullSustainSinceMs != null &&
                frame.tMs - pullSustainSinceMs!! >= config.pullAwaySustainMs
        val pullAway =
            sustained ||
                (gyro != null && kotlin.math.abs(gyro) >= config.standstillGyroDegPerS * 4.0)
        if (standstill) {
            if (pullAway) {
                imuMotion = true
                quietSinceMs = null
                standstill = false
                estSpeedMps = 0.0
                brakeSeenSinceLoss = false
                pullSustainSinceMs = null
            }
            return
        }

        val accelQuiet = accel == null || kotlin.math.abs(accel) < config.standstillAccelMps2
        val gyroQuiet = gyro == null || kotlin.math.abs(gyro) < config.standstillGyroDegPerS
        val hardBraking = accel != null && accel <= config.brakeAccelMps2
        // After braking to a low speed, "quiet" means no pull-away thrust so
        // cabin vibration cannot block the latch; before that, full quiet.
        // Hard braking itself starts/keeps the hold timer so the latch can fire
        // as soon as speed crosses the threshold after a real stop.
        val stopping = brakeSeenSinceLoss && estSpeedMps <= config.standstillSpeedMps
        val quietEnough =
            when {
                stopping -> !pullAway && gyroQuiet
                hardBraking -> gyroQuiet
                else -> accelQuiet && gyroQuiet
            }
        if (!quietEnough) {
            imuMotion = true
            quietSinceMs = null
            return
        }
        imuMotion = false
        val since = quietSinceMs ?: frame.tMs.also { quietSinceMs = it }
        if (
            brakeSeenSinceLoss &&
            estSpeedMps <= config.standstillSpeedMps &&
            frame.tMs - since >= config.standstillHoldMs
        ) {
            standstill = true
            estSpeedMps = 0.0
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

    private fun shouldGiveUp(
        tMs: Long,
        constraint: DrConstraint?,
    ): Boolean {
        if (mode != DrMode.Estimating) return false
        // On a tunnel span of the active route, keep estimating until the
        // along-track position has passed that span's end plus the margin.
        // No route, or a span that is not a tunnel, keeps the time and
        // distance limits.
        if (onOpenTunnelSpan(constraint)) return false
        if (tMs - drStartedMs >= config.maxDrDurationMs) return true
        if (drDistanceM >= config.maxDrDistanceM) return true
        return false
    }

    /**
     * True while [estAlongM] is inside a contiguous tunnel run of [constraint],
     * or still short of that run's end plus [DrConfig.tunnelExitMarginM].
     * Tunnel flags come from the route samples (graph `is_tunnel`). No names.
     */
    private fun onOpenTunnelSpan(constraint: DrConstraint?): Boolean {
        val samples = constraint?.samples ?: return false
        if (samples.size < 2) return false
        var i = 0
        while (i < samples.size) {
            if (!samples[i].isTunnel) {
                i += 1
                continue
            }
            val start = samples[i].cumM
            var end = start
            while (i < samples.size && samples[i].isTunnel) {
                end = samples[i].cumM
                i += 1
            }
            if (estAlongM >= start && estAlongM <= end + config.tunnelExitMarginM) {
                return true
            }
        }
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
