package no.navi.app

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

    @Volatile
    var executeJobs: Boolean = true

    fun resetForTests() {
        queue.clear()
        synchronized(seen) { seen.clear() }
        running.set(false)
        active.set(null)
        lastStatus.set("idle")
        executeJobs = false
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

    fun onAppIdle() {
        enqueueFromSnapshot()
        drainIfAllowed()
    }

    fun onPlanEnded() {
        drainIfAllowed()
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
        val stem = PackRegionAvailability.localStem(regionId).ifBlank { pbf.name.removeSuffix(".osm.pbf") }
        offer(
            Job(
                kind = Kind.PLACE_INDEX,
                stem = stem,
                packDir = pbf.parentFile ?: indexDb.parentFile ?: File("."),
                regionId = PackRegionAvailability.normalize(regionId),
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
        val indexDir = InstalledMaps.placeIndexDir()
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
        if (indexDir != null) {
            val db = File(indexDir, "place_index.db")
            for (m in snap.missingPlaceIndex) {
                val pbf = m.pbfPath ?: continue
                if (!pbf.isFile) continue
                offer(
                    Job(
                        kind = Kind.PLACE_INDEX,
                        stem = PackRegionAvailability.localStem(m.regionId),
                        packDir = pbf.parentFile ?: indexDir,
                        regionId = m.regionId,
                        pbf = pbf,
                        indexDb = db,
                    ),
                )
            }
        }
    }

    private fun drainIfAllowed() {
        if (!executeJobs) return
        if (RoutePlanGate.isRunning() || NaviMapTestHooks.pendingTripPlan != null) return
        if (queue.isEmpty()) return
        if (!running.compareAndSet(false, true)) return
        scope.launch {
            mutex.withLock {
                try {
                    while (true) {
                        if (RoutePlanGate.isRunning() || NaviMapTestHooks.pendingTripPlan != null) {
                            lastStatus.set("Idle pack jobs paused (planning)…")
                            break
                        }
                        val job = pollNext() ?: break
                        active.set(job)
                        val rem = remainingSummary()
                        lastStatus.set("Running ${stepLabel(job)} — $rem")
                        Log.i(TAG, "start ${job.kind} stem=${job.stem} remaining=${outstandingCount()}")
                        runCatching { runJob(job) }
                            .onFailure { t -> Log.e(TAG, "job ${job.kind} stem=${job.stem} crashed", t) }
                        synchronized(seen) { seen.remove(job.key()) }
                        active.set(null)
                    }
                } finally {
                    if (queue.isEmpty()) lastStatus.set("idle")
                    running.set(false)
                    if (queue.isNotEmpty() &&
                        !RoutePlanGate.isRunning() &&
                        NaviMapTestHooks.pendingTripPlan == null
                    ) {
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

    private fun runJob(job: Job) {
        when (job.kind) {
            Kind.FERRY_CAR ->
                FerrySidecarBackground.runJob(job.packDir, job.stem, TravelProfile.CAR)
            Kind.FERRY_TRUCK ->
                FerrySidecarBackground.runJob(job.packDir, job.stem, TravelProfile.TRUCK)
            Kind.SKELETON ->
                CorridorSkeletonBackground.runJob(job.packDir, job.stem, TravelProfile.CAR)
            Kind.PLACE_INDEX -> {
                val pbf = job.pbf ?: return
                val db = job.indexDb ?: return
                PlaceIndexBackground.runJob(pbf, db, job.regionId)
            }
        }
    }
}
