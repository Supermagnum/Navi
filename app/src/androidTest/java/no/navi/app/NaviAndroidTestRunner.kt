package no.navi.app

import android.app.Application
import android.content.Context
import androidx.test.runner.AndroidJUnitRunner

/**
 * Enables MapLibre Vulkan MapView destroy deferral for the whole instrumented
 * suite so Activity teardown does not SIGSEGV in
 * `AndroidVulkanRendererBackend::~` on the AAOS emulator FinalizerDaemon.
 *
 * Also marks the first-run speed-camera opt-in prompt as already shown so
 * Compose/UI tests are not blocked by the modal (especially after `pm clear`
 * or a fresh install on NO/UK devices).
 */
class NaviAndroidTestRunner : AndroidJUnitRunner() {
    override fun newApplication(
        cl: ClassLoader?,
        className: String?,
        context: Context?,
    ): Application {
        NaviMapTestHooks.deferMapViewDestroy = true
        return super.newApplication(cl, className, context)
    }

    override fun onStart() {
        NaviMapTestHooks.deferMapViewDestroy = true
        // Target app context — SharedPreferences must be the app under test.
        runCatching {
            val ctx = targetContext
            MapHudPrefs.saveSpeedCameraPromptShown(ctx, true)
            MapHudPrefs.saveSpeedCameraOptIn(ctx, false)
        }
        super.onStart()
    }
}
