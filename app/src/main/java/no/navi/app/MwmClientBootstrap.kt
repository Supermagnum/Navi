package no.navi.app

import android.system.Os
import android.util.Log

/**
 * MWM client is always enabled (Phase 8 / G4). Classic MapLibre / `.rkyv`
 * routing is not selectable in product builds; Phase 7 migration still detects
 * on-disk `.rkyv`. MapLibre may still be linked until full deletion lands.
 *
 * Aligns the process env so Rust sees `NAVI_MWM_CLIENT_ENABLED=1`.
 */
object MwmClientBootstrap {
    private const val TAG = "MwmClientBootstrap"
    private const val ENV = "NAVI_MWM_CLIENT_ENABLED"

    @Volatile
    private var applied = false

    /** True once `liborganicmaps` loaded in this process (set by [ensureInitialized]). */
    @Volatile
    var nativeLoaded = false
        private set

    /** Always true after Phase 8 client retirement. */
    fun isEnabled(): Boolean {
        ensureInitialized()
        return true
    }

    fun ensureInitialized() {
        if (applied) return
        synchronized(this) {
            if (applied) return
            runCatching { Os.setenv(ENV, "1", true) }
                .onFailure { Log.w(TAG, "setenv $ENV=1 failed", it) }
            if (!ComapsDrape.sdkPackaged()) {
                runCatching { Os.setenv("NAVI_COMAPS_HEADLESS", "1", true) }
            }
            nativeLoaded =
                runCatching { System.loadLibrary("organicmaps") }
                    .onFailure { Log.w(TAG, "liborganicmaps not loaded", it) }
                    .isSuccess
            applied = true
        }
    }
}
