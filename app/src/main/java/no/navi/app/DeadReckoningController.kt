package no.navi.app

import android.content.Context
import android.hardware.Sensor
import android.hardware.SensorEvent
import android.hardware.SensorEventListener
import android.hardware.SensorManager
import android.location.Location
import android.os.Handler
import android.os.Looper
import kotlin.math.cos
import kotlin.math.sin
import kotlin.math.sqrt

/**
 * Bridges [SensorManager] + GNSS into [DeadReckoningEngine] and publishes map
 * positions via [onOutput]. Injectable [sensorSource] / [engine] for tests.
 *
 * Mount calibration: when [mountYawOffsetDeg] is set (from IMU #162), it is
 * subtracted from rotation-vector yaw so vehicle heading matches the road.
 * Null offset = identity (no calibration fallback).
 */
class DeadReckoningController(
    private val engine: DeadReckoningEngine = DeadReckoningEngine(),
    private val sensorSource: DrSensorSource? = null,
    private val mainHandler: Handler = Handler(Looper.getMainLooper()),
    var mountYawOffsetDeg: Double? = null,
    var constraintProvider: () -> DrConstraint? = { null },
    var onOutput: (DrOutput) -> Unit = {},
) {
    /**
     * Instrumented replays set this so a live sensor tick cannot rewind
     * [DeadReckoningEngine] time. Production leaves it false.
     */
    @Volatile
    var suspendHardwareTicks: Boolean = false

    private var listening = false
    private var sm: SensorManager? = null
    private var lastRv: FloatArray? = null
    private var lastGyro: FloatArray? = null
    private var lastLinAccel: FloatArray? = null
    private var lastAccel: FloatArray? = null
    private var lastGravity: FloatArray? = null
    private val gravLp = doubleArrayOf(0.0, 0.0, 9.81)
    private var gravLpInit = false
    private var lastGravLpMs = 0L
    private var lastSensorWallMs: Long = 0L
    private var lastGnssSpeedMps: Double? = null
    private var lastGnssAccuracyM: Float? = null
    private val shadowGate = StandstillShadowGate()

    private val tickRunnable =
        object : Runnable {
            override fun run() {
                if (!listening) return
                if (suspendHardwareTicks) {
                    mainHandler.postDelayed(this, TICK_MS)
                    return
                }
                publishSensorTick()
                mainHandler.postDelayed(this, TICK_MS)
            }
        }

    private val listener =
        object : SensorEventListener {
            override fun onSensorChanged(event: SensorEvent) {
                when (event.sensor.type) {
                    Sensor.TYPE_ROTATION_VECTOR -> lastRv = event.values.clone()
                    Sensor.TYPE_GYROSCOPE -> lastGyro = event.values.clone()
                    Sensor.TYPE_LINEAR_ACCELERATION -> lastLinAccel = event.values.clone()
                    Sensor.TYPE_ACCELEROMETER -> lastAccel = event.values.clone()
                    Sensor.TYPE_GRAVITY -> lastGravity = event.values.clone()
                }
            }

            override fun onAccuracyChanged(
                sensor: Sensor?,
                accuracy: Int,
            ) {}
        }

    fun engine(): DeadReckoningEngine = engine

    fun start(context: Context) {
        if (listening) return
        listening = true
        if (sensorSource == null) {
            sm = context.getSystemService(SensorManager::class.java)
            sm?.getDefaultSensor(Sensor.TYPE_ROTATION_VECTOR)?.let {
                sm?.registerListener(listener, it, SensorManager.SENSOR_DELAY_GAME)
            }
            sm?.getDefaultSensor(Sensor.TYPE_GYROSCOPE)?.let {
                sm?.registerListener(listener, it, SensorManager.SENSOR_DELAY_GAME)
            }
            sm?.getDefaultSensor(Sensor.TYPE_LINEAR_ACCELERATION)?.let {
                sm?.registerListener(listener, it, SensorManager.SENSOR_DELAY_GAME)
            }
            sm?.getDefaultSensor(Sensor.TYPE_ACCELEROMETER)?.let {
                sm?.registerListener(listener, it, SensorManager.SENSOR_DELAY_GAME)
            }
            sm?.getDefaultSensor(Sensor.TYPE_GRAVITY)?.let {
                sm?.registerListener(listener, it, SensorManager.SENSOR_DELAY_GAME)
            }
        }
        mainHandler.post(tickRunnable)
    }

    fun stop() {
        listening = false
        mainHandler.removeCallbacks(tickRunnable)
        sm?.unregisterListener(listener)
        sm = null
    }

    fun reset() {
        engine.reset()
        shadowGate.reset()
        lastGnssSpeedMps = null
        lastGnssAccuracyM = null
    }

    /**
     * Ingest a LocationManager (or test) fix. Returns the DR output the map
     * should display. When estimating, callers should prefer this over raw GPS.
     */
    fun onLocation(loc: Location): DrOutput {
        val speed =
            if (loc.hasSpeed() && loc.speed.isFinite() && loc.speed >= 0f) {
                loc.speed.toDouble()
            } else {
                null
            }
        val bearing =
            if (loc.hasBearing() && loc.bearing.isFinite()) loc.bearing.toDouble() else null
        val acc = if (loc.hasAccuracy()) loc.accuracy else null
        val available =
            loc.provider != null &&
                (loc.latitude != 0.0 || loc.longitude != 0.0) &&
                loc.provider != "none"
        val sample =
            GpsSample(
                tMs = if (loc.time > 0L) loc.time else System.currentTimeMillis(),
                lat = loc.latitude,
                lon = loc.longitude,
                accuracyM = acc,
                speedMps = speed,
                bearingDeg = bearing,
                available = available,
            )
        lastGnssSpeedMps = speed
        lastGnssAccuracyM = acc
        val out = engine.onGps(sample, constraintProvider())
        onOutput(out)
        return out
    }

    /** Test / script injection of a sensor frame (bypasses SensorManager). */
    fun injectSensorFrame(frame: SensorFrame): DrOutput {
        maybeShadow(frame)
        val out = engine.onSensorTick(frame, constraintProvider())
        onOutput(out)
        return out
    }

    /** Test helper: feed a GNSS sample straight into the engine. */
    fun injectGpsSample(sample: GpsSample): DrOutput {
        lastGnssSpeedMps = sample.speedMps
        lastGnssAccuracyM = sample.accuracyM
        val out = engine.onGps(sample, constraintProvider())
        onOutput(out)
        return out
    }

    /** Test helper: signal GNSS loss so the engine enters Estimating. */
    fun notifyGpsLost(tMs: Long = System.currentTimeMillis()): DrOutput {
        val constraint = constraintProvider()
        // First unavailable sample starts the degrade window; a second past the
        // loss timeout forces Estimating even when the prior good fix was recent.
        engine.onGps(
            GpsSample(
                tMs = tMs,
                lat = 0.0,
                lon = 0.0,
                available = false,
            ),
            constraint,
        )
        val out =
            engine.onGps(
                GpsSample(
                    tMs = tMs + 2_500L,
                    lat = 0.0,
                    lon = 0.0,
                    available = false,
                ),
                constraint,
            )
        onOutput(out)
        return out
    }

    private fun publishSensorTick() {
        val now = System.currentTimeMillis()
        if (now - lastSensorWallMs < TICK_MS / 2) return
        lastSensorWallMs = now
        val frame =
            sensorSource?.latest(now)
                ?: buildFrameFromHardware(now)
                ?: SensorFrame(tMs = now)
        // Shadow standstill runs in every DR mode while diagnostics are on.
        // It never feeds back into [engine].
        maybeShadow(frame)
        // Fixes that simply stop (tunnel, or the GPS provider turned off)
        // never arrive as an unavailable sample. Silence past the loss
        // timeout is what enters estimating.
        engine.onGnssSilence(now)
        val mode = engine.mode()
        if (mode == DrMode.Gps && !frameHasMotion(frame)) {
            // Keep heading warm cheaply; full ticks only needed while estimating.
            frame.headingDeg?.let {
                engine.onSensorTick(frame, constraintProvider())
            }
            return
        }
        val out = engine.onSensorTick(frame, constraintProvider())
        if (out.estimated || out.positionLost || out.exitErrorM != null) {
            onOutput(out)
        }
    }

    private fun maybeShadow(frame: SensorFrame) {
        if (!LocationDiagnostics.isEnabled()) return
        val sample = shadowGate.observe(frame, lastGnssSpeedMps, lastGnssAccuracyM)
        LocationDiagnostics.logStandstillShadow(sample, engine.mode().name)
    }

    private fun frameHasMotion(frame: SensorFrame): Boolean =
        frame.headingDeg != null ||
            frame.yawRateDegPerS != null ||
            frame.forwardAccelMps2 != null ||
            frame.linAccelX != null

    private fun buildFrameFromHardware(now: Long): SensorFrame? {
        val heading = headingFromRv(lastRv)
        val yawRate = lastGyro?.getOrNull(2)?.let { Math.toDegrees(it.toDouble()) }
        val resolved = resolveLinearAccel(now)
        val lin = resolved.first
        engine.noteLinearAccelSource(resolved.second)
        val grav = lastGravity
        val forward = forwardAccel(lin, heading)
        if (heading == null && yawRate == null && forward == null && lin == null) return null
        return SensorFrame(
            tMs = now,
            headingDeg = heading,
            yawRateDegPerS = yawRate,
            forwardAccelMps2 = forward,
            linAccelX = lin?.getOrNull(0)?.toDouble(),
            linAccelY = lin?.getOrNull(1)?.toDouble(),
            linAccelZ = lin?.getOrNull(2)?.toDouble(),
            gravityX = grav?.getOrNull(0)?.toDouble(),
            gravityY = grav?.getOrNull(1)?.toDouble(),
            gravityZ = grav?.getOrNull(2)?.toDouble(),
        )
    }

    private fun headingFromRv(rv: FloatArray?): Double? {
        if (rv == null || rv.size < 3) return null
        val rot = FloatArray(9)
        SensorManager.getRotationMatrixFromVector(rot, rv)
        val orient = FloatArray(3)
        SensorManager.getOrientation(rot, orient)
        // azimuth radians → degrees clockwise from north
        var yaw = Math.toDegrees(orient[0].toDouble())
        if (yaw < 0) yaw += 360.0
        val offset = mountYawOffsetDeg
        if (offset != null) {
            yaw = DeadReckoningEngine.normalizeDeg(yaw - offset)
        }
        return yaw
    }

    /**
     * Prefer [Sensor.TYPE_LINEAR_ACCELERATION]. When it is missing, or it is
     * not tracking a real accelerometer reading, subtract gravity
     * ([Sensor.TYPE_GRAVITY], else a low-pass of the accelerometer).
     * Returns the vector and a source name for the diag log.
     */
    private fun resolveLinearAccel(nowMs: Long): Pair<FloatArray?, String> {
        val derived = derivedLinear(nowMs)
        val lin = lastLinAccel
        if (lin == null) {
            return if (derived == null) null to "none" else derived to derivedSource()
        }
        if (derived != null) {
            val linH = hypot2(lin[0], lin[1])
            val derH = hypot2(derived[0], derived[1])
            // A present linear sensor that stays near zero while the
            // accelerometer shows horizontal specific force is not the motion.
            if (derH > 0.4f && linH < derH * 0.25f) {
                return derived to derivedSource()
            }
        }
        return lin to "linear"
    }

    private fun derivedSource(): String = if (lastGravity != null) "accel_minus_gravity" else "accel_minus_lowpass"

    private fun derivedLinear(nowMs: Long): FloatArray? {
        val acc = lastAccel ?: return null
        val grav =
            lastGravity ?: run {
                gravLpInit =
                    LinearAccelFallback.stepLowpass(
                        gravLp,
                        acc,
                        gravityDt(nowMs),
                        GRAVITY_LP_TAU_S,
                        gravLpInit,
                    )
                floatArrayOf(gravLp[0].toFloat(), gravLp[1].toFloat(), gravLp[2].toFloat())
            }
        return LinearAccelFallback.minus(acc, grav)
    }

    private fun gravityDt(nowMs: Long): Double {
        val dt = if (lastGravLpMs == 0L) 0.05 else ((nowMs - lastGravLpMs) / 1000.0).coerceIn(0.01, 0.5)
        lastGravLpMs = nowMs
        return dt
    }

    private fun hypot2(
        a: Float,
        b: Float,
    ): Float = sqrt((a * a + b * b).toDouble()).toFloat()

    /**
     * Project device linear acceleration onto vehicle forward using current
     * heading. Without calibration this is approximate; road constraint limits
     * damage.
     */
    private fun forwardAccel(
        lin: FloatArray?,
        headingDeg: Double?,
    ): Double? {
        if (lin == null || lin.size < 3) return null
        // Device frame: Y often points toward top of screen. Use horizontal
        // magnitude signed by world-forward alignment when heading known.
        val ax = lin[0].toDouble()
        val ay = lin[1].toDouble()
        val horiz = sqrt(ax * ax + ay * ay)
        if (headingDeg == null) {
            // Signed by ay (device "forward" when portrait-mounted).
            return if (ay >= 0) horiz else -horiz
        }
        val br = Math.toRadians(headingDeg)
        // Approximate: treat device Y as vehicle forward after mount offset.
        val forward = ay * cos(0.0) + ax * sin(0.0)
        // Bias toward motion along reported heading using world projection.
        val worldX = ax // east-ish in device after RV would need full frame; keep simple
        val worldY = ay
        val along = worldY * cos(br) + worldX * sin(br)
        return along
    }

    companion object {
        const val TICK_MS = 100L
        const val PROVIDER_ESTIMATED = "navi-dead-reckon"
        const val PROVIDER_LOST = "navi-position-lost"
        const val GRAVITY_LP_TAU_S = 2.0
    }
}

