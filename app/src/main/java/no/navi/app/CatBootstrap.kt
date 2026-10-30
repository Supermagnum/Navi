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
                "was_marker=${marker.isFile} was_bytes=${wasmFile.length()} " +
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
     * App-owned drop folder. applicationId is `no.navi.app` with no suffix, so
     * this is `Android/data/no.navi.app/files/repeaters`.
     */
    fun repeaterDropDir(context: Context): File {
        val base = context.getExternalFilesDir(null) ?: context.filesDir
        return File(base, "repeaters")
    }

    const val REPEATER_DROP_README: String =
        "Put RepeaterBook exports here (KML/KMZ preferred; CSV accepted when it has valid latitude and longitude columns).\n" +
            "Exports may be split across multiple files when repeaters are networked; do not assume one file per country.\n" +
            "This folder is visible from a computer over USB (MTP) under Android/data/no.navi.app/files/repeaters.\n" +
            "Android deletes this folder when the app is uninstalled.\n" +
            "The app does not contact repeaterbook.com. You are responsible for complying with RepeaterBook's terms.\n"

    private fun ensureRepeaterDrop(context: Context) {
        val dir = repeaterDropDir(context)
        dir.mkdirs()
        val readme = File(dir, "README.txt")
        if (!readme.isFile) {
            readme.writeText(REPEATER_DROP_README)
            MediaScannerConnection.scanFile(
                context,
                arrayOf(readme.absolutePath),
                arrayOf("text/plain"),
                null,
            )
        }
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
