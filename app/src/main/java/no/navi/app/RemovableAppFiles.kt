package no.navi.app

import android.content.Context
import java.io.File

/**
 * Shared "SD if present, else internal" app-files root for large offline data
 * (maps, user-imported repeater KML). Searches every mounted volume.
 */
object RemovableAppFiles {
    /**
     * Prefer a mounted removable volume with an app-files directory; otherwise
     * [NaviAppData] (internal). Does not create directories.
     */
    fun resolve(context: Context): File {
        for (vol in NaviStorageVolumes.list(context)) {
            if (vol.id == NaviStorageVolumes.INTERNAL_ID) continue
            if (!vol.removable || !vol.mounted) continue
            val app = vol.appFilesDir ?: continue
            if (NaviStorageVolumes.probeWritable(app) || app.isDirectory) {
                return app
            }
        }
        return NaviAppData.resolve(context)
    }
}
