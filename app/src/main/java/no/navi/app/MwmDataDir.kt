package no.navi.app

import android.content.Context
import android.util.Log
import java.io.File

/**
 * Directory that owns the MWM tree (`mwm/`, `active.json`, leaves, World overlays).
 *
 * **Policy (maps):**
 * 1. Search **both** removable SD app-files dirs and internal for `mwm/World.mwm`.
 * 2. If an SD card is present (mounted removable with an app-files dir), prefer it
 *    for map data — even when the long-trip pack-volume picker says internal.
 * 3. If no SD is usable, use [NaviAppData] (internal).
 *
 * Existing World on a volume wins over an empty preferred path so a cold-start
 * write-probe flake cannot hide maps that already sit on SD.
 *
 * Settings, DATEX stamps, vehicle limits, and non-MWM caches stay on
 * [NaviAppData]; only MWM install / CoMaps session / drape roots use this path.
 */
object MwmDataDir {
    private const val TAG = "MwmDataDir"
    private const val WORLD_MWM = "World.mwm"

    private val logged =
        java.util.concurrent.atomic
            .AtomicBoolean(false)

    /** App-files root that should contain `mwm/` for install and routing. */
    fun resolve(context: Context): File {
        val internal = NaviAppData.resolve(context)
        val volumes = NaviStorageVolumes.list(context)
        val removable =
            volumes.filter {
                it.id != NaviStorageVolumes.INTERNAL_ID &&
                    it.removable &&
                    it.mounted &&
                    it.appFilesDir != null
            }

        // 1) Prefer any removable volume that already has World.mwm.
        for (vol in removable) {
            val app = vol.appFilesDir ?: continue
            if (hasWorldMwm(app)) {
                logOnce(app, vol.id)
                return app
            }
        }

        // 2) Prefer internal if it already has World (maps were installed there).
        if (hasWorldMwm(internal)) {
            logOnce(internal, NaviStorageVolumes.INTERNAL_ID)
            return internal
        }

        // 3) No World yet: SD present and usable → install maps on SD.
        for (vol in removable) {
            val app = vol.appFilesDir ?: continue
            if (NaviStorageVolumes.probeWritable(app) || app.isDirectory) {
                Log.i(TAG, "SD present; using ${app.absolutePath} for MWM (volume=${vol.id})")
                logOnce(app, vol.id)
                return app
            }
        }

        // 4) Also honour an explicit pack-volume id that is not internal (USB, etc.).
        val packId = LongTripPackStorage.selectedVolumeId(context)
        if (packId != NaviStorageVolumes.INTERNAL_ID) {
            val vol = NaviStorageVolumes.findById(context, packId)
            val app = vol?.appFilesDir
            if (vol != null &&
                vol.mounted &&
                app != null &&
                (hasWorldMwm(app) || NaviStorageVolumes.probeWritable(app))
            ) {
                logOnce(app, packId)
                return app
            }
        }

        Log.i(TAG, "no usable SD for MWM; using internal ${internal.absolutePath}")
        logOnce(internal, NaviStorageVolumes.INTERNAL_ID)
        return internal
    }

    /** `{resolve}/mwm` — CoMaps storage / writable root. */
    fun mwmRoot(context: Context): File = File(resolve(context), "mwm").also { it.mkdirs() }

    /**
     * Every candidate app-files root that may hold maps (removable first, then
     * internal). Used when diagnosing "maps on device but engine cannot start".
     */
    fun candidateAppFiles(context: Context): List<Pair<String, File>> {
        val out = ArrayList<Pair<String, File>>()
        for (vol in NaviStorageVolumes.list(context)) {
            val app = vol.appFilesDir ?: continue
            if (!vol.mounted) continue
            out.add(vol.id to app)
        }
        val internal = NaviAppData.resolve(context)
        if (out.none { it.second.absolutePath == internal.absolutePath }) {
            out.add(NaviStorageVolumes.INTERNAL_ID to internal)
        }
        return out
    }

    /** True when [appFiles]/mwm/World.mwm` exists as a regular file. */
    fun hasWorldMwm(appFiles: File): Boolean = File(appFiles, "mwm/$WORLD_MWM").isFile

    private fun logOnce(
        dir: File,
        volumeId: String,
    ) {
        if (logged.compareAndSet(false, true)) {
            Log.i(TAG, "mwm data dir=${dir.absolutePath} volume=$volumeId")
        }
    }
}
