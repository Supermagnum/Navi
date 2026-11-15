package no.navi.app

import android.system.Os
import android.util.Log

/**
 * Aligns the process env with [BuildConfig.MWM_CLIENT_ENABLED] so Rust
 * `NAVI_MWM_CLIENT_ENABLED` matches every build type (including F-Droid).
 *
 * Default is ON. Debug builds can force the classic `.rkyv` path with
 * `-Pnavi.mwmClient=false` or by setting the env before launch.
 */
object MwmClientBootstrap {
    private const val TAG = "MwmClientBootstrap"
    private const val ENV = "NAVI_MWM_CLIENT_ENABLED"

    @Volatile
    private var applied = false

    fun ensureInitialized() {
        if (applied) return
        synchronized(this) {
            if (applied) return
            val want = if (BuildConfig.MWM_CLIENT_ENABLED) "1" else "0"
            // Do not clobber an explicit process env (adb / instrumentation).
            val existing =
                runCatching { Os.getenv(ENV) }.getOrNull()?.trim().orEmpty()
            if (existing.isEmpty()) {
                runCatching { Os.setenv(ENV, want, true) }
                    .onFailure { Log.w(TAG, "setenv $ENV=$want failed", it) }
            }
            // Optional: CoMaps native core (from-source liborganicmaps.so).
            // Missing .so is fine for flag-OFF / host unit tests.
            runCatching { System.loadLibrary("organicmaps") }
                .onFailure { Log.i(TAG, "liborganicmaps not loaded (expected until Phase 3 package)") }
            applied = true
        }
    }
}
