package no.navi.app

import android.content.Context
import android.util.Log
import uniffi.navi.pmtilesPlanetUrl
import uniffi.navi.pmtilesQueueRegion
import uniffi.navi.pmtilesRunJob
import java.io.File
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Obtains the z0–z6 world overview through the same PMTiles extract path as
 * regional archives. Runs once when the file is missing or invalid; never on
 * every start. Until it is present the map uses a regional archive or the
 * online map — no blank screen and no error.
 *
 * Source: [pmtilesPlanetUrl] resolves the current Protomaps public planet
 * (`https://build-metadata.protomaps.dev/builds.json` →
 * `https://build.protomaps.com/{latest}.pmtiles`). A dated build that later
 * disappears is not pinned; the next extract (only if the local file is gone
 * or invalid) asks metadata again.
 */
object WorldOverviewDownload {
    const val TAG = "WorldOverview"
    const val REGION_KEY = BasemapStyleResolver.WORLD_OVERVIEW_REGION_KEY

    private val inFlight = AtomicBoolean(false)

    fun ensure(
        context: Context,
        dataDir: File = NaviAppData.resolve(context),
    ) {
        val existing =
            runCatching {
                BasemapStyleResolver.findWorldOverview(
                    uniffi.navi.pmtilesListJobs(dataDir.absolutePath),
                    dataDir,
                )
            }.getOrNull()
        if (existing != null &&
            PmtilesArchiveGate.isUsable(File(existing.localPath), REGION_KEY)
        ) {
            Log.i(TAG, "already present path=${existing.localPath}")
            return
        }
        if (!BasemapStyleResolver.hasNetwork(context)) {
            Log.i(TAG, "skip: no network; regional or online map until it is fetched")
            return
        }
        if (!inFlight.compareAndSet(false, true)) {
            Log.i(TAG, "skip: extract already running")
            return
        }
        Thread(
            {
                runCatching {
                    android.os.Process.setThreadPriority(
                        android.os.Process.THREAD_PRIORITY_BACKGROUND,
                    )
                }
                try {
                    fetch(context, dataDir)
                } finally {
                    inFlight.set(false)
                }
            },
            "WorldOverviewDownload",
        ).start()
    }

    internal fun resetForTests() {
        inFlight.set(false)
    }

    private fun fetch(
        context: Context,
        dataDir: File,
    ) {
        val planet =
            runCatching { pmtilesPlanetUrl() }.getOrElse { t ->
                Log.w(TAG, "planet URL resolve failed: ${t.message}")
                BasemapStyleResolver.PROTOMAPS_PLANET_FALLBACK
            }
        Log.i(TAG, "starting extract from $planet")
        val job =
            runCatching { pmtilesQueueRegion(dataDir.absolutePath, REGION_KEY, planet) }
                .getOrElse { t ->
                    Log.e(TAG, "queue failed", t)
                    return
                }
        if (job.id.isBlank() || job.status.startsWith("failed")) {
            Log.e(TAG, "queue status=${job.status}")
            return
        }
        if (job.status == "completed" &&
            PmtilesArchiveGate.isUsable(File(job.localPath), REGION_KEY)
        ) {
            Log.i(TAG, "job already completed path=${job.localPath} bytes=${job.bytesReceived}")
            runCatching { InstalledMaps.refresh(context) }
            return
        }
        val done =
            runCatching { pmtilesRunJob(dataDir.absolutePath, job.id) }
                .getOrElse { t ->
                    Log.e(TAG, "run failed id=${job.id}", t)
                    return
                }
        Log.i(
            TAG,
            "done id=${done.id} status=${done.status} path=${done.localPath} " +
                "bytes=${done.bytesReceived}",
        )
        runCatching { InstalledMaps.refresh(context) }
    }
}
