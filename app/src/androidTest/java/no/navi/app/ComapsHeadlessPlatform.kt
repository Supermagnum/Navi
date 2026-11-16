package no.navi.app

import android.content.Context
import android.util.Log
import app.organicmaps.sdk.OrganicMaps

/**
 * Headless CoMaps suites (Phase 5 search, Phase 4 parity, …) open a routing/search
 * session without [ComapsDrapeBootstrap] / `OrganicMaps.init`.
 *
 * On Phase 6b drape builds the CoMaps Java SDK is packaged, so `JNI_OnLoad` caches
 * `Utils` and address/type localization calls into it. Without a Platform Context
 * those calls used to abort; production UI sets Context via Framework init.
 *
 * Call [ensureContext] before the first search / reverse-geocode in a headless test.
 * Does not change production init order.
 */
object ComapsHeadlessPlatform {
    private const val TAG = "ComapsHeadlessPlat"

    /**
     * Install the application [Context] into CoMaps Platform when the Java SDK is
     * packaged. No-op on SDK-less (true headless) builds where Utils is not cached.
     */
    fun ensureContext(context: Context) {
        if (!ComapsDrape.sdkPackaged()) return
        MwmClientBootstrap.ensureInitialized()
        val app = context.applicationContext
        runCatching { OrganicMaps.nativeSetContext(app) }
            .onSuccess { Log.i(TAG, "Platform Context set for headless suite") }
            .onFailure { Log.e(TAG, "nativeSetContext failed", it) }
    }

    /**
     * Like [ensureContext], then fails the test loudly if localization would still
     * run without a Context (drape SDK packaged but SetContext failed).
     */
    fun requireContext(context: Context) {
        ensureContext(context)
        if (!ComapsDrape.sdkPackaged()) return
        // Probe: after SetContext, a second call must succeed. There is no public
        // GetContext from Java; a failed SetContext leaves the suite in the same
        // state that previously aborted inside Utils.getStringValueByKey.
        val probe =
            runCatching {
                OrganicMaps.nativeSetContext(context.applicationContext)
                true
            }.getOrElse { e ->
                throw AssertionError(
                    "CoMaps Platform Context missing for headless suite " +
                        "(Phase 6b drape SDK packaged). Call ComapsHeadlessPlatform.ensureContext " +
                        "before search/reverse-geocode. Cause: ${e.message}",
                    e,
                )
            }
        check(probe) {
            "CoMaps Platform Context missing for headless suite " +
                "(Phase 6b drape SDK packaged). Call ComapsHeadlessPlatform.ensureContext " +
                "before search/reverse-geocode."
        }
    }
}
