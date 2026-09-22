package no.navi.app

import android.content.Context
import android.util.Log
import java.io.File
import java.util.concurrent.atomic.AtomicReference

/**
 * Long-trip pack download location: optional redirect onto a removable volume's
 * app-specific directory ([Context.getExternalFilesDirs]), while place index,
 * Tools downloads, DEM, and ordinary region installs stay on
 * [NaviAppData.resolve] (internal).
 *
 * Reuse policy: if packs for a region are already installed under internal
 * [NaviAppData], a long-trip plan **reuses them in place** and does not copy or
 * re-download into the redirected volume.
 */
object LongTripPackStorage {
    private const val TAG = "LongTripPackStore"
    const val PACKS_SUBDIR = "long-trip-packs"

    private val activePackWriteDir = AtomicReference<File?>(null)
    private val volumeWatch = AtomicReference<java.io.Closeable?>(null)
    private val lastEvent = AtomicReference<NaviStorageVolumes.Event?>(null)

    sealed class PackTarget {
        /** Region packs already present under internal app data — do not duplicate. */
        data class ReuseInternal(
            val dataDir: File,
        ) : PackTarget()

        /** Download new long-trip packs into [packDir] (internal or removable). */
        data class DownloadTo(
            val packDir: File,
            val volumeId: String,
            val onRemovable: Boolean,
        ) : PackTarget()
    }

    /** Selected volume id from prefs (`internal` or a [NaviStorageVolumes] id). */
    fun selectedVolumeId(context: Context): String =
        MapHudPrefs.loadLongTripPackVolumeId(context).ifBlank { NaviStorageVolumes.INTERNAL_ID }

    fun saveSelectedVolumeId(
        context: Context,
        volumeId: String,
    ) {
        MapHudPrefs.saveLongTripPackVolumeId(context, volumeId)
    }

    /**
     * Directory that receives **new** long-trip pack files only.
     * Always under the chosen volume's app-files tree + [PACKS_SUBDIR].
     * Falls back to internal when the selected removable volume is missing/unmounted.
     */
    fun packDownloadDir(context: Context): File {
        val id = selectedVolumeId(context)
        if (id == NaviStorageVolumes.INTERNAL_ID) {
            return File(NaviAppData.resolve(context), PACKS_SUBDIR).also { it.mkdirs() }
        }
        val vol = NaviStorageVolumes.findById(context, id)
        if (vol == null || !vol.mounted || vol.appFilesDir == null) {
            Log.w(TAG, "volume $id unavailable; falling back to internal for packDownloadDir")
            return File(NaviAppData.resolve(context), PACKS_SUBDIR).also { it.mkdirs() }
        }
        return File(vol.appFilesDir, PACKS_SUBDIR).also { it.mkdirs() }
    }

    /**
     * Decide where packs for [geofabrikPath] should come from for a long-trip plan.
     * Internal install wins over redirect (no duplicate download).
     */
    fun resolvePackTarget(
        context: Context,
        geofabrikPath: String,
    ): PackTarget {
        val internal = NaviAppData.resolve(context)
        if (PackRegionAvailability.localBakeReady(internal, geofabrikPath)) {
            return PackTarget.ReuseInternal(internal)
        }
        val id = selectedVolumeId(context)
        val dir = packDownloadDir(context)
        // If packs already sit on the redirected volume from a prior long-trip run,
        // still DownloadTo that dir (caller treats localBakeReady there as installed).
        val onRemovable = id != NaviStorageVolumes.INTERNAL_ID
        return PackTarget.DownloadTo(dir, id, onRemovable)
    }

    /** True when packs exist on internal **or** the active long-trip pack dir. */
    fun packsReadySomewhere(
        context: Context,
        geofabrikPath: String,
    ): Boolean {
        val internal = NaviAppData.resolve(context)
        if (PackRegionAvailability.localBakeReady(internal, geofabrikPath)) return true
        return PackRegionAvailability.localBakeReady(packDownloadDir(context), geofabrikPath)
    }

    /**
     * Mark the directory currently receiving an HTTP pack write so an eject
     * handler can avoid promoting a truncated final.
     */
    fun beginPackWrite(packDir: File) {
        activePackWriteDir.set(packDir)
    }

    fun endPackWrite(packDir: File) {
        activePackWriteDir.compareAndSet(packDir, null)
    }

