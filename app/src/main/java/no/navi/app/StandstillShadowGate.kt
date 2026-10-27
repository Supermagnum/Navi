package no.navi.app

/**
 * Diagnostics-only standstill evaluation.
 *
 * Mirrors [DrConfig] accel / gyro / hold thresholds but always pretends GNSS
 * speed is missing, so a constant-speed cruise with a quiet IMU is scored as
 * a potential latch. Never writes into [DeadReckoningEngine] state.
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

    fun reset() {
        quietSinceMs = null
        latched = false
        slowSinceMs = null
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
            // Missing IMU: do not latch; do not clear an existing latch.
            return finish(
                frame.tMs,
                if (latched) State.WouldLatch else State.Quiet,
                accelAbs,
                yawAbs,
                gnssSpeedMps,
                gnssAccuracyM,
            )
        }

        val accelQuiet = accelAbs == null || accelAbs < config.standstillAccelMps2
        val gyroQuiet = yawAbs == null || yawAbs < config.standstillGyroDegPerS
        if (!accelQuiet || !gyroQuiet) {
            quietSinceMs = null
            latched = false
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
        val holding = frame.tMs - since < config.standstillHoldMs
        if (holding) {
            return finish(
                frame.tMs,
                State.Holding,
                accelAbs,
                yawAbs,
                gnssSpeedMps,
                gnssAccuracyM,
            )
        }
        latched = true
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
    }
}
