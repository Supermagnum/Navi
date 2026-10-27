package no.navi.app

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.hardware.Sensor
import android.hardware.SensorEvent
import android.hardware.SensorEventListener
import android.hardware.SensorManager
import android.location.GnssStatus
import android.location.LocationManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import androidx.core.content.ContextCompat
import org.json.JSONArray
import org.json.JSONObject
import java.io.BufferedWriter
import java.io.File
import java.io.FileOutputStream
import java.io.OutputStreamWriter
import java.nio.charset.StandardCharsets
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.TimeZone
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.math.max

/**
 * Location-diagnostics session log. Off by default.
 *
 * While disabled every entry point returns before allocating a line or touching
 * disk. While enabled, the main thread only [offer]s a finished line onto a
 * bounded queue. A background writer owns the file. A full queue drops the
 * line instead of blocking.
 *
 * Files live in the app-specific external files directory
 * (`Android/data/no.navi.app/files/location-diag/`) so `adb pull` works
 * without root.
 */
object LocationDiagnostics {
    const val JUMP_ALONG_DROP_M = 20.0
    const val JUMP_WINDOW_MS = 30_000L
    private const val DIR_NAME = "location-diag"
    private const val QUEUE_CAPACITY = 2048
    private const val DEFAULT_MAX_BYTES = 8L * 1024L * 1024L
    private const val MAX_FILES = 4

    data class FixEvent(
        val wallMs: Long,
        val elapsedRealtimeMs: Long,
        val fixTimeMs: Long,
        val fixElapsedRealtimeNanos: Long,
        val fixAgeMs: Long,
        val provider: String,
        val lat: Double,
        val lon: Double,
        val accuracyM: Float?,
        val speedMps: Float?,
        val speedAccMps: Float?,
        val bearingDeg: Float?,
        val bearingAccDeg: Float?,
        val isMock: Boolean,
    )

    data class NavEvent(
        val wallMs: Long,
        val elapsedRealtimeMs: Long,
        val source: String,
        val lat: Double,
        val lon: Double,
        val provider: String,
        val dr: DrDiagSnapshot?,
        val predictionOffsetM: Double,
        val snapIndex: Int,
        val alongM: Double,
        val crossTrackM: Double,
        val offRoute: Boolean,
        val offRouteReason: String,
        val offRouteAction: String,
        val reroute: Boolean,
        val rerouteOriginLat: Double?,
        val rerouteOriginLon: Double?,
        val rerouteTrigger: String?,
        val fromFollowsGps: Boolean,
        val routeNotJoined: Boolean,
        val simulating: Boolean,
    )

    data class Overlay(
        val accuracyM: Float?,
        val satsUsed: Int,
        val satsInView: Int,
        val source: String,
        val standstill: String,
        val snapIndex: Int,
        val alongM: Double,
        val offRoute: Boolean,
        val hasSample: Boolean,
    )

    private val enabled = AtomicBoolean(false)
    private val lock = Any()
    private val queue = ArrayBlockingQueue<String>(QUEUE_CAPACITY)
    private val running = AtomicBoolean(false)

    @Volatile
    private var writerThread: Thread? = null

    @Volatile
    private var sessionFile: File? = null

    @Volatile
    private var logsDirOverride: File? = null

    @Volatile
    private var maxBytes: Long = DEFAULT_MAX_BYTES

    @Volatile
    var droppedLines: Long = 0L
        private set

    @Volatile
    private var appContext: Context? = null

    @Volatile
    private var satsInView: Int = -1

    @Volatile
    private var satsUsed: Int = -1

    @Volatile
    private var meanCn0Used: Double = Double.NaN

    @Volatile
    private var lastAlongM: Double = Double.NaN

    @Volatile
    private var lastSnapIndex: Int = -1

    @Volatile
    private var overlay: Overlay = Overlay(null, -1, -1, "", "n/a", -1, 0.0, false, false)

