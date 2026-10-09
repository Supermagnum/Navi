package no.navi.app

import android.util.Log
import uniffi.navi.TravelProfile
import uniffi.navi.ensureFerrySidecar
import uniffi.navi.ferrySidecarIsReady
import uniffi.navi.ferrySidecarProgressSnapshot
import java.io.File
import java.util.concurrent.atomic.AtomicReference

/**
 * Ferry-overlay sidecar build. Scheduled by [IdlePackJobs] while idle.
 * Plans must never parse region PBFs for overlay on the plan thread.
 */
object FerrySidecarBackground {
    private const val TAG = "FerrySidecarBg"
    private val lastStatus = AtomicReference("idle")

    fun isRunning(): Boolean = IdlePackJobs.isRunning()

    fun statusLine(): String {
        if (IdlePackJobs.isRunning()) {
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
     * Enqueue a sidecar build for [stem] under [packDir] when the on-disk
     * sidecar is missing or stale vs the region PBF.
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
        IdlePackJobs.offerFerry(packDir, trimmed, profile)
    }

    internal fun runJob(
        packDir: File,
        stem: String,
        profile: TravelProfile,
    ) {
        val label = stem.removeSuffix("-latest").replace('-', ' ')
        lastStatus.set("Preparing ferry data for $label…")
        Log.i(TAG, "start ensureFerrySidecar stem=$stem dir=${packDir.absolutePath}")
        val report =
            runCatching {
                ensureFerrySidecar(packDir.absolutePath, stem, profile)
            }.getOrElse { t ->
                Log.e(TAG, "ensureFerrySidecar crashed", t)
                "FAIL: ${t.message}"
            }
        if (report.contains("PASS")) {
            lastStatus.set("Ferry data ready for $label")
            Log.i(TAG, "finished stem=$stem report=$report")
        } else {
            lastStatus.set("Ferry data failed for $label")
            Log.e(TAG, "failed stem=$stem report=$report")
        }
    }

    /** After a region install/refresh: build sidecar for the leaf stem. */
    fun ensureForRegionPath(
        packDir: File,
        geofabrikPath: String,
    ) {
        val stem = PackRegionAvailability.localStem(geofabrikPath).trim()
        if (stem.isEmpty()) return
        ensureStarted(packDir, stem)
        ensureStarted(packDir, stem, TravelProfile.TRUCK)
    }

    /**
     * Idle enqueue for every installed region whose sidecar is missing or
     * stale (PBF / [FERRY_SIDECAR_BUILD] fingerprint).
     */
    fun ensureFromInstalledMaps() {
        IdlePackJobs.onInstalledMapsChanged()
        IdlePackJobs.onAppIdle()
    }

    fun clearQueueForPlan() {
        IdlePackJobs.pauseForPlan()
        lastStatus.set("Ferry data paused (planning)…")
    }
}