    /**
     * Primary eject/unmount handling: keep incomplete transfers as `*.partial`,
     * delete any zero/truncated finals that lost their `.partial` sibling mid-rename,
     * and clear the active-write marker. Returns region stems that were mid-write
     * (for Unavailable marking by the orchestrator / Phase C).
     */
    fun handleVolumeUnavailable(
        context: Context,
        volumeId: String,
    ): List<String> {
        lastEvent.set(NaviStorageVolumes.Event.Unmounted(volumeId))
        val selected = selectedVolumeId(context)
        if (selected != volumeId && volumeId != "unknown" && !volumeId.startsWith("path:")) {
            // Unrelated volume.
            if (selected == NaviStorageVolumes.INTERNAL_ID) return emptyList()
        }
        val writing = activePackWriteDir.getAndSet(null)
        val affected = ArrayList<String>()
        val dirs = linkedSetOf<File>()
        writing?.let { dirs.add(it) }
        // If the selected volume matches, scrub its pack dir too.
        if (selected == volumeId || writing != null) {
            runCatching { packDownloadDir(context) }.getOrNull()?.let { dirs.add(it) }
        }
        for (dir in dirs) {
            affected.addAll(scrubIncompletePacks(dir))
        }
        Log.i(TAG, "volume unavailable id=$volumeId scrubbed stems=$affected")
        return affected
    }

    /**
     * Ensure no truncated non-partial pack artifacts remain. Incomplete downloads
     * must live only as `*.partial` (resume) or be deleted.
     */
    fun scrubIncompletePacks(packDir: File): List<String> {
        if (!packDir.isDirectory) return emptyList()
        val stems = ArrayList<String>()
        val files = packDir.listFiles() ?: return emptyList()
        val partials =
            files
                .filter { it.isFile && it.name.endsWith(".partial") }
                .map { it.name.removeSuffix(".partial") }
                .toHashSet()
        for (f in files) {
            if (!f.isFile) continue
            val name = f.name
            when {
                name.endsWith(".partial") -> {
                    // Keep for resume; record stem.
                    val stem =
                        name
                            .removeSuffix(".partial")
                            .removeSuffix(".navi-manifest.json")
                            .removeSuffix(".osm.pbf")
                            .removeSuffix(".pbf")
                    if (stem.isNotBlank()) stems.add(stem)
                }
                name.endsWith(".navi-manifest.json") || name.endsWith(".osm.pbf") -> {
                    // Final name while a same-named .partial exists → abort left a
                    // truncated final; delete the final, keep partial.
                    if (partials.contains(name) || File(packDir, "$name.partial").isFile) {
                        runCatching { f.delete() }
                        stems.add(
                            name
                                .removeSuffix(".navi-manifest.json")
                                .removeSuffix(".osm.pbf"),
                        )
                    }
                }
            }
        }
        return stems.distinct()
    }

    fun lastVolumeEvent(): NaviStorageVolumes.Event? = lastEvent.get()

    /**
     * Start watching volumes while long-trip mode is enabled. Idempotent replace.
     * [onUnavailable] receives the volume id + mid-write stems after scrub.
     */
    fun ensureWatching(
        context: Context,
        onUnavailable: (volumeId: String, stems: List<String>) -> Unit,
        onMounted: (volumeId: String) -> Unit = {},
    ) {
        stopWatching()
        val watch =
            NaviStorageVolumes.registerWatch(context) { event ->
                lastEvent.set(event)
                when (event) {
                    is NaviStorageVolumes.Event.Ejecting -> {
                        // Best-effort scrub before the volume disappears.
                        val stems = handleVolumeUnavailable(context, event.volumeId)
                        onUnavailable(event.volumeId, stems)
                    }
                    is NaviStorageVolumes.Event.Unmounted -> {
                        val stems = handleVolumeUnavailable(context, event.volumeId)
                        onUnavailable(event.volumeId, stems)
                    }
                    is NaviStorageVolumes.Event.Mounted -> onMounted(event.volumeId)
                }
            }
        volumeWatch.set(watch)
    }

    fun stopWatching() {
        volumeWatch.getAndSet(null)?.close()
    }

    /**
     * IOException fallback when a write fails because the volume vanished before
     * (or without) an eject broadcast. Call from download I/O catch sites
     * (Phase C [RegionDownloadBackground] wiring).
     */
    fun handleWriteIoFailure(
        context: Context,
        packDir: File,
        error: Throwable,
    ): List<String> {
        Log.w(TAG, "pack write I/O failure on ${packDir.absolutePath}: $error")
        activePackWriteDir.compareAndSet(packDir, null)
        val volumeId = selectedVolumeId(context)
        return handleVolumeUnavailable(context, volumeId)
    }
}