    private var gnssCallback: GnssStatus.Callback? = null
    private var sensorManager: SensorManager? = null
    private var imuListener: SensorEventListener? = null
    private val imuLock = Any()
    private var imu = ImuAccum()
    private var lastImuFlushElapsedMs = 0L

    private class AxisAccum {
        var n: Int = 0
        var mean: Double = 0.0
        var m2: Double = 0.0

        fun add(x: Double) {
            n += 1
            val d = x - mean
            mean += d / n
            m2 += d * (x - mean)
        }

        fun variance(): Double = if (n < 2) 0.0 else m2 / (n - 1)

        fun reset() {
            n = 0
            mean = 0.0
            m2 = 0.0
        }
    }

    private class ImuAccum {
        val ax = AxisAccum()
        val ay = AxisAccum()
        val az = AxisAccum()
        val gx = AxisAccum()
        val gy = AxisAccum()
        val gz = AxisAccum()

        fun reset() {
            ax.reset()
            ay.reset()
            az.reset()
            gx.reset()
            gy.reset()
            gz.reset()
        }
    }

    fun isEnabled(): Boolean = enabled.get()

    fun overlay(): Overlay = overlay

    fun currentSessionFile(): File? = sessionFile

    /** Test hook. Does not create a file. */
    fun setLogsDirForTest(dir: File?) {
        logsDirOverride = dir
    }

    fun setMaxBytesForTest(bytes: Long) {
        maxBytes = bytes
    }

    fun restoreFromPrefs(context: Context) {
        val on = MapHudPrefs.loadLocationDiagnostics(context)
        setEnabled(context, on)
    }

    fun setEnabled(
        context: Context,
        on: Boolean,
    ) {
        MapHudPrefs.saveLocationDiagnostics(context, on)
        appContext = context.applicationContext
        val dir =
            logsDirOverride
                ?: context.getExternalFilesDir(null)?.let { File(it, DIR_NAME) }
                ?: File(context.filesDir, DIR_NAME)
        applyEnabled(dir, on)
        if (on) {
            startSensors(context.applicationContext)
            logSessionHeader(context.applicationContext)
        } else {
            stopSensors()
        }
    }

    /**
     * Enable or disable against an already-resolved directory.
     * Unit tests use this so no Android sensor service is required.
     */
    fun applyEnabled(
        logsDir: File,
        on: Boolean,
    ) {
        synchronized(lock) {
            if (!on) {
                enabled.set(false)
                closeWriterLocked()
                resetTrackers()
                return
            }
            if (enabled.get() && writerThread != null) return
            logsDir.mkdirs()
            enabled.set(true)
            resetTrackers()
            openWriterLocked(logsDir)
        }
    }

    fun logFix(event: FixEvent) {
        if (!enabled.get()) return
        val o =
            JSONObject()
                .put("kind", "fix")
                .put("wall_ms", event.wallMs)
                .put("elapsed_realtime_ms", event.elapsedRealtimeMs)
                .put("fix_time_ms", event.fixTimeMs)
                .put("fix_elapsed_realtime_nanos", event.fixElapsedRealtimeNanos)
                .put("fix_age_ms", event.fixAgeMs)
                .put("provider", event.provider)
                .put("lat", event.lat)
                .put("lon", event.lon)
                .put("accuracy_m", event.accuracyM?.toDouble() ?: JSONObject.NULL)
                .put("speed_mps", event.speedMps?.toDouble() ?: JSONObject.NULL)
                .put("speed_acc_mps", event.speedAccMps?.toDouble() ?: JSONObject.NULL)
                .put("bearing_deg", event.bearingDeg?.toDouble() ?: JSONObject.NULL)
                .put("bearing_acc_deg", event.bearingAccDeg?.toDouble() ?: JSONObject.NULL)
                .put("is_mock", event.isMock)
                .put("sats_in_view", satsInView)
                .put("sats_used", satsUsed)
                .put("mean_cn0_used", if (meanCn0Used.isFinite()) meanCn0Used else JSONObject.NULL)
        enqueue(o)
    }

