package no.navi.app

import android.content.Context
import android.util.Log
import java.io.File

/**
 * Directory that owns the MWM tree (`mwm/`, `active.json`, leaves, World overlays).
 *
 * Follows the long-trip **pack volume** preference when a writable removable
 * app-files dir is selected — same volume as [LongTripPackStorage] / place index —
 * so corridor downloads land on SD when the user prefers SD. Falls back to
 * [NaviAppData.resolve] (internal) when the pack volume is internal or the
 * removable volume is not writable.
 *
 * Settings, DATEX stamps, vehicle limits, and non-MWM caches stay on
 * [NaviAppData]; only MWM install / CoMaps session / drape roots use this path.
 */
object MwmDataDir {
    private const val TAG = "MwmDataDir"

    private val logged =
        java.util.concurrent.atomic
            .AtomicBoolean(false)

    /** App-files root that should contain `mwm/` for install and routing. */
    fun resolve(context: Context): File {
        val internal = NaviAppData.resolve(context)
        val volumeId = LongTripPackStorage.selectedVolumeId(context)
        if (volumeId == NaviStorageVolumes.INTERNAL_ID) {
            logOnce(internal, volumeId)
            return internal
        }
        val vol = NaviStorageVolumes.findById(context, volumeId)
        val appFiles = vol?.appFilesDir
        if (vol != null && vol.mounted && appFiles != null && NaviStorageVolumes.probeWritable(appFiles)) {
            logOnce(appFiles, volumeId)
            return appFiles
        }
        Log.w(
            TAG,
            "pack volume $volumeId unavailable for MWM " +
                "(mounted=${vol?.mounted} appFiles=$appFiles); using internal",
        )
        logOnce(internal, NaviStorageVolumes.INTERNAL_ID)
        return internal
    }

    /** `{resolve}/mwm` — CoMaps storage / writable root. */
    fun mwmRoot(context: Context): File = File(resolve(context), "mwm").also { it.mkdirs() }

    private fun logOnce(
        dir: File,
        volumeId: String,
    ) {
        if (logged.compareAndSet(false, true)) {
            Log.i(TAG, "mwm data dir=${dir.absolutePath} volume=$volumeId")
        }
    }
}
