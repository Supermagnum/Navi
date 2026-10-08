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
import uniffi.navi.ensureFerrySidecar
import uniffi.navi.ferrySidecarIsReady
import uniffi.navi.ferrySidecarProgressSnapshot
import java.io.File
import java.util.concurrent.ConcurrentLinkedQueue
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference

/**
 * Background ferry-overlay sidecar build. Kicked while idle from
 * [InstalledMaps] (and from a plan that surfaces `ferry_preparing`).
 * Plans must never parse region PBFs for overlay on the plan thread.
 */
object FerrySidecarBackground {
    private const val TAG = "FerrySidecarBg"
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
            val snap = runCatching { ferrySidecarProgressSnapshot() }.getOrNull()
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
     * Enqueue a car-profile sidecar build for [stem] under [packDir] when the
     * on-disk sidecar is missing or stale vs the region PBF.
     */
    fun ensureStarted(
        packDir: File,
        stem: String,
        profile: TravelProfile = TravelProfile.CAR,
    ) {
        val trimmed = stem.trim()
        if (trimmed.isEmpty()) return
        // Leaf may share a country extract (e.g. norrbotten → sweden-latest.osm.pbf).
        // UniFFI resolve + ensure handles that; do not require `{stem}.osm.pbf`.
        val ready =
            runCatching {
                ferrySidecarIsReady(packDir.absolutePath, trimmed, profile)
            }.getOrDefault(false)
        if (ready) {
            Log.i(TAG, "ferry sidecar already ready stem=$trimmed")
            return
        }
        queue.offer(Job(packDir, trimmed, profile))
        drain()
    }

    /** After a region install/refresh: build sidecar for the leaf stem. */
    fun ensureForRegionPath(
        packDir: File,
        geofabrikPath: String,
    ) {
        val stem = PackRegionAvailability.localStem(geofabrikPath).trim()
        if (stem.isEmpty()) return
        ensureStarted(packDir, stem)
    }

    /**
     * Idle enqueue for every installed region whose sidecar is missing or
     * stale (PBF / [FERRY_SIDECAR_BUILD] fingerprint). Always probe UniFFI
     * readiness — file presence alone is not enough after a build bump.
     */
    fun ensureFromInstalledMaps() {
        if (RoutePlanGate.isRunning() || NaviMapTestHooks.pendingTripPlan != null) return
        val snap = InstalledMaps.current() ?: return
        for (r in snap.regions.values) {
            ensureStarted(r.packDir, r.stem, TravelProfile.CAR)
            // Truck / mobile_home plans load car packs when a region has no truck
            // graph, but still request the truck ferry sidecar.
            ensureStarted(r.packDir, r.stem, TravelProfile.TRUCK)
        }
    }

    fun clearQueueForPlan() {
        queue.clear()
        lastStatus.set("Ferry data paused (planning)…")
    }

    private fun drain() {
        if (!running.compareAndSet(false, true)) return
        scope.launch {
            mutex.withLock {
                try {
                    while (true) {
                        while (RoutePlanGate.isRunning()) {
                            lastStatus.set("Ferry data paused (planning)…")
                            delay(500)
                        }
                        val job = queue.poll() ?: break
                        if (RoutePlanGate.isRunning()) {
                            queue.offer(job)
                            continue
                        }
                        val label = job.stem.removeSuffix("-latest").replace('-', ' ')
                        lastStatus.set("Preparing ferry data for $label…")
                        Log.i(
                            TAG,
                            "start ensureFerrySidecar stem=${job.stem} dir=${job.packDir.absolutePath}",
                        )
                        val report =
                            runCatching {
                                ensureFerrySidecar(
                                    job.packDir.absolutePath,
                                    job.stem,
                                    job.profile,
                                )
                            }.getOrElse { t ->
                                Log.e(TAG, "ensureFerrySidecar crashed", t)
                                "FAIL: ${t.message}"
                            }
                        if (report.contains("PASS")) {
                            lastStatus.set("Ferry data ready for $label")
                            Log.i(TAG, "finished stem=${job.stem} report=$report")
                        } else {
                            lastStatus.set("Ferry data failed for $label")
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