    /**
     * One displayed-position update. [raw] is the LocationManager (or sim) fix
     * before dead-reckoning replacement. [shown] is what the map marker uses.
     * No-ops when diagnostics are off, before reading fix fields.
     */
    fun logDisplayed(
        raw: android.location.Location,
        shown: android.location.Location,
        estimated: Boolean,
        positionLost: Boolean,
        dr: DrDiagSnapshot?,
        snap: RouteProgressSnapshot?,
        offRouteAction: String,
        reroute: Boolean,
        rerouteTrigger: String?,
        fromFollowsGps: Boolean,
        routeNotJoined: Boolean,
        simulating: Boolean,
    ) {
        if (!enabled.get()) return
        val nowWall = System.currentTimeMillis()
        val nowElapsed = SystemClock.elapsedRealtime()
        val rawProvider = raw.provider.orEmpty()
        val synthetic =
            rawProvider == DeadReckoningController.PROVIDER_ESTIMATED ||
                rawProvider == DeadReckoningController.PROVIDER_LOST
        if (!synthetic) {
            noteAccuracy(if (raw.hasAccuracy()) raw.accuracy else null)
            logFix(
                FixEvent(
                    wallMs = nowWall,
                    elapsedRealtimeMs = nowElapsed,
                    fixTimeMs = raw.time,
                    fixElapsedRealtimeNanos = raw.elapsedRealtimeNanos,
                    fixAgeMs = fixAgeMs(raw),
                    provider = rawProvider,
                    lat = raw.latitude,
                    lon = raw.longitude,
                    accuracyM = if (raw.hasAccuracy()) raw.accuracy else null,
                    speedMps = if (raw.hasSpeed()) raw.speed else null,
                    speedAccMps = if (raw.hasSpeedAccuracy()) raw.speedAccuracyMetersPerSecond else null,
                    bearingDeg = if (raw.hasBearing()) raw.bearing else null,
                    bearingAccDeg = if (raw.hasBearingAccuracy()) raw.bearingAccuracyDegrees else null,
                    isMock = locationIsMock(raw),
                ),
            )
        }
        val reason =
            when {
                snap == null -> "no_tracker"
                snap.offRoute -> "cross_track_gt_threshold"
                else -> "on_route"
            }
        logNav(
            NavEvent(
                wallMs = nowWall,
                elapsedRealtimeMs = nowElapsed,
                source = displayedPositionSource(shown.provider.orEmpty(), estimated, positionLost),
                lat = shown.latitude,
                lon = shown.longitude,
                provider = shown.provider.orEmpty(),
                dr = dr,
                predictionOffsetM = 0.0,
                snapIndex = snap?.snapIndex ?: -1,
                alongM = snap?.alongM ?: Double.NaN,
                crossTrackM = snap?.crossTrackM ?: Double.NaN,
                offRoute = snap?.offRoute == true,
                offRouteReason = reason,
                offRouteAction = offRouteAction,
                reroute = reroute,
                rerouteOriginLat = if (reroute) shown.latitude else null,
                rerouteOriginLon = if (reroute) shown.longitude else null,
                rerouteTrigger = rerouteTrigger,
                fromFollowsGps = fromFollowsGps,
                routeNotJoined = routeNotJoined,
                simulating = simulating,
            ),
        )
    }

