package no.navi.app

import android.util.Log
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import uniffi.navi.convertProgressSnapshot
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
    private val activeRegionId = AtomicReference("")
    /** regionId|pbfName keys that already failed stem mismatch — skip re-log spam. */
    private val mismatchRefused = java.util.concurrent.ConcurrentHashMap.newKeySet<String>()

    fun isRunning(): Boolean = running.get()

    fun statusLine(): String = lastStatus.get()

    private fun annotate(
        label: String,
        regionId: String = activeRegionId.get(),
    ): String {
        val id = regionId.ifBlank { RegionDownloadBackground.activeRegionPath() }.trim().trim('/')
        if (id.isEmpty()) return label
        val seq = RegionProgressMessages.sequenceFor(id)
        return RegionProgressMessages.annotate(label, id, seq?.first, seq?.second)
    }

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
                runCatching { convertProgressSnapshot() }.getOrNull()?.takeIf {
                    it.label.isNotBlank()
                }
            val prog =
                if (snap != null && snap.label.isNotBlank()) {
                    val label = annotate(snap.label)
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
                    if (pct != null) "$label $pct%" else label
                } else {
                    lastStatus.get()
                }
            return "Indexed maps (background): $prog"
        }
        val st =
            runCatching { indexedMapsStatus(pbf.absolutePath, dataDir.absolutePath).trim() }
                .getOrDefault("error")
        return when (st) {
            "ready" -> annotate("Indexed maps: ready (pack-hit)")
            "version_mismatch" ->
                annotate(
                    "Indexed maps: outdated format — will try pack server, then local rebuild",
                )
            "stale_pbf" ->
                annotate(
                    "Indexed maps: stale vs PBF — will try pack server, then local rebuild",
                )
            "missing" -> annotate("Indexed maps: not built yet (planning uses PBF fallback)")
            else -> annotate("Indexed maps: $st")
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
        val rid = regionId?.trim()?.trim('/').orEmpty()
        val resolvedPbf =
            if (rid.isNotEmpty()) {
                val mismatchKey = "$rid|${pbf.name}"
                // Known stem mismatch: skip resolve + INFO spam (was flooding logcat
                // during long-trip plan while IndexedMapsBg retried every region).
                if (mismatchRefused.contains(mismatchKey)) {
                    activeRegionId.set(rid)
                    lastStatus.set(annotate("failed (pbf/region mismatch)", rid))
                    return
                }
                val matched =
                    PackRegionAvailability.resolvePbfForRegion(dataDir, rid)
                        ?: pbf.takeIf { PackRegionAvailability.pbfMatchesRegion(it, rid) }
                if (matched == null || !PackRegionAvailability.pbfMatchesRegion(matched, rid)) {
                    if (mismatchRefused.add(mismatchKey)) {
                        android.util.Log.e(
                            TAG,
                            "FAIL: PBF/region mismatch region_id=$rid pbf=${pbf.name} " +
                                "expected_stem=${PackRegionAvailability.localStem(rid)} — refusing convert",
                        )
                    }
                    activeRegionId.set(rid)
                    lastStatus.set(annotate("failed (pbf/region mismatch)", rid))
                    return
                }
                android.util.Log.i(
                    TAG,
                    "local-bake pbf resolved region_id=$rid pbf=${matched.absolutePath} " +
                        "expected_prefix=$rid",
                )
                matched
            } else {
                pbf
            }
        this.scope.launch {
            val shouldRun =
                mutex.withLock {
                    if (running.get()) {
                        return@withLock false
                    }
                    val st =
                        runCatching {
                            indexedMapsStatus(resolvedPbf.absolutePath, dataDir.absolutePath).trim()
                        }.getOrElse {
                            Log.e(TAG, "indexedMapsStatus failed", it)
                            "error"
                        }
                    if (st == "ready") {
                        lastStatus.set(annotate("ready", rid))
                        Log.i(TAG, "packs ready; skip refresh pbf=${resolvedPbf.name}")
                        return@withLock false
                    }
                    running.set(true)
                    activeRegionId.set(rid)
                    lastStatus.set(annotate("starting ($st) — pack server first", rid))
                    Log.i(
                        TAG,
                        "start ensureIndexedMaps status=$st pbf=${resolvedPbf.name} regionId=$rid",
                    )
                    true
                }
            if (!shouldRun) return@launch
            try {
                convertProgressClearSafe()
                val report =
                    ensureIndexedMaps(
                        resolvedPbf.absolutePath,
                        dataDir.absolutePath,
                        elevDir?.takeIf { it.isDirectory }?.absolutePath,
                        rid.ifBlank { null },
                        // Own Convert slot — never clobber RegionDownload Download progress.
                        progressOnConvertChannel = true,
                    )
                lastStatus.set(
                    annotate(
                        when {
                            report.contains("path=server_download") ->
                                "done (downloaded updated pack from server)"
                            report.contains("path=local_rebuild") ||
                                report.contains("rebuilding locally") ->
                                "done (rebuilt locally — ${extractRebuildReason(report)})"
                            report.contains("PASS") && report.contains("cache_hit=true") ->
                                "done (already ready)"
                            report.contains("PASS") -> "done"
                            report.contains("skipped=convert_in_progress") ||
                                report.contains("region convert already in progress") ->
                                "waiting (convert already running)"
                            else -> "failed"
                        },
                        rid,
                    ),
                )
                if (report.contains("PASS")) {
                    convertProgressClearSafe()
                }
                Log.i(TAG, "finished: $report")
            } catch (t: Throwable) {
                lastStatus.set(annotate("failed: ${t.message}", rid))
                Log.e(TAG, "ensureIndexedMaps crashed", t)
            } finally {
                running.set(false)
                activeRegionId.set("")
            }
        }
    }

    private fun convertProgressClearSafe() {
        runCatching { uniffi.navi.convertProgressClear() }
    }

    /** Pull `rebuilding locally (reason)` token from an ensureIndexedMaps report. */
    private fun extractRebuildReason(report: String): String {
        val m =
            Regex("""rebuilding locally \(([^)]+)\)""")
                .find(report)
        return m
            ?.groupValues
            ?.getOrNull(1)
            ?.trim()
            .orEmpty()
            .ifBlank { "local-bake" }
    }
}
