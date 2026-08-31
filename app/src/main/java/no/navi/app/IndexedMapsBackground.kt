package no.navi.app

import android.util.Log
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import uniffi.navi.convertProgressClear
import uniffi.navi.convertProgressSnapshot
import uniffi.navi.ensureIndexedMaps
import uniffi.navi.indexedMapsStatus
import java.io.File
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference

/**
 * Non-blocking indexed-map conversion. Region PBF remains usable via bbox/PBF
 * fallback until packs become `ready`; pack-hit engages automatically after.
 *
 * Uses a **process-scoped** [CoroutineScope] (same pattern as [PlaceIndexBackground])
 * so conversion survives Compose recomposition / Activity recreation. It does
 * **not** use WorkManager: a force-stop or process death still kills the job.
 * Convert progress is persisted to `{stem}.navi-convert-progress.json` so the
 * next [ensureStarted] skips completed graph/POI/wetland archives instead of
 * deleting them and rebuilding from scratch (see core `convert_region_packs`).
 */
object IndexedMapsBackground {
    private const val TAG = "IndexedMapsBg"

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val mutex = Mutex()
    private val running = AtomicBoolean(false)
    private val lastStatus = AtomicReference("idle")

    fun isRunning(): Boolean = running.get()

    fun statusLine(): String = lastStatus.get()

    /**
     * Tools status line. Empty when idle and packs are ready.
     */
    fun uiLine(
        pbf: File?,
        dataDir: File,
    ): String {
        if (pbf == null || !pbf.isFile) return ""
        if (running.get()) {
            val snap = runCatching { convertProgressSnapshot() }.getOrNull()
            val prog =
                if (snap != null && snap.label.isNotBlank()) {
                    val pct =
                        snap.unitsTotal?.let { tot ->
                            if (tot > 0uL) {
                                ((snap.unitsDone.toDouble() * 100.0) / tot.toDouble())
                                    .toInt()
                                    .coerceIn(0, 100)
                            } else {
                                null
                            }
                        }
                    if (pct != null) "${snap.label} $pct%" else snap.label
                } else {
                    lastStatus.get()
                }
            return "Indexed maps (background): $prog"
        }
        val st =
            runCatching { indexedMapsStatus(pbf.absolutePath, dataDir.absolutePath).trim() }
                .getOrDefault("error")
        return when (st) {
            "ready" -> "Indexed maps: ready (pack-hit)"
            "version_mismatch" -> "Indexed maps: outdated — background rebuild queued or needed"
            "stale_pbf" -> "Indexed maps: stale vs PBF — background rebuild needed"
            "missing" -> "Indexed maps: not built yet (planning uses PBF fallback)"
            else -> "Indexed maps: $st"
        }
    }

    /**
     * Start conversion if packs are not ready. Returns immediately; does not block.
     * No-op if already running or packs already ready.
     *
     * @param scope unused; kept for call-site compatibility. Work runs on the
     *   process-scoped scope so Compose cancellation cannot abort convert.
     */
    @Suppress("UNUSED_PARAMETER")
    fun ensureStarted(
        scope: CoroutineScope,
        pbf: File,
        dataDir: File,
        elevDir: File? = null,
    ) {
        ensureStarted(pbf, dataDir, elevDir)
    }

    fun ensureStarted(
        pbf: File,
        dataDir: File,
        elevDir: File? = null,
    ) {
        if (!pbf.isFile) return
        this.scope.launch {
            val shouldRun =
                mutex.withLock {
                    if (running.get()) {
                        return@withLock false
                    }
                    val st =
                        runCatching {
                            indexedMapsStatus(pbf.absolutePath, dataDir.absolutePath).trim()
                        }.getOrElse {
                            Log.e(TAG, "indexedMapsStatus failed", it)
                            "error"
                        }
                    if (st == "ready") {
                        lastStatus.set("ready")
                        Log.i(TAG, "packs ready; skip convert pbf=${pbf.name}")
                        return@withLock false
                    }
                    running.set(true)
                    lastStatus.set("starting ($st)")
                    Log.i(TAG, "start ensureIndexedMaps status=$st pbf=${pbf.name}")
                    true
                }
            if (!shouldRun) return@launch
            // Convert outside the mutex so status polls can observe [isRunning].
            try {
                convertProgressClear()
                val report =
                    ensureIndexedMaps(
                        pbf.absolutePath,
                        dataDir.absolutePath,
                        elevDir?.takeIf { it.isDirectory }?.absolutePath,
                    )
                lastStatus.set(
                    when {
                        report.contains("PASS") -> "done"
                        report.contains("skipped=convert_in_progress") ||
                            report.contains("region convert already in progress") -> "waiting (convert already running)"
                        else -> "failed"
                    },
                )
                Log.i(TAG, "finished: $report")
            } catch (t: Throwable) {
                lastStatus.set("failed: ${t.message}")
                Log.e(TAG, "ensureIndexedMaps crashed", t)
            } finally {
                running.set(false)
            }
        }
    }
}
