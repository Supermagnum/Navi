package no.navi.app

import android.content.Context
import android.util.Log
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import uniffi.navi.TravelProfile
import java.io.File
import java.util.concurrent.ConcurrentLinkedQueue
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference

/**
 * One idle-job queue for ferry sidecars, corridor skeletons, and place-index
 * builds. [InstalledMaps] changes enqueue work; an idle process with
 * outstanding work drains it. One job at a time; never during a plan. A
 * region's skeleton and sidecar do not wait on its map tiles.
 */
object IdlePackJobs {
    const val TAG = "IdlePackJobs"

    enum class Kind {
        FERRY_CAR,
        FERRY_TRUCK,
        SKELETON,
        PLACE_INDEX,
    }

    data class Job(
        val kind: Kind,
        val stem: String,
        val packDir: File,
        val regionId: String,
        val pbf: File? = null,
        val indexDb: File? = null,
    ) {
        fun key(): String = "${kind.name}|${packDir.absolutePath}|$stem|$regionId"
    }

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val mutex = Mutex()
    private val queue = ConcurrentLinkedQueue<Job>()
    private val seen = linkedSetOf<String>()
    private val running = AtomicBoolean(false)
    private val lastStatus = AtomicReference("idle")
    private val active = AtomicReference<Job?>(null)
    private val pausedJob = AtomicReference<Job?>(null)
    private val lastPause = AtomicReference(PauseOutcome())
    private val lastRecheckAccepted = java.util.concurrent.atomic.AtomicInteger(0)

    data class PauseOutcome(
        val action: String = "none",
        val kind: Kind? = null,
        val regionId: String = "",
        val durationMs: Long = 0,
    )

    @Volatile
    var testRunJob: ((Job) -> Boolean)? = null

    @Volatile
    var executeJobs: Boolean = true

    @Volatile
    private var appContext: Context? = null

    fun attachContext(context: Context) {
        appContext = context.applicationContext
    }

    fun resetForTests() {
        queue.clear()
        synchronized(seen) { seen.clear() }
        running.set(false)
        active.set(null)
        pausedJob.set(null)
        lastPause.set(PauseOutcome())
        lastRecheckAccepted.set(0)
        lastStatus.set("idle")
        executeJobs = false
        testRunJob = null
    }

    fun scheduledForTest(): List<Job> = queue.toList()

    fun isRunning(): Boolean = running.get()

    fun outstandingCount(): Int = queue.size + if (running.get()) 1 else 0

    fun remainingSummary(): String {
        val jobs = mutableListOf<Job>()
        active.get()?.let { jobs.add(it) }
        jobs.addAll(queue)
        if (jobs.isEmpty()) return ""
        val n = jobs.size
        val names =
            jobs
                .map { stepLabel(it) }
                .distinct()
                .joinToString(", ")
        return if (n == 1) {
            "1 step remains ($names)"
        } else {
            "$n steps remain ($names)"
        }
    }

    fun statusLine(): String {
        val rem = remainingSummary()
        val cur = lastStatus.get()
        return if (rem.isNotEmpty() && !cur.contains("step")) {
            "$cur — $rem"
        } else {
            cur
        }
    }

    fun onInstalledMapsChanged() {
        enqueueFromSnapshot()
    }

    fun lastRecheckAccepted(): Int = lastRecheckAccepted.get()

    fun onAppIdle() {
        recheckRejectedArchives()
        enqueueFromSnapshot()
        drainIfAllowed()
    }

    private fun recheckRejectedArchives() {
        if (RoutePlanGate.isRunning()) return
        val ctx = appContext ?: return
        val dataDir = NaviAppData.resolve(ctx)
        val n =
            runCatching { uniffi.navi.pmtilesRecheckRejected(dataDir.absolutePath).toInt() }
                .getOrDefault(0)
        lastRecheckAccepted.set(n)
        val audited = auditCompletedArchives(dataDir)
        if (n > 0 || audited > 0) {
            Log.i(TAG, "rejected_archives_accepted=$n completed_invalid=$audited")
            runCatching { InstalledMaps.refresh(ctx) }
        }
    }

