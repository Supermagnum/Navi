package no.navi.app

import android.util.Log
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import uniffi.navi.TravelProfile
import uniffi.navi.corridorSkeletonIsReady
import uniffi.navi.corridorSkeletonProgressSnapshot
import uniffi.navi.ensureCorridorSkeleton
import java.io.File
import java.util.concurrent.ConcurrentLinkedQueue
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference

/**
 * Background corridor-skeleton build. Kicked while idle from
 * [InstalledMaps] (and from a plan that surfaces `skeleton_preparing`).
 * Plans must never build skeletons on the plan thread.
 */
object CorridorSkeletonBackground {
    private const val TAG = "CorridorSkeletonBg"
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val mutex = Mutex()
    private val running = AtomicBoolean(false)
    private val lastStatus = AtomicReference("idle")
    private val queue = ConcurrentLinkedQueue<Job>()

    private data class Job(
        val packDir: File,
        val stem: String,
        val profile: TravelProfile,
    )

    fun isRunning(): Boolean = running.get()

    fun statusLine(): String {
        if (running.get()) {
            val snap = runCatching { corridorSkeletonProgressSnapshot() }.getOrNull()
            if (snap != null && snap.message.isNotBlank()) {
                return if (snap.pct.toInt() > 0) {
                    "${snap.message} ${snap.pct}%"
                } else {
                    snap.message
                }
            }
        }
        return lastStatus.get()
    }

    /**
     * Enqueue a car-profile skeleton build for [stem] under [packDir] when the
     * on-disk skeleton is missing or stale vs pack / neighbor fingerprints.
     */
    fun ensureStarted(
        packDir: File,
        stem: String,
        profile: TravelProfile = TravelProfile.CAR,
    ) {
        val trimmed = stem.trim()
        if (trimmed.isEmpty()) return
        val man = File(packDir, "$trimmed.navi-manifest.json")
        if (!man.isFile) {
            Log.i(TAG, "skip corridor skeleton: no manifest for stem=$trimmed")
            return
        }
        val ready =
            runCatching {
                corridorSkeletonIsReady(packDir.absolutePath, trimmed, profile)
            }.getOrDefault(false)
        if (ready) {
            Log.i(TAG, "corridor skeleton already ready stem=$trimmed")
            return
        }
        queue.offer(Job(packDir, trimmed, profile))
        drain()
    }

    /** After a region install/refresh: build skeleton for the leaf stem. */
    fun ensureForRegionPath(
        packDir: File,
        geofabrikPath: String,
    ) {
        val stem = PackRegionAvailability.localStem(geofabrikPath).trim()
        if (stem.isEmpty()) return
        ensureStarted(packDir, stem)
    }

    /**
     * Idle enqueue for every installed region whose skeleton is missing or
     * stale (pack / neighbor fingerprint). Always probes UniFFI readiness so a
     * newly appeared neighbor skeleton can trigger a border rebuild.
     */
    fun ensureFromInstalledMaps() {
        if (RoutePlanGate.isRunning()) return
        val snap = InstalledMaps.current() ?: return
        for (r in snap.regions.values) {
            ensureStarted(r.packDir, r.stem, TravelProfile.CAR)
        }
    }

    private fun drain() {
        if (!running.compareAndSet(false, true)) return
        scope.launch {
            mutex.withLock {
                try {
                    while (true) {
                        // Never build during a plan (same idle rule as ferry).
                        while (RoutePlanGate.isRunning()) {
                            lastStatus.set("Corridor skeleton paused (planning)…")
                            delay(500)
                        }
                        val job = queue.poll() ?: break
                        if (RoutePlanGate.isRunning()) {
                            queue.offer(job)
                            continue
                        }
                        val label = job.stem.removeSuffix("-latest").replace('-', ' ')
                        lastStatus.set("Preparing corridor skeleton for $label…")
                        Log.i(
                            TAG,
                            "start ensureCorridorSkeleton stem=${job.stem} dir=${job.packDir.absolutePath}",
                        )
                        val report =
                            runCatching {
                                ensureCorridorSkeleton(
                                    job.packDir.absolutePath,
                                    job.stem,
                                    job.profile,
                                )
                            }.getOrElse { t ->
                                Log.e(TAG, "ensureCorridorSkeleton crashed", t)
                                "FAIL: ${t.message}"
                            }
                        if (report.contains("PASS")) {
                            lastStatus.set("Corridor skeleton ready for $label")
                            Log.i(TAG, "finished stem=${job.stem} report=$report")
                        } else {
                            lastStatus.set("Corridor skeleton failed for $label")
                            Log.e(TAG, "failed stem=${job.stem} report=$report")
                        }
                    }
                } finally {
                    running.set(false)
                    if (queue.isNotEmpty()) {
                        drain()
                    }
                }
            }
        }
    }
}
