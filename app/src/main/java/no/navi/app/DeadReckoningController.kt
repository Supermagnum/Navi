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
    private var listening = false
    private var sm: SensorManager? = null
    private var lastRv: FloatArray? = null
    private var lastGyro: FloatArray? = null
    private var lastLinAccel: FloatArray? = null
    private var lastSensorWallMs: Long = 0L

    private val tickRunnable =
        object : Runnable {
            override fun run() {
                if (!listening) return
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
        val out = engine.onGps(sample, constraintProvider())
        onOutput(out)
        return out
    }

    /** Test / script injection of a sensor frame (bypasses SensorManager). */
    fun injectSensorFrame(frame: SensorFrame): DrOutput {
        val out = engine.onSensorTick(frame, constraintProvider())
        onOutput(out)
        return out
    }

    /** Test helper: feed a GNSS sample straight into the engine. */
    fun injectGpsSample(sample: GpsSample): DrOutput {
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

    private fun frameHasMotion(frame: SensorFrame): Boolean = frame.headingDeg != null || frame.yawRateDegPerS != null || frame.forwardAccelMps2 != null

    private fun buildFrameFromHardware(now: Long): SensorFrame? {
        val heading = headingFromRv(lastRv)
        val yawRate = lastGyro?.getOrNull(2)?.let { Math.toDegrees(it.toDouble()) }
        val forward = forwardAccel(lastLinAccel, heading)
        if (heading == null && yawRate == null && forward == null) return null
        return SensorFrame(
            tMs = now,
            headingDeg = heading,
            yawRateDegPerS = yawRate,
            forwardAccelMps2 = forward,
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
    }
}

/** Injectable sensor feed for unit / instrumented tests. */
fun interface DrSensorSource {
    fun latest(nowMs: Long): SensorFrame?
}
