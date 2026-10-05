package no.navi.app

import android.graphics.Bitmap
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File

/**
 * Shared instrumented map screenshot helpers.
 *
 * Prefer [awaitRenderSettled] before any UiAutomation / shell `screencap`.
 * Waiting only for [NaviMapTestHooks.styleReady] (or a fixed sleep) can freeze a
 * mid-composite hydro soft edge that does not appear during live interactive use.
 */
object InstrumentedMapCapture {
    /**
     * Wait until the current MapLibre style apply is ready.
     *
     * @param afterReadyGeneration when > 0, require a successful apply whose ready
     * generation is strictly greater (survives mid-wait reloads that flip readiness
     * false then true again). When 0, any current [NaviMapTestHooks.styleReady]
     * (or a reported overlay layer) counts.
     */
    fun awaitStyleReady(
        timeoutMs: Long = 60_000,
        afterReadyGeneration: Int = 0,
    ): Boolean {
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            if (isStyleReady(afterReadyGeneration)) {
                return true
            }
            Thread.sleep(100)
        }
        return isStyleReady(afterReadyGeneration)
    }

    private fun isStyleReady(afterReadyGeneration: Int): Boolean {
        val readyGen = NaviMapTestHooks.styleReadyGeneration()
        if (NaviMapTestHooks.styleReady && readyGen > afterReadyGeneration) {
            return true
        }
        // Layer count is only a boot-strap signal for the first wait, not for
        // "wait until the next style apply finishes".
        return afterReadyGeneration == 0 && NaviMapTestHooks.lastReportedLayerCount >= 1
    }

    /**
     * Ask the live [MainActivity] MapView to report a fully rendered frame and
     * idle, then return. Safe to call when the activity is not showing a map
     * (times out and returns false).
     */
    fun awaitRenderSettled(timeoutMs: Long = 25_000): Boolean {
        if (!awaitStyleReady(timeoutMs.coerceAtMost(15_000))) {
            return false
        }
        val req = NaviMapTestHooks.renderSettleRequestId + 1
        NaviMapTestHooks.renderSettleRequestId = req
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            if (NaviMapTestHooks.lastRenderSettleId >= req) {
                // One extra vsync-ish pause so SurfaceFlinger can present the
                // settled MapLibre buffer before we copy pixels.
                Thread.sleep(64)
                return true
            }
            Thread.sleep(40)
        }
        return NaviMapTestHooks.lastRenderSettleId >= req
    }

    fun shell(cmd: String) {
        val pfd =
            InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand(cmd)
        java.io.FileInputStream(pfd.fileDescriptor).use { input ->
            val buf = ByteArray(4096)
            while (input.read(buf) >= 0) {
            }
        }
        pfd.close()
    }

    /** UiAutomation full-display screenshot after render settle. */
    fun takeScreenshotAfterSettle(timeoutMs: Long = 25_000): Bitmap? {
        awaitRenderSettled(timeoutMs)
        return InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
    }

    /**
     * Shell `screencap` after render settle. [devicePath] is typically under
     * `/data/local/tmp/…`.
     */
    fun screencapAfterSettle(
        devicePath: String,
        timeoutMs: Long = 25_000,
    ) {
        awaitRenderSettled(timeoutMs)
        shell("screencap -p $devicePath")
        shell("chmod 644 $devicePath")
    }

    /** Write a settled UiAutomation PNG to [out] and optionally mirror via screencap. */
    fun capturePng(
        out: File,
        deviceMirrorPath: String? = null,
        timeoutMs: Long = 25_000,
    ): Bitmap {
        val shot =
            takeScreenshotAfterSettle(timeoutMs)
                ?: error("UiAutomation screenshot null for ${out.name}")
        out.outputStream().use {
            shot.compress(Bitmap.CompressFormat.PNG, 100, it)
        }
        if (deviceMirrorPath != null) {
            screencapAfterSettle(deviceMirrorPath, timeoutMs = 5_000)
        }
        return shot
    }
}