    private fun auditCompletedArchives(dataDir: File): Int {
        val jobs =
            runCatching { uniffi.navi.pmtilesListJobs(dataDir.absolutePath) }.getOrDefault(emptyList())
        val shown = PmtilesArchiveGate.readShownMarker(dataDir)
        var n = 0
        for (job in jobs) {
            if (!job.status.equals("completed", ignoreCase = true)) continue
            val file = File(job.localPath)
            val why = PmtilesArchiveGate.rejectionReason(file, job.regionKey) ?: continue
            val sidecar = File(file.absolutePath + ".reason")
            sidecar.writeText(why)
            if (shown != null && shown == file.absolutePath) {
                Log.i(TAG, "invalid_shown_archive path=${file.name} reason=$why (kept until style switch)")
            }
            n++
        }
        return n
    }

    fun onPlanEnded() {
        if (runCatching { uniffi.navi.foregroundPlanActive() }.getOrDefault(false)) {
            return
        }
        pausedJob.getAndSet(null)?.let { offer(it) }
        drainIfAllowed()
    }

    fun lastPauseOutcome(): PauseOutcome = lastPause.get()

    /**
     * Wait for a short sidecar/skeleton to finish, or for a place-index to
     * unwind at its next committed batch. Does not wait for a long index to
     * complete.
     */
    fun waitOrPauseForPlan(): PauseOutcome {
        val job = active.get()
        if (job == null && !running.get()) {
            val none = PauseOutcome()
            lastPause.set(none)
            return none
        }
        val t0 = System.currentTimeMillis()
        lastStatus.set("Idle pack jobs paused (planning)…")
        while (running.get()) {
            Thread.sleep(20)
        }
        val ms = System.currentTimeMillis() - t0
        val paused = pausedJob.get()
        val outcome =
            if (paused != null) {
                PauseOutcome("paused", paused.kind, paused.regionId, ms)
            } else if (job != null && (job.kind == Kind.SKELETON || job.kind == Kind.FERRY_CAR || job.kind == Kind.FERRY_TRUCK)) {
                PauseOutcome("waited", job.kind, job.regionId, ms)
            } else if (job?.kind == Kind.PLACE_INDEX) {
                PauseOutcome("paused", job.kind, job.regionId, ms)
            } else {
                PauseOutcome("none", job?.kind, job?.regionId ?: "", ms)
            }
        lastPause.set(outcome)
        Log.i(
            TAG,
            "plan_idle_pause action=${outcome.action} kind=${outcome.kind} " +
                "region=${outcome.regionId} duration_ms=${outcome.durationMs}",
        )
        return outcome
    }

    fun pauseForPlan() {
        queue.clear()
        synchronized(seen) { seen.clear() }
        lastStatus.set("Idle pack jobs paused (planning)…")
    }

    fun offerFerry(
        packDir: File,
        stem: String,
        profile: TravelProfile,
    ) {
        val kind =
            when (profile) {
                TravelProfile.TRUCK, TravelProfile.MOBILE_HOME -> Kind.FERRY_TRUCK
                else -> Kind.FERRY_CAR
            }
        val regionId = regionIdOf(packDir, stem)
        offer(Job(kind, stem, packDir, regionId))
        drainIfAllowed()
    }

    fun offerSkeleton(
        packDir: File,
        stem: String,
    ) {
        offer(Job(Kind.SKELETON, stem, packDir, regionIdOf(packDir, stem)))
        drainIfAllowed()
    }

    fun offerPlaceIndex(
        pbf: File,
        indexDb: File,
        regionId: String,
    ) {
        val rid = PackRegionAvailability.normalize(regionId)
        val product = appContext?.let { PlaceIndexStorage.resolveDb(it) }
        val writingProduct = product != null && indexDb.canonicalFile == product.canonicalFile
        val installed =
            PackRegionAvailability.installedPackRegionIds(
                InstalledMaps.current()?.regions?.values.orEmpty(),
            )
        if (!PackRegionAvailability.mayIndexRegion(rid, pbf, installed, writingProduct)) {
            Log.e(TAG, "refusing PLACE_INDEX region=$rid pbf=${pbf.name} product=$writingProduct")
            return
        }
        val stem = PackRegionAvailability.localStem(regionId).ifBlank { pbf.name.removeSuffix(".osm.pbf") }
        offer(
            Job(
                kind = Kind.PLACE_INDEX,
                stem = stem,
                packDir = pbf.parentFile ?: indexDb.parentFile ?: File("."),
                regionId = rid,
                pbf = pbf,
                indexDb = indexDb,
            ),
        )
        drainIfAllowed()
    }

