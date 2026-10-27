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
     * Trust of the GNSS-seeded speed after loss decays with this time
     * constant (seconds). After about one loss timeout the seed is no longer
     * used unchanged; by ~3τ the estimate is treated as IMU-only. 8 s sits
     * above the 2 s loss timeout and below a typical short tunnel transit.
     */
    val speedTrustTauS: Double = 8.0,
    /**
     * Positive forward accel at or above this releases standstill / cancels a
     * pending stop hold (m/s²). 1.0 is above cabin vibration (0.40 RMS) and
     * at or below a light pull-away; margin to vib 0.40 is about 0.5 m/s².
     */
    val pullAwayAccelMps2: Double = 1.0,
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
     * Sign that maps device [SensorFrame.forwardAccelMps2] onto the vehicle
     * longitudinal axis. Learned while GNSS is available (see [updateGnssAccel]).
     */
    private var forwardSign: Double = 1.0
    private var forwardSignAgree: Int = 0
    private var forwardSignDisagree: Int = 0
    private var prevGnssSpeedMps: Double? = null
    private var prevGnssSpeedMs: Long = 0L
    private var lastGnssAccelMps2: Double? = null
    private var lastDeviceForwardAccel: Double? = null

    fun mode(): DrMode = mode

    /** Test/diagnostics: current forward-axis sign (+1 or −1). */
    fun forwardSign(): Double = forwardSign

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
        lastGnssSpeedMps = null
        lastGnssReportMs = 0L
        quietSinceMs = null
        standstill = false
        imuMissing = true
        imuMotion = false
        brakeSeenSinceLoss = false
        forwardSign = 1.0
        forwardSignAgree = 0
        forwardSignDisagree = 0
        prevGnssSpeedMps = null
        prevGnssSpeedMs = 0L
        lastGnssAccelMps2 = null
        lastDeviceForwardAccel = null
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
            // Opportunistically keep heading warm and learn the forward axis.
            frame.headingDeg?.let { estBearingDeg = normalizeDeg(it) }
            observeForwardAxis(frame)
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

        // Stale GNSS speed is never used unchanged past the loss timeout.
        clearStaleGnssSpeed(frame.tMs)

        // Mark braking from this frame before integrating so the speed update
        // and the latch see the same brake/speed state.
        noteBraking(frame)
        val reportedStop = lastGnssSpeedMps != null && lastGnssSpeedMps!! <= 0.0
        if (standstill || reportedStop) {
            // A reported 0, or the zero-velocity gate, is 0. Do not raise it to
            // minSpeedMps and do not replace it with the planned route speed.
            estSpeedMps = 0.0
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
        quietSinceMs = null
        standstill = false
        // Prefer the calibrated mount axis (+1) for the gap; sparse GNSS
        // correlation during approach is too weak to trust a flip here.
        forwardSign = 1.0
        if (seed.speedMps != null && seed.speedMps.isFinite() && seed.speedMps > 0.0) {
            lastGnssSpeedMps = seed.speedMps
            lastGnssReportMs = seed.tMs
            // Seed from last good GNSS; IMU integration takes over within the
            // loss timeout (stale speed is cleared, never held unchanged).
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
        if (constraint != null) {
            estAlongM = alongOf(constraint, seed.lat, seed.lon)
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
     * Drop a positive GNSS speed report once it is older than the loss timeout
     * so DR never coasts on the stale value unchanged.
     */
    private fun clearStaleGnssSpeed(tMs: Long) {
        val last = lastGnssSpeedMps ?: return
        if (last <= 0.0) return
        if (lastGnssReportMs <= 0L) return
        if (tMs - lastGnssReportMs >= config.gpsLossTimeoutMs) {
            lastGnssSpeedMps = null
        }
    }

    /**
     * While GNSS is available, longitudinal accel from successive speeds is
     * correlated with the device forwardAccel to learn [forwardSign].
     */
    private fun updateGnssAccel(sample: GpsSample) {
        val v = sample.speedMps ?: return
        val prev = prevGnssSpeedMps
        val prevT = prevGnssSpeedMs
        if (prev != null && sample.tMs > prevT) {
            val dt = (sample.tMs - prevT) / 1000.0
            if (dt in 0.05..3.0) {
                lastGnssAccelMps2 = (v - prev) / dt
                maybeUpdateForwardSign()
            }
        }
        prevGnssSpeedMps = v
        prevGnssSpeedMs = sample.tMs
    }

    private fun observeForwardAxis(frame: SensorFrame) {
        lastDeviceForwardAccel = frame.forwardAccelMps2
        maybeUpdateForwardSign()
    }

    private fun maybeUpdateForwardSign() {
        val aGnss = lastGnssAccelMps2 ?: return
        val aDev = lastDeviceForwardAccel ?: return
        if (kotlin.math.abs(aGnss) < 0.5) return
        if (kotlin.math.abs(aDev) < 0.2) return
        if (aGnss * aDev > 0.0) {
            forwardSignAgree++
        } else {
            forwardSignDisagree++
        }
        // Require a clear majority before flipping; default remains +1 (mount
        // calibration already aims the device forward axis along the vehicle).
        forwardSign =
            if (forwardSignDisagree > forwardSignAgree + 5) {
                -1.0
            } else if (forwardSignAgree >= forwardSignDisagree) {
                1.0
            } else {
                forwardSign
            }
    }

    /**
     * Integrate longitudinal accel into [estSpeedMps] after GNSS loss.
     * Planned route speed is an upper bound, or a fallback when no IMU is
     * present — never a driver while the IMU shows a stop.
     */
    private fun updateSpeedFromImu(
        frame: SensorFrame,
        dt: Double,
        constraint: DrConstraint?,
    ) {
        val raw = frame.forwardAccelMps2
        val plannedMps =
            constraint?.let { sampleAtAlong(it, estAlongM)?.speedKmh }?.let { it / 3.6 }

        if (raw == null && frame.yawRateDegPerS == null) {
            // No IMU: keep the previous no-IMU fallback (planned when missing).
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

        val accel = (raw ?: 0.0) * forwardSign
        // After braking has brought speed into the latch band and accel has
        // settled (no longer hard-braking through a crawl target), hold at 0
        // unless there is real positive thrust. Cabin vibration must not
        // re-accelerate the estimate and defeat the stop.
        val settledStop =
            brakeSeenSinceLoss &&
                estSpeedMps <= config.standstillSpeedMps &&
                accel > config.brakeAccelMps2
        if (settledStop) {
            if (accel >= config.pullAwayAccelMps2) {
                estSpeedMps = (estSpeedMps + accel * dt).coerceAtLeast(0.0)
            } else {
                estSpeedMps = 0.0
            }
        } else {
            // Integrate only real braking or pull-away. Cabin vibration lives in
            // the mid-band and must not random-walk the coasting speed.
            val integrate =
                accel <= config.brakeAccelMps2 ||
                    accel >= config.pullAwayAccelMps2
            if (integrate) {
                estSpeedMps = (estSpeedMps + accel * dt).coerceAtLeast(0.0)
            }
        }

        // Trust decay of the GNSS seed (documentation / diagnostics). After
        // ~loss-timeout the seed is cleared; τ keeps the estimate IMU-led.
        val ageS = ((frame.tMs - drStartedMs).coerceAtLeast(0L)) / 1000.0
        val trust = kotlin.math.exp(-ageS / config.speedTrustTauS)
        // Planned speed is only an upper bound while we have IMU.
        if (plannedMps != null && plannedMps > 1.0 && estSpeedMps > plannedMps) {
            // Allow a small overshoot early (high trust), tighten as trust fades.
            val cap = plannedMps * (1.0 + 0.15 * trust)
            if (estSpeedMps > cap) estSpeedMps = cap
        }
        estSpeedMps = estSpeedMps.coerceAtMost(config.maxSpeedMps)
    }

    private fun noteBraking(frame: SensorFrame) {
        val accel = frame.forwardAccelMps2?.let { it * forwardSign } ?: return
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
        val accel = frame.forwardAccelMps2?.let { it * forwardSign }
        val gyro = frame.yawRateDegPerS
        if (accel == null && gyro == null) {
            imuMissing = true
            imuMotion = false
            quietSinceMs = null
            return
        }
        imuMissing = false

        // Release needs real forward thrust / turn — not cabin vibration and
        // not continued braking (abs(raw) would treat −2 m/s² as pull-away).
        val pullAway =
            (accel != null && accel >= config.pullAwayAccelMps2) ||
                (gyro != null && kotlin.math.abs(gyro) >= config.standstillGyroDegPerS * 2.0)
        if (standstill) {
            if (pullAway) {
                imuMotion = true
                quietSinceMs = null
                standstill = false
                estSpeedMps = 0.0
                brakeSeenSinceLoss = false
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
