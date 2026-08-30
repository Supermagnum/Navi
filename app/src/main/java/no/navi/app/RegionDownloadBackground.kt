package no.navi.app

import android.content.Context
import android.util.Log
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import org.json.JSONObject
import uniffi.navi.bindGeofabrikRegion
import uniffi.navi.downloadProgressSnapshot
import uniffi.navi.geofabrikLatestPbfUrl
import uniffi.navi.geofabrikPathForPbfName
import uniffi.navi.provisionRegionData
import java.io.File
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference

/**
 * Process-scoped region PBF download. Survives Compose cancellation; a
 * force-stop still kills the HTTP stream, but [JOB_FILE] plus the sibling
 * `.partial` let the next launch resume via HTTP Range instead of restarting.
 */
object RegionDownloadBackground {
    const val JOB_FILE = "region-download.json"
    private const val TAG = "RegionDownloadBg"
    private const val MIN_PBF_BYTES = 1_000_000L

    data class Job(
        val url: String,
        val filename: String,
        val geofabrikPath: String,
    )

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val mutex = Mutex()
    private val running = AtomicBoolean(false)
    private val lastStatus = AtomicReference("")
    private val resuming = AtomicBoolean(false)

    fun isRunning(): Boolean = running.get()

    fun isResuming(): Boolean = resuming.get()

    fun statusLine(): String = lastStatus.get()

    fun jobFile(dataDir: File): File = File(dataDir, JOB_FILE)

    fun findPartialPbf(dataDir: File): File? =
        dataDir.listFiles()?.firstOrNull {
            it.isFile && it.name.endsWith(".osm.pbf.partial") && it.length() > 0L
        }

    fun loadJob(dataDir: File): Job? {
        val f = jobFile(dataDir)
        if (!f.isFile) return null
        return runCatching {
            val obj = JSONObject(f.readText())
            Job(
                url = obj.getString("url"),
                filename = obj.getString("filename"),
                geofabrikPath = obj.optString("geofabrikPath"),
            )
        }.getOrNull()
    }

    fun writeJob(
        dataDir: File,
        job: Job,
    ) {
        dataDir.mkdirs()
        jobFile(dataDir).writeText(
            JSONObject()
                .put("url", job.url)
                .put("filename", job.filename)
                .put("geofabrikPath", job.geofabrikPath)
                .toString(),
        )
    }

    fun clearJob(dataDir: File) {
        jobFile(dataDir).delete()
    }

    fun partialBytes(
        dataDir: File,
        filename: String,
    ): Long {
        val partial = File(dataDir, "$filename.partial")
        return if (partial.isFile) partial.length() else 0L
    }

    /**
     * In-progress download: sidecar job whose dest is not yet a complete PBF,
     * or an orphan `.partial` left after a kill before the sidecar was written.
     */
    fun discoverPending(dataDir: File): Job? {
        loadJob(dataDir)?.let { job ->
            val dest = File(dataDir, job.filename)
            if (dest.isFile && dest.length() >= MIN_PBF_BYTES) {
                clearJob(dataDir)
                return null
            }
            return job
        }
        val partial = findPartialPbf(dataDir) ?: return null
        val filename = partial.name.removeSuffix(".partial")
        val path = geofabrikPathForPbfName(filename).trim().ifBlank { return null }
        return Job(
            url = geofabrikLatestPbfUrl(path),
            filename = filename,
            geofabrikPath = path,
        )
    }

    fun uiLine(): String {
        if (!running.get()) return lastStatus.get()
        val snap = runCatching { downloadProgressSnapshot() }.getOrNull() ?: return lastStatus.get()
        if (snap.label.isBlank()) return lastStatus.get()
        val tot = snap.unitsTotal
        val done = snap.unitsDone
        val pct =
            if (tot != null && tot > 0uL) {
                ((done.toDouble() * 100.0) / tot.toDouble()).toInt().coerceIn(0, 100)
            } else {
                null
            }
        return when {
            pct != null && tot != null -> "${snap.label} $pct% ($done / $tot)"
            pct != null -> "${snap.label} $pct%"
            else -> snap.label
        }
    }

    fun ensureStartedFromPending(
        context: Context,
        dataDir: File,
    ) {
        val job = discoverPending(dataDir) ?: return
        ensureStarted(context, dataDir, job.url, job.filename, job.geofabrikPath)
    }

    fun ensureStarted(
        context: Context,
        dataDir: File,
        url: String,
        filename: String,
        geofabrikPath: String,
    ) {
        scope.launch {
            val shouldRun =
                mutex.withLock {
                    if (running.get()) {
                        Log.i(TAG, "already running; skip")
                        return@withLock false
                    }
                    running.set(true)
                    val already = partialBytes(dataDir, filename)
                    resuming.set(already > 0L)
                    val job = Job(url, filename, geofabrikPath)
                    writeJob(dataDir, job)
                    lastStatus.set(
                        if (already > 0L) {
                            "Resuming download…"
                        } else {
                            "Downloading region… 0%"
                        },
                    )
                    Log.i(
                        TAG,
                        "start provision filename=$filename resume_bytes=$already path=$geofabrikPath",
                    )
                    true
                }
            if (!shouldRun) return@launch
            try {
                val report =
                    provisionRegionData(
                        dataDir = dataDir.absolutePath,
                        pbfUrl = url,
                        pbfFilename = filename,
                        elevationTarUrl = null,
                    )
                lastStatus.set(if (report.contains("PASS")) "done" else "failed")
                Log.i(TAG, "finished: ${report.take(240)}")
                if (report.contains("PASS")) {
                    clearJob(dataDir)
                    if (geofabrikPath.isNotBlank()) {
                        MapHudPrefs.saveGeofabrikPath(context, geofabrikPath)
                        runCatching {
                            bindGeofabrikRegion(
                                dataDir = dataDir.absolutePath,
                                geofabrikRegion = geofabrikPath,
                                pbfFilename = filename,
                                localSequence = null,
                            )
                        }
                    }
                    val pbf = File(dataDir, filename)
                    if (pbf.isFile) {
                        PlaceIndexBackground.ensureStarted(pbf, File(dataDir, "place_index.db"))
                        val elev = File(dataDir, "elevation").takeIf { it.isDirectory }
                        IndexedMapsBackground.ensureStarted(pbf, dataDir, elev)
                    }
                }
            } catch (t: Throwable) {
                lastStatus.set("failed: ${t.message}")
                Log.e(TAG, "provisionRegionData crashed", t)
            } finally {
                running.set(false)
                resuming.set(false)
            }
        }
    }
}
