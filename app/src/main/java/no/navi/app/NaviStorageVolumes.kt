package no.navi.app

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.os.Build
import android.os.Environment
import android.os.StatFs
import android.os.storage.StorageManager
import android.os.storage.StorageVolume
import android.util.Log
import java.io.Closeable
import java.io.File
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.Executor
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Read-only volume listing for long-trip pack storage (no SAF).
 *
 * [list] combines [StorageManager.getStorageVolumes] labels/removable flags with
 * [Context.getExternalFilesDirs] app-specific paths (scoped-storage safe, no extra
 * permission). Internal app files ([Context.getFilesDir] via [NaviAppData]) is
 * always offered as [INTERNAL_ID].
 */
object NaviStorageVolumes {
    const val TAG = "NaviStorageVol"
    const val INTERNAL_ID = "internal"

    data class Volume(
        val id: String,
        val label: String,
        /** True when the volume is a removable SD / USB mass-storage style mount. */
        val removable: Boolean,
        val mounted: Boolean,
        val freeBytes: Long,
        val totalBytes: Long,
        /**
         * App-specific files directory on this volume (`getExternalFilesDirs` entry),
         * or [NaviAppData.resolve] for [INTERNAL_ID]. Null when the volume is known
         * but currently unmounted (slot present as null in getExternalFilesDirs).
         */
        val appFilesDir: File?,
    )

    sealed class Event {
        data class Ejecting(
            val volumeId: String,
            val pathHint: String?,
        ) : Event()

        data class Unmounted(
            val volumeId: String,
        ) : Event()

        data class Mounted(
            val volumeId: String,
        ) : Event()
    }

    fun interface Listener {
        fun onVolumeEvent(event: Event)
    }

    fun list(context: Context): List<Volume> {
        val out = ArrayList<Volume>()
        val internalRoot = NaviAppData.resolve(context)
        out.add(
            Volume(
                id = INTERNAL_ID,
                label = "Internal storage",
                removable = false,
                mounted = true,
                freeBytes = usableSpace(internalRoot),
                totalBytes = totalSpace(internalRoot),
                appFilesDir = internalRoot,
            ),
        )

        val sm = context.getSystemService(StorageManager::class.java)
        val volumes = sm?.storageVolumes.orEmpty()
        val appDirs = context.getExternalFilesDirs(null) ?: emptyArray()

        for (vol in volumes) {
            if (vol.isPrimary && !vol.isRemovable) {
                // Emulated primary external — long-trip redirect targets removable
                // media; skip listing primary external as a separate choice to avoid
                // the tiny Automotive "SD" trap documented on NaviAppData.
                continue
            }
            val rootPath = volumeFilesystemPath(vol)
            val appDir = matchAppFilesDir(appDirs, rootPath)
            val mounted =
                when {
                    appDir != null -> true
                    rootPath != null -> {
                        val state =
                            if (Build.VERSION.SDK_INT >= 30 && vol.directory != null) {
                                Environment.getExternalStorageState(vol.directory)
                            } else {
                                vol.state
                            }
                        Environment.MEDIA_MOUNTED == state
                    }
                    else -> false
                }
            val id = volumeId(vol)
            val label =
                vol.getDescription(context)?.takeIf { it.isNotBlank() }
                    ?: if (vol.isRemovable) "Removable storage" else "External storage"
            val free =
                when {
                    appDir != null -> usableSpace(appDir)
                    mounted && rootPath != null -> usableSpace(File(rootPath))
                    else -> 0L
                }
            val total =
                when {
                    appDir != null -> totalSpace(appDir)
                    mounted && rootPath != null -> totalSpace(File(rootPath))
                    else -> 0L
                }
            out.add(
                Volume(
                    id = id,
                    label = label,
                    removable = vol.isRemovable,
                    mounted = mounted,
                    freeBytes = free,
                    totalBytes = total,
                    appFilesDir = appDir,
                ),
            )
        }

        // Secondary getExternalFilesDirs entries not matched to a StorageVolume
        // (defensive — should be rare).
        for ((idx, dir) in appDirs.withIndex()) {
            if (dir == null) continue
            if (out.any { it.appFilesDir?.absolutePath == dir.absolutePath }) continue
            if (idx == 0) continue // primary external skipped above
            val rem =
                runCatching { Environment.isExternalStorageRemovable(dir) }.getOrDefault(true)
            out.add(
                Volume(
                    id = "external_files_$idx",
                    label = "Removable storage",
                    removable = rem,
                    mounted = true,
                    freeBytes = usableSpace(dir),
                    totalBytes = totalSpace(dir),
                    appFilesDir = dir,
                ),
            )
        }
        return out
    }

    /** Removable + internal choices for the long-trip pack location picker. */
    fun listPickerOptions(context: Context): List<Volume> =
        list(context).filter { it.id == INTERNAL_ID || it.removable }

    fun findById(
        context: Context,
        id: String,
    ): Volume? = list(context).firstOrNull { it.id == id }

