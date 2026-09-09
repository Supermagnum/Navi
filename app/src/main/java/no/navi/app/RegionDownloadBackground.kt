package no.navi.app

import android.content.Context
import android.util.Log
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import uniffi.navi.bindGeofabrikRegion
import uniffi.navi.decideRegionAcquisition
import uniffi.navi.downloadProgressSnapshot
import uniffi.navi.ensurePackRegionPlaceIndex
import uniffi.navi.ensurePlaceIndex
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
 * Pack-server path: install published packs → Geofabrik extract + place index
 * (app becomes usable for routing/search) → Protomaps basemap extract
 * (optional picture; does not block usability). Local-bake path: Geofabrik PBF
 * + local packs → place index → basemap. A force-stop still kills the HTTP
 * stream, but [JOB_FILE] (with [Phase]) plus `.partial` let the next launch
 * resume without the user tapping Download again.
 */
object RegionDownloadBackground {
    const val JOB_FILE = "region-download.json"
    private const val TAG = "RegionDownloadBg"
    private const val MIN_PBF_BYTES = 1_000_000L

    /** Status prefix while basemap still runs after packs + place index. */
    const val USABLE_STATUS_PREFIX = "Place index ready"

    enum class Phase {
        PACKS,
        PLACE_INDEX,
        BASEMAP,
        ;

        fun wire(): String = name.lowercase()

        companion object {
            fun parse(raw: String?): Phase =
                when (raw?.trim()?.lowercase()) {
                    "place_index", "place-index", "index" -> PLACE_INDEX
                    "basemap", "pmtiles" -> BASEMAP
                    else -> PACKS
                }
        }
    }

    data class Job(
        val url: String,
        val filename: String,
        val geofabrikPath: String,
        val phase: Phase = Phase.PACKS,
    )

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val mutex = Mutex()
    private val running = AtomicBoolean(false)
    private val lastStatus = AtomicReference("")
    private val resuming = AtomicBoolean(false)

    /** Last successful region path (for UI style refresh after full `done`). */
    private val lastCompletedPath = AtomicReference("")

    /**
     * Region path that became usable for routing/search (packs + place index)
     * while basemap may still be downloading. Consumed once by the UI.
     */
    private val lastUsablePath = AtomicReference("")

    fun isRunning(): Boolean = running.get()

    fun isResuming(): Boolean = resuming.get()

    fun statusLine(): String = lastStatus.get()

    fun takeLastCompletedPath(): String = lastCompletedPath.getAndSet("")

    fun takeLastUsablePath(): String = lastUsablePath.getAndSet("")

    fun jobFile(dataDir: File): File = File(dataDir, JOB_FILE)

    fun findPartialPbf(dataDir: File): File? =
        dataDir.listFiles()?.firstOrNull {
            it.isFile && it.name.endsWith(".osm.pbf.partial") && it.length() > 0L
        }

    fun loadJob(dataDir: File): Job? {
        val f = jobFile(dataDir)
        if (!f.isFile) return null
        return runCatching {
            val text = f.readText()
            Job(
                url = jsonStringField(text, "url") ?: return null,
                filename = jsonStringField(text, "filename") ?: return null,
                geofabrikPath = jsonStringField(text, "geofabrikPath").orEmpty(),
                phase = Phase.parse(jsonStringField(text, "phase")),
            )
        }.getOrNull()
    }

    fun writeJob(
        dataDir: File,
        job: Job,
    ) {
        dataDir.mkdirs()
        // Hand-rolled JSON so JVM unit tests do not need org.json mocks.
        jobFile(dataDir).writeText(
            buildString {
                append('{')
                append("\"url\":").append(jsonQuote(job.url)).append(',')
                append("\"filename\":").append(jsonQuote(job.filename)).append(',')
                append("\"geofabrikPath\":").append(jsonQuote(job.geofabrikPath)).append(',')
                append("\"phase\":").append(jsonQuote(job.phase.wire()))
                append('}')
            },
        )
    }

    private fun jsonQuote(s: String): String =
        buildString {
            append('"')
            for (c in s) {
                when (c) {
                    '\\' -> append("\\\\")
                    '"' -> append("\\\"")
                    '\n' -> append("\\n")
                    '\r' -> append("\\r")
                    '\t' -> append("\\t")
                    else -> append(c)
                }
            }
            append('"')
        }

