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
 * thread). Starts after place index for the region is ready — never in parallel
 * with download / indexing. A region's skeleton does not wait on its map tiles.
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
        IdlePackJobs.offerSkeleton(packDir, trimmed, profile)
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
            // Prefer "ready" over a leftover "Preparing…" so idle status lines
            // and any UI that polls [statusLine] never keep a false preparing state.
            lastStatus.set("Corridor skeleton ready for $label")
            Log.i(TAG, "finished stem=$stem report=$report")
        } else {
            lastStatus.set("Corridor skeleton failed for $label")
            Log.e(TAG, "failed stem=$stem report=$report")
        }
    }

    /** After place index finishes for [regionId]: build car (+ truck) skeletons. */
    fun ensureAfterPlaceIndex(
        packDir: File,
        regionId: String,
    ) {
        val rid = PackRegionAvailability.normalize(regionId)
        if (rid.isEmpty()) return
        val stem =
            InstalledMaps.region(rid)?.stem
                ?: PackRegionAvailability.localStem(rid)
        if (stem.isBlank()) return
        // Ferry overlay before skeleton so Stage B sees pier/approach stubs.
        FerrySidecarBackground.ensureForRegionPath(packDir, rid)
        if (carSkeletonReady(packDir, stem)) {
            LongTripCoordinator.onRoutesReady(rid)
            ensureStarted(packDir, stem, TravelProfile.TRUCK)
            IdlePackJobs.onAppIdle()
            return
        }
        LongTripCoordinator.onBuildingRoutes(rid)
        ensureStarted(packDir, stem, TravelProfile.CAR)
        ensureStarted(packDir, stem, TravelProfile.TRUCK)
        IdlePackJobs.onAppIdle()
    }

    /** After a region install/refresh: only when place index is already intact. */
    fun ensureForRegionPath(
        packDir: File,
        geofabrikPath: String,
    ) {
        val rid = PackRegionAvailability.normalize(geofabrikPath)
        val r = InstalledMaps.region(rid)
        val indexOk =
            r != null &&
                (
                    r.placeIndex == InstalledMaps.PlaceIndexState.INTACT ||
                        r.placeIndex == InstalledMaps.PlaceIndexState.LEGACY_INTACT
                )
        if (!indexOk || r == null) {
            Log.i(TAG, "defer corridor skeleton until place index ready path=$rid")
            return
        }
        val stem = r.stem.trim().ifBlank { PackRegionAvailability.localStem(geofabrikPath) }
        if (stem.isEmpty()) return
        ensureStarted(packDir, stem)
    }

    fun ensureFromInstalledMaps() {
        IdlePackJobs.onInstalledMapsChanged()
        val snap = InstalledMaps.current() ?: return
        if (RoutePlanGate.isRunning() || NaviMapTestHooks.pendingTripPlan != null) return
        for (r in snap.regions.values) {
            val indexOk =
                r.placeIndex == InstalledMaps.PlaceIndexState.INTACT ||
                    r.placeIndex == InstalledMaps.PlaceIndexState.LEGACY_INTACT
            if (!indexOk) continue
            ensureStarted(r.packDir, r.stem, TravelProfile.CAR)
            // Truck/MobileHome Stage B: truck ensure aliases to car when needed.
            ensureStarted(r.packDir, r.stem, TravelProfile.TRUCK)
        }
        IdlePackJobs.onAppIdle()
    }

    fun onSkeletonFinished(
        regionId: String,
        packDir: File,
        stem: String,
    ) {
        val rid =
            PackRegionAvailability.normalize(regionId).ifBlank {
                PackRegionAvailability.normalize(
                    stem.removeSuffix("-latest").replace('_', '-'),
                )
            }
        if (carSkeletonReady(packDir, stem)) {
            LongTripCoordinator.onRoutesReady(rid)
        }
    }

    private fun carSkeletonReady(
        packDir: File,
        stem: String,
    ): Boolean = RegionReadiness.carSkeletonReady(packDir, stem)

    fun clearQueueForPlan() {
        IdlePackJobs.pauseForPlan()
        lastStatus.set("Corridor skeleton paused (planning)…")
    }
}
