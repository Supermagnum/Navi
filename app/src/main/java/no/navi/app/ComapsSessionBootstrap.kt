package no.navi.app

import android.content.Context
import android.system.Os
import android.system.OsConstants
import android.util.Log
import org.json.JSONObject
import uniffi.navi.comapsNativeAvailable
import uniffi.navi.comapsSessionOpen
import java.io.File
import java.util.concurrent.Executors

/**
 * Opens the persistent CoMaps session (`comapsSessionOpen`) on the installed MWM
 * root so search, snap and climb activate outside instrumented tests.
 *
 * Production layout (see `core/src/mwm/storage.rs`), under `{filesDir}/mwm`:
 * ```
 * active.json, world-release.json
 * World.mwm, WorldCoasts.mwm            (promoted by activate_release)
 * releases/{ver}/leaves/{Leaf}.mwm
 * releases/{ver}/sidecars/...           (optional)
 * ```
 * CoMaps scans the MWM files inside `{writable}/{version}` (version = up to 6 digits) and reads
 * World plus the classificator / categories data from the resource dir. So:
 *
 * - storage_path = writable_path = `{filesDir}/mwm`
 * - the active release is exposed as `{filesDir}/mwm/{ver}` → prefer a symlink to
 *   `releases/{ver}/leaves`; on VFAT/exFAT (no symlinks) mirror leaf `.mwm` files
 *   into a real `{ver}/` directory (stale version dirs, symlink or real, are pruned)
 * - `assets/comaps-data` is staged next to World.mwm (same as the device tests)
 * - sidecar_dir = `releases/{ver}/sidecars` when it exists
 *
 * Everything runs on one background thread and never throws: missing maps, a
 * missing native library or a World/active mismatch only produce a log line.
 * [openAsync] is idempotent for an unchanged installation; call [onMwmsChanged]
 * after a release activation or leaf download so CoMaps re-scans the files (the
 * session re-open also resets the search data source).
 */
object ComapsSessionBootstrap {
    private const val TAG = "ComapsSession"
    private const val MWM_ROOT = "mwm"
    private const val ACTIVE_FILE = "active.json"
    private const val WORLD_RELEASE_FILE = "world-release.json"
    private const val WORLD_MWM_FILE = "World.mwm"
    private const val ASSET_DIR = "comaps-data"

    /**
     * Headless router resources taken from the CoMaps SDK's APK root assets when the drape build ships
     * them (so `assets/comaps-data` does not have to duplicate them).
     */
    internal val ROOT_ASSETS =
        listOf(
            "classificator.txt",
            "types.txt",
            "visibility.txt",
            "countries.txt",
            "countries_meta.txt",
            "countries-strings",
            "packed_polygons.bin",
            "categories.txt",
            "categories_brands.txt",
            "subtypes.csv",
            "colors.txt",
            "patterns.txt",
            "drules_proto_default_light.bin",
            "drules_proto_default_dark.bin",
            // Search / transliteration (Everywhere free-text). Not needed for routing alone.
            "icudt75l.dat",
        )
    private const val ASSET_STAMP = "comaps-data.stamp"
    private val VERSION_DIR = Regex("\\d{1,6}")

    private const val THREAD_NAME = "comaps-session"

    /**
     * The CoMaps router owns objects with a thread checker (the map downloader inside its
     * `Storage`): they must be destroyed on the thread that created them. Every open / re-open
     * therefore runs on this one thread, whoever asks (planner, downloader, UI).
     */
    private val executor =
        Executors.newSingleThreadExecutor { r ->
            Thread(r, THREAD_NAME).apply { isDaemon = true }
        }

    private val lock = Any()

    @Volatile
    private var openKey: String? = null

    /** True while the session opened by this bootstrap is believed to be live. */
    fun isOpen(): Boolean = openKey != null

    /**
     * True when the active release's country leaves are visible to CoMaps under
     * `{mwmRoot}/{version}/` (symlink or VFAT mirror). World-only roots return false.
     */
    fun countryMapsExposed(context: Context): Boolean {
        val root = MwmDataDir.mwmRoot(context)
        val version = readVersion(File(root, ACTIVE_FILE)) ?: return false
        return exposedLeafCount(root, version) > 0
    }