    private fun jsonStringField(
        json: String,
        key: String,
    ): String? {
        val needle = "\"$key\""
        val keyAt = json.indexOf(needle)
        if (keyAt < 0) return null
        var i = keyAt + needle.length
        while (i < json.length && (json[i] == ' ' || json[i] == '\t' || json[i] == ':')) i++
        if (i >= json.length || json[i] != '"') return null
        i++
        val out = StringBuilder()
        while (i < json.length) {
            val c = json[i++]
            when {
                c == '\\' && i < json.length -> {
                    when (val e = json[i++]) {
                        'n' -> out.append('\n')
                        'r' -> out.append('\r')
                        't' -> out.append('\t')
                        else -> out.append(e)
                    }
                }
                c == '"' -> return out.toString()
                else -> out.append(c)
            }
        }
        return null
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
     * Incomplete region work persisted on disk: phase-aware [JOB_FILE], or an
     * orphan `.partial` PBF. Unlike the old PBF-only check, a completed PBF does
     * **not** clear the job while [Phase.PLACE_INDEX] / [Phase.BASEMAP] remain.
     */
    fun discoverPending(dataDir: File): Job? {
        loadJob(dataDir)?.let { job ->
            val path =
                job.geofabrikPath.ifBlank {
                    runCatching { geofabrikPathForPbfName(job.filename) }.getOrDefault("")
                }
            val advanced = advanceFinishedPhases(dataDir, job.copy(geofabrikPath = path))
            if (advanced == null) {
                clearJob(dataDir)
                return null
            }
            if (advanced.phase != job.phase || advanced.geofabrikPath != job.geofabrikPath) {
                writeJob(dataDir, advanced)
            }
            return advanced
        }
        val partial = findPartialPbf(dataDir) ?: return null
        val filename = partial.name.removeSuffix(".partial")
        val path =
            runCatching { geofabrikPathForPbfName(filename) }
                .getOrDefault("")
                .trim()
                .ifBlank { return null }
        val url =
            runCatching { geofabrikLatestPbfUrl(path) }
                .getOrDefault("https://download.geofabrik.de/$path-latest.osm.pbf")
        return Job(
            url = url,
            filename = filename,
            geofabrikPath = path,
            phase = Phase.PACKS,
        )
    }

    /**
     * If packs are on disk for [preferredPath] but place index / basemap still
     * missing, synthesize a job so launch resume does not require a leftover
     * PBF `.partial` or an intact sidecar from mid-download.
     */
    fun discoverIncompleteForPath(
        dataDir: File,
        preferredPath: String,
    ): Job? {
        discoverPending(dataDir)?.let { return it }
        val path = preferredPath.trim().trim('/')
        if (path.isEmpty()) return null
        if (!PackRegionAvailability.localBakeReady(dataDir, path)) return null
        val leaf = path.substringAfterLast('/')
        val filename = "$leaf-latest.osm.pbf"
        val phase =
            when {
                !placeIndexLooksReady(dataDir, path) -> Phase.PLACE_INDEX
                !PackRegionAvailability.localPmtilesReady(dataDir, path) -> Phase.BASEMAP
                else -> return null
            }
        // URL is rebuilt at resume time if needed; avoid UniFFI in pure discovery.
        val url =
            runCatching { geofabrikLatestPbfUrl(path) }
                .getOrDefault("https://download.geofabrik.de/$path-latest.osm.pbf")
        return Job(
            url = url,
            filename = filename,
            geofabrikPath = path,
            phase = phase,
        )
    }

    /** Drop phases that are already satisfied; null when everything is done. */
    private fun advanceFinishedPhases(
        dataDir: File,
        job: Job,
    ): Job? {
        var phase = job.phase
        val path = job.geofabrikPath.trim().trim('/')
        if (path.isEmpty()) return job
        val packsReady = PackRegionAvailability.localBakeReady(dataDir, path)
        val indexReady = placeIndexLooksReady(dataDir, path)
        val basemapReady = PackRegionAvailability.localPmtilesReady(dataDir, path)
        if (packsReady && indexReady && basemapReady) {
            return null
        }
        if (phase == Phase.PACKS && packsReady) {
            phase = Phase.PLACE_INDEX
        }
        if (phase == Phase.PLACE_INDEX && indexReady) {
            phase = Phase.BASEMAP
        }
        if (phase == Phase.BASEMAP && basemapReady) {
            return null
        }
        return job.copy(phase = phase, geofabrikPath = path)
    }

    /**
     * Cheap readiness check for launch rediscovery — never calls ensurePlaceIndex
     * (that can rebuild for minutes). Prefer a read-only region_id probe; fall
     * back to "any entries exist" when the column is missing (pre-v3 DBs).
     */
    internal fun placeIndexLooksReady(
        dataDir: File,
        regionId: String,
    ): Boolean {
        val dbFile = File(dataDir, "place_index.db")
        if (!dbFile.isFile || dbFile.length() < 10_000L) return false
        val rid = regionId.trim().trim('/')
        if (rid.isEmpty()) return false
        return runCatching {
            android.database.sqlite.SQLiteDatabase
                .openDatabase(
                    dbFile.absolutePath,
                    null,
                    android.database.sqlite.SQLiteDatabase.OPEN_READONLY,
                ).use { db ->
                    val byRegion =
                        runCatching {
                            db
                                .rawQuery(
                                    "SELECT 1 FROM name_entries WHERE region_id = ? LIMIT 1",
                                    arrayOf(rid),
                                ).use { it.moveToFirst() }
                        }
                    when {
                        byRegion.isSuccess -> byRegion.getOrThrow()
                        else ->
                            // Pre-v3 / missing column: any row is enough for single-region devices.
                            db.rawQuery("SELECT 1 FROM name_entries LIMIT 1", null).use {
                                it.moveToFirst()
                            }
                    }
                }
        }.getOrDefault(false)
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
        preferredPath: String = "",
    ) {
        val job =
            discoverPending(dataDir)
                ?: discoverIncompleteForPath(dataDir, preferredPath)
                ?: return
        lastStatus.set(
            when (job.phase) {
                Phase.PACKS ->
                    if (partialBytes(dataDir, job.filename) > 0L) {
                        "Resuming download of ${job.geofabrikPath}…"
                    } else {
                        "Resuming download of ${job.geofabrikPath}…"
                    }
                Phase.PLACE_INDEX -> "Resuming place index for ${job.geofabrikPath}…"
                Phase.BASEMAP -> "Resuming basemap for ${job.geofabrikPath}…"
            },
        )
        ensureStarted(
            context,
            dataDir,
            job.url,
            job.filename,
            job.geofabrikPath,
            startPhase = job.phase,
        )
    }

    fun ensureStarted(
        context: Context,
        dataDir: File,
        url: String,
        filename: String,
        geofabrikPath: String,
        startPhase: Phase = Phase.PACKS,
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
                    resuming.set(already > 0L || startPhase != Phase.PACKS)
                    val job =
                        Job(
                            url = url,
                            filename = filename,
                            geofabrikPath = geofabrikPath,
                            phase = startPhase,
                        )
                    writeJob(dataDir, job)
                    if (lastStatus.get().isBlank() || !lastStatus.get().startsWith("Resuming")) {
                        lastStatus.set(
                            if (already > 0L || startPhase != Phase.PACKS) {
                                "Resuming download…"
                            } else {
                                "Downloading region… 0%"
                            },
                        )
                    }
                    Log.i(
                        TAG,
                        "start provision filename=$filename resume_bytes=$already " +
                            "path=$geofabrikPath phase=$startPhase",
                    )
                    true
                }
            if (!shouldRun) return@launch
            try {
                val pathForDecision =
                    geofabrikPath.ifBlank {
                        geofabrikPathForPbfName(filename)
                    }
                var phase = startPhase
                if (pathForDecision.isNotBlank() && phase == Phase.PACKS) {
                    val checkStarted = System.nanoTime()
                    lastStatus.set("Fetching from pack server…")
                    persistPhase(dataDir, url, filename, pathForDecision, Phase.PACKS)
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
                            phase = Phase.PLACE_INDEX
                            persistPhase(dataDir, url, filename, pathForDecision, phase)
                            if (!runPlaceIndexPackServer(context, dataDir, pathForDecision)) {
                                return@launch
                            }
                            phase = Phase.BASEMAP
                            persistPhase(dataDir, url, filename, pathForDecision, phase)
                            val basemapOk =
                                downloadBasemapPmtiles(context, dataDir, pathForDecision)
                            clearJob(dataDir)
                            lastCompletedPath.set(pathForDecision)
                            lastStatus.set(
                                if (basemapOk) {
                                    "done"
                                } else {
                                    "done (basemap failed)"
                                },
                            )
                            Log.i(
                                TAG,
                                "pack server install + place index + basemap finished " +
                                    "for $pathForDecision basemap_ok=$basemapOk",
                            )
                            return@launch
                        }
                    }
                }