    fun logNav(event: NavEvent) {
        if (!enabled.get()) return
        val dr = event.dr
        val o =
            JSONObject()
                .put("kind", "nav")
                .put("wall_ms", event.wallMs)
                .put("elapsed_realtime_ms", event.elapsedRealtimeMs)
                .put("source", event.source)
                .put("lat", event.lat)
                .put("lon", event.lon)
                .put("provider", event.provider)
                .put("prediction_offset_m", num(event.predictionOffsetM))
                .put("prediction", "none")
                .put("snap_index", event.snapIndex)
                .put("along_m", num(event.alongM))
                .put("cross_track_m", num(event.crossTrackM))
                .put("off_route", event.offRoute)
                .put("off_route_reason", event.offRouteReason)
                .put("off_route_action", event.offRouteAction)
                .put("reroute", event.reroute)
                .put("reroute_origin_lat", event.rerouteOriginLat ?: JSONObject.NULL)
                .put("reroute_origin_lon", event.rerouteOriginLon ?: JSONObject.NULL)
                .put("reroute_trigger", event.rerouteTrigger ?: JSONObject.NULL)
                .put("from_follows_gps", event.fromFollowsGps)
                .put("route_not_joined", event.routeNotJoined)
                .put("simulating", event.simulating)
                .put("standstill", dr?.standstill ?: JSONObject.NULL)
        if (dr != null) {
            o
                .put("dr_mode", dr.mode.name)
                .put("dr_seed_lat", dr.seedLat ?: JSONObject.NULL)
                .put("dr_seed_lon", dr.seedLon ?: JSONObject.NULL)
                .put("dr_seed_kind", dr.seedKind)
                .put("dr_integrated_m", num(dr.integratedM))
                .put("dr_speed_mps", num(dr.speedMps))
                .put("dr_speed_uncertainty_mps", num(dr.speedUncertaintyMps))
                .put("dr_forward_axis", dr.forwardAxisState)
                .put("dr_accel_bias_mps2", num(dr.accelBiasMps2))
                .put("dr_accel_bias_confidence", num(dr.accelBiasConfidence))
                .put("dr_linear_accel_source", dr.linearAccelSource)
                .put("dr_bearing_deg", num(dr.bearingDeg))
                .put("dr_along_m", num(dr.alongM))
                .put("dr_blending", dr.blending)
        }
        maybeFlagJump(event, o)
        enqueue(o)
        overlay =
            Overlay(
                accuracyM = overlay.accuracyM,
                satsUsed = satsUsed,
                satsInView = satsInView,
                source = event.source,
                standstill = dr?.standstill?.toString() ?: "n/a",
                snapIndex = event.snapIndex,
                alongM = event.alongM,
                offRoute = event.offRoute,
                hasSample = true,
            )
    }

    fun noteAccuracy(accuracyM: Float?) {
        if (!enabled.get()) return
        val cur = overlay
        overlay = cur.copy(accuracyM = accuracyM)
    }

    /**
     * Shadow standstill sample. Log-only; does not change positioning.
     * No-ops when diagnostics are off.
     */
    fun logStandstillShadow(
        sample: StandstillShadowGate.Sample,
        drMode: String,
    ) {
        if (!enabled.get()) return
        val o =
            JSONObject()
                .put("kind", "standstill_shadow")
                .put("wall_ms", sample.tMs)
                .put("elapsed_realtime_ms", SystemClock.elapsedRealtime())
                .put("dr_mode", drMode)
                .put("gate_state", sample.state.name.lowercase())
                .put(
                    "accel_abs_mps2",
                    sample.accelAbsMps2?.let { num(it) } ?: JSONObject.NULL,
                ).put(
                    "yaw_abs_deg_per_s",
                    sample.yawAbsDegPerS?.let { num(it) } ?: JSONObject.NULL,
                ).put(
                    "gnss_speed_mps",
                    sample.gnssSpeedMps?.let { num(it) } ?: JSONObject.NULL,
                ).put(
                    "gnss_accuracy_m",
                    sample.gnssAccuracyM?.toDouble()?.let { num(it) } ?: JSONObject.NULL,
                )
        enqueue(o)
        if (sample.falseStandstill) {
            enqueue(
                JSONObject()
                    .put("kind", "false_standstill")
                    .put("wall_ms", sample.tMs)
                    .put("elapsed_realtime_ms", SystemClock.elapsedRealtime())
                    .put("dr_mode", drMode)
                    .put("gnss_speed_mps", num(sample.gnssSpeedMps ?: Double.NaN))
                    .put(
                        "gnss_accuracy_m",
                        sample.gnssAccuracyM?.toDouble()?.let { num(it) } ?: JSONObject.NULL,
                    ),
            )
        }
        if (sample.missedStandstill) {
            enqueue(
                JSONObject()
                    .put("kind", "missed_standstill")
                    .put("wall_ms", sample.tMs)
                    .put("elapsed_realtime_ms", SystemClock.elapsedRealtime())
                    .put("dr_mode", drMode)
                    .put("gnss_speed_mps", num(sample.gnssSpeedMps ?: Double.NaN))
                    .put(
                        "gnss_accuracy_m",
                        sample.gnssAccuracyM?.toDouble()?.let { num(it) } ?: JSONObject.NULL,
                    ),
            )
        }
    }

