package no.navi.app

import android.content.Context
import android.content.Intent
import android.media.MediaScannerConnection
import android.os.Environment
import android.os.StatFs
import android.util.Log
import androidx.core.content.FileProvider
import org.json.JSONArray
import uniffi.navi.CorridorRouteResult
import uniffi.navi.detectedParallelism
import uniffi.navi.routingWorkerCount
import uniffi.navi.setRoutePlanTimingEnabled
import java.io.BufferedWriter
import java.io.File
import java.io.FileOutputStream
import java.io.OutputStreamWriter
import java.nio.charset.StandardCharsets
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.TimeZone
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference
import kotlin.math.abs
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.sin
import kotlin.math.sqrt

/**
 * Session-scoped structured diagnostic log.
 *
 * Writes under shared storage so a user can copy the file over a plain USB/MTP
 * cable without adb: prefer Documents/debug (visible as Internal storage →
 * Documents → debug). Top-level /debug is not creatable without
 * MANAGE_EXTERNAL_STORAGE on modern Android. Basemap/DEM downloads stay in
 * app-private storage — this class does not touch them.
 *
 * Deliberately narrow: only the categories listed in [Category]. When disabled,
 * every public write entry point is a genuine no-op (no file create, no I/O).
 *
 * GPS rate limit: at most one [Category.GPS] line every [GPS_MIN_INTERVAL_MS],
 * or sooner when the fix moves more than [GPS_MIN_MOVE_M] metres — not a raw
 * LocationListener firehose. GPS lines include MapLibre `zoom` when known.
 *
 * CAMERA lines: on camera-idle when zoom moves by [CAMERA_ZOOM_DELTA] or every
 * [CAMERA_MIN_INTERVAL_MS].
 *
 * SYSTEM snapshots: at most once every [SYSTEM_INTERVAL_MS] while enabled.
 */
object DiagnosticLog {
    private const val TAG = "NaviDiag"
    private const val DIR_NAME = "debug"
    private const val MAX_SESSION_FILES = 10

    /** Min gap between GPS lines unless the fix moved meaningfully. */
    const val GPS_MIN_INTERVAL_MS = 3_000L

    /** Movement threshold that bypasses the GPS time rate limit. */
    const val GPS_MIN_MOVE_M = 25.0

    /** Min gap between SYSTEM resource snapshots. */
    const val SYSTEM_INTERVAL_MS = 300_000L

    /** Min zoom change that forces a CAMERA line before the interval elapses. */
    const val CAMERA_ZOOM_DELTA = 0.15

    /** Min gap between CAMERA lines when zoom is stable. */
    const val CAMERA_MIN_INTERVAL_MS = 5_000L

    enum class Category {
        GPS,
        CAMERA,
        TOGGLE,
        SETTING_SAVED,
        ROUTE_PLAN,
        ROUTE_PLAN_STAGES,
        ECO_CALC,
        POI_FOUND,
        PAUSE_PLANNED,
        INSTRUCTION,
        FUEL_ADDED,
        SYSTEM,
    }

    private val enabled = AtomicBoolean(false)
    private val sessionFile = AtomicReference<File?>(null)
    private val writer = AtomicReference<BufferedWriter?>(null)
    private val lock = Any()

    private var lastGpsMs = 0L
    private var lastGpsLat = Double.NaN
    private var lastGpsLon = Double.NaN
    private var lastSystemMs = 0L
    private var lastInstructionIndex = -1
    private var lastCameraZoom = Double.NaN
    private var lastCameraMs = 0L
    private var appContext: Context? = null
    private var logsDirOverride: File? = null

    /** Latest MapLibre camera zoom observed (NaN until first report). */
    @Volatile
    var lastReportedZoom: Double = Double.NaN
        private set

    /** Test hook: redirect log dir (and skip Context). */
    fun setLogsDirForTest(dir: File?) {
        synchronized(lock) {
            closeSessionLocked()
            logsDirOverride = dir
            lastGpsMs = 0L
            lastGpsLat = Double.NaN
            lastGpsLon = Double.NaN
            lastSystemMs = 0L
            lastInstructionIndex = -1
            lastCameraZoom = Double.NaN
            lastCameraMs = 0L
            lastReportedZoom = Double.NaN
        }
    }

