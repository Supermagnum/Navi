package no.navi.app

import android.content.Context
import android.media.MediaScannerConnection
import android.util.Log
import uniffi.navi.catPluginConfigure
import uniffi.navi.catPluginInstallGuest
import uniffi.navi.catPluginSetEnabled
import java.io.File
import java.util.TimeZone
import java.util.concurrent.atomic.AtomicBoolean

/** CATS plugin session setup (configure + wasm guest from assets). */
object CatBootstrap {
    private val configured = AtomicBoolean(false)

    /** On-device / SD folder for user-copied OpenRepeater (and similar) KML exports. */
    const val REPEATER_DATA_DIR_NAME = "repeater-data"

    /** Legacy drop folder name (pre-`repeater-data`); still scanned if present. */
    const val LEGACY_REPEATER_DROP_DIR_NAME = "repeaters"

    fun ensureInitialized(context: Context) {
        if (!configured.compareAndSet(false, true)) return
        val filesDir = context.filesDir
        // Stable drop directory for AnyTone CPS CSV exports (see docs/cat-test.md).
        File(filesDir, "cat/import").mkdirs()
        ensureRepeaterDrop(context)
        releaseLegacyRepeaterCsvGrants(context)
        catPluginConfigure(
            filesDir.absolutePath,
            filesDir.absolutePath,
            TimeZone.getDefault().id,
        )
        installGuestFromAssets(context, "cat")
        val want = MapHudPrefs.loadCatPluginEnabled(context)
        catPluginSetEnabled(want)
        if (!MapHudPrefs.loadCatDropImported(context)) {
            val payload =
                org.json
                    .JSONObject()
                    .put("dir", repeaterDropDir(context).absolutePath)
                    .put("base", "https://navigate-me.duckdns.org")
                    .toString()
            Thread {
                runCatching { uniffi.navi.catPluginUpdateRepeatersFromServer(payload) }
                MapHudPrefs.saveCatDropImported(context, true)
            }.start()
        }
    }

    /**
     * Copy the guest staged under `assets/plugins/$name` into the session.
     * Reinstalls when [BuildConfig.VERSION_CODE] changes or the on-disk wasm
     * size no longer matches the asset (stale guests never set follow switches).
     */
    private fun installGuestFromAssets(
        context: Context,
        name: String,
    ) {
        val dir = File(context.filesDir, "plugins/$name")
        val marker = File(dir, ".installed")
        val wasmFile = File(dir, "plugin.wasm")
        val version = BuildConfig.VERSION_CODE.toString()
        val am = context.assets
        val assetWasm =
            am.open("plugins/$name/plugin.wasm").use { it.readBytes() }
        val markerOk = marker.isFile && marker.readText().trim() == version
        val sizeOk = wasmFile.isFile && wasmFile.length() == assetWasm.size.toLong()
        if (markerOk && sizeOk) return
        Log.i(
            "NaviCat",
            "reinstall guest name=$name version=$version " +
                "was_marker=${marker.isFile} was_size=${wasmFile.length()} " +
                "asset_bytes=${assetWasm.size}",
        )
        val manifest =
            am.open("plugins/$name/plugin.json").use { it.readBytes().decodeToString() }
        val status = catPluginInstallGuest(name, manifest, assetWasm)
        if (status.startsWith("OK")) {
            dir.mkdirs()
            marker.writeText(version)
        } else {
            Log.w("NaviCat", "guest install failed name=$name status=$status")
        }
    }

    /**
     * User writeable drop folder for OpenRepeater / RepeaterBook-style KML exports.
     * Prefer SD (`RemovableAppFiles`) when present; otherwise internal app files.
     * Path example on SD:
     * `Android/data/no.navi.app/files/repeater-data`.
     */
    fun repeaterDropDir(context: Context): File {
        val base = RemovableAppFiles.resolve(context)
        return File(base, REPEATER_DATA_DIR_NAME)
    }

    /**
     * Import directories to scan: primary [repeaterDropDir], plus legacy `repeaters`
     * on the same volume and on primary external / internal if they still hold files.
     */
    fun repeaterImportDirs(context: Context): List<File> {
        val primary = repeaterDropDir(context)
        val out = LinkedHashSet<File>()
        out.add(primary)
        val bases = LinkedHashSet<File>()
        bases.add(primary.parentFile ?: RemovableAppFiles.resolve(context))
        context.getExternalFilesDir(null)?.let { bases.add(it) }
        bases.add(context.filesDir)
        for (base in bases) {
            out.add(File(base, REPEATER_DATA_DIR_NAME))
            out.add(File(base, LEGACY_REPEATER_DROP_DIR_NAME))
        }
        return out.filter { it.isDirectory || it == primary }
    }

    const val REPEATER_DROP_README: String =
        "OpenRepeater / RepeaterBook KML import (no API — copy files yourself)\n" +
            "================================================================\n" +
            "\n" +
            "1. Export repeaters as KML (or KMZ) from OpenRepeater (or RepeaterBook).\n" +
            "2. Copy the files into this folder. Any file name is fine; do not overwrite\n" +
            "   existing files — keep one file per export (e.g. Norway.kml, Sweden.kml).\n" +
            "3. In Navi: enable the CAT plugin, open CAT tools, tap Update repeaters.\n" +
            "\n" +
            "Navi never contacts repeaterbook.com or OpenRepeater over the network for\n" +
            "these files. You are responsible for complying with each site's terms.\n" +
            "Android deletes this folder when the app is uninstalled.\n" +
            "USB/MTP path: Android/data/no.navi.app/files/repeater-data\n"

    private fun ensureRepeaterDrop(context: Context) {
        val dir = repeaterDropDir(context)
        if (!dir.exists() && !dir.mkdirs()) {
            Log.w("NaviCat", "could not create repeater-data at ${dir.absolutePath}")
            return
        }
        // Ensure the directory is user-writable for MTP drops.
        runCatching {
            dir.setWritable(true, false)
            dir.setReadable(true, false)
        }
        val readme = File(dir, "README.txt")
        val want = REPEATER_DROP_README
        if (!readme.isFile || readme.readText() != want) {
            readme.writeText(want)
            MediaScannerConnection.scanFile(
                context,
                arrayOf(dir.absolutePath, readme.absolutePath),
                arrayOf(null, "text/plain"),
                null,
            )
        }
        Log.i("NaviCat", "repeater-data ready at ${dir.absolutePath}")
    }

    private fun releaseLegacyRepeaterCsvGrants(context: Context) {
        val resolver = context.contentResolver
        for (perm in resolver.persistedUriPermissions) {
            val uri = perm.uri
            if (!uri.toString().contains("repeater-csv")) continue
            val flags =
                (if (perm.isReadPermission) android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION else 0) or
                    (if (perm.isWritePermission) android.content.Intent.FLAG_GRANT_WRITE_URI_PERMISSION else 0)
            runCatching { resolver.releasePersistableUriPermission(uri, flags) }
        }
    }
}