                if (phase == Phase.PLACE_INDEX || phase == Phase.BASEMAP) {
                    // Resume mid-pipeline after packs (or after place index).
                    if (phase == Phase.PLACE_INDEX) {
                        persistPhase(dataDir, url, filename, pathForDecision, Phase.PLACE_INDEX)
                        if (PackRegionAvailability.localBakeReady(dataDir, pathForDecision) &&
                            pathForDecision.isNotBlank()
                        ) {
                            if (!runPlaceIndexPackServer(context, dataDir, pathForDecision)) {
                                return@launch
                            }
                        } else {
                            if (!runPlaceIndexLocal(dataDir, filename, pathForDecision)) {
                                clearJob(dataDir)
                                lastCompletedPath.set(pathForDecision)
                                lastStatus.set("done (place index failed)")
                                downloadBasemapPmtiles(context, dataDir, pathForDecision)
                                return@launch
                            }
                            markUsable(pathForDecision)
                        }
                        phase = Phase.BASEMAP
                        persistPhase(dataDir, url, filename, pathForDecision, phase)
                    }
                    if (phase == Phase.BASEMAP && pathForDecision.isNotBlank()) {
                        if (!markUsableAlready()) {
                            markUsable(pathForDecision)
                        }
                        val basemapOk =
                            downloadBasemapPmtiles(context, dataDir, pathForDecision)
                        clearJob(dataDir)
                        lastCompletedPath.set(pathForDecision)
                        lastStatus.set(
                            if (basemapOk) "done" else "done (basemap failed)",
                        )
                        return@launch
                    }
                }

