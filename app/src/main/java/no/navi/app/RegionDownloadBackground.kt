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
import uniffi.navi.decideRegionAcquisition
import uniffi.navi.downloadProgressSnapshot
import uniffi.navi.ensurePackRegionPlaceIndex
import uniffi.navi.geofabrikLatestPbfUrl
import uniffi.navi.geofabrikPathForPbfName
import uniffi.navi.pmtilesQueueRegion
import uniffi.navi.pmtilesRunJob
import uniffi.navi.provisionRegionData
import java.io.File
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference

/**
 * Process-scoped region download. Survives Compose cancellation.
 *
 * When the pack server lists the region as ready, installs published packs,
 * then downloads the real Geofabrik PBF and builds `place_index.db` (same
 * NameIndex path as local convert), then range-extracts the Protomaps basemap
 * for the same Geofabrik path. Otherwise downloads Geofabrik PBF and builds
 * packs + place index locally, then the basemap. A force-stop still kills the
 * HTTP stream, but [JOB_FILE] plus the sibling `.partial` let the next Geofabrik
 * launch resume via HTTP Range.
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

    /** Last successful region path (for UI style refresh after `done`). */
    private val lastCompletedPath = AtomicReference("")

    fun isRunning(): Boolean = running.get()

    fun isResuming(): Boolean = resuming.get()

    fun statusLine(): String = lastStatus.get()

    fun takeLastCompletedPath(): String = lastCompletedPath.getAndSet("")

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
                val pathForDecision =
                    geofabrikPath.ifBlank {
                        geofabrikPathForPbfName(filename)
                    }
                if (pathForDecision.isNotBlank()) {
                    // decideRegionAcquisition(dataDir=…) probes the catalog and,
                    // when published, downloads/installs all pack files.
                    val checkStarted = System.nanoTime()
                    lastStatus.set("Fetching from pack server…")
                    val decision =
                        runCatching {
                            decideRegionAcquisition(
                                regionId = pathForDecision,
                                packServerBaseUrl = null,
                                dataDir = dataDir.absolutePath,
                            )
                        }.getOrElse { t ->
                            Log.i(
                                TAG,
                                "pack routing failed soft: ${t.message}; using local convert",
                            )
                            null
                        }
                    val checkMs = (System.nanoTime() - checkStarted) / 1_000_000L
                    if (decision != null) {
                        Log.i(
                            TAG,
                            "pack routing source=${decision.source} " +
                                "data_source=${decision.dataSource} " +
                                "execute_local=${decision.executeLocalConvert} " +
                                "reason=${decision.reason} " +
                                "decide_region_acquisition_ms=$checkMs",
                        )
                        if (!decision.executeLocalConvert) {
                            lastStatus.set("Installing packs from ${decision.dataSource}…")
                            runCatching {
                                bindGeofabrikRegion(
                                    dataDir = dataDir.absolutePath,
                                    geofabrikRegion = pathForDecision,
                                    pbfFilename = filename,
                                    localSequence = null,
                                )
                            }
                            if (geofabrikPath.isNotBlank()) {
                                MapHudPrefs.saveGeofabrikPath(context, geofabrikPath)
                            }
                            // Packs are ready. Fetch offline basemap next (independent of
                            // place search), then Geofabrik PBF + place index. Previously
                            // place-index ran first and a concurrent PlaceIndexBackground
                            // open could fail with "database is locked", aborting before
                            // PMTiles — leaving packs installed but no offline tiles.
                            if (!downloadBasemapPmtiles(context, dataDir, pathForDecision)) {
                                lastStatus.set("failed: basemap")
                                return@launch
                            }
                            lastStatus.set("Downloading extract + building place index…")
                            Log.i(
                                TAG,
                                "pack server packs + basemap ready; starting Geofabrik PBF + " +
                                    "place index for $pathForDecision",
                            )
                            val placeReport =
                                runCatching {
                                    ensurePackRegionPlaceIndex(
                                        dataDir = dataDir.absolutePath,
                                        regionId = pathForDecision,
                                        forceRebuild = true,
                                    )
                                }.getOrElse { t ->
                                    Log.e(TAG, "ensurePackRegionPlaceIndex crashed", t)
                                    "FAIL: ${t.message}\n"
                                }
                            Log.i(TAG, "pack region place index: ${placeReport.take(400)}")
                            if (!placeReport.contains("PASS")) {
                                // Basemap is already on disk; surface place-index failure
                                // without discarding the successful pack+tile install.
                                lastStatus.set("done (place index failed)")
                                clearJob(dataDir)
                                lastCompletedPath.set(pathForDecision)
                                Log.e(
                                    TAG,
                                    "place index failed after packs+basemap; leaving basemap in place",
                                )
                                return@launch
                            }
                            clearJob(dataDir)
                            lastCompletedPath.set(pathForDecision)
                            lastStatus.set("done")
                            Log.i(
                                TAG,
                                "pack server install + basemap + place index finished for $pathForDecision",
                            )
                            return@launch
                        }
                    }
                }
                lastStatus.set(
                    if (resuming.get()) "Resuming Geofabrik download…" else "Downloading region… 0%",
                )
                val report =
                    provisionRegionData(
                        dataDir = dataDir.absolutePath,
                        pbfUrl = url,
                        pbfFilename = filename,
                        elevationTarUrl = null,
                    )
                Log.i(TAG, "finished: ${report.take(240)}")
                if (report.contains("PASS")) {
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
                    if (pbf.isFile && pbf.length() >= MIN_PBF_BYTES) {
                        val elev = File(dataDir, "elevation").takeIf { it.isDirectory }
                        IndexedMapsBackground.ensureStarted(pbf, dataDir, elev)
                    }
                    val basemapPath = geofabrikPath.ifBlank { pathForDecision }
                    if (basemapPath.isNotBlank()) {
                        if (!downloadBasemapPmtiles(context, dataDir, basemapPath)) {
                            lastStatus.set("failed: basemap")
                            return@launch
                        }
                    }
                    // Place index after basemap so a locked DB cannot block offline tiles.
                    if (pbf.isFile && pbf.length() >= MIN_PBF_BYTES) {
                        PlaceIndexBackground.ensureStarted(
                            pbf,
                            File(dataDir, "place_index.db"),
                            geofabrikPath.ifBlank { pathForDecision }.ifBlank { null },
                        )
                    }
                    clearJob(dataDir)
                    lastCompletedPath.set(basemapPath)
                    lastStatus.set("done")
                } else {
                    lastStatus.set("failed")
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

    /**
     * Range-extract Protomaps basemap for [geofabrikPath]. Returns false on
     * queue/run failure. Skips the network extract when the file already exists.
     */
    private fun downloadBasemapPmtiles(
        context: Context,
        dataDir: File,
        geofabrikPath: String,
    ): Boolean {
        val path = geofabrikPath.trim().trim('/')
        if (path.isEmpty()) return false
        if (PackRegionAvailability.localPmtilesReady(dataDir, path)) {
            val key = PackRegionAvailability.geofabrikPathToRegionKey(path)
            MapHudPrefs.rememberDownloadedPmtilesRegion(context, key)
            Log.i(TAG, "basemap already present for $path ($key)")
            return true
        }
        lastStatus.set("Downloading basemap (PMTiles)…")
        Log.i(TAG, "starting PMTiles extract for $path")
        val job =
            runCatching {
                pmtilesQueueRegion(dataDir.absolutePath, path, null)
            }.getOrElse { t ->
                Log.e(TAG, "pmtilesQueueRegion failed", t)
                return false
            }
        if (job.id.isBlank() || job.status.startsWith("failed")) {
            Log.e(TAG, "pmtiles queue failed: ${job.status}")
            return false
        }
        val done =
            runCatching {
                pmtilesRunJob(dataDir.absolutePath, job.id)
            }.getOrElse { t ->
                Log.e(TAG, "pmtilesRunJob crashed", t)
                return false
            }
        if (done.status != "completed") {
            Log.e(TAG, "pmtiles job not completed: ${done.status} path=${done.localPath}")
            return false
        }
        val key =
            done.regionKey.ifBlank {
                PackRegionAvailability.geofabrikPathToRegionKey(path)
            }
        MapHudPrefs.rememberDownloadedPmtilesRegion(context, key)
        Log.i(TAG, "basemap ready region_key=$key path=${done.localPath}")
        return true
    }
}
