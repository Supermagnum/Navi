package no.navi.app

import android.app.Application
import android.content.Context
import androidx.test.runner.AndroidJUnitRunner

/**
 * Enables MapLibre Vulkan MapView destroy deferral for the whole instrumented
 * suite so Activity teardown does not SIGSEGV in
 * `AndroidVulkanRendererBackend::~` on the AAOS emulator FinalizerDaemon.
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
        super.onStart()
    }
}