    fun isEnabled(): Boolean = enabled.get()

    /**
     * Human-readable location for UI / docs (Documents/debug preferred).
     * Does not create directories.
     */
    fun publicLocationDescription(): String = "Internal storage / Documents / debug"

    /**
     * Enable/disable logging. Turning on opens a new date-stamped session file.
     * Turning off flushes and closes the current file. No file is created while off.
     */
    fun setEnabled(
        context: Context,
        on: Boolean,
    ) {
        MapHudPrefs.saveDiagnosticLogging(context, on)
        applyEnabled(context, on)
    }

    /** Restore persisted toggle without creating a file when off. */
    fun restoreFromPrefs(context: Context) {
        val on = MapHudPrefs.loadDiagnosticLogging(context)
        applyEnabled(context, on)
    }

    fun applyEnabled(
        context: Context,
        on: Boolean,
    ) {
        appContext = context.applicationContext
        applyEnabled(resolveLogsDir(context), on)
    }

    /**
     * Enable against an already-resolved directory (unit tests / callers that
     * already hold a [File]). Prefer [applyEnabled] with [Context] on device.
     */
    fun applyEnabled(
        logsDir: File,
        on: Boolean,
    ) {
        synchronized(lock) {
            if (on == enabled.get() && (!on || writer.get() != null)) {
                enabled.set(on)
                syncNativeTimingLocked(on)
                return
            }
            if (!on) {
                enabled.set(false)
                syncNativeTimingLocked(false)
                closeSessionLocked()
                return
            }
            enabled.set(true)
            syncNativeTimingLocked(true)
            openNewSessionLocked(logsDirectory(logsDir))
        }
    }

    /** Mirror the Diagnostic toggle into native per-stage plan timing. */
    private fun syncNativeTimingLocked(on: Boolean) {
        // Silent on JVM unit tests (no libnavi / Log mock). Device failures are rare.
        runCatching { setRoutePlanTimingEnabled(on) }
    }