    private fun regionIdOf(
        packDir: File,
        stem: String,
    ): String {
        val snap = InstalledMaps.current()
        snap?.regions?.values?.firstOrNull { it.stem == stem && it.packDir == packDir }?.let {
            return it.regionId
        }
        return stem.removeSuffix("-latest").replace('_', '-')
    }

    private fun offer(job: Job) {
        val k = job.key()
        synchronized(seen) {
            if (!seen.add(k)) return
        }
        queue.offer(job)
        Log.i(TAG, "scheduled ${job.kind} stem=${job.stem} region=${job.regionId}")
    }

    private fun enqueueFromSnapshot() {
        val snap = InstalledMaps.current() ?: return
        val ctx = appContext
        val indexDir =
            if (ctx != null) {
                PlaceIndexStorage.indexDir(ctx)
            } else {
                null
            }
        val installed = PackRegionAvailability.installedPackRegionIds(snap.regions.values)
        for (r in snap.regions.values) {
            val man = File(r.packDir, "${r.stem}.navi-manifest.json")
            if (!man.isFile) continue
            // Skeleton and sidecar do not wait on map tiles.
            if (!r.ferrySidecarCar) {
                offer(Job(Kind.FERRY_CAR, r.stem, r.packDir, r.regionId))
            }
            if (!r.ferrySidecarTruck) {
                offer(Job(Kind.FERRY_TRUCK, r.stem, r.packDir, r.regionId))
            }
            if (!r.corridorSkeleton) {
                offer(Job(Kind.SKELETON, r.stem, r.packDir, r.regionId))
            }
        }
        val indexRoot =
            indexDir
                ?: snap.regions.values
                    .firstOrNull()
                    ?.packDir
        if (indexRoot != null) {
            val db = File(indexRoot, "place_index.db")
            for (m in snap.missingPlaceIndex) {
                val r = snap.regions[m.regionId]
                val stamp =
                    r?.let { File(it.packDir, "${it.stem}.navi-server-install.json") }
                val pbf = m.pbfPath?.takeIf { it.isFile && it.length() >= RegionDownloadBackground.MIN_PBF_BYTES }
                if (pbf == null && stamp?.isFile != true) continue
                if (!PackRegionAvailability.mayIndexRegion(m.regionId, pbf, installed, ctx != null)) {
                    Log.i(TAG, "skip PLACE_INDEX region=${m.regionId}: not own source / not an installed pack")
                    continue
                }
                offer(
                    Job(
                        kind = Kind.PLACE_INDEX,
                        stem = r?.stem ?: PackRegionAvailability.localStem(m.regionId),
                        packDir = r?.packDir ?: pbf?.parentFile ?: indexRoot,
                        regionId = m.regionId,
                        pbf = pbf,
                        indexDb = db,
                    ),
                )
            }
            // Intact index from another source: record the first listed
            // place-source sha256 without rebuilding (follow-up 37 item 7).
            for (r in snap.regions.values) {
                if (r.placeIndex != InstalledMaps.PlaceIndexState.INTACT &&
                    r.placeIndex != InstalledMaps.PlaceIndexState.LEGACY_INTACT
                ) {
                    continue
                }
                if (r.placeIndexSourceSha.isNotBlank()) continue
                val stamp = File(r.packDir, "${r.stem}.navi-server-install.json")
                if (!stamp.isFile) continue
                if (!PackRegionAvailability.mayIndexRegion(r.regionId, r.pbfPath, installed, true)) {
                    continue
                }
                offer(
                    Job(
                        kind = Kind.PLACE_INDEX,
                        stem = r.stem,
                        packDir = r.packDir,
                        regionId = r.regionId,
                        pbf = r.pbfPath,
                        indexDb = db,
                    ),
                )
            }
        }
    }

    private fun planBlocksIdle(): Boolean {
        if (RoutePlanGate.isRunning() || NaviMapTestHooks.pendingTripPlan != null) return true
        return runCatching { uniffi.navi.foregroundPlanActive() }.getOrDefault(false)
    }

