package no.navi.app

import android.content.Context
import android.util.Log
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import org.json.JSONArray
import org.json.JSONObject
import uniffi.navi.bindGeofabrikRegion
import uniffi.navi.decideRegionAcquisition
import uniffi.navi.downloadProgressSnapshot
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
 * Process-scoped region download queue. Survives Compose cancellation.
 *
 * Regions are processed **one at a time**. When several are requested, the
 * region containing the user's current GPS fix is first; remaining regions
 * keep request order. For each region: download packs + Geofabrik extract +
 * basemap to completion; on local-bake, build place index from the extract
 * next (not gated on full convert), then hand convert to IndexedMapsBackground.
 * Never start place-index while that region's downloads are still in progress;
 * never start the next region's download until this region's place index finishes.
 * A force-stop still kills the HTTP stream, but [JOB_FILE] (with [Phase]) plus
 * `.partial` / [QUEUE_FILE] let the next launch resume without tapping Download
 * again.
 */
object RegionDownloadBackground {
    const val JOB_FILE = "region-download.json"
    const val QUEUE_FILE = "region-download-queue.json"
    private const val TAG = "RegionDownloadBg"
    private const val MIN_PBF_BYTES = 1_000_000L

    /** Status prefix once packs, basemap, and place index have finished. */
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
                !PackRegionAvailability.localPmtilesReady(dataDir, path) -> Phase.BASEMAP
                !placeIndexLooksReady(dataDir, path) -> Phase.PLACE_INDEX
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

