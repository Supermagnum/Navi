package no.navi.app

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.navi.ImuCallKind
import uniffi.navi.ImuTickResult

class ImuPluginControllerTest {
    private class FakeApi : ImuPluginApi {
        var enabled = false
        var tickResult = ok(state("uncalibrated"))
        var ticks = 0
        var releases = 0
        val commands = mutableListOf<String>()

        override fun tick(): ImuTickResult {
            ticks++
            return tickResult
        }

        override fun isEnabled() = enabled

        override fun setEnabled(enabled: Boolean): String {
            this.enabled = enabled
            return "OK"
        }

        override fun requestCalibrate(): String {
            commands += "calibrate"
            return "OK"
        }

        override fun requestReset(): String {
            commands += "reset"
            return "OK"
        }

        override fun releaseSensors() {
            releases++
        }
    }

    private val api = FakeApi()
    private val controller = ImuPluginController(api)

    @Test
    fun nothingTicksUntilInitializedAndEnabled() {
        controller.tickOnce()
        assertEquals(0, api.ticks)
        controller.markInitialized()
        controller.tickOnce()
        assertEquals("plugin is off by default", 0, api.ticks)
        assertFalse(controller.state.value.enabled)
    }

    @Test
    fun enabledStateIsRestoredFromTheNativeStore() {
        api.enabled = true
        controller.markInitialized()
        assertTrue(controller.state.value.enabled)
    }

    @Test
    fun runLoopIsGatedOnForeground() {
        controller.markInitialized()
        controller.setEnabled(true)
        assertFalse("background: do not run", controller.shouldRun())
        controller.setForeground(true)
        assertTrue(controller.shouldRun())
        controller.setForeground(false)
        assertFalse(controller.shouldRun())
    }

    @Test
    fun tickPublishesTheReadout() {
        controller.markInitialized()
        controller.setEnabled(true)
        controller.tickOnce()
        val ui = controller.state.value
        assertEquals(1, api.ticks)
        assertEquals(ImuStatus.UNCALIBRATED, ui.readout!!.status)
        assertNull(ui.message)
        assertFalse(ui.failed)
    }

    @Test
    fun failedTickReleasesSensorsAndStopsTheLoop() {
        controller.markInitialized()
        controller.setEnabled(true)
        controller.setForeground(true)
        api.tickResult =
            ImuTickResult(
                ImuCallKind.TIMEOUT,
                "IMU plugin disabled for this session: wall-clock timeout",
                null,
                1u,
            )
        controller.tickOnce()
        val ui = controller.state.value
        assertTrue(ui.failed)
        assertNull(ui.readout)
        assertEquals("IMU plugin disabled for this session: wall-clock timeout", ui.message)
        assertEquals(1, api.releases)
        assertFalse(controller.shouldRun())
        // Toggling the plugin clears the failure so it can retry.
        controller.setEnabled(false)
        controller.setEnabled(true)
        assertFalse(controller.state.value.failed)
        assertTrue(controller.shouldRun())
    }

    @Test
    fun disablingUnregistersSensorsAndClearsTheReadout() {
        controller.markInitialized()
        controller.setEnabled(true)
        controller.tickOnce()
        assertNotNull(controller.state.value.readout)
        controller.setEnabled(false)
        assertEquals(1, api.releases)
        assertNull(controller.state.value.readout)
        assertFalse(api.enabled)
    }

    @Test
    fun calibrateResetReachTheNativeSession() {
        controller.markInitialized()
        assertEquals("OK", controller.calibrate())
        assertEquals("OK", controller.reset())
        assertEquals(listOf("calibrate", "reset"), api.commands)
    }

    @Test
    fun loopTicksWhileRunnableAndUnregistersSensorsWhenStopped() =
        runBlocking {
            controller.markInitialized()
            controller.setEnabled(true)
            controller.setForeground(true)
            val scope = CoroutineScope(Dispatchers.Default)
            val job = scope.launch { controller.runLoop(tickMs = 5) }
            val deadline = System.currentTimeMillis() + 2_000
            while (api.ticks < 3 && System.currentTimeMillis() < deadline) Thread.sleep(5)
            assertTrue("loop ticked", api.ticks >= 3)
            job.cancelAndJoin()
            assertTrue("sensors released when the loop stops", api.releases >= 1)
        }

    private companion object {
        fun ok(json: String) = ImuTickResult(ImuCallKind.OK, "ok", json, 1u)

        fun state(status: String) =
            """
            {"status":"$status","status_text":"t","notice":null,"instruction":"x","calibrated":false,
             "calibrated_date":null,"pitch_deg":1.0,"roll_deg":-2.0,
             "tilt_deg":2.2,"heading_deg":null,
             "sensors":{"gravity":true,"accelerometer":true,"gyroscope":false,
                        "linear_acceleration":false,"magnetic_field":false},"updated_ms":1}
            """.trimIndent()
    }
}