    /**
     * Block until [ensureOpen] succeeds and [countryMapsExposed] is true, or
     * [timeoutMs] elapses. Used to gate auto-plan so CoMaps is not asked to
     * route with an empty `numMwmIds` (native abort).
     */
    fun awaitCountryMapsReady(
        context: Context,
        timeoutMs: Long = 120_000L,
        pollMs: Long = 500L,
    ): Boolean {
        val deadline = System.currentTimeMillis() + timeoutMs.coerceAtLeast(0L)
        val app = context.applicationContext
        while (true) {
            val open = ensureOpen(app, force = !countryMapsExposed(app))
            if (open && countryMapsExposed(app)) return true
            if (System.currentTimeMillis() >= deadline) return false
            try {
                Thread.sleep(pollMs.coerceAtLeast(50L))
            } catch (_: InterruptedException) {
                Thread.currentThread().interrupt()
                return false
            }
        }
    }

    /** Open (or keep) the session off the main thread. No-op when already open and unchanged. */
    fun openAsync(context: Context) = submit(context.applicationContext, force = false)

    /** Call after MWM downloads / release activation: re-link, re-scan and re-open. */
    fun onMwmsChanged(context: Context) = submit(context.applicationContext, force = true)

    private fun submit(
        appContext: Context,
        force: Boolean,
    ) {
        runCatching { executor.execute { runCatching { openInternal(appContext, force) } } }
            .onFailure { Log.w(TAG, "could not schedule session open", it) }
    }

    /** Blocking variant. Returns true when a session is open afterwards. */
    fun ensureOpen(
        context: Context,
        force: Boolean = false,
    ): Boolean =
        try {
            val app = context.applicationContext
            if (Thread.currentThread().name == THREAD_NAME) {
                openInternal(app, force)
            } else {
                executor.submit(java.util.concurrent.Callable { openInternal(app, force) }).get()
            }
        } catch (t: Throwable) {
            Log.w(TAG, "session open failed", t)
            false
        }

