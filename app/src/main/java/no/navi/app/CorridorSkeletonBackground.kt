package no.navi.app

import android.util.Log
import uniffi.navi.TravelProfile
import uniffi.navi.corridorSkeletonIsReady
import uniffi.navi.corridorSkeletonProgressSnapshot
import uniffi.navi.ensureCorridorSkeleton
import java.io.File
import java.util.concurrent.atomic.AtomicReference

/**
 * Corridor-skeleton build. Scheduled by [IdlePackJobs] (never on the plan
 * thread). A region's skeleton does not wait on its map tiles.
 */
object CorridorSkeletonBackground {
    private const val TAG = "CorridorSkeletonBg"
    private val lastStatus = AtomicReference("idle")

    fun isRunning(): Boolean = IdlePackJobs.isRunning()

    fun statusLine(): String {
        if (IdlePackJobs.isRunning()) {
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
        IdlePackJobs.offerSkeleton(packDir, trimmed)
    }

    internal fun runJob(
        packDir: File,
        stem: String,
        profile: TravelProfile,
    ) {
        val label = stem.removeSuffix("-latest").replace('-', ' ')
        lastStatus.set("Preparing corridor skeleton for $label…")
        Log.i(TAG, "start ensureCorridorSkeleton stem=$stem dir=${packDir.absolutePath}")
        val report =
            runCatching {
                ensureCorridorSkeleton(packDir.absolutePath, stem, profile)
            }.getOrElse { t ->
                Log.e(TAG, "ensureCorridorSkeleton crashed", t)
                "FAIL: ${t.message}"
            }
        if (report.contains("PASS")) {
            lastStatus.set("Corridor skeleton ready for $label")
            Log.i(TAG, "finished stem=$stem report=$report")
        } else {
            lastStatus.set("Corridor skeleton failed for $label")
            Log.e(TAG, "failed stem=$stem report=$report")
        }
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

    fun ensureFromInstalledMaps() {
        IdlePackJobs.onInstalledMapsChanged()
        val snap = InstalledMaps.current() ?: return
        if (RoutePlanGate.isRunning() || NaviMapTestHooks.pendingTripPlan != null) return
        for (r in snap.regions.values) {
            ensureStarted(r.packDir, r.stem, TravelProfile.CAR)
        }
        IdlePackJobs.onAppIdle()
    }

    fun clearQueueForPlan() {
        IdlePackJobs.pauseForPlan()
        lastStatus.set("Corridor skeleton paused (planning)…")
    }
}
