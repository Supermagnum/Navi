package no.navi.app

import android.util.Log
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import uniffi.navi.downloadProgressClear
import uniffi.navi.downloadProgressSnapshot
import uniffi.navi.ensureIndexedMaps
import uniffi.navi.indexedMapsStatus
import java.io.File
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference

/**
 * Non-blocking indexed-map conversion. Region PBF remains usable via bbox/PBF
 * fallback until packs become `ready`; pack-hit engages automatically after.
 */
object IndexedMapsBackground {
    private const val TAG = "IndexedMapsBg"

    private val mutex = Mutex()
    private val running = AtomicBoolean(false)
    private val lastStatus = AtomicReference("idle")
    private var job: Job? = null

    fun isRunning(): Boolean = running.get()

    fun statusLine(): String = lastStatus.get()

    /**
     * Passive progress for Tools (passive). Empty when idle and packs are ready.
     */
    fun uiLine(
        pbf: File?,
        dataDir: File,
    ): String {
        if (pbf == null || !pbf.isFile) return ""
        if (running.get()) {
            val snap = runCatching { downloadProgressSnapshot() }.getOrNull()
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
     */
    fun ensureStarted(
        scope: CoroutineScope,
        pbf: File,
        dataDir: File,
        elevDir: File? = null,
    ) {
        if (!pbf.isFile) return
        scope.launch(Dispatchers.IO) {
            mutex.withLock {
                if (running.get()) return@withLock
                val st = indexedMapsStatus(pbf.absolutePath, dataDir.absolutePath).trim()
                if (st == "ready") {
                    lastStatus.set("ready")
                    return@withLock
                }
                running.set(true)
                lastStatus.set("starting ($st)")
                Log.i(TAG, "start ensureIndexedMaps status=$st pbf=${pbf.name}")
                job =
                    launch(Dispatchers.IO) {
                        try {
                            downloadProgressClear()
                            val report =
                                ensureIndexedMaps(
                                    pbf.absolutePath,
                                    dataDir.absolutePath,
                                    elevDir?.takeIf { it.isDirectory }?.absolutePath,
                                )
                            lastStatus.set(
                                if (report.contains("PASS")) {
                                    "done"
                                } else {
                                    "failed"
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
    }
}