    fun logRoute(
        wallMs: Long,
        samples: List<RouteSimSample>,
        fromFollowsGps: Boolean,
        routeNotJoined: Boolean,
    ) {
        if (!enabled.get()) return
        val arr = JSONArray()
        for (s in samples) {
            arr.put(
                JSONObject()
                    .put("lat", s.lat)
                    .put("lon", s.lon)
                    .put("cum_m", s.cumM)
                    .put("speed_kmh", s.speedKmh),
            )
        }
        enqueue(
            JSONObject()
                .put("kind", "route")
                .put("wall_ms", wallMs)
                .put("from_follows_gps", fromFollowsGps)
                .put("route_not_joined", routeNotJoined)
                .put("samples", arr),
        )
    }

    /** Block until the queue has been written. Tests only. */
    fun flushForTest(timeoutMs: Long = 2_000L) {
        val deadline = SystemClock.elapsedRealtime() + timeoutMs
        while (queue.isNotEmpty() && SystemClock.elapsedRealtime() < deadline) {
            Thread.sleep(10)
        }
        val t = writerThread
        if (t != null) {
            // The writer flushes on each line; a short pause covers the last write.
            Thread.sleep(30)
        }
    }

    private fun maybeFlagJump(
        event: NavEvent,
        line: JSONObject,
    ) {
        val prevAlong = lastAlongM
        val prevIndex = lastSnapIndex
        val alongDrop =
            prevAlong.isFinite() && event.alongM.isFinite() && prevAlong - event.alongM > JUMP_ALONG_DROP_M
        val indexBack = prevIndex >= 0 && event.snapIndex >= 0 && event.snapIndex < prevIndex
        lastAlongM = event.alongM
        lastSnapIndex = event.snapIndex
        if (!alongDrop && !indexBack) return
        line.put("jump", true)
        enqueue(
            JSONObject()
                .put("kind", "jump")
                .put("wall_ms", event.wallMs)
                .put("elapsed_realtime_ms", event.elapsedRealtimeMs)
                .put("window_ms", JUMP_WINDOW_MS)
                .put("prev_along_m", if (prevAlong.isFinite()) prevAlong else JSONObject.NULL)
                .put("along_m", event.alongM)
                .put("delta_along_m", if (prevAlong.isFinite() && event.alongM.isFinite()) event.alongM - prevAlong else JSONObject.NULL)
                .put("prev_snap_index", prevIndex)
                .put("snap_index", event.snapIndex)
                .put("source", event.source)
                .put("reason", if (alongDrop) "along_drop" else "snap_index_back")
                .put("off_route_reason", event.offRouteReason)
                .put("reroute_trigger", event.rerouteTrigger ?: JSONObject.NULL),
        )
    }

