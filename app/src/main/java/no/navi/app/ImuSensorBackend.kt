package no.navi.app

import android.content.Context
import android.hardware.Sensor
import android.hardware.SensorEvent
import android.hardware.SensorEventListener
import android.hardware.SensorManager
import android.os.Handler
import android.os.HandlerThread
import uniffi.navi.ImuSensorSample
import uniffi.navi.SensorPlatform
import uniffi.navi.imuPluginAndroidSensorTypes

/** Receives raw platform events (a [SensorEventListener] adapter in production). */
fun interface SensorSink {
    fun onSensorChanged(
        androidType: Int,
        values: FloatArray,
        timestampNanos: Long,
        accuracy: Int,
    )
}

/** The slice of `SensorManager` the backend needs, so the policy is JVM-testable. */
interface SensorSource {
    /** True when the device has a default sensor of this `Sensor.TYPE_*`. */
    fun hasSensor(androidType: Int): Boolean

    /** Start delivering events for [androidType] to [sink]; false if registration failed. */
    fun register(
        androidType: Int,
        samplingPeriodUs: Int,
        sink: SensorSink,
    ): Boolean

    fun unregister(androidType: Int)
}

/**
 * Android implementation of the sensor hub backend contract (`SensorPlatform`).
 *
 * The Rust `SensorHub` owns the policy (lazy register on first read, per-sensor
 * rate limit, unregister after idle). This class maps the host's stable sensor
 * codes onto `Sensor.TYPE_*` (ambient temperature prefers `TYPE_AMBIENT_TEMPERATURE`,
 * falling back to the deprecated `TYPE_TEMPERATURE`), detects availability, drops
 * events faster than the requested period, and keeps only the latest sample.
 */
class ImuSensorBackend(
    private val source: SensorSource,
    private val typesFor: (UInt) -> List<Int> = { imuPluginAndroidSensorTypes(it) },
) : SensorPlatform,
    SensorSink {
    private class Registration(
        val kindCode: UInt,
        val periodMs: Long,
    ) {
        var lastAcceptedMs: Long? = null
    }

    private val lock = Any()
    private val resolved = HashMap<UInt, Int?>()
    private val registrations = HashMap<Int, Registration>()
    private val latest = HashMap<UInt, ImuSensorSample>()

    /** The `Sensor.TYPE_*` serving [kindCode], or null when the device has none. */
    fun resolveAndroidType(kindCode: UInt): Int? =
        synchronized(lock) {
            resolved.getOrPut(kindCode) {
                typesFor(kindCode).firstOrNull { source.hasSensor(it) }
            }
        }

    override fun isSensorAvailable(kindCode: UInt): Boolean = resolveAndroidType(kindCode) != null

    override fun registerSensor(
        kindCode: UInt,
        samplingPeriodUs: UInt,
    ): Boolean {
        val type = resolveAndroidType(kindCode) ?: return false
        synchronized(lock) {
            if (registrations.containsKey(type)) return true
        }
        val period = samplingPeriodUs.coerceAtMost(Int.MAX_VALUE.toUInt()).toInt()
        if (!source.register(type, period, this)) return false
        synchronized(lock) {
            registrations[type] = Registration(kindCode, period / 1_000L)
        }
        return true
    }

    override fun unregisterSensor(kindCode: UInt) {
        val type = resolveAndroidType(kindCode) ?: return
        val had =
            synchronized(lock) {
                latest.remove(kindCode)
                registrations.remove(type) != null
            }
        if (had) source.unregister(type)
    }

    override fun latestSensorSample(kindCode: UInt): ImuSensorSample? = synchronized(lock) { latest[kindCode] }

    /** Android sensor types currently registered (diagnostics / tests). */
    fun registeredAndroidTypes(): Set<Int> = synchronized(lock) { registrations.keys.toSet() }

    override fun onSensorChanged(
        androidType: Int,
        values: FloatArray,
        timestampNanos: Long,
        accuracy: Int,
    ) {
        val timestampMs = timestampNanos / 1_000_000L
        synchronized(lock) {
            val reg = registrations[androidType] ?: return
            val last = reg.lastAcceptedMs
            // Platform periods are hints: drop events arriving much faster than asked.
            if (last != null && timestampMs - last < reg.periodMs * RATE_SLACK_NUM / RATE_SLACK_DEN) return
            reg.lastAcceptedMs = timestampMs
            latest[reg.kindCode] =
                ImuSensorSample(
                    values = values.map { it.toDouble() },
                    timestampMs = timestampMs,
                    accuracy = if (accuracy in 0..3) accuracy else -1,
                )
        }
    }

    companion object {
        /** Accept an event once 80% of the requested period has elapsed (jitter margin). */
        private const val RATE_SLACK_NUM = 4L
        private const val RATE_SLACK_DEN = 5L
    }
}

/** Real `SensorManager`, delivering events on a private background thread. */
class AndroidSensorSource(
    context: Context,
) : SensorSource {
    private val manager = context.applicationContext.getSystemService(Context.SENSOR_SERVICE) as? SensorManager
    private val lock = Any()
    private val listeners = HashMap<Int, SensorEventListener>()
    private var thread: HandlerThread? = null
    private var handler: Handler? = null

    override fun hasSensor(androidType: Int): Boolean = manager?.getDefaultSensor(androidType) != null

    override fun register(
        androidType: Int,
        samplingPeriodUs: Int,
        sink: SensorSink,
    ): Boolean {
        val mgr = manager ?: return false
        val sensor: Sensor = mgr.getDefaultSensor(androidType) ?: return false
        synchronized(lock) {
            if (listeners.containsKey(androidType)) return true
            val h = handler ?: startThread()
            val listener =
                object : SensorEventListener {
                    override fun onSensorChanged(event: SensorEvent) {
                        sink.onSensorChanged(androidType, event.values, event.timestamp, event.accuracy)
                    }

                    override fun onAccuracyChanged(
                        sensor: Sensor,
                        accuracy: Int,
                    ) = Unit
                }
            val ok = mgr.registerListener(listener, sensor, samplingPeriodUs, h)
            if (ok) {
                listeners[androidType] = listener
            } else if (listeners.isEmpty()) {
                stopThread()
            }
            return ok
        }
    }

    override fun unregister(androidType: Int) {
        synchronized(lock) {
            val listener = listeners.remove(androidType) ?: return
            manager?.unregisterListener(listener)
            if (listeners.isEmpty()) stopThread()
        }
    }

    private fun startThread(): Handler {
        val t = HandlerThread("navi-imu-sensors").also { it.start() }
        thread = t
        return Handler(t.looper).also { handler = it }
    }

    private fun stopThread() {
        thread?.quitSafely()
        thread = null
        handler = null
    }
}
