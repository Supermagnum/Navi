package no.navi.app

import android.util.Log
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import uniffi.navi.downloadProgressSnapshot
import uniffi.navi.ensurePlaceIndex
import java.io.File
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference

/**
 * Place-index rebuild that outlives Compose [LaunchedEffect] cancellation.
 * Indexing a regional PBF takes minutes; tying it to composition cancelled the
 * work before `place_index.db` was created.
 *
 * When [regionId] is non-blank, rows are merged under that Geofabrik path so
 * other regions already in the shared DB stay searchable.
 */
object PlaceIndexBackground {
    private const val TAG = "PlaceIndexBg"
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val mutex = Mutex()
    private val running = AtomicBoolean(false)
    private val lastStatus = AtomicReference("idle")

    fun isRunning(): Boolean = running.get()

    /**
     * Claim before launching IO so a concurrent [ensureStarted] sees busy.
     * Pair with [RegionDownloadBackground.isRunning] at call sites: the region
     * pipeline owns place-index when a download/resume is already claimed.
     */
    internal fun claimWorker(): Boolean = running.compareAndSet(false, true)

    internal fun releaseWorker() {
        running.set(false)
    }

    /** True when this process must not start a second `ensurePlaceIndex`. */
    internal fun shouldSkipStandaloneIndex(): Boolean = RegionDownloadBackground.isRunning() || running.get()

    fun statusLine(): String {
        if (running.get()) {
            val snap = runCatching { downloadProgressSnapshot() }.getOrNull()
            if (snap != null && snap.label.isNotBlank()) {
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
        }
        return lastStatus.get()
    }

    fun ensureStarted(
        pbf: File,
        indexDb: File,
        regionId: String? = null,
    ) {
        if (!pbf.isFile) return
        if (RegionDownloadBackground.isRunning()) {
            Log.i(TAG, "region pipeline already running; skip standalone ensurePlaceIndex")
            return
        }
        if (!claimWorker()) {
            Log.i(TAG, "already running; skip")
            return
        }
        lastStatus.set("building")
        val rid = regionId?.trim()?.trim('/')?.ifBlank { null }
        Log.i(
            TAG,
            "start ensurePlaceIndex pbf=${pbf.absolutePath} db=${indexDb.absolutePath} region=$rid",
        )
        scope.launch {
            mutex.withLock {
                try {
                    val report =
                        ensurePlaceIndex(
                            pbf.absolutePath,
                            indexDb.absolutePath,
                            rid,
                        )
                    val bytes = if (indexDb.isFile) indexDb.length() else 0L
                    if (report.contains("PASS")) {
                        val dataDir = indexDb.parentFile
                        if (dataDir != null && !rid.isNullOrBlank()) {
                            PlaceIndexReady.markReady(dataDir, rid)
                        }
                        lastStatus.set("Place index ready 100% (6 / 6)")
                        // Own [running] is still true until [finally]; skip if the
                        // region pipeline has taken the lock for another build.
                        if (DownloadProgressClear.shouldClear(
                                regionRunning = RegionDownloadBackground.isRunning(),
                                placeIndexRunning = false,
                            )
                        ) {
                            runCatching { uniffi.navi.downloadProgressClear() }
                        }
                    } else {
                        lastStatus.set("failed")
                    }
                    Log.i(TAG, "finished bytes=$bytes report=$report")
                } catch (t: Throwable) {
                    lastStatus.set("failed: ${t.message}")
                    Log.e(TAG, "ensurePlaceIndex crashed", t)
                } finally {
                    releaseWorker()
                }
            }
        }
    }
}