    private fun logSessionHeader(context: Context) {
        val cfg = DrConfig()
        val lm = context.getSystemService(LocationManager::class.java)
        val providers = runCatching { lm?.allProviders ?: emptyList() }.getOrDefault(emptyList())
        val enabledProviders =
            providers.filter { name ->
                runCatching { lm?.isProviderEnabled(name) == true }.getOrDefault(false)
            }
        val unusual =
            providers.filter { name ->
                name != LocationManager.GPS_PROVIDER &&
                    name != LocationManager.NETWORK_PROVIDER &&
                    name != LocationManager.PASSIVE_PROVIDER &&
                    name != "fused"
            }
        val mockOp = mockLocationOp(context)
        enqueue(
            JSONObject()
                .put("kind", "session")
                .put("wall_ms", System.currentTimeMillis())
                .put("elapsed_realtime_ms", SystemClock.elapsedRealtime())
                .put("providers", JSONArray(providers))
                .put("providers_enabled", JSONArray(enabledProviders))
                .put("unusual_providers", JSONArray(unusual))
                .put("test_provider_registered", unusual.isNotEmpty())
                .put("mock_location_appop", mockOp)
                .put("prediction", "none")
                .put("standstill_detector", false)
                .put("dr_good_accuracy_m", cfg.goodAccuracyM.toDouble())
                .put("dr_degrade_accuracy_m", cfg.gpsDegradeAccuracyM.toDouble())
                .put("dr_degrade_hold_ms", cfg.gpsDegradeHoldMs)
                .put("dr_min_speed_mps", cfg.minSpeedMps)
                .put("dr_accel_deadband_mps2", cfg.accelDeadbandMps2),
        )
    }

    private fun mockLocationOp(context: Context): String {
        return runCatching {
            val appOps =
                context.getSystemService(android.app.AppOpsManager::class.java)
                    ?: return "unavailable"
            val mode =
                appOps.unsafeCheckOpNoThrow(
                    android.app.AppOpsManager.OPSTR_MOCK_LOCATION,
                    android.os.Process.myUid(),
                    context.packageName,
                )
            when (mode) {
                android.app.AppOpsManager.MODE_ALLOWED -> "allowed"
                android.app.AppOpsManager.MODE_IGNORED -> "ignored"
                android.app.AppOpsManager.MODE_ERRORED -> "errored"
                android.app.AppOpsManager.MODE_DEFAULT -> "default"
                else -> "mode_$mode"
            }
        }.getOrDefault("unavailable")
    }

    private fun num(v: Double): Any = if (v.isFinite()) v else JSONObject.NULL

    private fun enqueue(o: JSONObject) {
        if (!enabled.get()) return
        val line = o.toString()
        if (!queue.offer(line)) {
            droppedLines += 1
        }
    }

    private fun resetTrackers() {
        lastAlongM = Double.NaN
        lastSnapIndex = -1
        droppedLines = 0L
        overlay = Overlay(null, -1, -1, "", "n/a", -1, 0.0, false, false)
        synchronized(imuLock) { imu.reset() }
    }

    private fun openWriterLocked(logsDir: File) {
        closeWriterLocked()
        val stamp =
            SimpleDateFormat("yyyyMMdd-HHmmss", Locale.US)
                .apply {
                    timeZone = TimeZone.getTimeZone("UTC")
                }.format(Date())
        val file = File(logsDir, "location-diag-$stamp.jsonl")
        sessionFile = file
        running.set(true)
        val thread =
            Thread({
                writeLoop(logsDir, file)
            }, "location-diag")
        thread.isDaemon = true
        writerThread = thread
        thread.start()
    }

    private fun closeWriterLocked() {
        running.set(false)
        queue.offer("")
        val thread = writerThread
        writerThread = null
        if (thread != null && thread !== Thread.currentThread()) {
            runCatching { thread.join(500) }
        }
        queue.clear()
        sessionFile = null
    }

