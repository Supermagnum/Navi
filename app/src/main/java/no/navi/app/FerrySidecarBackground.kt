package no.navi.app

import android.util.Log
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
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
 * Background ferry-overlay sidecar build, kicked lazily when a plan needs
 * overlay and the on-disk sidecar is missing/stale (not at pack install).
 *
 * Plans must never parse region PBFs for ferry overlay on the plan thread;
 * they load the sidecar or return `ferry_preparing` until ensure finishes
 * (Rust also spawns `ensure_ferry_sidecar` from the plan path).
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
        val pbf = File(packDir, "$trimmed.osm.pbf")
        val ferryPbf = File(packDir, "$trimmed.ferry.osm.pbf")
        if (!pbf.isFile && !ferryPbf.isFile) {
            Log.i(TAG, "skip ferry sidecar: no PBF for stem=$trimmed")
            return
        }
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

    private fun drain() {
        if (!running.compareAndSet(false, true)) return
        scope.launch {
            mutex.withLock {
                try {
                    while (true) {
                        val job = queue.poll() ?: break
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