    /** Drop phases that are already satisfied; null when everything is done.
     *
     * Pipeline order: packs (+ Geofabrik extract) → basemap → place index.
     */
    private fun advanceFinishedPhases(
        dataDir: File,
        job: Job,
    ): Job? {
        var phase = job.phase
        val path = job.geofabrikPath.trim().trim('/')
        if (path.isEmpty()) return job
        val packsReady = PackRegionAvailability.localBakeReady(dataDir, path)
        val basemapReady = PackRegionAvailability.localPmtilesReady(dataDir, path)
        val indexReady = placeIndexLooksReady(dataDir, path)
        if (packsReady && basemapReady && indexReady) {
            return null
        }
        if (phase == Phase.PACKS && packsReady) {
            phase =
                when {
                    !basemapReady -> Phase.BASEMAP
                    !indexReady -> Phase.PLACE_INDEX
                    else -> return null
                }
        }
        if (phase == Phase.BASEMAP && basemapReady) {
            phase = if (!indexReady) Phase.PLACE_INDEX else return null
        }
        if (phase == Phase.PLACE_INDEX && indexReady) {
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
        val rid = regionId.trim().trim('/')
        if (rid.isEmpty()) return false
        if (PlaceIndexReady.isReady(dataDir, rid)) return true
        // Once a stamp file exists it is authoritative — do not treat partial
        // mid-build rows as ready (clearReady leaves an updated stamp).
        if (PlaceIndexReady.readyFile(dataDir).isFile) return false
        val dbFile = File(dataDir, "place_index.db")
        if (!dbFile.isFile || dbFile.length() < 10_000L) return false
        val hasRows =
            runCatching {
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
                                db.rawQuery("SELECT 1 FROM name_entries LIMIT 1", null).use {
                                    it.moveToFirst()
                                }
                        }
                    }
            }.getOrDefault(false)
        if (hasRows) {
            // Legacy DB rows without a ready stamp — adopt them once.
            PlaceIndexReady.markReady(dataDir, rid)
        }
        return hasRows
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
        userLat: Double? = null,
        userLon: Double? = null,
    ) {
        scope.launch {
            val path = GeofabrikDownloadCatalog.canonicalizePath(geofabrikPath)
            val job =
                Job(
                    url = url,
                    filename = filename,
                    geofabrikPath = path,
                    phase = startPhase,
                )
            val shouldStartWorker =
                mutex.withLock {
                    enqueueJobLocked(dataDir, job, userLat, userLon)
                    if (running.get()) {
                        Log.i(TAG, "queued behind active download path=$path")
                        lastStatus.set(queueStatusLine(dataDir, path))
                        false
                    } else {
                        running.set(true)
                        true
                    }
                }
            if (!shouldStartWorker) return@launch
            try {
                drainQueue(context, dataDir)
            } finally {
                running.set(false)
                resuming.set(false)
            }
        }
    }

    private fun queueStatusLine(
        dataDir: File,
        justQueued: String,
    ): String {
        val n = loadQueue(dataDir).size
        val active = loadJob(dataDir)?.geofabrikPath.orEmpty()
        return when {
            active.isNotBlank() && n > 0 ->
                "Queued $justQueued (after $active; $n waiting)…"
            n > 0 -> "Queued $justQueued ($n waiting)…"
            else -> "Queued $justQueued…"
        }
    }

    private fun enqueueJobLocked(
        dataDir: File,
        job: Job,
        userLat: Double?,
        userLon: Double?,
    ) {
        val path = job.geofabrikPath.trim().trim('/')
        if (path.isEmpty()) return
        val active = loadJob(dataDir)
        val q = loadQueue(dataDir).toMutableList()
        val alreadyActive =
            active != null &&
                PackRegionAvailability.regionIdsMatchForCatalog(active.geofabrikPath, path)
        val alreadyQueued =
            q.any { PackRegionAvailability.regionIdsMatchForCatalog(it.geofabrikPath, path) }
        if (!alreadyActive && !alreadyQueued) {
            q.add(job)
        }
        val orderedPaths =
            PlaceIndexReady.prioritizePaths(
                q.map { it.geofabrikPath },
                userLat,
                userLon,
            )
        val byPath = q.associateBy { PackRegionAvailability.normalize(it.geofabrikPath) }
        val ordered =
            orderedPaths.mapNotNull { p ->
                byPath[PackRegionAvailability.normalize(p)]
            }
        saveQueue(dataDir, ordered)
        if (userLat != null && userLon != null) {
            saveQueueLocation(dataDir, userLat, userLon)
        }
    }

    private const val QUEUE_LOC_FILE = "region-download-queue-loc.json"

    private fun queueFile(dataDir: File) = File(dataDir, QUEUE_FILE)

    /**
     * Parse [QUEUE_FILE] via [JSONArray]. Dedupes by Geofabrik path (first wins)
     * so a corrupt/legacy file cannot schedule the same region twice.
     */
    internal fun loadQueue(dataDir: File): List<Job> {
        val f = queueFile(dataDir)
        if (!f.isFile) return emptyList()
        return runCatching {
            val arr = JSONArray(f.readText())
            val jobs = mutableListOf<Job>()
            for (i in 0 until arr.length()) {
                val o = arr.optJSONObject(i) ?: continue
                val url = o.optString("url", "")
                val filename = o.optString("filename", "")
                if (url.isBlank() || filename.isBlank()) continue
                jobs.add(
                    Job(
                        url = url,
                        filename = filename,
                        geofabrikPath = o.optString("geofabrikPath", ""),
                        phase = Phase.parse(o.optString("phase").ifBlank { null }),
                    ),
                )
            }
            dedupeQueueJobs(jobs)
        }.getOrDefault(emptyList())
    }

    /** Keep first job per normalized Geofabrik path. */
    internal fun dedupeQueueJobs(jobs: List<Job>): List<Job> {
        val seen = LinkedHashSet<String>()
        val out = ArrayList<Job>(jobs.size)
        for (j in jobs) {
            val key = PackRegionAvailability.normalize(j.geofabrikPath)
            if (key.isEmpty()) continue
            if (!seen.add(key)) continue
            out.add(j.copy(geofabrikPath = key))
        }
        return out
    }

    internal fun saveQueue(
        dataDir: File,
        jobs: List<Job>,
    ) {
        dataDir.mkdirs()
        val deduped = dedupeQueueJobs(jobs)
        if (deduped.isEmpty()) {
            queueFile(dataDir).delete()
            return
        }
        val arr = JSONArray()
        for (j in deduped) {
            arr.put(
                JSONObject()
                    .put("url", j.url)
                    .put("filename", j.filename)
                    .put("geofabrikPath", j.geofabrikPath)
                    .put("phase", j.phase.wire()),
            )
        }
        queueFile(dataDir).writeText(arr.toString())
    }

    /** Pop the next prioritized queue job (GPS location from [QUEUE_LOC_FILE]). */
    internal fun popQueue(dataDir: File): Job? {
        val q = loadQueue(dataDir).toMutableList()
        if (q.isEmpty()) return null
        val loc = loadQueueLocation(dataDir)
        val orderedPaths =
            PlaceIndexReady.prioritizePaths(
                q.map { it.geofabrikPath },
                loc?.first,
                loc?.second,
            )
        val firstPath = orderedPaths.firstOrNull() ?: return null
        val idx =
            q.indexOfFirst {
                PackRegionAvailability.regionIdsMatchForCatalog(it.geofabrikPath, firstPath)
            }
        if (idx < 0) return null
        val job = q.removeAt(idx)
        saveQueue(dataDir, q)
        return job
    }

    private fun saveQueueLocation(
        dataDir: File,
        lat: Double,
        lon: Double,
    ) {
        File(dataDir, QUEUE_LOC_FILE).writeText("""{"lat":$lat,"lon":$lon}""")
    }

    private fun loadQueueLocation(dataDir: File): Pair<Double, Double>? {
        val f = File(dataDir, QUEUE_LOC_FILE)
        if (!f.isFile) return null
        val text = f.readText()
        val lat =
            Regex(""""lat"\s*:\s*(-?\d+(?:\.\d+)?)""")
                .find(text)
                ?.groupValues
                ?.get(1)
                ?.toDoubleOrNull()
                ?: return null
        val lon =
            Regex(""""lon"\s*:\s*(-?\d+(?:\.\d+)?)""")
                .find(text)
                ?.groupValues
                ?.get(1)
                ?.toDoubleOrNull()
                ?: return null
        return lat to lon
    }

    private suspend fun drainQueue(
        context: Context,
        dataDir: File,
    ) {
        while (true) {
            val next =
                mutex.withLock {
                    popQueue(dataDir)
                        ?: loadJob(dataDir)?.also {
                            // Solo sidecar resume (no queue entry yet).
                        }
                } ?: break
            // Avoid processing the same sidecar twice if it was also queued.
            if (loadJob(dataDir)?.geofabrikPath == next.geofabrikPath) {
                // runOneRegion will rewrite the sidecar.
            }
            runOneRegion(context, dataDir, next)
        }
    }

    private suspend fun runOneRegion(
        context: Context,
        dataDir: File,
        incoming: Job,
    ) {
        val url = incoming.url
        val filename = incoming.filename
        val geofabrikPath = incoming.geofabrikPath.trim().trim('/')
        var startPhase = incoming.phase
        val already = partialBytes(dataDir, filename)
        resuming.set(already > 0L || startPhase != Phase.PACKS)
        writeJob(dataDir, incoming)
        // In-progress download must not leave searchable place rows for this region.
        if (geofabrikPath.isNotBlank()) {
            PlaceIndexReady.clearReady(dataDir, geofabrikPath)
        }
        if (lastStatus.get().isBlank() || !lastStatus.get().startsWith("Resuming")) {
            lastStatus.set(
                if (already > 0L || startPhase != Phase.PACKS) {
                    "Resuming download of $geofabrikPath…"
                } else {
                    "Downloading $geofabrikPath… 0%"
                },
            )
        }
        Log.i(
            TAG,
            "start provision filename=$filename resume_bytes=$already " +
                "path=$geofabrikPath phase=$startPhase",
        )
        try {
            runOneRegionPipeline(context, dataDir, url, filename, geofabrikPath, startPhase)
        } catch (t: Throwable) {
            lastStatus.set("failed: ${t.message}")
            Log.e(TAG, "provisionRegionData crashed", t)
        } finally {
            clearJob(dataDir)
        }
    }

    private fun runOneRegionPipeline(
        context: Context,
        dataDir: File,
        url: String,
        filename: String,
        geofabrikPath: String,
        startPhase: Phase,
    ) {
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
                        "decision_reason=${decision.decisionReason} " +
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
                    // Download Geofabrik extract BEFORE place index / basemap.
                    lastStatus.set("Downloading extract for place index…")
                    val pbfReport =
                        provisionRegionData(
                            dataDir = dataDir.absolutePath,
                            pbfUrl = url,
                            pbfFilename = filename,
                            elevationTarUrl = null,
                        )
                    Log.i(TAG, "pack-server PBF provision: ${pbfReport.take(240)}")
                    if (!pbfReport.contains("PASS")) {
                        lastStatus.set("failed (extract download)")
                        lastCompletedPath.set(pathForDecision)
                        return
                    }
                    phase = Phase.BASEMAP
                    persistPhase(dataDir, url, filename, pathForDecision, phase)
                    val basemapOk =
                        downloadBasemapPmtiles(context, dataDir, pathForDecision)
                    if (!basemapOk) {
                        lastCompletedPath.set(pathForDecision)
                        lastStatus.set("done (basemap failed)")
                        return
                    }
                    phase = Phase.PLACE_INDEX
                    persistPhase(dataDir, url, filename, pathForDecision, phase)
                    if (!runPlaceIndexLocal(dataDir, filename, pathForDecision)) {
                        lastStatus.set("done (place index failed)")
                        lastCompletedPath.set(pathForDecision)
                        return
                    }
                    PlaceIndexReady.markReady(dataDir, pathForDecision)
                    markUsable(pathForDecision)
                    lastCompletedPath.set(pathForDecision)
                    lastStatus.set("done")
                    Log.i(
                        TAG,
                        "pack server install + extract + basemap + place index finished " +
                            "for $pathForDecision",
                    )
                    return
                }
            }
        }

        if (phase == Phase.PLACE_INDEX || phase == Phase.BASEMAP) {
            // Resume mid-pipeline. New order: basemap before place index.
            if (phase == Phase.BASEMAP) {
                persistPhase(dataDir, url, filename, pathForDecision, Phase.BASEMAP)
                if (pathForDecision.isNotBlank()) {
                    val basemapOk =
                        downloadBasemapPmtiles(context, dataDir, pathForDecision)
                    if (!basemapOk) {
                        lastCompletedPath.set(pathForDecision)
                        lastStatus.set("done (basemap failed)")
                        return
                    }
                }
                phase = Phase.PLACE_INDEX
                persistPhase(dataDir, url, filename, pathForDecision, phase)
            }
            if (phase == Phase.PLACE_INDEX) {
                persistPhase(dataDir, url, filename, pathForDecision, Phase.PLACE_INDEX)
                val ok =
                    if (PackRegionAvailability.localBakeReady(dataDir, pathForDecision) &&
                        pathForDecision.isNotBlank() &&
                        File(dataDir, filename).length() >= MIN_PBF_BYTES
                    ) {
                        runPlaceIndexLocal(dataDir, filename, pathForDecision)
                    } else if (PackRegionAvailability.localBakeReady(dataDir, pathForDecision) &&
                        pathForDecision.isNotBlank()
                    ) {
                        // Packs present but extract missing — fetch then index.
                        lastStatus.set("Downloading extract for place index…")
                        val pbfReport =
                            provisionRegionData(
                                dataDir = dataDir.absolutePath,
                                pbfUrl = url,
                                pbfFilename = filename,
                                elevationTarUrl = null,
                            )
                        if (!pbfReport.contains("PASS")) {
                            lastStatus.set("done (place index failed)")
                            lastCompletedPath.set(pathForDecision)
                            return
                        }
                        runPlaceIndexLocal(dataDir, filename, pathForDecision)
                    } else {
                        runPlaceIndexLocal(dataDir, filename, pathForDecision)
                    }
                if (!ok) {
                    lastStatus.set("done (place index failed)")
                    lastCompletedPath.set(pathForDecision)
                    return
                }
                PlaceIndexReady.markReady(dataDir, pathForDecision)
                markUsable(pathForDecision)
                // Local-bake resume: place index is done; convert is non-blocking.
                val resumePbf = File(dataDir, filename)
                if (resumePbf.isFile &&
                    resumePbf.length() >= MIN_PBF_BYTES &&
                    PackRegionAvailability.localBakeReady(dataDir, pathForDecision)
                ) {
                    val elev = File(dataDir, "elevation").takeIf { it.isDirectory }
                    IndexedMapsBackground.ensureStarted(
                        resumePbf,
                        dataDir,
                        elev,
                        pathForDecision.ifBlank { null },
                    )
                    Log.i(
                        TAG,
                        "local-bake place index ready (resume); convert handed to " +
                            "IndexedMapsBackground for $pathForDecision",
                    )
                }
                lastCompletedPath.set(pathForDecision)
                lastStatus.set("done")
                return
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
            val basemapPath = geofabrikPath.ifBlank { pathForDecision }
            // All network downloads for this region before any index/convert work.
            if (basemapPath.isNotBlank()) {
                persistPhase(dataDir, url, filename, basemapPath, Phase.BASEMAP)
                if (!downloadBasemapPmtiles(context, dataDir, basemapPath)) {
                    lastCompletedPath.set(basemapPath)
                    lastStatus.set("done (basemap failed)")
                    return
                }
            }
            val pbf = File(dataDir, filename)
            if (pbf.isFile && pbf.length() >= MIN_PBF_BYTES) {
                // Place index only needs the OSM extract — do not wait on a
                // multi-hour local convert (same readiness as pack-server path).
                persistPhase(dataDir, url, filename, basemapPath, Phase.PLACE_INDEX)
                if (!runPlaceIndexLocal(dataDir, filename, basemapPath)) {
                    lastStatus.set("done (place index failed)")
                    lastCompletedPath.set(basemapPath)
                    return
                }
                if (basemapPath.isNotBlank()) {
                    PlaceIndexReady.markReady(dataDir, basemapPath)
                    markUsable(basemapPath)
                }
                // Hand convert to IndexedMapsBackground (Convert progress slot) so
                // the region queue can proceed to the next download.
                val elev = File(dataDir, "elevation").takeIf { it.isDirectory }
                IndexedMapsBackground.ensureStarted(
                    pbf,
                    dataDir,
                    elev,
                    geofabrikPath.ifBlank { basemapPath }.ifBlank { null },
                )
                Log.i(
                    TAG,
                    "local-bake place index ready; convert handed to IndexedMapsBackground " +
                        "for ${geofabrikPath.ifBlank { basemapPath }}",
                )
            }
            lastCompletedPath.set(basemapPath)
            lastStatus.set("done")
        } else {
            lastStatus.set("failed")
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

    private fun markUsable(path: String) {
        val trimmed = path.trim().trim('/')
        if (trimmed.isEmpty()) return
        lastUsablePath.set(trimmed)
        lastStatus.set("$USABLE_STATUS_PREFIX — region ready for routing and search")
        Log.i(TAG, "region usable for routing/search path=$trimmed")
    }

    private fun runPlaceIndexLocal(
        dataDir: File,
        filename: String,
        regionId: String,
    ): Boolean {
        val pbf = File(dataDir, filename)
        if (!pbf.isFile || pbf.length() < MIN_PBF_BYTES) return false
        val rid = regionId.trim().trim('/')
        if (rid.isNotEmpty()) {
            Log.i(
                TAG,
                "local-bake pbf resolved region_id=$rid pbf=${pbf.absolutePath} expected_prefix=$rid",
            )
            if (!PackRegionAvailability.pbfMatchesRegion(pbf, rid)) {
                Log.e(
                    TAG,
                    "FAIL: PBF/region mismatch for place index region_id=$rid pbf=${pbf.name} " +
                        "expected_stem=${PackRegionAvailability.localStem(rid)}",
                )
                lastStatus.set("failed (pbf/region mismatch)")
                return false
            }
        }
        lastStatus.set("Place index: starting… 0% (0 / 6)")
        val placeReport =
            runCatching {
                ensurePlaceIndex(
                    pbf.absolutePath,
                    File(dataDir, "place_index.db").absolutePath,
                    rid.ifBlank { null },
                )
            }.getOrElse { t ->
                Log.e(TAG, "ensurePlaceIndex crashed", t)
                "FAIL: ${t.message}\n"
            }
        Log.i(TAG, "local-bake place index: ${placeReport.take(400)}")
        if (placeReport.contains("PASS")) {
            lastStatus.set("Place index ready 100% (6 / 6)")
        }
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
