package no.navi.app

import android.content.Context
import uniffi.navi.campingPluginConfigure
import uniffi.navi.campingPluginInstallGuest
import java.io.File
import java.util.TimeZone
import java.util.concurrent.atomic.AtomicBoolean

/** One-time camping plugin session setup (configure + wasm guest from assets). */
object CampingBootstrap {
    private val configured = AtomicBoolean(false)

    fun ensureInitialized(context: Context) {
        if (!configured.compareAndSet(false, true)) return
        val filesDir = context.filesDir
        // Indexed graphs and region extracts live in filesDir (not a navi-data subdir).
        campingPluginConfigure(
            filesDir.absolutePath,
            filesDir.absolutePath,
            TimeZone.getDefault().id,
        )
        installGuestFromAssets(context, "right_to_roam_camping")
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
        val status = campingPluginInstallGuest(name, manifest, wasm)
        if (status.startsWith("OK")) {
            marker.parentFile?.mkdirs()
            marker.writeText(status)
        }
    }
}