    private fun writeLoop(
        logsDir: File,
        initial: File,
    ) {
        var file = initial
        var writer = openAppend(file)
        var bytes = file.length()
        try {
            while (running.get() || queue.isNotEmpty()) {
                val line = queue.poll(1, TimeUnit.SECONDS)
                if (!line.isNullOrEmpty()) {
                    writer.write(line)
                    writer.write("\n")
                    bytes += line.length + 1L
                    writer.flush()
                    if (bytes >= maxBytes) {
                        writer.close()
                        file = rotate(logsDir)
                        writer = openAppend(file)
                        bytes = 0L
                        sessionFile = file
                    }
                }
                flushImuIfDue(writer)
            }
        } catch (_: InterruptedException) {
            Thread.currentThread().interrupt()
        } finally {
            runCatching { writer.flush() }
            runCatching { writer.close() }
        }
    }

    private fun flushImuIfDue(writer: BufferedWriter) {
        if (!enabled.get()) return
        val now = SystemClock.elapsedRealtime()
        if (lastImuFlushElapsedMs == 0L) {
            lastImuFlushElapsedMs = now
            return
        }
        if (now - lastImuFlushElapsedMs < 1_000L) return
        val snap =
            synchronized(imuLock) {
                val copy = imu
                imu = ImuAccum()
                copy
            }
        lastImuFlushElapsedMs = now
        if (snap.ax.n == 0 && snap.gx.n == 0) return
        val o =
            JSONObject()
                .put("kind", "imu")
                .put("wall_ms", System.currentTimeMillis())
                .put("elapsed_realtime_ms", now)
                .put("n_accel", snap.ax.n)
                .put("n_gyro", snap.gx.n)
                .put("mean_ax", snap.ax.mean)
                .put("mean_ay", snap.ay.mean)
                .put("mean_az", snap.az.mean)
                .put("var_ax", snap.ax.variance())
                .put("var_ay", snap.ay.variance())
                .put("var_az", snap.az.variance())
                .put("mean_gx", snap.gx.mean)
                .put("mean_gy", snap.gy.mean)
                .put("mean_gz", snap.gz.mean)
                .put("var_gx", snap.gx.variance())
                .put("var_gy", snap.gy.variance())
                .put("var_gz", snap.gz.variance())
        writer.write(o.toString())
        writer.write("\n")
        writer.flush()
    }

    private fun rotate(logsDir: File): File {
        val files =
            logsDir
                .listFiles { f ->
                    f.name.startsWith("location-diag-") && f.name.endsWith(".jsonl")
                }?.sortedBy { it.name }
                .orEmpty()
        val overflow = files.size - MAX_FILES + 1
        if (overflow > 0) {
            files.take(overflow).forEach { runCatching { it.delete() } }
        }
        val stamp =
            SimpleDateFormat("yyyyMMdd-HHmmss-SSS", Locale.US)
                .apply {
                    timeZone = TimeZone.getTimeZone("UTC")
                }.format(Date())
        return File(logsDir, "location-diag-$stamp.jsonl")
    }

    private fun openAppend(file: File): BufferedWriter = BufferedWriter(OutputStreamWriter(FileOutputStream(file, true), StandardCharsets.UTF_8))

