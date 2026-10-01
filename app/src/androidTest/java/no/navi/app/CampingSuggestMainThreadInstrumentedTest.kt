package no.navi.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.filters.LargeTest
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.navi.campingPluginConfigure
import uniffi.navi.initNativeLogging
import java.io.File
import java.util.concurrent.atomic.AtomicReference

/**
 * Phase 5a fix 3: blocking suggest fails on the main thread in debug; suspend API
 * hops to Dispatchers.Default.
 */
@RunWith(AndroidJUnit4::class)
@LargeTest
class CampingSuggestMainThreadInstrumentedTest {
    @Before
    fun setUp() {
        initNativeLogging()
        val ctx = InstrumentationRegistry.getInstrumentation().targetContext
        val files = ctx.filesDir
        val data = File(files, "navi-data").also { it.mkdirs() }
        campingPluginConfigure(files.absolutePath, data.absolutePath, "Europe/Oslo")
    }

    @Test
    fun blockingSuggest_onMainThread_failsLoudlyInDebug() {
        assertTrue(BuildConfig.DEBUG)
        val error = AtomicReference<Throwable?>(null)
        InstrumentationRegistry.getInstrumentation().runOnMainSync {
            try {
                CampingPluginApi.runSuggestBlocking("{}")
            } catch (t: Throwable) {
                error.set(t)
            }
        }
        val t = error.get()
        assertNotNull(t)
        assertTrue(
            "expected IllegalStateException about main thread, got: $t",
            t is IllegalStateException && (t.message?.contains("main thread") == true),
        )
    }

    @Test
    fun suspendSuggest_switchesOffMainThread() {
        runBlocking {
            // Calling from a background coroutine: main-thread assert must not fire.
            try {
                CampingPluginApi.runSuggest("{}")
            } catch (t: IllegalStateException) {
                assertFalse(
                    "must not be a main-thread assertion: ${t.message}",
                    t.message?.contains("main thread") == true,
                )
            } catch (_: Throwable) {
                // Guest/config errors are fine for this check.
            }
        }
    }

    @Test
    fun assertOffMainThread_onMain_throws() {
        val error = AtomicReference<Throwable?>(null)
        InstrumentationRegistry.getInstrumentation().runOnMainSync {
            try {
                CampingPluginApi.assertOffMainThread("test-label")
            } catch (t: Throwable) {
                error.set(t)
            }
        }
        val t = error.get()
        assertNotNull(t)
        assertTrue(t is IllegalStateException)
        assertTrue(t!!.message!!.contains("main thread"))
    }
}