    private fun openInternal(
        ctx: Context,
        force: Boolean,
    ): Boolean {
        if (!MwmClientBootstrap.isEnabled()) return false
        if (!MwmClientBootstrap.nativeLoaded) {
            Log.i(TAG, "skip: liborganicmaps not loaded")
            return false
        }
        if (!comapsNativeAvailable()) {
            Log.i(TAG, "skip: CoMaps native core not linked")
            return false
        }
        val root = MwmDataDir.mwmRoot(ctx)
        val version = readVersion(File(root, ACTIVE_FILE))
        if (version == null) {
            Log.i(TAG, "skip: no active MWM release under ${root.absolutePath}")
            return false
        }
        if (!VERSION_DIR.matches(version)) {
            Log.w(TAG, "skip: release '$version' is not a CoMaps version dir (1-6 digits)")
            return false
        }
        val worldVersion = readVersion(File(root, WORLD_RELEASE_FILE))
        if (worldVersion != version || !File(root, WORLD_MWM_FILE).isFile) {
            Log.w(TAG, "skip: World overlays do not match active=$version world=$worldVersion")
            return false
        }
        val leaves =
            File(root, "releases/$version/leaves")
                .listFiles { f -> f.isFile && f.name.endsWith(".mwm") }
                .orEmpty()
        if (leaves.isEmpty()) {
            Log.i(TAG, "skip: no leaf MWMs in ${root.absolutePath}/releases/$version/leaves")
            return false
        }
        synchronized(lock) {
            val key = "${root.absolutePath}|$version|${leaves.size}|${leaves.maxOf { it.lastModified() }}"
            if (!force && key == openKey && exposedLeafCount(root, version) > 0) return true
            val drapeScanVersion = readAssetCountriesVersion(ctx)
            if (!linkActiveRelease(root, version, leaves, drapeScanVersion)) {
                openKey = null
                Log.w(
                    TAG,
                    "skip: could not expose release $version leaves under ${root.absolutePath}/$version " +
                        "(symlink and mirror both failed)",
                )
                return false
            }
            val exposed = exposedLeafCount(root, version)
            if (exposed <= 0) {
                openKey = null
                Log.w(
                    TAG,
                    "skip: ${root.absolutePath}/$version has no country .mwm files after link/mirror " +
                        "(source leaves=${leaves.size})",
                )
                return false
            }
            if (!stageResources(ctx, root)) return false
            // CoMaps refuses to scan a version directory newer than countries.txt
            // "v". The APK asset is often an older release than the leaves just
            // downloaded, which leaves the map as World-only (no cities, streets,
            // rivers) and the storage UI as "not downloaded".
            alignCountriesListVersion(root, version)
            // Search address/type strings need Platform Context (Utils JNI). The
            // drape Framework also sets this, but search can run before the map
            // surface is ready — set it here so cold Everywhere queries get
            // addresses instead of empty fields / readiness races.
            // Reflection: :comaps-sdk (OrganicMaps) is not on every variant
            // classpath (CI debug / -Pnavi.comapsDrape=false).
            if (ComapsDrape.sdkPackaged()) {
                runCatching {
                    val clazz = Class.forName("app.organicmaps.sdk.OrganicMaps")
                    val method = clazz.getMethod("nativeSetContext", Context::class.java)
                    method.invoke(null, ctx.applicationContext)
                }.onFailure { Log.w(TAG, "nativeSetContext before session open failed", it) }
            }
            Log.i(
                TAG,
                "mwm root=" +
                    root.listFiles().orEmpty().sortedBy { it.name }.joinToString(",") {
                        "${it.name}${if (isSymlink(it)) {
                            "@"
                        } else if (it.isDirectory) {
                            "/"
                        } else {
                            ":${it.length()}"
                        }}"
                    },
            )
            val sidecars = File(root, "releases/$version/sidecars").takeIf { it.isDirectory }
            val res =
                comapsSessionOpen(
                    storagePath = root.absolutePath,
                    writablePath = root.absolutePath,
                    sidecarDir = sidecars?.absolutePath,
                    // Wetland Soft/Hard costs and EL02 elevation overrides need the verified sidecars
                    // of the installed leaves; without them the router runs as plain CoMaps.
                    loadWetlandSidecars = sidecars?.list()?.any { it.endsWith(".navi-sidecar.v1.zst") } == true,
                )
            if (res.ok) {
                openKey = key
                Log.i(
                    TAG,
                    "session open ok root=${root.absolutePath} release=$version leaves=${leaves.size} " +
                        "exposed=$exposed sidecars=${sidecars?.absolutePath ?: "none"} " +
                        "init_ms=${res.initMs} register_ms=${res.registerMs}",
                )
                // Drape Framework scans {writable}/{version}/ independently of the
                // headless session — re-register so the map surface picks up leaves.
                android.os.Handler(android.os.Looper.getMainLooper()).post {
                    // Reload, not incremental add: an older version dir (for example
                    // 261105/Norway_Western next to active 261115) pins VersionAlreadyExists
                    // and the drape never draws the active leaves (blank cities/streets/rivers).
                    runCatching { ComapsDrape.backend?.registerMaps() }
                        .onFailure { Log.w(TAG, "registerMaps after session open failed", it) }
                }
            } else {
                openKey = null
                Log.w(TAG, "session open failed root=${root.absolutePath} release=$version: ${res.errorMessage}")
            }
            return res.ok
        }
    }

    private fun readVersion(file: File): String? =
        runCatching {
            if (!file.isFile) null else JSONObject(file.readText()).optString("version").trim().ifEmpty { null }
        }.getOrNull()

    private fun isSymlink(f: File): Boolean = runCatching { OsConstants.S_ISLNK(Os.lstat(f.path).st_mode) }.getOrDefault(false)

    private fun listLeafMwms(dir: File): List<File> =
        dir
            .listFiles { f -> f.isFile && f.name.endsWith(".mwm", ignoreCase = true) }
            .orEmpty()
            .toList()

    private fun exposedLeafCount(
        root: File,
        version: String,
    ): Int = listLeafMwms(File(root, version)).size

    /**
     * Drop every `{root}/{digits}` entry that is not [activeVersion].
     *
     * CoMaps registers every version directory it finds. A leftover older
     * release (real dir on VFAT, not only a symlink) wins `VersionAlreadyExists`
     * and later leaves of the active release never reach the renderer or the
     * storage "downloaded" list.
     */
    internal fun readCountriesListVersion(root: File): String? {
        val file = File(root, "countries.txt")
        if (!file.isFile) return null
        val text = runCatching { file.readText() }.getOrNull() ?: return null
        return Regex(""""v"\s*:\s*(\d+)""").find(text)?.groupValues?.get(1)
    }

    internal fun pruneStaleReleaseDirs(
        root: File,
        activeVersion: String,
        keepVersion: String? = null,
    ) {
        root.listFiles()?.forEach { f ->
            if (f.name == activeVersion || f.name == keepVersion || !VERSION_DIR.matches(f.name)) {
                return@forEach
            }
            val gone =
                runCatching {
                    if (isSymlink(f)) f.delete() else f.deleteRecursively()
                }.getOrDefault(false)
            Log.i(TAG, "pruned stale MWM release ${f.absolutePath} gone=$gone")
        }
    }

    /** `v` inside the APK `countries.txt`. The drape scans only directories at or below this. */
    internal fun readAssetCountriesVersion(ctx: Context): String? {
        val text =
            runCatching {
                ctx.assets.open("countries.txt").use { input ->
                    input.bufferedReader().readText()
                }
            }.getOrNull() ?: return null
        return Regex(""""v"\s*:\s*(\d+)""").find(text)?.groupValues?.get(1)
    }

    /**
     * Expose `releases/{ver}/leaves` at `{root}/{ver}` for CoMaps.
     *
     * Prefer a relative symlink (ext4 / internal). On VFAT/exFAT (EACCES /
     * EPERM) create a real directory and mirror each leaf `.mwm` (hardlink when
     * possible, else copy). Also mirror into the APK countries.txt version
     * directory when that is older than [version], because the drape will not
     * scan a newer directory. Returns false when no country maps end up exposed.
     */
    private fun linkActiveRelease(
        root: File,
        version: String,
        leaves: Array<out File>,
        drapeScanVersion: String?,
    ): Boolean {
        pruneStaleReleaseDirs(root, version, keepVersion = drapeScanVersion)
        // Drape reads countries.txt from the APK (resource dir), whose "v" is
        // often older than active.json. It skips any directory newer than that
        // "v", so the active release dir is invisible and the map stays World-only.
        // Writable countries.txt may already have been raised to [version]. The
        // drape still uses the APK copy, so the scan directory name is that copy.
        if (drapeScanVersion != null && drapeScanVersion != version) {
            val scanDir = File(root, drapeScanVersion)
            if (isSymlink(scanDir)) runCatching { scanDir.delete() }
            if (!scanDir.isDirectory) scanDir.mkdirs()
            mirrorLeavesInto(scanDir, leaves)
            Log.i(TAG, "mirrored ${leaves.size} leaves into $drapeScanVersion for drape scan")
        }
        val link = File(root, version)
        val target = "releases/$version/leaves"
        if (isSymlink(link)) {
            if (runCatching { Os.readlink(link.path) }.getOrNull() == target) {
                return exposedLeafCount(root, version) > 0 || leaves.isEmpty()
            }
            runCatching { link.delete() }
        } else if (link.exists() && link.isDirectory) {
            // Real dir (prior VFAT mirror): refresh contents from source leaves.
            return mirrorLeavesInto(link, leaves)
        } else if (link.exists()) {
            Log.w(TAG, "${link.absolutePath} is a real non-directory path; cannot expose leaves")
            return false
        }
        val sym =
            runCatching {
                Os.symlink(target, link.path)
                true
            }.getOrElse {
                Log.w(TAG, "symlink ${link.path} -> $target failed: ${it.message}")
                false
            }
        if (sym) {
            val n = exposedLeafCount(root, version)
            if (n > 0 || leaves.isEmpty()) return true
            Log.w(TAG, "symlink ${link.path} exists but exposes 0 leaves; falling back to mirror")
            runCatching { link.delete() }
        }
        if (!link.mkdirs() && !link.isDirectory) {
            Log.w(TAG, "mkdir ${link.absolutePath} failed")
            return false
        }
        Log.i(
            TAG,
            "mirroring ${leaves.size} leaves into ${link.absolutePath} (no symlink support on this volume)",
        )
        return mirrorLeavesInto(link, leaves)
    }

    /**
     * Sync [leaves] into [destDir] for CoMaps. Skips same-size existing files.
     * Tries hardlink first (cheap on ext4); copies on VFAT. Removes stale
     * `.mwm` names that are no longer in the source release.
     */
    internal fun mirrorLeavesInto(
        destDir: File,
        leaves: Array<out File>,
    ): Boolean {
        if (!destDir.isDirectory && !destDir.mkdirs()) return false
        val want = leaves.map { it.name }.toSet()
        for (stale in listLeafMwms(destDir)) {
            if (stale.name !in want) {
                runCatching { stale.delete() }
            }
        }
        var ok = 0
        for (src in leaves) {
            val dest = File(destDir, src.name)
            if (dest.isFile && dest.length() == src.length() && dest.length() > 0L) {
                ok++
                continue
            }
            runCatching { if (dest.exists()) dest.delete() }
            val linked =
                runCatching {
                    Os.link(src.absolutePath, dest.absolutePath)
                    true
                }.getOrDefault(false)
            if (linked && dest.isFile && dest.length() == src.length()) {
                ok++
                continue
            }
            runCatching { if (dest.exists()) dest.delete() }
            val copied =
                runCatching {
                    src.inputStream().use { input ->
                        dest.outputStream().use { output -> input.copyTo(output) }
                    }
                    dest.isFile && dest.length() == src.length()
                }.getOrElse {
                    Log.w(TAG, "copy ${src.name} -> ${dest.absolutePath} failed: ${it.message}")
                    false
                }
            if (copied) ok++ else Log.w(TAG, "failed to mirror leaf ${src.name}")
        }
        return ok > 0 || leaves.isEmpty()
    }

    /**
     * Raise `countries.txt` `"v"` up to [activeVersion] when the staged list is older.
     *
     * Does not rewrite the country tree. CoMaps uses this integer as the newest
     * directory it will scan (`version > latestVersion` is skipped).
     */
    internal fun alignCountriesListVersion(
        root: File,
        activeVersion: String,
    ) {
        val active = activeVersion.toLongOrNull() ?: return
        val file = File(root, "countries.txt")
        if (!file.isFile) return
        val text = runCatching { file.readText() }.getOrNull() ?: return
        val match = Regex(""""v"\s*:\s*(\d+)""").find(text) ?: return
        val current = match.groupValues[1].toLongOrNull() ?: return
        if (active <= current) return
        val updated = text.replaceRange(match.range, "\"v\": $active")
        runCatching { file.writeText(updated) }
            .onSuccess { Log.i(TAG, "countries.txt v $current -> $active (active release dir)") }
            .onFailure { Log.w(TAG, "countries.txt version align failed", it) }
    }

    /** Copy `assets/comaps-data` next to World.mwm once per app install/update. */
    private fun stageResources(
        ctx: Context,
        root: File,
    ): Boolean {
        val stampFile = File(root, ASSET_STAMP)
        val stamp =
            runCatching {
                ctx.packageManager
                    .getPackageInfo(ctx.packageName, 0)
                    .lastUpdateTime
                    .toString()
            }.getOrDefault("0")
        val legacy =
            ctx.assets
                .list(ASSET_DIR)
                .orEmpty()
                .filter { it != "MANIFEST.txt" }
        // Drape build: the SDK's root assets are the single copy; SDK-less builds stage comaps-data.
        val rootNames =
            ctx.assets
                .list("")
                .orEmpty()
                .toSet()
        val fromRoot = legacy.isEmpty()
        val names = if (fromRoot) ROOT_ASSETS.filter { it in rootNames } else legacy
        if (names.isEmpty() || (fromRoot && names.size < ROOT_ASSETS.size)) {
            Log.w(TAG, "skip: CoMaps resources missing in the APK (comaps-data=${legacy.size}, root=${names.size})")
            return false
        }
        val assetBase = if (fromRoot) "" else "$ASSET_DIR/"
        val current = runCatching { stampFile.readText() }.getOrNull() == stamp
        val missing =
            names.any { n ->
                File(root, n).let { if (it.isDirectory) it.list().isNullOrEmpty() else !it.isFile || it.length() == 0L }
            }
        if (current && !missing) return true
        return runCatching {
            for (name in names) copyAsset(ctx, "$assetBase$name", File(root, name), skipExisting = current)
            stampFile.writeText(stamp)
            true
        }.getOrElse {
            Log.w(TAG, "staging $ASSET_DIR into ${root.absolutePath} failed", it)
            false
        }
    }

    /** Copies an asset file or (recursively) directory; directories such as `countries-strings`. */
    private fun copyAsset(
        ctx: Context,
        assetPath: String,
        dest: File,
        skipExisting: Boolean,
    ) {
        val children = ctx.assets.list(assetPath).orEmpty()
        if (children.isNotEmpty()) {
            dest.mkdirs()
            for (child in children) copyAsset(ctx, "$assetPath/$child", File(dest, child), skipExisting)
            return
        }
        if (skipExisting && dest.isFile && dest.length() > 0L) return
        dest.parentFile?.mkdirs()
        val tmp = File(dest.parentFile, "${dest.name}.tmp")
        // An empty directory in the checkout (e.g. countries-strings/ta.json/) is not packaged by AAPT:
        // it lists as a name but has nothing to open.
        val input =
            try {
                ctx.assets.open(assetPath)
            } catch (_: java.io.FileNotFoundException) {
                Log.i(TAG, "asset $assetPath not packaged (empty directory in the checkout); skipped")
                return
            }
        input.use { tmp.outputStream().use { out -> it.copyTo(out) } }
        if (!tmp.renameTo(dest)) error("rename ${tmp.name} failed")
    }
}