                lastStatus.set(
                    if (resuming.get()) "Resuming Geofabrik download…" else "Downloading region… 0%",
                )
                persistPhase(dataDir, url, filename, pathForDecision, Phase.PACKS)
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
                    persistPhase(dataDir, url, filename, basemapPath, Phase.PLACE_INDEX)
                    if (pbf.isFile && pbf.length() >= MIN_PBF_BYTES) {
                        if (!runPlaceIndexLocal(dataDir, filename, basemapPath)) {
                            lastStatus.set("done (place index failed)")
                            clearJob(dataDir)
                            lastCompletedPath.set(basemapPath)
                            if (basemapPath.isNotBlank()) {
                                downloadBasemapPmtiles(context, dataDir, basemapPath)
                            }
                            return@launch
                        }
                    }
                    if (basemapPath.isNotBlank()) {
                        markUsable(basemapPath)
                        persistPhase(dataDir, url, filename, basemapPath, Phase.BASEMAP)
                        if (!downloadBasemapPmtiles(context, dataDir, basemapPath)) {
                            clearJob(dataDir)
                            lastCompletedPath.set(basemapPath)
                            lastStatus.set("done (basemap failed)")
                            return@launch
                        }
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

    private fun persistPhase(
        dataDir: File,
        url: String,
        filename: String,
        geofabrikPath: String,
        phase: Phase,
    ) {
        writeJob(
            dataDir,
            Job(
                url = url,
                filename = filename,
                geofabrikPath = geofabrikPath,
                phase = phase,
            ),
        )
    }

    private fun markUsableAlready(): Boolean = lastStatus.get().startsWith(USABLE_STATUS_PREFIX)

    private fun markUsable(path: String) {
        val trimmed = path.trim().trim('/')
        if (trimmed.isEmpty()) return
        lastUsablePath.set(trimmed)
        lastStatus.set("$USABLE_STATUS_PREFIX — downloading basemap…")
        Log.i(TAG, "region usable for routing/search path=$trimmed (basemap may continue)")
    }

    /** Pack-server place index; marks usable on PASS. Returns false if failed (and handled). */
    private fun runPlaceIndexPackServer(
        context: Context,
        dataDir: File,
        pathForDecision: String,
    ): Boolean {
        lastStatus.set("Downloading extract + building place index…")
        Log.i(
            TAG,
            "starting Geofabrik PBF + place index for $pathForDecision",
        )
        val placeReport =
            runCatching {
                ensurePackRegionPlaceIndex(
                    dataDir = dataDir.absolutePath,
                    regionId = pathForDecision,
                    forceRebuild = false,
                )
            }.getOrElse { t ->
                Log.e(TAG, "ensurePackRegionPlaceIndex crashed", t)
                "FAIL: ${t.message}\n"
            }
        Log.i(TAG, "pack region place index: ${placeReport.take(400)}")
        if (!placeReport.contains("PASS")) {
            lastStatus.set("done (place index failed)")
            clearJob(dataDir)
            lastCompletedPath.set(pathForDecision)
            Log.e(TAG, "place index failed after packs; still fetching basemap")
            downloadBasemapPmtiles(context, dataDir, pathForDecision)
            return false
        }
        markUsable(pathForDecision)
        return true
    }

    private fun runPlaceIndexLocal(
        dataDir: File,
        filename: String,
        regionId: String,
    ): Boolean {
        val pbf = File(dataDir, filename)
        if (!pbf.isFile || pbf.length() < MIN_PBF_BYTES) return false
        lastStatus.set("Downloading extract + building place index…")
        val placeReport =
            runCatching {
                ensurePlaceIndex(
                    pbf.absolutePath,
                    File(dataDir, "place_index.db").absolutePath,
                    regionId.ifBlank { null },
                )
            }.getOrElse { t ->
                Log.e(TAG, "ensurePlaceIndex crashed", t)
                "FAIL: ${t.message}\n"
            }
        Log.i(TAG, "local-bake place index: ${placeReport.take(400)}")
        return placeReport.contains("PASS")
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
