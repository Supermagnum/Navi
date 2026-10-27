package no.navi.app

/**
 * Diagnostics-only standstill evaluation.
 *
 * Mirrors the live braking-aware gate: would-latch only after a braking event
 * and a low-speed quiet hold. Quiet cruise alone never would-latch. GNSS speed
 * is ignored for the latch decision (as if missing) and used only for false /
 * missed events. Never writes into [DeadReckoningEngine] state.
 */
class StandstillShadowGate(
    private val config: DrConfig = DrConfig(),
) {
    enum class State {
        Quiet,
        Holding,
        WouldLatch,
    }

    data class Sample(
        val tMs: Long,
        val state: State,
        val accelAbsMps2: Double?,
        val yawAbsDegPerS: Double?,
        val gnssSpeedMps: Double?,
        val gnssAccuracyM: Float?,
        val falseStandstill: Boolean,
        val missedStandstill: Boolean,
    )

    private var quietSinceMs: Long? = null
    private var latched: Boolean = false
    private var slowSinceMs: Long? = null
    private var brakeSeen: Boolean = false

    /** Assume moving until braking integrates this down. */
    private var shadowSpeedMps: Double = ASSUMED_CRUISE_MPS

    fun reset() {
        quietSinceMs = null
        latched = false
        slowSinceMs = null
        brakeSeen = false
        shadowSpeedMps = ASSUMED_CRUISE_MPS
    }

    /**
     * Observe one IMU sample. [gnssSpeedMps] / [gnssAccuracyM] are logged and
     * used only for false / missed events — they do not keep the shadow gate
     * open the way a reported speed does in the live engine.
     */
    fun observe(
        frame: SensorFrame,
        gnssSpeedMps: Double?,
        gnssAccuracyM: Float?,
    ): Sample {
        val accel = frame.forwardAccelMps2
        val gyro = frame.yawRateDegPerS
        val accelAbs = accel?.let { kotlin.math.abs(it) }
        val yawAbs = gyro?.let { kotlin.math.abs(it) }

        if (accel == null && gyro == null) {
            quietSinceMs = null
            return finish(
                frame.tMs,
                if (latched) State.WouldLatch else State.Quiet,
                accelAbs,
                yawAbs,
                gnssSpeedMps,
                gnssAccuracyM,
            )
        }

        if (accel != null && accel <= config.brakeAccelMps2) {
            brakeSeen = true
        }
        // Shadow integrates a coarse speed so "slow enough after brake" works
        // without reading the live engine.
        if (accel != null) {
            if (brakeSeen && shadowSpeedMps <= config.standstillSpeedMps) {
                if (accel >= config.pullAwayAccelMps2) {
                    shadowSpeedMps = (shadowSpeedMps + accel * 0.1).coerceAtLeast(0.0)
                } else {
                    shadowSpeedMps = 0.0
                }
            } else if (
                accel <= config.brakeAccelMps2 ||
                accel >= config.pullAwayAccelMps2
            ) {
                shadowSpeedMps = (shadowSpeedMps + accel * 0.1).coerceAtLeast(0.0)
            }
        }

        val pullAway =
            (accel != null && accel >= config.pullAwayAccelMps2) ||
                (gyro != null && kotlin.math.abs(gyro) >= config.standstillGyroDegPerS * 2.0)
        if (latched) {
            if (pullAway) {
                latched = false
                brakeSeen = false
                quietSinceMs = null
                shadowSpeedMps = 0.0
                return finish(
                    frame.tMs,
                    State.Quiet,
                    accelAbs,
                    yawAbs,
                    gnssSpeedMps,
                    gnssAccuracyM,
                )
            }
            return finish(
                frame.tMs,
                State.WouldLatch,
                accelAbs,
                yawAbs,
                gnssSpeedMps,
                gnssAccuracyM,
            )
        }

        val gyroQuiet = yawAbs == null || yawAbs < config.standstillGyroDegPerS
        val stopping = brakeSeen && shadowSpeedMps <= config.standstillSpeedMps
        val quietEnough =
            if (stopping) {
                !pullAway && gyroQuiet
            } else {
                (accelAbs == null || accelAbs < config.standstillAccelMps2) && gyroQuiet
            }
        if (!quietEnough || !brakeSeen) {
            quietSinceMs = null
            return finish(
                frame.tMs,
                State.Quiet,
                accelAbs,
                yawAbs,
                gnssSpeedMps,
                gnssAccuracyM,
            )
        }

        val since = quietSinceMs ?: frame.tMs.also { quietSinceMs = it }
        if (frame.tMs - since < config.standstillHoldMs) {
            return finish(
                frame.tMs,
                State.Holding,
                accelAbs,
                yawAbs,
                gnssSpeedMps,
                gnssAccuracyM,
            )
        }
        if (shadowSpeedMps > config.standstillSpeedMps) {
            quietSinceMs = null
            return finish(
                frame.tMs,
                State.Quiet,
                accelAbs,
                yawAbs,
                gnssSpeedMps,
                gnssAccuracyM,
            )
        }
        latched = true
        shadowSpeedMps = 0.0
        return finish(
            frame.tMs,
            State.WouldLatch,
            accelAbs,
            yawAbs,
            gnssSpeedMps,
            gnssAccuracyM,
        )
    }

    private fun finish(
        tMs: Long,
        state: State,
        accelAbs: Double?,
        yawAbs: Double?,
        gnssSpeedMps: Double?,
        gnssAccuracyM: Float?,
    ): Sample {
        val speed = gnssSpeedMps
        val acc = gnssAccuracyM
        val goodAcc = acc != null && acc.isFinite() && acc > 0f && acc <= config.goodAccuracyM
        val falseStandstill =
            state == State.WouldLatch &&
                speed != null &&
                speed > FALSE_STILL_SPEED_MPS &&
                goodAcc

        if (speed != null && speed < MISSED_STILL_SPEED_MPS) {
            val since = slowSinceMs ?: tMs.also { slowSinceMs = it }
            val missed =
                tMs - since >= MISSED_STILL_HOLD_MS && state != State.WouldLatch
            return Sample(
                tMs = tMs,
                state = state,
                accelAbsMps2 = accelAbs,
                yawAbsDegPerS = yawAbs,
                gnssSpeedMps = speed,
                gnssAccuracyM = acc,
                falseStandstill = falseStandstill,
                missedStandstill = missed,
            )
        }
        slowSinceMs = null
        return Sample(
            tMs = tMs,
            state = state,
            accelAbsMps2 = accelAbs,
            yawAbsDegPerS = yawAbs,
            gnssSpeedMps = speed,
            gnssAccuracyM = acc,
            falseStandstill = falseStandstill,
            missedStandstill = false,
        )
    }

    companion object {
        const val FALSE_STILL_SPEED_MPS = 2.0
        const val MISSED_STILL_SPEED_MPS = 0.3
        const val MISSED_STILL_HOLD_MS = 3_000L
        private const val ASSUMED_CRUISE_MPS = 20.0
    }
}
