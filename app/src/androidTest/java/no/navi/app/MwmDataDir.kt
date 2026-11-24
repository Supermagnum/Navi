package no.navi.app

import android.content.Context
import android.util.Log
import java.io.File

/**
 * OD androidTest helper for pilgrim campaign.
 * Matches product [MwmMapDownloader], which always installs under [NaviAppData]
 * (internal). Pack-volume / SD is for long-trip Geofabrik packs, not CoMaps MWM.
 * Not production code — campaign compile shim for tests branch.
 */
object MwmDataDir {
    private const val TAG = "MwmDataDirOD"

    fun resolve(context: Context): File {
        val internal = NaviAppData.resolve(context)
        Log.i(TAG, "mwm data dir=${internal.absolutePath} (product internal)")
        return internal
    }

    fun mwmRoot(context: Context): File = File(resolve(context), "mwm").also { it.mkdirs() }
}