    private fun drainIfAllowed() {
        if (!executeJobs) return
        if (planBlocksIdle()) return
        if (queue.isEmpty()) return
        if (!running.compareAndSet(false, true)) return
        scope.launch {
            mutex.withLock {
                try {
                    while (true) {
                        if (planBlocksIdle()) {
                            lastStatus.set("Idle pack jobs paused (planning)…")
                            break
                        }
                        val job = pollNext() ?: break
                        if (job.kind == Kind.PLACE_INDEX && !placeIndexDownloadAllowed(job)) {
                            lastStatus.set("Waiting for unmetered connection to download place-source…")
                            synchronized(seen) { seen.remove(job.key()) }
                            break
                        }
                        active.set(job)
                        val rem = remainingSummary()
                        lastStatus.set("Running ${stepLabel(job)} — $rem")
                        Log.i(TAG, "start ${job.kind} stem=${job.stem} remaining=${outstandingCount()}")
                        val paused =
                            runCatching { runJob(job) }
                                .onFailure { t -> Log.e(TAG, "job ${job.kind} stem=${job.stem} crashed", t) }
                                .getOrDefault(false)
                        if (paused) {
                            pausedJob.set(job)
                            lastStatus.set("Idle pack jobs paused (planning)…")
                            break
                        }
                        synchronized(seen) { seen.remove(job.key()) }
                        active.set(null)
                    }
                } finally {
                    if (queue.isEmpty() && pausedJob.get() == null) lastStatus.set("idle")
                    running.set(false)
                    active.set(null)
                    if (queue.isNotEmpty() && !planBlocksIdle()) {
                        drainIfAllowed()
                    }
                }
            }
        }
    }

    private fun pollNext(): Job? {
        val preferred =
            listOf(Kind.FERRY_CAR, Kind.FERRY_TRUCK, Kind.SKELETON, Kind.PLACE_INDEX)
        for (kind in preferred) {
            val found = queue.firstOrNull { it.kind == kind } ?: continue
            queue.remove(found)
            return found
        }
        return queue.poll()
    }

    private fun placeIndexDownloadAllowed(job: Job): Boolean {
        val haveSource =
            job.packDir.listFiles()?.any { it.name.endsWith(".navi-place-source.osm.pbf") } == true
        val pbfOk =
            job.pbf != null &&
                job.pbf.isFile &&
                job.pbf.length() >= RegionDownloadBackground.MIN_PBF_BYTES
        if (haveSource || pbfOk) return true
        val ctx = appContext ?: return true
        val longTripPacks =
            job.packDir.name == LongTripPackStorage.PACKS_SUBDIR ||
                job.packDir.path.contains("long-trip-packs")
        if (!longTripPacks) return true
        return NetworkUnmetered.isWifiOrEthernet(ctx)
    }

    private fun stepLabel(job: Job): String {
        val name =
            RegionProgressMessages.regionName(job.regionId).ifBlank {
                job.stem.removeSuffix("-latest").replace('-', ' ')
            }
        return when (job.kind) {
            Kind.FERRY_CAR -> "ferry sidecar (car) for $name"
            Kind.FERRY_TRUCK -> "ferry sidecar (truck) for $name"
            Kind.SKELETON -> "corridor skeleton for $name"
            Kind.PLACE_INDEX -> "place index for $name"
        }
    }

    /** True when the job paused for a plan and must be resumed. */
    private fun runJob(job: Job): Boolean {
        testRunJob?.let {
            return it(job)
        }
        return when (job.kind) {
            Kind.FERRY_CAR -> {
                FerrySidecarBackground.runJob(job.packDir, job.stem, TravelProfile.CAR)
                false
            }
            Kind.FERRY_TRUCK -> {
                FerrySidecarBackground.runJob(job.packDir, job.stem, TravelProfile.TRUCK)
                false
            }
            Kind.SKELETON -> {
                CorridorSkeletonBackground.runJob(job.packDir, job.stem, TravelProfile.CAR)
                false
            }
            Kind.PLACE_INDEX -> {
                val db = job.indexDb ?: return false
                PlaceIndexBackground.runPackJob(job.packDir, db, job.regionId, job.pbf)
            }
        }
    }
}