    /**
     * Watch volume mount/unmount.
     *
     * **minSdk 26:** [Intent.ACTION_MEDIA_EJECT] / [Intent.ACTION_MEDIA_UNMOUNTED] /
     * [Intent.ACTION_MEDIA_BAD_REMOVAL] / [Intent.ACTION_MEDIA_MOUNTED] (file scheme)
     * are the floor-compatible primary signal.
     *
     * **API 30+:** also registers [StorageManager.StorageVolumeCallback] for state
     * changes (mounted/unmounted). Callbacks do not replace broadcasts — both fire;
     * listeners should be idempotent.
     */
    fun registerWatch(
        context: Context,
        listener: Listener,
    ): Closeable {
        val appCtx = context.applicationContext
        val closed = AtomicBoolean(false)
        val listeners = CopyOnWriteArrayList<Listener>().apply { add(listener) }

        fun emit(event: Event) {
            if (closed.get()) return
            for (l in listeners) {
                runCatching { l.onVolumeEvent(event) }
                    .onFailure { Log.w(TAG, "listener failed: $it") }
            }
        }

        val receiver =
            object : BroadcastReceiver() {
                override fun onReceive(
                    ctx: Context?,
                    intent: Intent?,
                ) {
                    val action = intent?.action ?: return
                    val path = intent.data?.path
                    val volId = resolveVolumeIdForPath(appCtx, path)
                    when (action) {
                        Intent.ACTION_MEDIA_EJECT ->
                            emit(Event.Ejecting(volId, path))
                        Intent.ACTION_MEDIA_UNMOUNTED,
                        Intent.ACTION_MEDIA_BAD_REMOVAL,
                        ->
                            emit(Event.Unmounted(volId))
                        Intent.ACTION_MEDIA_MOUNTED ->
                            emit(Event.Mounted(volId))
                    }
                }
            }

        val filter =
            IntentFilter().apply {
                addAction(Intent.ACTION_MEDIA_EJECT)
                addAction(Intent.ACTION_MEDIA_UNMOUNTED)
                addAction(Intent.ACTION_MEDIA_BAD_REMOVAL)
                addAction(Intent.ACTION_MEDIA_MOUNTED)
                addDataScheme("file")
            }
        if (Build.VERSION.SDK_INT >= 33) {
            appCtx.registerReceiver(receiver, filter, Context.RECEIVER_NOT_EXPORTED)
        } else {
            @Suppress("UnspecifiedRegisterReceiverFlag")
            appCtx.registerReceiver(receiver, filter)
        }

        var volumeCallback: StorageManager.StorageVolumeCallback? = null
        if (Build.VERSION.SDK_INT >= 30) {
            val sm = appCtx.getSystemService(StorageManager::class.java)
            if (sm != null) {
                val cb =
                    object : StorageManager.StorageVolumeCallback() {
                        override fun onStateChanged(volume: StorageVolume) {
                            val id = volumeId(volume)
                            when (volume.state) {
                                "ejecting",
                                ->
                                    emit(Event.Ejecting(id, volumeFilesystemPath(volume)))
                                Environment.MEDIA_UNMOUNTED,
                                Environment.MEDIA_BAD_REMOVAL,
                                Environment.MEDIA_REMOVED,
                                ->
                                    emit(Event.Unmounted(id))
                                Environment.MEDIA_MOUNTED ->
                                    emit(Event.Mounted(id))
                            }
                        }
                    }
                val executor: Executor = appCtx.mainExecutor
                sm.registerStorageVolumeCallback(executor, cb)
                volumeCallback = cb
            }
        }

        return Closeable {
            if (!closed.compareAndSet(false, true)) return@Closeable
            runCatching { appCtx.unregisterReceiver(receiver) }
            if (Build.VERSION.SDK_INT >= 30) {
                volumeCallback?.let { cb ->
                    appCtx
                        .getSystemService(StorageManager::class.java)
                        ?.unregisterStorageVolumeCallback(cb)
                }
            }
            listeners.clear()
        }
    }

    internal fun volumeId(vol: StorageVolume): String {
        val uuid = vol.uuid
        if (!uuid.isNullOrBlank()) return "uuid:$uuid"
        return if (vol.isRemovable) "removable_primary" else "external_primary"
    }

    internal fun volumeFilesystemPath(vol: StorageVolume): String? {
        if (Build.VERSION.SDK_INT >= 30) {
            return vol.directory?.absolutePath
        }
        return runCatching {
            @Suppress("DEPRECATION")
            val m = StorageVolume::class.java.getMethod("getPath")
            m.invoke(vol) as? String
        }.getOrNull()
    }

    private fun matchAppFilesDir(
        dirs: Array<File?>,
        volumeRoot: String?,
    ): File? {
        if (volumeRoot.isNullOrBlank()) return null
        val root = volumeRoot.trimEnd('/')
        return dirs.filterNotNull().firstOrNull { dir ->
            val p = dir.absolutePath
            p == root || p.startsWith("$root/")
        }
    }

    private fun resolveVolumeIdForPath(
        context: Context,
        path: String?,
    ): String {
        if (path.isNullOrBlank()) return "unknown"
        for (v in list(context)) {
            val dir = v.appFilesDir?.absolutePath ?: continue
            if (path == dir || path.startsWith("$dir/") || dir.startsWith("$path/")) {
                return v.id
            }
            // Broadcast path is often the volume root, not the app-files dir.
            if (dir.startsWith(path.trimEnd('/'))) return v.id
        }
        return "path:$path"
    }

    private fun usableSpace(dir: File): Long =
        runCatching {
            if (!dir.exists()) dir.mkdirs()
            StatFs(dir.absolutePath).availableBytes
        }.getOrDefault(0L)

    private fun totalSpace(dir: File): Long =
        runCatching {
            if (!dir.exists()) dir.mkdirs()
            StatFs(dir.absolutePath).totalBytes
        }.getOrDefault(0L)
}
