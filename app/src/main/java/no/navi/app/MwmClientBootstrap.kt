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

    /** True once `liborganicmaps` loaded in this process (set by [ensureInitialized]). */
    @Volatile
    var nativeLoaded = false
        private set

    /** Effective flag: explicit process env wins, otherwise the build default. */
    fun isEnabled(): Boolean {
        ensureInitialized()
        val raw =
            runCatching { Os.getenv(ENV) }
                .getOrNull()
                ?.trim()
                ?.lowercase()
                .orEmpty()
        return when (raw) {
            "0", "false", "no", "off" -> false
            "1", "true", "yes", "on" -> true
            else -> BuildConfig.MWM_CLIENT_ENABLED
        }
    }

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
            // Headless CoMaps (routing / search only): JNI_OnLoad caches the JavaVM and
            // skips the CoMaps Java class cache. When the CoMaps Java SDK is packaged
            // (:comaps-sdk, Phase 6 drape) JNI_OnLoad must cache those classes, so the
            // headless marker is left unset and the Framework can be created later.
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