    /**
     * Resolve the session-log directory.
     *
     * Order: test override → Documents/debug → Download/debug → app-private
     * fallback (last resort only; not MTP-visible on Android 11+).
     */
    fun resolveLogsDir(context: Context): File {
        logsDirOverride?.let { override ->
            if (!override.exists()) override.mkdirs()
            return override
        }
        val candidates =
            listOf(
                File(
                    Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOCUMENTS),
                    DIR_NAME,
                ),
                File(
                    Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS),
                    DIR_NAME,
                ),
            )
        for (dir in candidates) {
            if (ensureWritableDir(dir)) {
                Log.i(TAG, "diagnostic logs dir=${dir.absolutePath}")
                return dir
            }
        }
        val fallback = File(context.filesDir, "diagnostic_logs")
        if (!fallback.exists()) fallback.mkdirs()
        Log.w(
            TAG,
            "public Documents/Download debug dirs not writable; falling back to ${fallback.absolutePath}",
        )
        return fallback
    }

    /** @deprecated Prefer [resolveLogsDir]; kept for call sites that still pass filesDir. */
    fun logsDirectory(filesDir: File): File {
        logsDirOverride?.let { override ->
            if (!override.exists()) override.mkdirs()
            return override
        }
        appContext?.let { return resolveLogsDir(it) }
        val legacy = File(filesDir, "diagnostic_logs")
        if (!legacy.exists()) legacy.mkdirs()
        return legacy
    }

    fun currentSessionFile(): File? = sessionFile.get()

    fun listSessionFiles(context: Context): List<File> = listSessionFilesIn(resolveLogsDir(context))

    fun listSessionFiles(filesDir: File): List<File> = listSessionFilesIn(logsDirectory(filesDir))

    private fun listSessionFilesIn(dir: File): List<File> =
        dir
            .listFiles { f -> f.isFile && f.name.startsWith("navi_session_") && f.name.endsWith(".log") }
            ?.sortedByDescending { it.lastModified() }
            ?: emptyList()

    fun shareLatest(
        context: Context,
    ): Boolean {
        val file = currentSessionFile() ?: listSessionFiles(context).firstOrNull() ?: return false
        return shareFile(context, file)
    }

    fun shareFile(
        context: Context,
        file: File,
    ): Boolean {
        if (!file.isFile) return false
        return runCatching {
            // Prefer a cache copy so FileProvider stays under app-owned paths even
            // when the live session file lives under Documents/debug.
            val shareCopy = File(context.cacheDir, file.name)
            file.copyTo(shareCopy, overwrite = true)
            val uri =
                FileProvider.getUriForFile(
                    context,
                    "${context.packageName}.fileprovider",
                    shareCopy,
                )
            val intent =
                Intent(Intent.ACTION_SEND).apply {
                    type = "text/plain"
                    putExtra(Intent.EXTRA_STREAM, uri)
                    putExtra(Intent.EXTRA_SUBJECT, file.name)
                    addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                }
            context.startActivity(Intent.createChooser(intent, "Export diagnostic log"))
            true
        }.getOrElse {
            Log.w(TAG, "share failed: ${it.message}")
            false
        }
    }

    fun formatLine(
        category: Category,
        fields: Map<String, Any?>,
        epochMs: Long = System.currentTimeMillis(),
    ): String {
        val ts = isoUtc(epochMs)
        val body =
            fields.entries.joinToString(" ") { (k, v) ->
                val raw =
                    when (v) {
                        null -> ""
                        is Double -> formatNum(v)
                        is Float -> formatNum(v.toDouble())
                        is String -> quoteIfNeeded(v)
                        else -> v.toString()
                    }
                "$k=$raw"
            }
        return "$ts | ${category.name} | $body"
    }

    fun write(
        category: Category,
        fields: Map<String, Any?>,
    ) {
        if (!enabled.get()) return
        val line = formatLine(category, fields)
        synchronized(lock) {
            if (!enabled.get()) return
            val w = writer.get() ?: return
            runCatching {
                w.write(line)
                w.newLine()
                w.flush()
            }.onFailure { Log.w(TAG, "write failed: ${it.message}") }
        }
    }

    fun logToggle(
        name: String,
        value: Any?,
        extra: Map<String, Any?> = emptyMap(),
    ) {
        write(Category.TOGGLE, mapOf("name" to name, "value" to value) + extra)
    }

    fun logSettingSaved(
        key: String,
        value: Any?,
    ) {
        write(Category.SETTING_SAVED, mapOf("key" to key, "value" to value))
    }

    fun logGps(
        lat: Double,
        lon: Double,
        altAslM: Double?,
        accuracyM: Float?,
        satellites: Int?,
        fixType: String,
        zoom: Double? = null,
        pitchDeg: Double? = null,
        bearingDeg: Double? = null,
        nowMs: Long = System.currentTimeMillis(),
    ) {
        if (!enabled.get()) return
        val moved =
            if (lastGpsLat.isNaN()) {
                true
            } else {
                haversineM(lastGpsLat, lastGpsLon, lat, lon) >= GPS_MIN_MOVE_M
            }
        val due = nowMs - lastGpsMs >= GPS_MIN_INTERVAL_MS
        if (!moved && !due) return
        lastGpsMs = nowMs
        lastGpsLat = lat
        lastGpsLon = lon
        if (zoom != null && !zoom.isNaN()) {
            lastReportedZoom = zoom
        }
        write(
            Category.GPS,
            mapOf(
                "fix" to fixType,
                "lat" to lat,
                "lon" to lon,
                "alt_asl_m" to altAslM,
                "accuracy_m" to accuracyM?.toDouble(),
                "satellites" to satellites,
                "zoom" to zoom,
                "pitch_deg" to pitchDeg,
                "bearing_deg" to bearingDeg,
            ).filterValues { it != null },
        )
    }

    /**
     * MapLibre camera idle: log zoom (and pitch/bearing) when zoom moves by
     * ≥ [CAMERA_ZOOM_DELTA] or at least every [CAMERA_MIN_INTERVAL_MS].
     */
    fun logCamera(
        zoom: Double,
        lat: Double? = null,
        lon: Double? = null,
        pitchDeg: Double? = null,
        bearingDeg: Double? = null,
        nowMs: Long = System.currentTimeMillis(),
    ) {
        if (!enabled.get()) return
        if (zoom.isNaN()) return
        lastReportedZoom = zoom
        val zoomMoved = lastCameraZoom.isNaN() || abs(zoom - lastCameraZoom) >= CAMERA_ZOOM_DELTA
        val due = nowMs - lastCameraMs >= CAMERA_MIN_INTERVAL_MS
        if (!zoomMoved && !due) return
        lastCameraMs = nowMs
        lastCameraZoom = zoom
        write(
            Category.CAMERA,
            mapOf(
                "zoom" to zoom,
                "lat" to lat,
                "lon" to lon,
                "pitch_deg" to pitchDeg,
                "bearing_deg" to bearingDeg,
            ).filterValues { it != null },
        )
    }

    fun logRoutePlanStart(
        profile: String,
        startLat: Double,
        startLon: Double,
        endLat: Double,
        endLon: Double,
    ) {
        lastInstructionIndex = -1
        write(
            Category.ROUTE_PLAN,
            mapOf(
                "status" to "start",
                "profile" to profile,
                "start" to String.format(Locale.US, "%.6f,%.6f", startLat, startLon),
                "end" to String.format(Locale.US, "%.6f,%.6f", endLat, endLon),
            ),
        )
    }

    fun logRoutePlanComplete(result: CorridorRouteResult) {
        val breaks =
            runCatching {
                JSONArray(result.breakPoisJson.ifBlank { "[]" }).length()
            }.getOrDefault(0)
        val planMs = parseReportDouble(result.report, "plan_duration_ms")
        val packHit = parseReportToken(result.report, "pack_hit")
        val poiPackHit = parseReportToken(result.report, "poi_pack_hit")
        write(
            Category.ROUTE_PLAN,
            mapOf(
                "status" to "complete",
                "distance_km" to result.distanceKm,
                "eta_min" to result.etaMinutes,
                "breaks" to breaks,
                "plan_duration_ms" to planMs,
                "pack_hit" to packHit,
                "poi_pack_hit" to poiPackHit,
                "toll_policy" to result.tollPolicy,
                "pad_attempts_json" to result.padAttemptsJson,
                "search_expansions" to result.searchExpansions.toLong(),
                "search_terminate_reason" to result.searchTerminateReason,
                "toll_avoidance_incomplete" to result.tollAvoidanceIncomplete,
                "route_uses_tolls" to result.routeUsesTolls,
            ).filterValues { it != null },
        )
        logRoutePlanStagesFromReport(result.report)
        logEcoFromReport(result.report)
        logPoisFromJson(result.breakPoisJson)
        logPausesFromDaysJson(result.daysJson)
        logPausesFromBreakPois(result.breakPoisJson)
        logInstructionsIssued(result.maneuversJson)
    }

    /**
     * Emit one greppable [Category.ROUTE_PLAN_STAGES] line from a native plan
     * report (`ROUTE_PLAN_STAGES | key=ms …`). No-op when logging is off or the
     * report has no stage line.
     */
    fun logRoutePlanStagesFromReport(report: String) {
        if (!enabled.get()) return
        val raw =
            report.lineSequence().firstOrNull { line ->
                line.trimStart().startsWith("ROUTE_PLAN_STAGES")
            } ?: return
        val body =
            raw
                .substringAfter('|', missingDelimiterValue = "")
                .trim()
                .ifEmpty {
                    raw
                        .trimStart()
                        .removePrefix("ROUTE_PLAN_STAGES")
                        .trim()
                        .trimStart('|')
                        .trim()
                }
        if (body.isEmpty()) return
        val fields = linkedMapOf<String, Any?>()
        for (part in body.split(Regex("\\s+"))) {
            if (part.isEmpty()) continue
            val eq = part.indexOf('=')
            if (eq <= 0) continue
            val key = part.substring(0, eq)
            val value = part.substring(eq + 1)
            fields[key] = value.toLongOrNull() ?: value.toDoubleOrNull() ?: value
        }
        if (fields.isNotEmpty()) {
            write(Category.ROUTE_PLAN_STAGES, fields)
        }
    }

    fun logRoutePlanFailed(
        reason: String,
        result: CorridorRouteResult? = null,
    ) {
        write(
            Category.ROUTE_PLAN,
            buildMap {
                put("status", "failed")
                put("reason", reason)
                if (result != null) {
                    put("toll_policy", result.tollPolicy)
                    put("pad_attempts_json", result.padAttemptsJson)
                    put("search_expansions", result.searchExpansions.toLong())
                    put("search_terminate_reason", result.searchTerminateReason)
                    put("toll_avoidance_incomplete", result.tollAvoidanceIncomplete)
                    put("route_uses_tolls", result.routeUsesTolls)
                }
            },
        )
    }

    fun logRoutePlanCancelled(
        reason: String,
        report: String,
    ) {
        write(
            Category.ROUTE_PLAN,
            mapOf(
                "status" to "cancelled",
                "reason" to reason,
                "plan_duration_ms" to parseReportDouble(report, "plan_duration_ms"),
                "pack_hit" to parseReportToken(report, "pack_hit"),
                "poi_pack_hit" to parseReportToken(report, "poi_pack_hit"),
            ).filterValues { it != null },
        )
        logRoutePlanStagesFromReport(report)
    }

    fun logFuelAdded(
        amount: Double,
        unit: String,
        tankPctAfter: Double?,
    ) {
        write(
            Category.FUEL_ADDED,
            mapOf(
                "amount_l" to if (unit == "gallons") amount * 3.785411784 else amount,
                "unit" to unit,
                "tank_pct_after" to tankPctAfter,
            ).filterValues { it != null },
        )
    }

    fun logInstructionCompleted(
        index: Int,
        of: Int,
    ) {
        write(
            Category.INSTRUCTION,
            mapOf("status" to "completed", "index" to index, "of" to of),
        )
    }

    /** Call when the active maneuver index advances (issued = newly current). */
    fun onManeuverProgress(
        index: Int,
        of: Int,
        kind: String?,
        street: String?,
        distanceM: Double?,
    ) {
        if (!enabled.get()) return
        if (index < 0 || of <= 0) return
        if (index != lastInstructionIndex) {
            if (lastInstructionIndex >= 0) {
                logInstructionCompleted(lastInstructionIndex, of)
            }
            lastInstructionIndex = index
            write(
                Category.INSTRUCTION,
                mapOf(
                    "status" to "issued",
                    "index" to index,
                    "of" to of,
                    "maneuver" to (kind ?: "unknown"),
                    "street" to (street ?: ""),
                    "distance_m" to distanceM,
                ).filterValues { it != null },
            )
        }
    }

    fun maybeLogSystem(
        filesDir: File,
        nowMs: Long = System.currentTimeMillis(),
    ) {
        if (!enabled.get()) return
        if (nowMs - lastSystemMs < SYSTEM_INTERVAL_MS && lastSystemMs != 0L) return
        lastSystemMs = nowMs
        val rt = Runtime.getRuntime()
        val usedMb = (rt.totalMemory() - rt.freeMemory()) / (1024.0 * 1024.0)
        val totalMb = rt.maxMemory() / (1024.0 * 1024.0)
        val diskFreeMb =
            runCatching {
                val path = sessionFile.get()?.parentFile?.absolutePath ?: filesDir.absolutePath
                val s = StatFs(path)
                s.availableBlocksLong * s.blockSizeLong / (1024.0 * 1024.0)
            }.getOrDefault(-1.0)
        val cores = runCatching { detectedParallelism().toInt() }.getOrDefault(rt.availableProcessors())
        val workers = runCatching { routingWorkerCount().toInt() }.getOrDefault(-1)
        write(
            Category.SYSTEM,
            mapOf(
                "mem_used_mb" to usedMb,
                "mem_total_mb" to totalMb,
                "disk_free_mb" to diskFreeMb,
                "cpu_cores_detected" to cores,
                "workers_active" to workers,
            ),
        )
    }

    fun logEcoCalc(
        profile: String,
        ecoMode: Boolean,
        climbM: Double,
        descentM: Double,
        uphillJ: Double,
        downhillJ: Double,
        regenCreditJ: Double,
        netJ: Double,
    ) {
        write(
            Category.ECO_CALC,
            mapOf(
                "profile" to profile,
                "eco_mode" to ecoMode,
                "climb_m" to climbM,
                "descent_m" to descentM,
                "uphill_energy_j" to uphillJ,
                "downhill_energy_j" to downhillJ,
                "regen_credit_j" to regenCreditJ,
                "net_energy_j" to netJ,
            ),
        )
    }

    fun logEcoFromReport(report: String) {
        if (!enabled.get()) return
        val climb = parseReportDouble(report, "eco_climb_m") ?: return
        val descent = parseReportDouble(report, "eco_descent_m") ?: 0.0
        val up = parseReportDouble(report, "eco_uphill_j") ?: 0.0
        val down = parseReportDouble(report, "eco_downhill_j") ?: 0.0
        val regen = parseReportDouble(report, "eco_regen_credit_j") ?: 0.0
        val net = parseReportDouble(report, "eco_net_j") ?: (up + down)
        val profile = parseReportToken(report, "profile") ?: "unknown"
        val ecoMode = parseReportToken(report, "use_eco")?.equals("true", true) ?: true
        logEcoCalc(profile, ecoMode, climb, descent, up, down, regen, net)
    }

    fun logPoisFromJson(breakPoisJson: String) {
        if (!enabled.get()) return
        runCatching {
            val arr = JSONArray(breakPoisJson.ifBlank { "[]" })
            for (i in 0 until arr.length()) {
                val o = arr.optJSONObject(i) ?: continue
                write(
                    Category.POI_FOUND,
                    mapOf(
                        "category" to o.optString("kind").ifBlank { "poi" },
                        "name" to o.optString("name"),
                        "dist_from_route_m" to o.optDouble("dist_m", 0.0),
                    ),
                )
            }
        }
    }

    fun logPausesFromDaysJson(daysJson: String) {
        if (!enabled.get()) return
        runCatching {
            val arr = JSONArray(daysJson.ifBlank { "[]" })
            for (i in 0 until arr.length()) {
                val o = arr.optJSONObject(i) ?: continue
                val kind = o.optString("rest_kind").trim()
                if (kind.isEmpty()) continue
                write(
                    Category.PAUSE_PLANNED,
                    mapOf(
                        "kind" to kind,
                        "position_km" to o.optDouble("end_km", o.optDouble("start_km", 0.0)),
                        "duration_min" to o.optDouble("rest_hours", 0.0) * 60.0,
                        "name" to o.optString("overnight_name"),
                    ).filterValues { it != null && it != "" },
                )
            }
        }
    }

    fun logPausesFromBreakPois(breakPoisJson: String) {
        if (!enabled.get()) return
        runCatching {
            val arr = JSONArray(breakPoisJson.ifBlank { "[]" })
            for (i in 0 until arr.length()) {
                val o = arr.optJSONObject(i) ?: continue
                val kind = o.optString("kind").ifBlank { "interval" }
                if (!isPauseKind(kind)) continue
                write(
                    Category.PAUSE_PLANNED,
                    mapOf(
                        "kind" to kind,
                        "position_km" to o.optDouble("along_km", Double.NaN).takeIf { !it.isNaN() },
                        "duration_min" to o.optDouble("duration_min", Double.NaN).takeIf { !it.isNaN() },
                        "lat" to o.optDouble("lat"),
                        "lon" to o.optDouble("lon"),
                        "name" to o.optString("name"),
                    ).filterValues { it != null && it != "" && !(it is Double && (it as Double).isNaN()) },
                )
            }
        }
    }

    /** Rest / break markers only — water springs and other POIs stay in [logPoisFromJson]. */
    fun isPauseKind(kind: String): Boolean {
        val k = kind.lowercase()
        return k.contains("rest") ||
            k.contains("break") ||
            k.contains("pause") ||
            k.contains("overnight") ||
            k == "interval" ||
            k == "main" ||
            k == "alt" ||
            k == "tent"
    }

    fun logInstructionsIssued(
        @Suppress("UNUSED_PARAMETER") maneuversJson: String,
    ) {
        // Live issued/completed lines come from [onManeuverProgress] during navigation.
    }

    private fun ensureWritableDir(dir: File): Boolean {
        return runCatching {
            if (!dir.exists() && !dir.mkdirs()) return false
            if (!dir.isDirectory) return false
            val probe = File(dir, ".navi_write_probe")
            probe.writeText("ok")
            probe.delete()
            true
        }.getOrDefault(false)
    }

    private fun openNewSessionLocked(logsDir: File) {
        closeSessionLocked()
        if (!logsDir.exists()) logsDir.mkdirs()
        rotateOldSessionsLocked(logsDir)
        val name =
            "navi_session_" +
                SimpleDateFormat("yyyy-MM-dd_HH-mm-ss", Locale.US)
                    .apply { timeZone = TimeZone.getDefault() }
                    .format(Date()) +
                ".log"
        val file = File(logsDir, name)
        val bw =
            BufferedWriter(
                OutputStreamWriter(FileOutputStream(file, true), StandardCharsets.UTF_8),
            )
        sessionFile.set(file)
        writer.set(bw)
        lastGpsMs = 0L
        lastGpsLat = Double.NaN
        lastGpsLon = Double.NaN
        lastSystemMs = 0L
        lastInstructionIndex = -1
        requestMediaScan(file)
    }

    private fun closeSessionLocked() {
        val closed = sessionFile.get()
        writer.getAndSet(null)?.let { w ->
            runCatching {
                w.flush()
                w.close()
            }
        }
        sessionFile.set(null)
        closed?.let { requestMediaScan(it) }
    }

    /** Nudge MTP / gallery indexes so USB browsers see the new/updated file. */
    private fun requestMediaScan(file: File) {
        val ctx = appContext ?: return
        runCatching {
            MediaScannerConnection.scanFile(
                ctx,
                arrayOf(file.absolutePath),
                arrayOf("text/plain"),
                null,
            )
        }
    }

    private fun rotateOldSessionsLocked(dir: File) {
        val files =
            dir
                .listFiles { f -> f.isFile && f.name.startsWith("navi_session_") && f.name.endsWith(".log") }
                ?.sortedByDescending { it.lastModified() }
                ?: return
        files.drop(MAX_SESSION_FILES - 1).forEach { runCatching { it.delete() } }
    }

    private fun isoUtc(epochMs: Long): String {
        val fmt = SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss.SSS'Z'", Locale.US)
        fmt.timeZone = TimeZone.getTimeZone("UTC")
        return fmt.format(Date(epochMs))
    }

    private fun formatNum(v: Double): String =
        if (v == v.toLong().toDouble() && abs(v) < 1e12) {
            v.toLong().toString()
        } else {
            String.format(Locale.US, "%.6f", v).trimEnd('0').trimEnd('.')
        }

    private fun quoteIfNeeded(s: String): String =
        if (s.any { it.isWhitespace() || it == '=' || it == '|' || it == '"' }) {
            "\"" + s.replace("\"", "'") + "\""
        } else {
            s
        }

    private fun parseReportDouble(
        report: String,
        key: String,
    ): Double? {
        val re = Regex("""(?:^|[;\s])$key=([-+0-9.eE]+)""")
        return re
            .find(report)
            ?.groupValues
            ?.getOrNull(1)
            ?.toDoubleOrNull()
    }

    private fun parseReportToken(
        report: String,
        key: String,
    ): String? {
        val re = Regex("""(?:^|[;\s])$key=([^\s;]+)""")
        return re.find(report)?.groupValues?.getOrNull(1)
    }

    private fun haversineM(
        lat1: Double,
        lon1: Double,
        lat2: Double,
        lon2: Double,
    ): Double {
        val r = 6_371_000.0
        val p1 = Math.toRadians(lat1)
        val p2 = Math.toRadians(lat2)
        val dp = Math.toRadians(lat2 - lat1)
        val dl = Math.toRadians(lon2 - lon1)
        val a = sin(dp / 2) * sin(dp / 2) + cos(p1) * cos(p2) * sin(dl / 2) * sin(dl / 2)
        return 2 * r * atan2(sqrt(a), sqrt(1 - a))
    }
}
