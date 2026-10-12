package no.navi.app

import android.content.Context
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
        catPluginConfigure(
            filesDir.absolutePath,
            filesDir.absolutePath,
            TimeZone.getDefault().id,
        )
        installGuestFromAssets(context, "cat")
        val want = MapHudPrefs.loadCatPluginEnabled(context)
        catPluginSetEnabled(want)
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
}