    private fun startSensors(context: Context) {
        stopSensors()
        val fine =
            ContextCompat.checkSelfPermission(context, Manifest.permission.ACCESS_FINE_LOCATION) ==
                PackageManager.PERMISSION_GRANTED
        if (fine) {
            val lm = context.getSystemService(LocationManager::class.java)
            if (lm != null) {
                val cb =
                    object : GnssStatus.Callback() {
                        override fun onSatelliteStatusChanged(status: GnssStatus) {
                            var used = 0
                            var sum = 0.0
                            val n = status.satelliteCount
                            for (i in 0 until n) {
                                if (status.usedInFix(i)) {
                                    used += 1
                                    sum += status.getCn0DbHz(i).toDouble()
                                }
                            }
                            satsInView = n
                            satsUsed = used
                            meanCn0Used = if (used > 0) sum / used else Double.NaN
                        }
                    }
                gnssCallback = cb
                runCatching {
                    if (Build.VERSION.SDK_INT >= 30) {
                        lm.registerGnssStatusCallback(
                            java.util.concurrent.Executor { command -> command.run() },
                            cb,
                        )
                    } else {
                        @Suppress("DEPRECATION")
                        lm.registerGnssStatusCallback(cb, Handler(Looper.getMainLooper()))
                    }
                }
            }
        }
        val sm = context.getSystemService(SensorManager::class.java) ?: return
        sensorManager = sm
        val listener =
            object : SensorEventListener {
                override fun onSensorChanged(event: SensorEvent) {
                    if (!enabled.get()) return
                    val v = event.values
                    if (v.size < 3) return
                    synchronized(imuLock) {
                        when (event.sensor.type) {
                            Sensor.TYPE_LINEAR_ACCELERATION -> {
                                imu.ax.add(v[0].toDouble())
                                imu.ay.add(v[1].toDouble())
                                imu.az.add(v[2].toDouble())
                            }
                            Sensor.TYPE_GYROSCOPE -> {
                                imu.gx.add(v[0].toDouble())
                                imu.gy.add(v[1].toDouble())
                                imu.gz.add(v[2].toDouble())
                            }
                        }
                    }
                }

                override fun onAccuracyChanged(
                    sensor: Sensor?,
                    accuracy: Int,
                ) {}
            }
        imuListener = listener
        sm.getDefaultSensor(Sensor.TYPE_LINEAR_ACCELERATION)?.let {
            sm.registerListener(listener, it, SensorManager.SENSOR_DELAY_NORMAL)
        }
        sm.getDefaultSensor(Sensor.TYPE_GYROSCOPE)?.let {
            sm.registerListener(listener, it, SensorManager.SENSOR_DELAY_NORMAL)
        }
    }

    private fun stopSensors() {
        val ctx = appContext
        val cb = gnssCallback
        if (ctx != null && cb != null) {
            val lm = ctx.getSystemService(LocationManager::class.java)
            runCatching { lm?.unregisterGnssStatusCallback(cb) }
        }
        gnssCallback = null
        val sm = sensorManager
        val listener = imuListener
        if (sm != null && listener != null) {
            runCatching { sm.unregisterListener(listener) }
        }
        sensorManager = null
        imuListener = null
        satsInView = -1
        satsUsed = -1
        meanCn0Used = Double.NaN
    }

    fun publicLocationDescription(): String = "Android/data/no.navi.app/files/location-diag"

    /** Newest session file, if any. Does not create one. */
    fun newestSession(context: Context): File? {
        val dir =
            logsDirOverride
                ?: context.getExternalFilesDir(null)?.let { File(it, DIR_NAME) }
                ?: File(context.filesDir, DIR_NAME)
        return dir
            .listFiles { f ->
                f.name.startsWith("location-diag-") && f.name.endsWith(".jsonl")
            }?.maxByOrNull { it.lastModified() }
    }
}

internal fun locationIsMock(loc: android.location.Location): Boolean =
    if (Build.VERSION.SDK_INT >= 31) {
        loc.isMock
    } else {
        @Suppress("DEPRECATION")
        loc.isFromMockProvider
    }

internal fun displayedPositionSource(
    provider: String,
    estimated: Boolean,
    positionLost: Boolean,
): String =
    when {
        provider == "navi-route-sim" || provider == "navi-test-inject" -> "sim"
        positionLost -> "lost"
        estimated -> "dr"
        provider == LocationManager.GPS_PROVIDER -> "gps"
        provider == LocationManager.NETWORK_PROVIDER -> "network"
        provider == LocationManager.PASSIVE_PROVIDER -> "passive"
        provider == "fused" -> "fused"
        else -> if (provider.isBlank()) "unknown" else provider
    }

/** Age of a fix against [SystemClock.elapsedRealtimeNanos], or -1 when unknown. */
internal fun fixAgeMs(loc: android.location.Location): Long {
    val elapsed = loc.elapsedRealtimeNanos
    if (elapsed <= 0L) return -1L
    val now = SystemClock.elapsedRealtimeNanos()
    return max(0L, (now - elapsed) / 1_000_000L)
}
