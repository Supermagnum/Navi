package no.navi.app

import android.util.Log
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import uniffi.navi.convertProgressSnapshot
import uniffi.navi.downloadProgressSnapshot
import uniffi.navi.ensureIndexedMaps
import uniffi.navi.indexedMapsStatus
import java.io.File
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference

/**
 * Non-blocking indexed-map conversion / pack refresh. Region PBF remains usable
 * via bbox/PBF fallback until packs become `ready`; pack-hit engages afterward.
 *
 * Preference order (see UniFFI [ensureIndexedMaps]):
 * 1. Download a client-compatible pack from the navi-server pack host
 * 2. Fall back to on-device PBF convert only when the server pack is unavailable
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
            val snap =
                runCatching { downloadProgressSnapshot() }.getOrNull()?.takeIf {
                    it.label.isNotBlank() &&
                        (
                            it.label.contains("pack", ignoreCase = true) ||
                                it.label.contains("server", ignoreCase = true) ||
                                it.label.contains("Downloading updated", ignoreCase = true) ||
                                it.label.contains("Rebuilding locally", ignoreCase = true) ||
                                it.label.contains("Checking pack format", ignoreCase = true) ||
                                it.label.contains("Fetching packs", ignoreCase = true)
                        )
                } ?: runCatching { convertProgressSnapshot() }.getOrNull()
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
            "version_mismatch" ->
                "Indexed maps: outdated format — will try pack server, then local rebuild"
            "stale_pbf" ->
                "Indexed maps: stale vs PBF — will try pack server, then local rebuild"
            "missing" -> "Indexed maps: not built yet (planning uses PBF fallback)"
            else -> "Indexed maps: $st"
        }
    }

    /**
     * Start conversion / pack refresh if packs are not ready. Returns immediately.
     * No-op if already running or packs already ready.
     *
     * @param scope unused; kept for call-site compatibility.
     * @param regionId Optional Geofabrik path (`europe/norway/ostlandet`) for pack-server lookup.
     */
    @Suppress("UNUSED_PARAMETER")
    fun ensureStarted(
        scope: CoroutineScope,
        pbf: File,
        dataDir: File,
        elevDir: File? = null,
        regionId: String? = null,
    ) {
        ensureStarted(pbf, dataDir, elevDir, regionId)
    }

    fun ensureStarted(
        pbf: File,
        dataDir: File,
        elevDir: File? = null,
        regionId: String? = null,
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
                        Log.i(TAG, "packs ready; skip refresh pbf=${pbf.name}")
                        return@withLock false
                    }
                    running.set(true)
                    lastStatus.set("starting ($st) — pack server first")
                    Log.i(
                        TAG,
                        "start ensureIndexedMaps status=$st pbf=${pbf.name} regionId=$regionId",
                    )
                    true
                }
            if (!shouldRun) return@launch
            try {
                convertProgressClearSafe()
                val report =
                    ensureIndexedMaps(
                        pbf.absolutePath,
                        dataDir.absolutePath,
                        elevDir?.takeIf { it.isDirectory }?.absolutePath,
                        regionId?.trim()?.trim('/')?.ifBlank { null },
                    )
                lastStatus.set(
                    when {
                        report.contains("path=server_download") ->
                            "done (downloaded updated pack from server)"
                        report.contains("path=local_rebuild") ||
                            report.contains("rebuilding locally") ->
                            "done (rebuilt locally — server pack unavailable)"
                        report.contains("PASS") && report.contains("cache_hit=true") -> "done (already ready)"
                        report.contains("PASS") -> "done"
                        report.contains("skipped=convert_in_progress") ||
                            report.contains("region convert already in progress") ->
                            "waiting (convert already running)"
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

    private fun convertProgressClearSafe() {
        runCatching { uniffi.navi.convertProgressClear() }
    }
}
