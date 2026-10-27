package no.navi.app

import android.content.Context
import android.media.MediaScannerConnection
import uniffi.navi.catPluginConfigure
import uniffi.navi.catPluginInstallGuest
import uniffi.navi.catPluginSetEnabled
import java.io.File
import java.util.TimeZone
import java.util.concurrent.atomic.AtomicBoolean

/** One-time CATS plugin session setup (configure + wasm guest from assets). */
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

    private fun installGuestFromAssets(
        context: Context,
        name: String,
    ) {
        val marker = File(context.filesDir, "plugins/$name/.installed")
        if (marker.isFile) return
        val am = context.assets
        val manifest =
            am.open("plugins/$name/plugin.json").use { it.readBytes().decodeToString() }
        val wasm = am.open("plugins/$name/plugin.wasm").use { it.readBytes() }
        val status = catPluginInstallGuest(name, manifest, wasm)
        if (status.startsWith("OK")) {
            marker.parentFile?.mkdirs()
            marker.writeText(status)
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
        "Put RepeaterBook exports here, one file per country.\n" +
            "KML and KMZ are the primary source. CSV is accepted too; coordinates are used only when the file has valid latitude and longitude columns.\n" +
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