/**
 * Gravity removal when [android.hardware.Sensor.TYPE_LINEAR_ACCELERATION] is
 * missing. [minus] is accelerometer minus gravity. [stepLowpass] is the
 * gravity estimate used when [android.hardware.Sensor.TYPE_GRAVITY] is also
 * missing. Both are pure so unit tests can drive the fallback without a device.
 */
internal object LinearAccelFallback {
    fun minus(
        accel: FloatArray,
        gravity: FloatArray,
    ): FloatArray =
        floatArrayOf(
            accel[0] - gravity[0],
            accel[1] - gravity[1],
            accel[2] - gravity[2],
        )

    /** Updates [state] in place. Returns whether [state] is initialised. */
    fun stepLowpass(
        state: DoubleArray,
        accel: FloatArray,
        dt: Double,
        tau: Double,
        init: Boolean,
    ): Boolean {
        if (!init) {
            state[0] = accel[0].toDouble()
            state[1] = accel[1].toDouble()
            state[2] = accel[2].toDouble()
            return true
        }
        val alpha = (dt / (tau + dt)).coerceIn(0.0, 1.0)
        state[0] += alpha * (accel[0] - state[0])
        state[1] += alpha * (accel[1] - state[1])
        state[2] += alpha * (accel[2] - state[2])
        return true
    }
}

/** Injectable sensor feed for unit / instrumented tests. */
fun interface DrSensorSource {
    fun latest(nowMs: Long): SensorFrame?
}
